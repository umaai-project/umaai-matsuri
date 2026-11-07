//! 在线路由与决策：一帧该问什么、问谁、以及 `--verify-training` 的验算
//!
//! ## 路由（真机 `playing_state` / `source` / `isAfterTraining` 三者定档）
//!
//! | 帧形态 | 走向 | 原因 |
//! |---|---|---|
//! | `source == special` + `playing_state = 51` + 教诲窗口 | 教诲分配 | T2/T11/T23/T35/T47/T59；按窗口去重 |
//! | 其余 `source == special`（`playing_state = 49/51`） | 跳过 | 発表会 / 本番的结算中间态，没有可选动作 |
//! | 有「多选项待选事件」 | 事件决策 | 事件由服务器挂起，等玩家选（结局 / 抽签等） |
//! | `isAfterTraining = true`（→ `AfterTrain`） | 跳过 | 动作已执行，剩余事件已结算完 |
//! | 预览非零 + 分布全空（T0 载入快照） | 跳过 | 帧自相矛盾，等 `command` 帧 |
//! | 其余（→ `Train`） | 训练 / 比赛决策 | 决策点 |
//!
//! 「多选项」口径照抄引擎 `Session::run_events`：`player_select && choices.len() > 1`
//! 才算需要决策——单选项事件只是一个结果，不问。

use anyhow::Result;
use matsuri_core::game::MatsuriGame;
use matsuri_core::{Action, Stage};
use umaai::decision::{Candidate, DecisionRecord};
use umaai_core::gamedata::EventData;

use crate::frame::{ParsedFrame, TrainingFrame};
use crate::output::{TrainingDeviation, TrainingReport};

/// 本帧的走向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// 训练 / 比赛决策
    Train,
    /// 教诲分配决策（窗口下标 0–5）
    Advice(usize),
    /// 事件选项决策
    Event,
    /// 不需要决策（附原因）
    Skip(&'static str),
}

/// 定路由
#[must_use]
pub fn route(game: &MatsuriGame, frame: &ParsedFrame) -> Route {
    // `special` 帧：ps=51 且落在教诲窗口 = 分配决策；其余是発表会 / 本番 / 合宿结算中间态
    if frame.source() == "special" {
        if frame.status.playing_state == 51
            && let Some(window) = game.plan.advice_window
        {
            return Route::Advice(window);
        }
        return Route::Skip("结算中间态（source=special）");
    }
    if pending_event(game).is_some() {
        return Route::Event;
    }
    if frame.is_after_training() {
        return Route::Skip("动作已执行且无待选事件（isAfterTraining）");
    }
    if game.stage != Stage::Train {
        return Route::Skip("还原阶段不是决策点");
    }
    // 过期快照（T0 载入帧）：预览说有人、分布说没人——帧自相矛盾，
    // 训练决策会建立在「人数恒 0」的还原态上；等下一张 `command` 帧即可
    if stale_snapshot(game, frame) {
        return Route::Skip("分布未捕获但预览非零（疑似载入快照，等 command 帧）");
    }
    Route::Train
}

/// 过期快照：训练预览已含落位效果（非零）但分布全空——帧自相矛盾
///
/// 只认这一类组合。预览全零 + 分布全空的帧（比赛回合、事件帧、合宿中间帧）
/// 是**合法**形态：分布本来就不随这类帧下发，比赛 / 事件决策也不依赖分布，
/// 照常走原路由。
#[must_use]
pub fn stale_snapshot(game: &MatsuriGame, frame: &ParsedFrame) -> bool {
    !distribution_captured(game)
        && frame
            .training
            .values()
            .any(|row| row.total().iter().any(|value| *value != 0))
}

/// 帧的分布是否已捕获（`personDistribution` 全空 = 未随帧下发 / 未捕获）
///
/// 中途重开的 `load` 帧（如 6308 T4、6310 T12）分布完整，照常参与决策；
/// 只有 T0 的 `load` 快照分布全空。判定只看分布本身，不看 `source`。
#[must_use]
pub fn distribution_captured(game: &MatsuriGame) -> bool {
    game.base.distribution.iter().any(|row| !row.is_empty())
}

/// 本帧待选的多选项事件（引擎口径：`player_select && choices.len() > 1`）
#[must_use]
pub fn pending_event(game: &MatsuriGame) -> Option<&EventData> {
    game.base
        .unresolved_events
        .iter()
        .find(|event| event.player_select && event.choices.len() > 1)
}

/// 手写档的决策解释（打分表 + 中选 + 理由；守门链命中时打分表为空）
pub struct TrainExplanation<'a> {
    /// 全部候选评分 `(动作下标, 分数)`
    pub candidates: &'a [(usize, f64)],
    /// 与 `candidates` 同长的搜索样本数；手写档为 `None`（记 0）
    pub counts: Option<&'a [u32]>,
    /// 中选动作下标
    pub chosen: usize,
    /// 中选原因
    pub reason: String,
}

/// 训练决策记录：手写档带打分表；搜索档带候选均分 / 样本数（由会话从 `last_search()` 取出）
pub fn train_record(
    game: &MatsuriGame,
    actions: &[Action],
    action_index: usize,
    explanation: Option<TrainExplanation<'_>>,
) -> Result<DecisionRecord> {
    let kind = if game.plan.race { "race" } else { "train" };
    let describe = |index: usize| {
        actions
            .get(index)
            .map_or_else(|| format!("#{index}"), |action| action.decision_label())
    };
    let mut record = DecisionRecord::action_only(kind, describe(action_index));
    match explanation {
        Some(TrainExplanation {
            candidates,
            counts,
            chosen,
            reason,
        }) => {
            record.candidates = candidates
                .iter()
                .enumerate()
                .map(|(position, (index, score))| Candidate {
                    description: describe(*index),
                    score: Some(*score),
                    n: counts
                        .and_then(|counts| counts.get(position))
                        .copied()
                        .unwrap_or(0),
                })
                .collect();
            // `DecisionRecord.action_index` 是**候选表下标**（不是动作下标）：
            // 教诲分配搜索只搜剪枝子集，必须把原动作下标映射回它在候选表里的位置。
            record.action_index = candidates
                .iter()
                .position(|(index, _)| *index == chosen)
                .unwrap_or(chosen);
            record.action_desc = describe(chosen);
            record.extra = Some(serde_json::json!({ "reason": reason }));
        }
        None => {
            record.candidates = actions
                .iter()
                .map(|action| Candidate {
                    description: action.decision_label(),
                    score: None,
                    n: 0,
                })
                .collect();
            record.action_index = action_index;
            record.action_desc = describe(action_index);
        }
    }
    Ok(record)
}

/// 教诲分配决策记录：候选为分配方案（搜索档取搜索摘要的 16 个剪枝候选）
#[must_use]
pub fn advice_record(
    game: &MatsuriGame,
    actions: &[Action],
    action_index: usize,
    summary: Option<&matsuri_core::SearchSummary>,
    fusion: Option<&str>,
) -> DecisionRecord {
    let describe = |index: usize| match actions.get(index) {
        Some(Action::AssignAdvice(assign)) => {
            assign.describe(&game.advice.slots, &game.advice.fused)
        }
        Some(action) => action.decision_label(),
        None => format!("#{index}"),
    };
    let mut record = DecisionRecord::action_only("advice", describe(action_index));
    record.candidates = actions
        .iter()
        .enumerate()
        .map(|(index, _)| Candidate {
            description: describe(index),
            score: None,
            n: 0,
        })
        .collect();
    record.action_index = action_index;
    if let Some(summary) = summary {
        record.candidates = summary
            .candidates
            .iter()
            .map(|candidate| Candidate {
                description: candidate.description.clone(),
                score: Some(candidate.score),
                n: candidate.n,
            })
            .collect();
        record.action_index = summary
            .candidates
            .iter()
            .position(|candidate| candidate.index == summary.chosen)
            .unwrap_or(0);
        record.action_desc = summary.candidates.get(record.action_index).map_or_else(
            || describe(summary.chosen),
            |candidate| candidate.description.clone(),
        );
    }
    let mut extra = serde_json::json!({
        "reason": "教诲分配：候选搜索",
        "advice_window": game.plan.advice_window,
    });
    if let Some(fusion) = fusion {
        extra["fusion"] = serde_json::json!(fusion);
    }
    record.extra = Some(extra);
    record
}

/// 事件决策记录：候选 = 选项组序号（协议没有选项名，用「选项 n」+ 事件名）
///
/// `scores` 为搜索档的 `(选项下标, 均分, 样本数)`；手写档传 `None`（保持旧 JSON 口径）。
#[must_use]
pub fn event_record(
    event: &EventData,
    choice: usize,
    scores: Option<&[(usize, f64, u32)]>,
) -> DecisionRecord {
    let candidates: Vec<Candidate> = event
        .choices
        .iter()
        .enumerate()
        .map(|(index, _group)| {
            let scored = scores.and_then(|scores| {
                scores
                    .iter()
                    .find(|(candidate, _, _)| *candidate == index)
                    .map(|(_, score, n)| (*score, *n))
            });
            Candidate {
                description: format!("选项{}", index + 1),
                score: scored.map(|(score, _)| score),
                n: scored.map_or(0, |(_, n)| n),
            }
        })
        .collect();
    let mut record = DecisionRecord::action_only("event", format!("选项{}", choice + 1));
    record.action_index = choice;
    record.candidates = candidates;
    record.extra = Some(serde_json::json!({
        "reason": format!("事件 {}", event.name),
        "event_id": event.id,
        "choice_groups": event.choices.len(),
    }));
    record
}

/// 训练协议真值 → `(训练位, 是否稽古)`
///
/// **指令 id 不是 101–105 顺序对应训练位**：官方 `GameGlobal.ToTrainIndex`
/// （小黑板 `LegendScenarioAnalyzer/GameGlobal.cs`）给的是
/// `101→速(0)`、`105→耐(1)`、`102→力(2)`、`103→根(3)`、`106→智(4)`；
/// 転成家族 `601–605` 反而是顺序的（`XiahesuIds`：`101→601`、`105→602`、`102→603`、
/// `103→604`、`106→605`）。按 101–105 顺排会把耐/力/根/智整列搬错位。
///
/// 其余家族（`1101–1105` / `2101–2105` / `2201–2205` / `2301–2305` / `901·902·906`）
/// 同一张表里也是顺序或既定点位；`5101–5105` 是演目稽古（与本仓训练位同序）。
#[must_use]
pub fn command_to_train(command_id: i32) -> Option<(usize, bool)> {
    match command_id {
        // 演目稽古（与本仓训练位同序，真机 T12 五条逐位验证过）
        5101..=5105 => Some(((command_id - 5101) as usize, true)),
        // 普通训练（官方 ToTrainIndex）
        101 => Some((0, false)),
        105 => Some((1, false)),
        102 => Some((2, false)),
        103 => Some((3, false)),
        106 => Some((4, false)),
        // 転成（下位→上位）家族：601–605 顺序
        601..=605 => Some(((command_id - 601) as usize, false)),
        // 同一训练位的其它 id 家族
        1101..=1105 => Some(((command_id - 1101) as usize, false)),
        2101..=2105 | 2201..=2205 | 2301..=2305 => Some((((command_id % 100) - 1) as usize, false)),
        901 => Some((0, false)),
        902 => Some((2, false)),
        906 => Some((4, false)),
        _ => None,
    }
}

/// `--verify-training`：把还原态的训练预览与帧真值逐项比对
///
/// `G13`：本函数只验算，不参与还原、不参与决策；默认在线运行不调用。
pub fn verify_training(
    game: &MatsuriGame,
    frame_training: &std::collections::BTreeMap<String, TrainingFrame>,
) -> Result<TrainingReport> {
    // 合宿期帧同时给「普通训练」与「稽古」两套预览，但普通训练此时不可用，
    // 游戏给的那套是被压小的占位值（T69：`101` 力 帧 1 / 本仓 22）。按阶段只比可用的那套。
    let camp = frame_training.keys().any(|key| {
        key.parse::<i32>()
            .is_ok_and(|id| command_to_train(id).is_some_and(|(_, keiko)| keiko))
    });
    let mut rows = Vec::new();
    for (key, training) in frame_training {
        let Ok(command_id) = key.parse::<i32>() else {
            continue;
        };
        let Some((train, keiko)) = command_to_train(command_id) else {
            continue;
        };
        if camp != keiko {
            continue;
        }
        let preview = game.preview_training(train, keiko)?;
        // 7 项定长：速 / 耐 / 力 / 根 / 智 / 技能点 / 体力
        let preview_values: Vec<i32> = preview
            .value
            .status_pt
            .iter()
            .copied()
            .chain(std::iter::once(preview.value.vital))
            .collect();
        let frame_values = training.total();
        // 全 0 = 本帧没有预览真值（事件 / 结算帧也会带一份空 `training`），不参与比对
        if frame_values.iter().all(|value| *value == 0) {
            continue;
        }
        // 帧内 `training` 永远是**表示値**（`ReviseOver1200`）：当前值 ≥1200 的轴整段折半，
        // 当前值 <1200 的轴只有「跨过 1200 的那部分」折半。统一交给
        // [`matsuri_core::restore_displayed_gain`] 还原，PT / 体力不参与折算。
        let frame_values: Vec<i32> = frame_values
            .iter()
            .enumerate()
            .map(|(axis, value)| {
                if axis < 5 {
                    matsuri_core::restore_displayed_gain(game.uma.five_status[axis], *value)
                } else {
                    *value
                }
            })
            .collect();
        let delta: Vec<i32> = preview_values
            .iter()
            .zip(frame_values.iter())
            .map(|(preview, frame)| preview - frame)
            .collect();
        rows.push(TrainingDeviation {
            train,
            keiko,
            command_id,
            frame: frame_values,
            preview: preview_values,
            delta,
            frame_failure_rate: training.failure_rate,
            preview_failure_rate: preview.failure_rate,
        });
    }
    rows.sort_by_key(|row| (row.train, !row.keiko));
    Ok(TrainingReport { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方映射：101 速 / 105 耐 / 102 力 / 103 根 / 106 智（**不是** 101–105 顺排）
    #[test]
    fn command_ids_map_to_trains() {
        // 稽古：与训练位同序
        assert_eq!(command_to_train(5101), Some((0, true)));
        assert_eq!(command_to_train(5105), Some((4, true)));
        // 普通训练：官方 ToTrainIndex
        assert_eq!(command_to_train(101), Some((0, false)), "101 = 速");
        assert_eq!(command_to_train(105), Some((1, false)), "105 = 耐");
        assert_eq!(command_to_train(102), Some((2, false)), "102 = 力");
        assert_eq!(command_to_train(103), Some((3, false)), "103 = 根");
        assert_eq!(command_to_train(106), Some((4, false)), "106 = 智");
        // 転成家族顺序
        assert_eq!(command_to_train(601), Some((0, false)));
        assert_eq!(command_to_train(605), Some((4, false)));
        // 其它家族
        assert_eq!(command_to_train(1103), Some((2, false)));
        assert_eq!(command_to_train(2204), Some((3, false)));
        assert_eq!(command_to_train(901), Some((0, false)));
        assert_eq!(command_to_train(902), Some((2, false)));
        // 未登记
        assert_eq!(command_to_train(104), None, "104 不在官方表里");
        assert_eq!(command_to_train(606), None);
    }
}
