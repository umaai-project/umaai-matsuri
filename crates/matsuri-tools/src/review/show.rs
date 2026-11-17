//! `review show`：按 `(turn, seq)` 输出指定帧的玩家可见状态（JSON）。
//!
//! 只读帧：不跑搜索、不重算。用于报告列出显著回合后，回答「这一回合当时是什么状态」。

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use matsuri_ai::online::command_to_train;
use matsuri_ai::{ParsedFrame, TrainingFrame};
use matsuri_core::{MatsuriConfig, MatsuriData, PersonView};
use serde_json::{Value, json};
use umaai_core::Data;

use super::discover::GameFiles;
use super::eval::parse_frame_name;
use super::facts::open_session;

const AXIS_NAMES: [&str; 5] = ["速", "耐", "力", "根", "智"];
const ACT_NAMES: [&str; 5] = ["流镝马", "神旗争夺战", "纳舞", "大曳神舆", "飞悬"];

/// `review show` 参数
pub struct ShowArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 回合（0 基）
    pub turn: i32,
    /// 同回合帧序号（`turn{N}_{seq}.json`；无后缀为 0）
    pub seq: u32,
    /// 数据根；`None` = 按 exe 位置解析
    pub data_root: Option<&'a Path>,
}

/// 跑一次展示，返回玩家可见状态 JSON
pub fn show(args: ShowArgs<'_>) -> Result<Value> {
    let files = GameFiles::scan(args.input, args.game)?;
    let seqs: Vec<u32> = files
        .frames
        .iter()
        .filter_map(|frame| parse_frame_name(&frame.name))
        .filter(|(turn, _)| *turn == args.turn)
        .map(|(_, seq)| seq)
        .collect();
    let target = files
        .frames
        .iter()
        .find(|frame| parse_frame_name(&frame.name) == Some((args.turn, args.seq)))
        .ok_or_else(|| {
            anyhow!(
                "T{} 没有 seq={} 的帧；本回合帧：{:?}",
                args.turn,
                args.seq,
                seqs
            )
        })?;

    let data_root = args
        .data_root
        .map_or_else(super::default_data_root, Path::to_path_buf);
    matsuri_ai::session::ensure_data(&data_root)?;
    let engine = umaai::Config::load_with_overlay_file(&data_root, ".")?;
    let data = Arc::new(MatsuriData::load(&data_root)?);
    let config = MatsuriConfig::load_with_overlay_file(&data_root, ".")?;
    let mut session = open_session(&engine, Arc::clone(&data), config)?;

    // 顺序回放到目标帧：只取该帧的视图（还原口径与 open/eval 一致）
    let mut view = None;
    for frame in &files.frames {
        match session.handle_frame(&frame.contents) {
            Ok(out) => {
                if frame.name == target.name {
                    view = Some(out.view);
                    break;
                }
            }
            Err(error) => {
                if frame.name == target.name {
                    return Err(error).with_context(|| format!("目标帧 {} 还原失败", frame.name));
                }
            }
        }
    }
    let view = view.ok_or_else(|| anyhow!("目标帧 {} 未产生视图", target.name))?;
    let parsed = ParsedFrame::parse(&target.contents).context("目标帧解析失败")?;
    let raw: Value = serde_json::from_str(&target.contents).context("目标帧 JSON 解析失败")?;
    let base = raw.get("baseGame").cloned().unwrap_or(Value::Null);
    let festa = raw.get("festa").cloned().unwrap_or(Value::Null);

    let uma_id = view.uma.uma_id;
    let uma = json!({
        "id": uma_id,
        "name": uma_name(uma_id),
        "vital": base.get("vital"),
        "maxVital": base.get("maxVital"),
        "motivation": base.get("motivation"),
        "five": base.get("fiveStatus"),
        "fiveLimit": base.get("fiveStatusLimit"),
        "skillPt": base.get("skillPt"),
        "skillPtSpent": base.get("skillPtSpent"),
        "totalHints": base.get("totalHints"),
    });

    let card_ids = base
        .get("cardId")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let cards: Vec<Value> = card_ids
        .iter()
        .filter_map(Value::as_u64)
        .map(|id| {
            // 帧内卡 ID = 基础卡 ID × 10 + 突破档（如 303174 → 30317 / 4）
            let base = (id / 10) as u32;
            json!({
                "id": base,
                "rank": id % 10,
                "name": card_name(base),
            })
        })
        .collect();

    let training = training_table(&parsed.training, &view);
    let persons: Vec<Value> = view.board.persons.iter().map(person_json).collect();
    let acts = acts_json(&festa);
    let event = event_json(&base);

    Ok(json!({
        "game": files.game,
        "frame": target.name,
        "turn": args.turn,
        "seq": args.seq,
        "frames": seqs,
        "source": base.get("source"),
        "playingState": base.get("playing_state"),
        "afterTraining": base.get("isAfterTraining"),
        "uma": uma,
        "cards": cards,
        "training": training,
        "persons": persons,
        "acts": acts,
        "turnState": {
            "stage": view.turn.stage.get(),
            "year": view.turn.year,
            "race": view.turn.race,
            "uraRace": view.turn.ura_race,
            "finals": view.turn.finals,
            "checkPoint": view.turn.check_point,
            "adviceWindow": view.turn.advice_window,
            "forcedRest": view.turn.forced_rest,
            "selfRaceAvailable": view.turn.self_race_available,
            "practice": view.turn.practice,
            "interlude": view.turn.interlude,
            "uraPeriod": view.turn.ura_period,
        },
        "friend": {
            "state": serde_json::to_value(view.friend.out_state).unwrap_or(Value::Null),
            "outUsed": view.friend.out_used,
            "nextStep": view.friend.next_step,
            "fatigueRecovery": view.friend.fatigue_recovery,
        },
        "camp": {
            "active": view.camp.active,
            "year": view.camp.year,
            "extended": view.camp.extended,
            "rest": view.camp.rest,
            "levelUp": view.camp.level_up,
        },
        "schedule": {
            "nextCheck": serde_json::to_value(view.schedule.next_check_point).unwrap_or(Value::Null),
            "goalRaces": view.schedule.goal_races,
        },
        "event": event,
    }))
}

/// 5 个训练位：命令号 → 训练位；收益 = lower + upper（速/耐/力/根/智/PT/体力）
fn training_table(
    training: &std::collections::BTreeMap<String, TrainingFrame>,
    view: &matsuri_core::MatsuriView,
) -> Vec<Value> {
    let mut rows: std::collections::BTreeMap<usize, (Option<usize>, TrainingFrame)> =
        std::collections::BTreeMap::new();
    for (cmd, row) in training {
        let Ok(id) = cmd.parse::<i32>() else { continue };
        let Some((index, is_act)) = command_to_train(id) else {
            continue;
        };
        let train = if is_act {
            view.resources
                .act_trains
                .get(index)
                .copied()
                .unwrap_or(index)
        } else {
            index
        };
        rows.insert(train, (is_act.then_some(index), row.clone()));
    }
    rows.into_iter()
        .map(|(train, (act, row))| {
            let gain = row.total();
            let members: Vec<Value> = view.board.distribution[train]
                .iter()
                .filter_map(|member| {
                    view.board
                        .persons
                        .get(member.person)
                        .map(|person| (member, person))
                })
                .map(|(member, person)| {
                    json!({
                        "name": person_label(person),
                        "friendship": person.friendship,
                        "hint": person.hint,
                        "shining": member.shining,
                    })
                })
                .collect();
            let name = act.map_or_else(
                || format!("{}训练", AXIS_NAMES.get(train).copied().unwrap_or("?")),
                |act| format!("{}稽古", act_name(act)),
            );
            json!({
                "train": train,
                "name": name,
                "practice": act.is_some(),
                "failureRate": row.failure_rate,
                "gain": {
                    "five": gain.get(..5),
                    "pt": gain.get(5),
                    "vital": gain.get(6),
                },
                "members": members,
            })
        })
        .collect()
}

/// 编成外 / 卡 / 友人的短名（与面板同口径）
fn person_label(person: &PersonView) -> String {
    let short = chara_name(person.chara_id);
    let short2: String = short.chars().take(2).collect();
    match person.origin {
        matsuri_core::game::PersonOrigin::Guest => format!("[NPC]{short2}"),
        matsuri_core::game::PersonOrigin::Yayoi => "理事长".to_string(),
        matsuri_core::game::PersonOrigin::Reporter => "记者".to_string(),
        matsuri_core::game::PersonOrigin::Card | matsuri_core::game::PersonOrigin::Friend => {
            if let Some(card_id) = person.card_id
                && let Ok(data) = Data::get()
                && let Ok(card) = data.get_card(card_id)
            {
                return card.short_name();
            }
            format!("[???]{short2}")
        }
    }
}

/// 单个人头的 JSON
fn person_json(person: &PersonView) -> Value {
    json!({
        "name": person_label(person),
        "origin": person.origin,
        "act": person.act,
        "friendship": person.friendship,
        "hint": person.hint,
        "available": person.available,
    })
}

/// 演目表（原始帧的 festa.acts）
fn acts_json(festa: &Value) -> Vec<Value> {
    festa
        .get("acts")
        .and_then(Value::as_array)
        .map(|acts| {
            acts.iter()
                .filter_map(|act| {
                    let act_type = act.get("actType")?.as_i64()? as usize;
                    let index = act_type.checked_sub(1)?;
                    Some(json!({
                        "act": index,
                        "name": act_name(index),
                        "gauge": act.get("gauge"),
                        "level": act.get("level"),
                        "advice": act.get("adviceId").and_then(Value::as_i64).map(|id| advice_name(id as i32)),
                        "fusion": act.get("fusionId").and_then(Value::as_i64).map(|id| advice_name(id as i32)),
                    }))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 事件（原始帧的 baseGame.story：名称 + 选项文本）
fn event_json(base: &Value) -> Value {
    let Some(story) = base.get("story") else {
        return Value::Null;
    };
    let choices: Vec<Vec<String>> = story
        .get("Choices")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|options| {
                            options
                                .iter()
                                .filter_map(|option| option.get("Option").and_then(Value::as_str))
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": story.get("Id"),
        "name": story.get("Name"),
        "choices": choices,
    })
}

fn text_name(group: &str, key: &str) -> Option<String> {
    Data::get()
        .ok()
        .and_then(|data| data.text.get(group))
        .and_then(|entries| entries.get(key))
        .cloned()
}

fn act_name(index: usize) -> String {
    text_name("462", &(index + 1).to_string())
        .unwrap_or_else(|| ACT_NAMES.get(index).copied().unwrap_or("演目").to_string())
}

fn advice_name(id: i32) -> String {
    if id == 0 {
        return "—".to_string();
    }
    text_name("463", &id.to_string())
        .map(|name| name.trim_end_matches("教诲").to_string())
        .unwrap_or_else(|| format!("#{id}"))
}

fn uma_name(uma_id: u32) -> String {
    Data::get().map_or_else(
        |_| format!("#{uma_id}"),
        |data| {
            data.get_uma(uma_id)
                .map_or_else(|_| format!("#{uma_id}"), |row| row.name.clone())
        },
    )
}

fn chara_name(chara_id: u32) -> String {
    Data::get().map_or_else(
        |_| format!("#{chara_id}"),
        |data| data.get_chara_name(chara_id).to_string(),
    )
}

fn card_name(card_id: u32) -> String {
    Data::get()
        .ok()
        .and_then(|data| data.get_card(card_id).ok())
        .map_or_else(|| format!("#{card_id}"), |card| card.card_name.clone())
}
