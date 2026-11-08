//! 训练公式分层拆解：把真机 `training` 的 `lower/upper` 与本仓的「下层/上层」逐层对照
//!
//! 本文件是**排查工具**，不设断言阈值：
//! `cargo test -p matsuri-ai --test layers -- --nocapture`
//!
//! 每行打印：帧两层、本仓两层、合计差、参与人头（担当 / 羁绊 / 闪彩），以及决定上层的那几项
//! （演目 Lv、教诲槽、発表会 buff、训练 Lv）。用于定位「下层错」还是「上层错」。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{MatsuriConfig, MatsuriData};
use umaai_core::Data;
use umasim::game::Game;

/// 要拆解的回合：剧本首帧（已知 0 偏差）/ 首次偏差 / 発表会后 / 稽古重训练 / 后期
const TURNS: [i32; 9] = [2, 3, 12, 16, 20, 24, 40, 48, 60];

fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_Ai_Test/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

#[test]
fn dump_training_layers() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在");
        return Ok(());
    };
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".")?;
    let data = Arc::new(MatsuriData::load(&root)?);
    let mut session = OnlineSession::new(
        &cfg,
        Arc::clone(&data),
        matsuri_cfg,
        TrainerKind::Hand,
        false,
    )?;
    let mut source = FrameSource::open_replay(&dir)?;

    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind != umaai::FrameKind::ThisTurn {
            continue;
        }
        let parsed = matsuri_ai::ParsedFrame::parse(&frame.contents)?;
        if parsed.epoch_key() != 6310 {
            continue;
        }
        let output = session.handle_frame(&frame.contents)?;
        let turn = output.turn;
        if !TURNS.contains(&turn) {
            continue;
        }
        if parsed.training.is_empty() {
            continue;
        }
        let view = &output.view;
        let res = &view.resources;
        println!(
            "\n===== T{turn}  训练Lv {:?}  演目Lv {:?}  gauge {:?}",
            view.board.train_levels, res.act_level, res.act_gauge
        );
        println!(
            "      教诲槽 {:?} 転成 {:?}  発表会 友情+{} 擅长+{} 灵感+{}  助力{} 追稽{}",
            res.advice_slots,
            res.advice_fused,
            res.presentation_friendship,
            res.presentation_deyilv,
            res.presentation_hint_rate,
            res.assist_support_left,
            res.kensan_pending
        );

        let mut keys: Vec<&String> = parsed.training.keys().collect();
        keys.sort();
        for key in keys {
            let Ok(command_id) = key.parse::<i32>() else {
                continue;
            };
            let Some((train, keiko)) = matsuri_ai::online::command_to_train(command_id) else {
                continue;
            };
            let row = &parsed.training[key];
            let frame_total = row.total();
            // 帧值是「表示値」：属性 ≥1200 时折半，这里按验算同口径 ×2 还原后再比
            let half: Vec<bool> = (0..5)
                .map(|axis| {
                    output.game.uma.five_status[axis] >= matsuri_core::DISPLAY_HALF_FROM_STATUS
                })
                .collect();
            let frame_total: Vec<i32> = frame_total
                .iter()
                .enumerate()
                .map(|(axis, value)| {
                    if half.get(axis).copied().unwrap_or(false) {
                        value * 2
                    } else {
                        *value
                    }
                })
                .collect();
            if frame_total.iter().all(|value| *value == 0) {
                continue;
            }
            let layers = output.game.preview_training_layers(train, keiko)?;
            let total = &layers.total.status_pt;
            let delta: Vec<i32> = (0..6).map(|axis| total[axis] - frame_total[axis]).collect();
            let delta_lower: Vec<i32> = (0..5)
                .map(|axis| layers.lower.status_pt[axis] - row.lower[axis])
                .collect();
            let delta_upper: Vec<i32> = (0..5)
                .map(|axis| layers.upper.status_pt[axis] - row.upper[axis])
                .collect();

            println!(
                "  cmd{command_id} 训练位{train}{} 闪彩{}  帧 lower{:?} upper{:?} 总{:?}",
                if keiko { "·稽古" } else { "" },
                layers.shining,
                &row.lower[..5],
                &row.upper[..5],
                &frame_total[..6]
            );
            println!(
                "                我 lower{:?} upper{:?} 总{:?} 稽古flat{}",
                &layers.lower.status_pt[..5],
                &layers.upper.status_pt[..5],
                &total[..6],
                layers.keiko_flat
            );
            println!("                Δ总{delta:?} Δ下层{delta_lower:?} Δ上层{delta_upper:?}");
            // 交叉口径：同一训练位按「另一套表」再算一遍，判断是不是选表错了
            let alt = output.game.preview_training_layers(train, !keiko)?;
            println!(
                "                交叉{}口径 total{:?}（差帧 {:?}）",
                if keiko { "普通" } else { "稽古" },
                &alt.total.status_pt[..6],
                (0..6)
                    .map(|axis| alt.total.status_pt[axis] - frame_total[axis])
                    .collect::<Vec<_>>()
            );

            // 下层公式的输入：基础表行、人数、buff
            let level = output.game.train_level(train).saturating_sub(1);
            let table_row: Option<Vec<i32>> = if keiko {
                output
                    .game
                    .data
                    .training
                    .keiko
                    .junior
                    .get(train)
                    .map(|row| row.to_vec())
            } else {
                output
                    .game
                    .data
                    .training
                    .normal
                    .get(train)
                    .and_then(|rows| rows.get(level))
                    .map(|row| row.to_vec())
            };
            let buffs = output.game.calc_training_buff(train)?;
            println!(
                "                成长率{:?} 干劲{} 体力{}",
                output.game.uma().five_status_bonus,
                output.game.uma().motivation,
                output.game.uma().vital
            );
            println!(
                "                下层输入 表行{:?} 人数{} 友情{:.1} 训练{:.1} 干劲{:.1} 羁绊{:?} 得意{:?} 体力减免{:.0}% 我体力{} 帧体力{}",
                table_row,
                output.game.count_training_persons(train),
                buffs.youqing,
                buffs.xunlian,
                buffs.ganjing,
                buffs.bonus,
                buffs.deyilv,
                buffs.vital_cost_drop,
                layers.total.vital,
                frame_total.get(6).copied().unwrap_or(0),
            );
            // 逐个参与者的友情（看是不是乘算）
            if layers.shining > 0 {
                let mut parts = Vec::new();
                for member in &view.board.distribution[train] {
                    let Some(card) = output.game.deck.get(member.person) else {
                        continue;
                    };
                    let effect = card.calc_training_effect(&output.game, train as i32);
                    parts.push(format!(
                        "{}{}友情{:.1}%",
                        card.card_id,
                        if member.shining { "★" } else { "" },
                        effect.youqing
                    ));
                }
                println!("                人才友情 {}", parts.join(" "));
            }

            // 参与人头：origin / 担当 / 羁绊 / 闪彩
            let members: Vec<String> = view.board.distribution[train]
                .iter()
                .map(|member| {
                    let person = &view.board.persons[member.person];
                    format!(
                        "{:?}#{}a{}b{}{}",
                        person.origin,
                        person.chara_id,
                        person.act.map_or("-".to_string(), |act| act.to_string()),
                        person.friendship,
                        if member.shining { "★" } else { "" }
                    )
                })
                .collect();
            println!("                参与 {}", members.join(" "));
        }
        if turn >= *TURNS.last().unwrap_or(&60) {
            break;
        }
    }
    Ok(())
}
