//! 事实层：逐帧还原 → 回合事实 + 终局 + 检查项 + 待重算清单
//!
//! 零重算：只用帧 + 终局帧（+ `review` 段）。T(n)/regret 一律进 `pending`。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use matsuri_ai::{OnlineSession, TrainerKind, parse_final_score};
use matsuri_core::{DISPLAY_HALF_FROM_STATUS, MatsuriConfig, MatsuriData};
use serde::Serialize;
use serde_json::Value;
use umasim::search::FlatSearchGame;

use super::discover::GameFiles;

/// 构建会话（手写训练员：事实层不跑搜索）
pub fn open_session(
    engine: &umaai::Config,
    data: Arc<MatsuriData>,
    config: MatsuriConfig,
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
    /// 检查项
    pub checks: Vec<Check>,
    /// 待重算（`review eval` 的输入）
    pub pending: Vec<Pending>,
}

/// 数据来源
#[derive(Debug, Serialize)]
pub struct SourceFact {
    /// 输入形态：`frames`（散帧目录）
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
    /// 五维
    pub five: [i32; 5],
    /// 五维上限
    pub five_limit: [i32; 5],
    /// 技能点
    pub skill_pt: i32,
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
    /// 人头数
    pub persons: usize,
    /// 后续目标赛回合
    pub goal_races: Vec<i32>,
    /// 该局面官方口径评分（`search_score(1.0)`）
    pub score: f64,
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
    /// `warn` / `info`
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
    pub fn build(files: &GameFiles, session: &mut OnlineSession) -> Result<Self> {
        let mut turns = Vec::new();
        let mut failed = Vec::new();
        for path in &files.frames {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("读取帧失败：{}", path.display()))?;
            match session.handle_frame(&text) {
                Ok(out) => {
                    let view = out.view;
                    let score = out.game.search_score(1.0).unwrap_or(f64::NAN);
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
                        act_level: view.resources.act_level,
                        act_gauge: view.resources.act_gauge,
                        advice_slots: view.resources.advice_slots,
                        advice_fused: view.resources.advice_fused,
                        check_points: serde_json::to_value(view.resources.check_points)
                            .unwrap_or(Value::Null),
                        camp_counted: view.camp.counted,
                        camp_total: view.camp.total,
                        persons: view.board.persons.len(),
                        goal_races: view.schedule.goal_races,
                        score,
                    });
                }
                Err(error) => {
                    failed.push(format!(
                        "{}: {error}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
            }
        }

        let (final_score, review) = match files.final_frame.as_deref() {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("读取终局帧失败：{}", path.display()))?;
                let frame = parse_final_score(&text)?;
                let out = session.handle_final(&text)?;
                let raw: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let review = raw.get("review").cloned();
                let fact = match (out.accepted, out.score, out.rank) {
                    (true, Some(score), Some(rank)) => Some(FinalFact {
                        score,
                        rank,
                        skill_pt: frame.skill_pt,
                        five: frame.five_status,
                        five_limit: frame.five_status_limit,
                        actions: frame.actions.as_ref().map(Vec::len),
                        review,
                    }),
                    _ => None,
                };
                (fact, raw.get("review").cloned())
            }
            None => (None, None),
        };

        let first_turn = turns.first().map(|row| row.turn);
        let last_turn = turns.last().map(|row| row.turn);
        let missing_turns = match (first_turn, last_turn) {
            (Some(first), Some(last)) => {
                let present: std::collections::BTreeSet<i32> =
                    turns.iter().map(|row| row.turn).collect();
                (first..=last)
                    .filter(|turn| !present.contains(turn))
                    .collect()
            }
            _ => Vec::new(),
        };

        let checks = build_checks(&turns, final_score.as_ref(), &missing_turns);
        let pending = build_pending(&turns);

        Ok(Self {
            game: files.game,
            source: SourceFact {
                kind: "frames".to_string(),
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
            checks,
            pending,
        })
    }
}

/// 首批检查项（判据简单、可复现；复杂项等真机语义确认后再加）
fn build_checks(
    turns: &[TurnFact],
    final_score: Option<&FinalFact>,
    missing: &[i32],
) -> Vec<Check> {
    let mut checks = Vec::new();
    if final_score.is_none() {
        checks.push(Check {
            level: "warn".to_string(),
            kind: "no_final".to_string(),
            turn: turns.last().map(|row| row.turn),
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
    let low: Vec<&TurnFact> = turns.iter().filter(|row| row.vital < 35).collect();
    if !low.is_empty() {
        let min = low.iter().map(|row| row.vital).min().unwrap_or(0);
        let min_turn = low.iter().find(|row| row.vital == min).map(|row| row.turn);
        checks.push(Check {
            level: "info".to_string(),
            kind: "low_vital".to_string(),
            turn: min_turn,
            text: format!(
                "回合末体力 < 35 共 {} 次，最低 {}（{}）",
                low.len(),
                min,
                fmt_turn(min_turn)
            ),
        });
    }
    if let Some(fact) = final_score
        && fact.skill_pt > 0
    {
        checks.push(Check {
            level: "info".to_string(),
            kind: "skill_pt_left".to_string(),
            turn: None,
            text: format!("终局仍有 {} 技能点未花", fact.skill_pt),
        });
    }
    checks
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
    let mut seen = std::collections::BTreeSet::new();
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

fn fmt_turn(turn: Option<i32>) -> String {
    turn.map_or_else(|| "—".to_string(), |turn| format!("T{turn}"))
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
