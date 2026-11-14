//! 事实层：逐帧还原 → 回合事实 + 终局 + 反推 + 检查项 + 待重算清单
//!
//! 零重算：只用帧 + 终局帧（含 `review` 段与 `actions`）。T(n) / regret 一律进 `pending`。

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use matsuri_ai::{OnlineSession, ParsedFrame, TrainerKind, TrainingFrame, parse_final_score};
use matsuri_core::data::CheckPointSchedule;
use matsuri_core::{DISPLAY_HALF_FROM_STATUS, MatsuriData};
use serde::Serialize;
use serde_json::Value;
use umaai::protocol::final_score::FinalAction;
use umasim::game::Game;
use umasim::search::FlatSearchGame;

use super::derive::{self, Derive, TraceRow, TrainPreview};
use super::discover::GameFiles;
use super::tuning::Tuning;

/// 构建会话（手写训练员：事实层不跑搜索）
pub fn open_session(
    engine: &umaai::Config,
    data: Arc<MatsuriData>,
    config: matsuri_core::MatsuriConfig,
) -> Result<OnlineSession> {
    OnlineSession::new(engine, data, config, TrainerKind::Hand, false)
}

/// 一局的事实层产物
#[derive(Debug, Serialize)]
pub struct Facts {
    /// 局号
    pub game: u64,
    /// 数据来源
    pub source: SourceFact,
    /// 覆盖情况
    pub coverage: Coverage,
    /// 逐回合事实（按回合升序）
    pub turns: Vec<TurnFact>,
    /// 终局（无正常终局时为空）
    pub final_score: Option<FinalFact>,
    /// 帧 diff 反推（教诲 / 転成 / 学技能 / 追加稽古 / 动作）
    pub derive: Derive,
    /// 剧本专属事实（演目 / 出行 / swings / 继承 / 状态 / 自选赛）
    pub script: ScriptFacts,
    /// 检查项
    pub checks: Vec<Check>,
    /// 待重算（`review eval` 的输入）
    pub pending: Vec<Pending>,
}

/// 数据来源
#[derive(Debug, Serialize)]
pub struct SourceFact {
    /// 输入形态：`frames`（散帧目录）/ `zip`
    pub kind: String,
    /// 帧数
    pub frames: usize,
}

/// 覆盖情况
#[derive(Debug, Serialize)]
pub struct Coverage {
    /// 首回合
    pub first_turn: Option<i32>,
    /// 末回合
    pub last_turn: Option<i32>,
    /// 缺回合（首末之间）
    pub missing_turns: Vec<i32>,
    /// `review` 段是否可读
    pub has_review: bool,
    /// 解析失败帧
    pub failed_frames: Vec<String>,
}

/// 逐回合事实
#[derive(Debug, Serialize)]
pub struct TurnFact {
    /// 回合
    pub turn: i32,
    /// 年次
    pub year: matsuri_core::MatsuriYear,
    /// 阶段号
    pub stage: Value,
    /// 本回合是否目标赛
    pub race: bool,
    /// 本回合是否 URA 决赛
    pub ura_race: bool,
    /// 進捗発表会 / 本番场次
    pub check_point: Option<usize>,
    /// 是否教诲窗口
    pub advice_window: bool,
    /// 按排练处理的演目
    pub practice: [bool; 5],
    /// 强制休息
    pub forced_rest: bool,
    /// 体力
    pub vital: i32,
    /// 干劲
    pub motivation: i32,
    /// 五维（真值，≥1200 不减半）
    pub five: [i32; 5],
    /// 五维上限
    pub five_limit: [i32; 5],
    /// 技能点（还原口径：剩余 + 本局累计花掉）
    pub skill_pt: i32,
    /// 帧内剩余技能点**原值**（学技能反推用）
    pub remaining_skill_pt: i32,
    /// 演目 Lv
    pub act_level: [i32; 5],
    /// 演目 gauge
    pub act_gauge: [i32; 5],
    /// 教诲槽（0 = 未分配）
    pub advice_slots: [i32; 5],
    /// 転成教诲（0 = 未転成）
    pub advice_fused: [i32; 5],
    /// 発表会结果
    pub check_points: Value,
    /// 合宿：本年度已计入回合
    pub camp_counted: usize,
    /// 合宿：本年度窗口总回合
    pub camp_total: Option<usize>,
    /// 本回合是否延长期合宿
    pub camp_extended: bool,
    /// 人头数
    pub persons: usize,
    /// 已用友人出行段数
    pub out_used: i32,
    /// 助力支撑剩余
    pub assist_support_left: i32,
    /// 后续目标赛回合
    pub goal_races: Vec<i32>,
    /// 帧来源（`command` / `event` / `special` / `load`）
    pub source: String,
    /// 帧内 `storyId`
    pub story_id: Option<i32>,
    /// 帧内训练预览（只有训练屏帧非空）
    pub train: Vec<PreviewFact>,
    /// 该局面官方口径评分（`search_score(1.0)`）
    pub score: f64,
    /// 评分分量（五维得分 + PT + 技能分；swings 分解用）
    pub parts: [i32; 7],
}

/// 帧内一条训练预览（精简）
#[derive(Debug, Serialize)]
pub struct PreviewFact {
    /// 命令 id
    pub cmd: String,
    /// 失败率
    pub failure_rate: f32,
}

/// 终局事实
#[derive(Debug, Serialize)]
pub struct FinalFact {
    /// 评分（点技能前口径）
    pub score: i32,
    /// 档次
    pub rank: String,
    /// 剩余技能点
    pub skill_pt: i32,
    /// 本局累计花掉的技能点
    pub skill_pt_spent: i32,
    /// 继承增量（逐次）
    pub inherit_gains: Vec<i32>,
    /// 五维
    pub five: [i32; 5],
    /// 五维上限
    pub five_limit: [i32; 5],
    /// 玩家动作条数
    pub actions: Option<usize>,
    /// 终局帧的 `review` 段原文
    pub review: Option<Value>,
}

/// 检查项
#[derive(Debug, Serialize)]
pub struct Check {
    /// `gate` / `warn` / `info`
    pub level: String,
    /// 类型
    pub kind: String,
    /// 相关回合
    pub turn: Option<i32>,
    /// 说明
    pub text: String,
}

/// 待重算项
#[derive(Debug, Serialize)]
pub struct Pending {
    /// 决策类型：`advice` / `checkpoint`
    pub kind: String,
    /// 回合
    pub turn: i32,
    /// 估算耗时（毫秒，N=1024）
    pub est_ms: u64,
}

impl Facts {
    /// 读帧 → 事实
    pub fn build(
        files: &GameFiles,
        session: &mut OnlineSession,
        data: &MatsuriData,
        tuning: &Tuning,
    ) -> Result<Self> {
        let mut turns = Vec::new();
        let mut trace = Vec::new();
        let mut failed = Vec::new();
        let mut cards: Vec<u32> = Vec::new();
        let mut last_flags: Option<umaai_core::UmaFlags> = None;
        let mut last_races: Vec<i32> = Vec::new();
        let mut last_free: Vec<(i32, i32, u32)> = Vec::new();
        for frame in &files.frames {
            let out = match session.handle_frame(&frame.contents) {
                Ok(out) => out,
                Err(error) => {
                    failed.push(format!("{}: {error}", frame.name));
                    continue;
                }
            };
            let parsed = match ParsedFrame::parse(&frame.contents) {
                Ok(parsed) => parsed,
                Err(error) => {
                    failed.push(format!("{}: {error}", frame.name));
                    continue;
                }
            };
            let view = out.view;
            let score = out.game.search_score(1.0).unwrap_or(f64::NAN);
            let parts = out.game.uma().score_parts().map_or([0; 7], |parts| {
                let mut out = [0i32; 7];
                out[..5].copy_from_slice(&parts.five_status);
                out[5] = parts.pt;
                out[6] = parts.skill;
                out
            });
            if cards.is_empty() {
                cards = parsed.status.card_id.clone();
            }
            last_flags = Some(view.uma.flags.clone());
            last_races = out.game.uma().list_races();
            last_free = out
                .game
                .uma()
                .free_races
                .iter()
                .map(|race| (race.start_turn as i32, race.end_turn as i32, race.count))
                .collect();
            let source = parsed.status.source.clone().unwrap_or_default();
            let previews: Vec<TrainPreview> = parsed
                .training
                .iter()
                .map(|(cmd, row)| TrainPreview {
                    cmd: cmd.clone(),
                    failure_rate: row.failure_rate,
                    total: total7(row),
                })
                .collect();
            let train: Vec<PreviewFact> = previews
                .iter()
                .map(|row| PreviewFact {
                    cmd: row.cmd.clone(),
                    failure_rate: row.failure_rate,
                })
                .collect();
            trace.push(TraceRow {
                name: frame.name.clone(),
                turn: view.turn.turn,
                is_after_training: parsed.status.is_after_training,
                source,
                story_id: parsed.story_id,
                skill_pt: parsed.status.skill_pt,
                vital: view.uma.vital,
                motivation: view.uma.motivation,
                five: view.uma.five_status,
                act_level: view.resources.act_level,
                advice: view.resources.advice_slots,
                fusion: view.resources.advice_fused,
                practice: view.turn.practice,
                kensan_act: view.resources.kensan_act,
                camp_extended: view.camp.extended,
                check_point: view.turn.check_point,
                out_used: view.friend.out_used.iter().filter(|used| **used).count() as i32,
                training: previews,
                inherit_gains: parsed.status.inherit_gains.clone(),
            });
            turns.push(TurnFact {
                turn: view.turn.turn,
                year: view.turn.year,
                stage: serde_json::to_value(view.turn.stage).unwrap_or(Value::Null),
                race: view.turn.race,
                ura_race: view.turn.ura_race,
                check_point: view.turn.check_point,
                advice_window: view.turn.advice_window,
                practice: view.turn.practice,
                forced_rest: view.turn.forced_rest,
                vital: view.uma.vital,
                motivation: view.uma.motivation,
                five: view.uma.five_status,
                five_limit: view.uma.five_status_limit,
                skill_pt: view.uma.skill_pt,
                remaining_skill_pt: parsed.status.skill_pt,
                act_level: view.resources.act_level,
                act_gauge: view.resources.act_gauge,
                advice_slots: view.resources.advice_slots,
                advice_fused: view.resources.advice_fused,
                check_points: serde_json::to_value(view.resources.check_points)
                    .unwrap_or(Value::Null),
                camp_counted: view.camp.counted,
                camp_total: view.camp.total,
                camp_extended: view.camp.extended,
                persons: view.board.persons.len(),
                out_used: view.friend.out_used.iter().filter(|used| **used).count() as i32,
                assist_support_left: view.resources.assist_support_left,
                goal_races: view.schedule.goal_races,
                source: parsed.status.source.clone().unwrap_or_default(),
                story_id: parsed.story_id,
                train,
                score,
                parts,
            });
        }

        let mut actions: Vec<FinalAction> = Vec::new();
        let mut spent = 0i32;
        let (final_score, review) = match files.final_frame.as_ref() {
            Some(final_frame) => {
                let text = &final_frame.contents;
                let parsed_final = parse_final_score(text)?;
                actions = parsed_final.actions.clone().unwrap_or_default();
                spent = parsed_final.skill_pt_spent;
                let raw: Value = serde_json::from_str(text).unwrap_or(Value::Null);
                let review = raw.get("review").cloned();
                let out = session.handle_final(text)?;
                let fact = match (out.accepted, out.score, out.rank) {
                    (true, Some(score), Some(rank)) => Some(FinalFact {
                        score,
                        rank,
                        skill_pt: parsed_final.skill_pt,
                        skill_pt_spent: parsed_final.skill_pt_spent,
                        inherit_gains: parsed_final.inherit_gains.clone(),
                        five: parsed_final.five_status,
                        five_limit: parsed_final.five_status_limit,
                        actions: parsed_final.actions.as_ref().map(Vec::len),
                        review: review.clone(),
                    }),
                    _ => None,
                };
                (fact, review)
            }
            None => (None, None),
        };

        let derive = derive::build(&trace, &actions, spent);
        let first_turn = turns.first().map(|row| row.turn);
        let last_turn = turns.last().map(|row| row.turn);
        let missing_turns = match (first_turn, last_turn) {
            (Some(first), Some(last)) => {
                let present: BTreeSet<i32> = turns.iter().map(|row| row.turn).collect();
                (first..=last)
                    .filter(|turn| !present.contains(turn))
                    .collect()
            }
            _ => Vec::new(),
        };
        let script = build_script(ScriptInput {
            turns: &turns,
            trace: &trace,
            derive: &derive,
            tuning,
            data,
            cards: &cards,
            flags: last_flags.as_ref(),
            races: &last_races,
            free_races: &last_free,
        });
        let checks = build_checks(
            &turns,
            &trace,
            &derive,
            &script,
            final_score.as_ref(),
            &missing_turns,
            &failed,
            review.is_some(),
            data,
        );
        let pending = build_pending(&turns);

        Ok(Self {
            game: files.game,
            source: SourceFact {
                kind: files.source.clone(),
                frames: files.frames.len(),
            },
            coverage: Coverage {
                first_turn,
                last_turn,
                missing_turns,
                has_review: review.is_some(),
                failed_frames: failed,
            },
            turns,
            final_score,
            derive,
            script,
            checks,
            pending,
        })
    }
}

/// 检查项总表：数据健康 4 条 + 机制门禁（B1 / B4 / B6 / B7 / C3 / C4）
#[allow(clippy::too_many_arguments)]
fn build_checks(
    turns: &[TurnFact],
    trace: &[TraceRow],
    derive: &Derive,
    script: &ScriptFacts,
    final_score: Option<&FinalFact>,
    missing: &[i32],
    failed: &[String],
    has_review: bool,
    data: &MatsuriData,
) -> Vec<Check> {
    let mut checks = Vec::new();
    checks.extend(health_checks(final_score, missing, failed, has_review));
    checks.extend(check_point_checks(trace, &data.schedule));
    checks.extend(camp_checks(trace));
    checks.extend(vital_checks(trace, derive));
    checks.extend(motivation_checks(trace, derive));
    checks.extend(train_fail_checks(derive));
    checks.extend(resource_checks(turns, final_score.is_some()));
    checks.extend(outing_checks(script));
    checks.extend(free_race_checks(script, final_score.is_some()));
    checks
}

/// 判据 (2)：出行两条 warn（T23 前 <4 段 / T36 前未走满 5 段）
fn outing_checks(script: &ScriptFacts) -> Vec<Check> {
    let mut checks = Vec::new();
    if script.outing.before_t23 < 4 {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "outing_t23".to_string(),
            turn: Some(23),
            text: format!("T23 前只走了 {}/5 段友人出行", script.outing.before_t23),
        });
    }
    if !script.outing.full_before_t36 {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "outing_t36".to_string(),
            turn: Some(36),
            text: match script.outing.full_turn {
                Some(turn) => format!("T36 前未走满 5 段（走满于 T{turn}）"),
                None => format!("T36 前未走满 5 段（局末 {}/5）", script.outing.used),
            },
        });
    }
    checks
}

/// E7：自选赛区间未达标（info）
fn free_race_checks(script: &ScriptFacts, has_final: bool) -> Vec<Check> {
    let mut checks = Vec::new();
    if !has_final {
        return checks;
    }
    for race in &script.free_races {
        if race.done >= race.required {
            continue;
        }
        checks.push(Check {
            level: "info".to_string(),
            kind: "e7_free_race".to_string(),
            turn: Some(race.end),
            text: format!(
                "自选赛区间 T{}–T{} 只跑 {}/{} 场（未打满）",
                race.start, race.end, race.done, race.required
            ),
        });
    }
    checks
}

/// 数据健康：无终局 / 缺回合 / 解析失败 / `review` 段缺失
fn health_checks(
    final_score: Option<&FinalFact>,
    missing: &[i32],
    failed: &[String],
    has_review: bool,
) -> Vec<Check> {
    let mut checks = Vec::new();
    if final_score.is_none() {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "no_final".to_string(),
            turn: None,
            text: "没有正常终局帧：未终局或终局帧缺失，终局评分与总运气不可用".to_string(),
        });
    }
    if !missing.is_empty() {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "missing_turns".to_string(),
            turn: missing.first().copied(),
            text: format!("缺 {} 个回合帧：{}", missing.len(), fmt_turns(missing)),
        });
    }
    if !failed.is_empty() {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "failed_frames".to_string(),
            turn: None,
            text: format!("解析失败 {} 帧：{}", failed.len(), failed.join("；")),
        });
    }
    if final_score.is_some() && !has_review {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "no_review".to_string(),
            turn: None,
            text: "终局帧没有 review 段（旧插件或人工拼包）：局边界与缺帧只能自算".to_string(),
        });
    }
    checks
}

/// B1：進捗発表会 / 本番结算前演目 Lv 未达门槛（gate）
fn check_point_checks(trace: &[TraceRow], schedule: &[CheckPointSchedule]) -> Vec<Check> {
    let mut checks = Vec::new();
    for row in schedule {
        // 没打到这一回合（局中途结束 / 缺帧）不判失败，也不给"缺帧不可判"噪声
        if !trace.iter().any(|frame| frame.turn >= row.turn) {
            continue;
        }
        let Some(frame) = trace.iter().rev().find(|frame| frame.turn < row.turn) else {
            checks.push(Check {
                level: "info".to_string(),
                kind: format!("b1_t{}", row.turn),
                turn: Some(row.turn),
                text: "発表会结算前没有可用帧：门槛不可判".to_string(),
            });
            continue;
        };
        let low: Vec<String> = (0..5)
            .filter(|act| frame.act_level[*act] < row.need_act_level)
            .map(|act| {
                format!(
                    "{}(Lv{})",
                    ["速", "耐", "力", "根", "智"][act],
                    frame.act_level[act]
                )
            })
            .collect();
        if low.is_empty() {
            continue;
        }
        checks.push(Check {
            level: "gate".to_string(),
            kind: format!("b1_t{}", row.turn),
            turn: Some(row.turn),
            text: format!(
                "発表会结算前演目 Lv 未达门槛 {}：未达标 {}（需 {} 个演目达标）",
                row.need_act_level,
                low.join(" / "),
                row.need_act_num
            ),
        });
    }
    checks
}

/// B4：合宿延长期回合（info）
fn camp_checks(trace: &[TraceRow]) -> Vec<Check> {
    let extended: Vec<i32> = trace
        .iter()
        .filter(|row| row.camp_extended)
        .map(|row| row.turn)
        .collect::<BTreeSet<i32>>()
        .into_iter()
        .collect();
    if extended.is_empty() {
        return Vec::new();
    }
    vec![Check {
        level: "info".to_string(),
        kind: "b4_camp_extended".to_string(),
        turn: extended.first().copied(),
        text: format!(
            "合宿延长期回合 {} 个：{}",
            extended.len(),
            fmt_turns(&extended)
        ),
    }]
}

/// B6：回合末体力 < 32 且当回合训练无失败风险（gate）
fn vital_checks(trace: &[TraceRow], derive: &Derive) -> Vec<Check> {
    const VITAL_FLOOR: i32 = 32;
    let mut checks = Vec::new();
    for record in &derive.training {
        if record.base_after {
            continue;
        }
        let Some(rate) = record.failure_rate else {
            continue;
        };
        if rate > f32::EPSILON {
            continue;
        }
        let Some(next) = trace.iter().find(|row| row.turn > record.turn) else {
            continue;
        };
        if next.vital >= VITAL_FLOOR {
            continue;
        }
        checks.push(Check {
            level: "gate".to_string(),
            kind: "b6_vital_line".to_string(),
            turn: Some(record.turn),
            text: format!(
                "在 0% 失败率的训练上硬练，回合末体力 {}（<{}）",
                next.vital, VITAL_FLOOR
            ),
        });
    }
    checks
}

/// B7：干劲掉落未在 3 回合内恢复（gate）
fn motivation_checks(trace: &[TraceRow], derive: &Derive) -> Vec<Check> {
    let rests: BTreeSet<i32> = derive
        .decisions
        .iter()
        .filter(|row| row.kind == "rest")
        .map(|row| row.turn)
        .collect();
    let mut checks = Vec::new();
    let mut seen = BTreeSet::new();
    for pair in trace.windows(2) {
        let (prev, cur) = (&pair[0], &pair[1]);
        if prev.motivation <= cur.motivation || prev.turn == cur.turn {
            continue;
        }
        if !seen.insert(cur.turn) {
            continue;
        }
        let window_end = cur.turn + 3;
        let recovered = trace.iter().any(|row| {
            row.turn > cur.turn && row.turn <= window_end && row.motivation > cur.motivation
        });
        let acted = rests
            .iter()
            .any(|turn| *turn >= cur.turn && *turn <= window_end)
            || trace.iter().any(|row| {
                row.turn >= cur.turn && row.turn <= window_end && row.out_used > cur.out_used
            });
        if recovered || acted {
            continue;
        }
        checks.push(Check {
            level: "gate".to_string(),
            kind: "b7_motivation".to_string(),
            turn: Some(cur.turn),
            text: format!(
                "干劲从 {} 掉到 {}，之后 3 回合既没出行也没休息",
                prev.motivation, cur.motivation
            ),
        });
    }
    checks
}

/// C3：训练失败（期望非零、实际五维零增长）
fn train_fail_checks(derive: &Derive) -> Vec<Check> {
    derive
        .training
        .iter()
        .filter(|record| record.failed && !record.base_after)
        .map(|record| Check {
            level: "gate".to_string(),
            kind: "c3_train_failed".to_string(),
            turn: Some(record.turn),
            text: format!(
                "训练失败：命令 {} 的预览五维增量非零，实际五维零增长",
                record.cmd
            ),
        })
        .collect()
}

/// C4：关键资源（吾妻出行 5 段 / 助力支撑剩余）
///
/// 只在**正常终局**时判：中途局（未终局）走不满属正常，不做门禁。
fn resource_checks(turns: &[TurnFact], has_final: bool) -> Vec<Check> {
    let mut checks = Vec::new();
    let Some(last) = turns.last() else {
        return checks;
    };
    if !has_final {
        return checks;
    }
    if last.out_used < 5 {
        checks.push(Check {
            level: "gate".to_string(),
            kind: "c4_outing".to_string(),
            turn: Some(last.turn),
            text: format!("吾妻出行只走了 {}/5 段（末帧）", last.out_used),
        });
    }
    if last.assist_support_left > 0 {
        checks.push(Check {
            level: "info".to_string(),
            kind: "c4_assist".to_string(),
            turn: Some(last.turn),
            text: format!("末帧仍有 {} 次助力支撑未用", last.assist_support_left),
        });
    }
    checks
}

/// 训练预览 7 项定长（速 / 耐 / 力 / 根 / 智 / 技能点 / 体力）
fn total7(row: &TrainingFrame) -> [i32; 7] {
    let mut out = [0; 7];
    for (slot, value) in out.iter_mut().zip(row.total()) {
        *slot = value;
    }
    out
}

/// 待重算：教诲窗口与発表会（需要候选表 / 价值矩阵）
///
/// 同一回合可能有多帧（训练前 / 训练后），按 `(kind, turn)` 去重。
/// 耗时按串行 `候选数 × N × 1ms` 的量级估计，只用于让 skill 判断值不值得花算力。
fn build_pending(turns: &[TurnFact]) -> Vec<Pending> {
    const ROLLOUT_MS: u64 = 1;
    const ADVICE_CANDIDATES: u64 = 16;
    const EVENT_CANDIDATES: u64 = 5;
    let mut pending = Vec::new();
    let mut seen = BTreeSet::new();
    for row in turns {
        let item = if row.advice_window {
            Some(("advice", ADVICE_CANDIDATES))
        } else if row.check_point.is_some() {
            Some(("checkpoint", EVENT_CANDIDATES))
        } else {
            None
        };
        let Some((kind, candidates)) = item else {
            continue;
        };
        if !seen.insert((kind, row.turn)) {
            continue;
        }
        pending.push(Pending {
            kind: kind.to_string(),
            turn: row.turn,
            est_ms: candidates * 1024 * ROLLOUT_MS,
        });
    }
    pending
}

/// 五维显示值（>1200 部分减半）
pub fn display(value: i32) -> i32 {
    if value > DISPLAY_HALF_FROM_STATUS {
        (value - DISPLAY_HALF_FROM_STATUS) / 2 + DISPLAY_HALF_FROM_STATUS
    } else {
        value
    }
}

fn fmt_turns(turns: &[i32]) -> String {
    let mut parts = Vec::new();
    for turn in turns {
        parts.push(format!("T{turn}"));
        if parts.len() >= 8 {
            parts.push("…".to_string());
            break;
        }
    }
    parts.join(", ")
}
// ---------------------------------------------------------------------------
// 剧本专属事实（判据 (1)–(6)，(5) 复用 derive.kensan）
// ---------------------------------------------------------------------------

/// 剧本专属事实
#[derive(Debug, Serialize)]
pub struct ScriptFacts {
    /// 各演目进度（判据 (1)）
    pub acts: Vec<ActProgress>,
    /// 友人出行（判据 (2)）
    pub outing: OutingFacts,
    /// 运气波动窗口（判据 (3)）
    pub swings: Vec<SwingWindow>,
    /// 继承对照（判据 (4)）
    pub inherit: Vec<InheritJudge>,
    /// 状态资源（判据 (6)）
    pub status: StatusFacts,
    /// 自选赛进度（E7）
    pub free_races: Vec<FreeRaceProgress>,
}

/// 单个演目的进度
#[derive(Debug, Serialize)]
pub struct ActProgress {
    /// 演目下标（0–4）
    pub act: usize,
    /// 对应训练位
    pub train: usize,
    /// 该训练位是否带支援卡（卡位判定；无卡位只判结算达标）
    pub card: bool,
    /// 局末 Lv
    pub max_level: i32,
    /// 首次到达各级的回合（`(Lv, turn)`，按 Lv 升序）
    pub nodes: Vec<(i32, i32)>,
    /// 稽古动作次数（`derive.training` 里该演目的稽古）
    pub keiko_actions: usize,
}

/// 友人出行（吾妻）
#[derive(Debug, Serialize)]
pub struct OutingFacts {
    /// 局末已走段数
    pub used: i32,
    /// 每段完成的回合（按完成顺序）
    pub stages: Vec<i32>,
    /// 走满 5 段的回合
    pub full_turn: Option<i32>,
    /// T23 前已走段数
    pub before_t23: i32,
    /// 是否在 T36 前走满
    pub full_before_t36: bool,
}

/// 一个 5 回合阴跌窗口
#[derive(Debug, Serialize)]
pub struct SwingWindow {
    /// 窗口起回合
    pub from: i32,
    /// 窗口止回合
    pub to: i32,
    /// 窗口内累计分差
    pub delta: i32,
}

/// 一次继承的对照
#[derive(Debug, Serialize)]
pub struct InheritJudge {
    /// 回合
    pub turn: i32,
    /// 实际增量
    pub gain: i32,
    /// 标准（`tuning.inherit_expect`）
    pub expect: f64,
    /// `gain − expect`
    pub delta: f64,
}

/// 状态资源
#[derive(Debug, Serialize)]
pub struct StatusFacts {
    /// 切者
    pub qie: bool,
    /// 小切
    pub xiaoqie: bool,
    /// 心情盾剩余
    pub positive_thinking: i32,
    /// 幸运体质次数
    pub lucky: i32,
    /// 休息心得
    pub refresh_mind: bool,
    /// 生病
    pub ill: bool,
}

/// 一个自选赛区间的完成情况
#[derive(Debug, Serialize)]
pub struct FreeRaceProgress {
    /// 区间开始回合
    pub start: i32,
    /// 区间结束回合
    pub end: i32,
    /// 要求场数
    pub required: u32,
    /// 已完成场数
    pub done: u32,
    /// 达标回合（该区间内第 `required` 场）
    pub filled_turn: Option<i32>,
}

/// 局末快照（build 阶段采集，供剧本专属事实使用）
pub struct ScriptInput<'a> {
    /// 逐回合事实
    pub turns: &'a [TurnFact],
    /// 帧轨迹
    pub trace: &'a [TraceRow],
    /// 反推结果
    pub derive: &'a Derive,
    /// 复盘口径
    pub tuning: &'a super::tuning::Tuning,
    /// 剧本资产（演目→训练位）
    pub data: &'a MatsuriData,
    /// 首帧卡组
    pub cards: &'a [u32],
    /// 局末状态位
    pub flags: Option<&'a umaai_core::UmaFlags>,
    /// 局末已赛回合
    pub races: &'a [i32],
    /// 自选赛区间（起 / 止 / 要求场数）
    pub free_races: &'a [(i32, i32, u32)],
}

/// 汇总剧本专属事实
pub fn build_script(input: ScriptInput<'_>) -> ScriptFacts {
    ScriptFacts {
        acts: act_progress(input.turns, input.derive, input.data, input.cards),
        outing: outing_facts(input.trace),
        swings: swing_windows(input.turns),
        inherit: inherit_judges(input.derive, input.tuning),
        status: status_facts(input.flags),
        free_races: free_race_progress(input.free_races, input.races),
    }
}

/// 演目进度：节点 / 局末 Lv / 稽古次数 / 卡位
fn act_progress(
    turns: &[TurnFact],
    derive: &Derive,
    data: &MatsuriData,
    cards: &[u32],
) -> Vec<ActProgress> {
    let mut out = Vec::new();
    for act in 0..5 {
        let train = data.acts.get(act).map_or(act, |row| row.train);
        let card = cards.iter().any(|id| {
            umasim::game::SupportCard::new(*id).is_ok_and(|card| card.card_type == train as i32)
        });
        let max_level = turns
            .iter()
            .map(|row| row.act_level[act])
            .max()
            .unwrap_or(0);
        let mut nodes = Vec::new();
        for level in 2..=max_level.max(2) {
            if let Some(turn) = turns
                .iter()
                .find(|row| row.act_level[act] >= level)
                .map(|row| row.turn)
            {
                nodes.push((level, turn));
            }
        }
        let keiko_actions = derive
            .training
            .iter()
            .filter(|record| record.keiko == Some(act))
            .count();
        out.push(ActProgress {
            act,
            train,
            card,
            max_level,
            nodes,
            keiko_actions,
        });
    }
    out
}

/// 友人出行：每段完成回合 / 走满回合 / 截止线前状态
fn outing_facts(trace: &[TraceRow]) -> OutingFacts {
    let mut stages = Vec::new();
    let mut prev = 0i32;
    let mut full_turn = None;
    let mut before_t23 = 0i32;
    for row in trace {
        if row.out_used <= prev {
            continue;
        }
        for _ in prev..row.out_used {
            stages.push(row.turn);
        }
        prev = row.out_used;
        if row.turn < 23 {
            before_t23 = row.out_used;
        }
        if row.out_used >= 5 && full_turn.is_none() {
            full_turn = Some(row.turn);
        }
    }
    OutingFacts {
        used: prev,
        stages,
        full_turn,
        before_t23,
        full_before_t36: full_turn.is_some_and(|turn| turn < 36),
    }
}

/// 5 回合窗口累计分差 < −300 的时段（只陈述，不归因）
fn swing_windows(turns: &[TurnFact]) -> Vec<SwingWindow> {
    let mut per_turn: Vec<(i32, i32)> = Vec::new();
    for row in turns {
        let total: i32 = row.parts.iter().sum();
        if per_turn.last().is_some_and(|(turn, _)| *turn == row.turn) {
            let index = per_turn.len() - 1;
            per_turn[index] = (row.turn, total);
        } else {
            per_turn.push((row.turn, total));
        }
    }
    let mut out = Vec::new();
    if per_turn.len() < 2 {
        return out;
    }
    for index in 0..per_turn.len().saturating_sub(4) {
        let from = per_turn[index].0;
        let to = per_turn[index + 4].0;
        let start = per_turn[index].1;
        let end = per_turn[index + 4].1;
        let delta = end - start;
        if delta < -300 {
            out.push(SwingWindow { from, to, delta });
        }
    }
    out
}

/// 继承对照（只出实际值与标准，不做 verdict）
fn inherit_judges(derive: &Derive, tuning: &super::tuning::Tuning) -> Vec<InheritJudge> {
    derive
        .inherit
        .iter()
        .map(|step| InheritJudge {
            turn: step.turn,
            gain: step.gain,
            expect: tuning.inherit_expect,
            delta: f64::from(step.gain) - tuning.inherit_expect,
        })
        .collect()
}

/// 状态资源（局末）
fn status_facts(flags: Option<&umaai_core::UmaFlags>) -> StatusFacts {
    match flags {
        Some(flags) => StatusFacts {
            qie: flags.qiezhe,
            xiaoqie: flags.xiaoqie,
            positive_thinking: flags.positive_thinking_count,
            lucky: flags.lucky_count,
            refresh_mind: flags.refresh_mind > 0,
            ill: !flags.ill.is_empty(),
        },
        None => StatusFacts {
            qie: false,
            xiaoqie: false,
            positive_thinking: 0,
            lucky: 0,
            refresh_mind: false,
            ill: false,
        },
    }
}

/// 自选赛进度：区间内已完成场数与达标回合
fn free_race_progress(free_races: &[(i32, i32, u32)], races: &[i32]) -> Vec<FreeRaceProgress> {
    free_races
        .iter()
        .map(|(start, end, required)| {
            let in_range: Vec<i32> = races
                .iter()
                .copied()
                .filter(|turn| *turn >= *start && *turn <= *end)
                .collect();
            let done = in_range.len() as u32;
            let filled_turn = in_range.get(required.saturating_sub(1) as usize).copied();
            FreeRaceProgress {
                start: *start,
                end: *end,
                required: *required,
                done,
                filled_turn,
            }
        })
        .collect()
}
