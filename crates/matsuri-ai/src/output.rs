//! 在线一帧的输出 DTO（JSON 与面板共用同一份数据）
//!
//! 纪律：本模块**只产出值**，不打印、不落盘；JSON 由 `matsuri-cli` 逐行写出。
//! JSON 形态严格沿用旧 `StdoutJsonSink` 的 10 字段（路线 A）：
//! `type / turn / scenario / decision_kind / action_index / score / candidate_scores /
//! candidate_descriptions / candidate_n / scenario_extra`，`type ∈ decision / info / final`。
//!
//! **手写档没有搜索分**：`score = 0.0`、`candidate_n = 0`，文档口径「0 = 无搜索分，非有效评分」；
//! 候选打分表仍在 `candidate_scores` / `candidate_descriptions` 里（供小黑板显示），
//! 由 `scenario_extra.score_source` 标明来源。

use matsuri_core::{Action, MAX_TURN, MatsuriGame, MatsuriView, SearchSummary};
use serde_json::{Value, json};
use umaai::DecisionRecord;
use umaai_core::Data;

/// 一帧处理后的走向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnKind {
    /// 出了一个决策（动作 / 事件选项）
    Decision,
    /// 该帧不需要决策（重复帧 / 结算中间态 / 已执行且无待选事件）
    Skipped,
}

/// 一帧的输出
#[derive(Debug, Clone)]
pub struct OnlineTurnOutput {
    /// 回合（0 基）
    pub turn: i32,
    /// 育成局号
    pub epoch: u64,
    /// 剧本名（JSON `scenario` 字段）
    pub scenario: String,
    /// 帧原文摘要
    pub frame_digest: u64,
    /// 帧 `baseGame.storyId`（待选事件 id；无事件帧为 `None`）
    pub story_id: Option<i32>,
    /// 帧 `story` 里各选项的文本（按选项组；空串 = 该组没有文本）
    ///
    /// 只作呈现：属性仍按还原出的 `EventData` 显示（帧文本可能与本仓数据不同语言 / 精度）。
    pub story_options: Vec<String>,
    /// 走向
    pub kind: TurnKind,
    /// 决策记录（`kind == Decision` 时必有）
    pub record: Option<DecisionRecord>,
    /// 跳过原因（`kind == Skipped` 时必有）
    pub skip_reason: Option<String>,
    /// 帧解析 / 还原的附注（引擎 `FrameOutcome.notes` 等）
    pub notes: Vec<String>,
    /// 回合信息快照（面板与下游消费者的单一数据源）
    pub view: MatsuriView,
    /// 该决策的分数是否来自搜索（false = 手写档 / 无搜索分）
    pub search_backed: bool,
    /// `--verify-training` 的偏差表（未开启时 `None`）
    pub verify: Option<TrainingReport>,
    /// 还原出的局面：**只给终端面板渲染用**（JSON 渲染不读；面板需要完整局面而非视图投影）
    pub game: MatsuriGame,
    /// 本帧的动作候选（面板列名；跳过帧为空）
    pub actions: Vec<Action>,
    /// 面板用候选分 `(动作下标, 分数)`；JSON 不读（手写档为打分表，搜索档为搜索均分）
    pub candidate_scores: Vec<(usize, f64)>,
    /// 本次搜索的完整摘要（搜索档；含终局五维 / PT / 分辨率；手写档为 `None`）
    pub summary: Option<SearchSummary>,
}

impl OnlineTurnOutput {
    /// 生成 JSON 行（10 字段，未格式化）
    ///
    /// 手写档按旧口径把 `score` / `candidate_n` 归零，并在 `scenario_extra` 里标注来源。
    #[must_use]
    pub fn json_payload(&self) -> Value {
        match (self.kind, &self.record) {
            (TurnKind::Decision, Some(record)) => {
                let scores: Vec<Option<f64>> = record
                    .candidates
                    .iter()
                    .map(|candidate| candidate.score)
                    .collect();
                let descriptions: Vec<&str> = record
                    .candidates
                    .iter()
                    .map(|candidate| candidate.description.as_str())
                    .collect();
                let score = if self.search_backed {
                    record
                        .candidates
                        .get(record.action_index)
                        .and_then(|candidate| candidate.score)
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                let candidate_n = if self.search_backed {
                    record
                        .candidates
                        .get(record.action_index)
                        .map_or(0, |candidate| candidate.n)
                } else {
                    0
                };
                let mut extra = record.extra.clone().unwrap_or_else(|| json!({}));
                if let Some(map) = extra.as_object_mut() {
                    map.insert(
                        "score_source".to_string(),
                        json!(if self.search_backed { "search" } else { "hand" }),
                    );
                    // 待选事件 id（插件只发 id；未收录事件便于对账）
                    if let Some(story_id) = self.story_id {
                        map.insert("story_id".to_string(), json!(story_id));
                    }
                    // 期望分（官方口径）/ 单局终局 σ / 档次（搜索档；与终端运气行同口径）
                    if self.search_backed
                        && let Some(summary) = &self.summary
                    {
                        map.insert(
                            "expected_score".to_string(),
                            json!(summary.expected_score.round() as i64),
                        );
                        map.insert(
                            "expected_stdev".to_string(),
                            json!(summary.baseline_stdev.round() as i64),
                        );
                        map.insert("expected_rank".to_string(), json!(summary.expected_rank));
                    }
                }
                json!({
                    "type": "decision",
                    "turn": self.turn,
                    "scenario": self.scenario,
                    "decision_kind": record.kind,
                    "action_index": record.action_index,
                    "score": score,
                    "candidate_scores": scores,
                    "candidate_descriptions": descriptions,
                    "candidate_n": candidate_n,
                    "scenario_extra": extra,
                })
            }
            (TurnKind::Skipped, _) => json!({
                "type": "info",
                "event": "frame_skipped",
                "turn": self.turn,
                "scenario": self.scenario,
                "story_id": self.story_id,
                "reason": self.skip_reason.clone().unwrap_or_default(),
            }),
            // `Decision` 却没记录属内部矛盾；退化成 info，不 panic
            (TurnKind::Decision, None) => json!({
                "type": "info",
                "event": "decision_missing",
                "turn": self.turn,
                "scenario": self.scenario,
            }),
        }
    }

    /// 终局帧的 JSON 行（`type = final`）
    #[must_use]
    pub fn final_payload(
        turn: i32,
        scenario: &str,
        accepted: bool,
        score: Option<i32>,
        rank: Option<&str>,
        reason: Option<&str>,
    ) -> Value {
        json!({
            "type": "final",
            "turn": turn,
            "scenario": scenario,
            "accepted": accepted,
            "score": score,
            "rank": rank,
            "reason": reason,
        })
    }
}

/// 终局帧处理结果（`matsuri-ai` 只产出值，渲染归 CLI）
#[derive(Debug, Clone)]
pub struct FinalTurnOutput {
    /// 终局回合（0 基）
    pub turn: i32,
    /// 育成局号
    pub epoch: u64,
    /// 剧本名（JSON `scenario` 字段）
    pub scenario: String,
    /// 是否被本会话接受（局号不符 / 数值不可用 → false）
    pub accepted: bool,
    /// 终局评分（点技能前口径）；未接受时 `None`
    pub score: Option<i32>,
    /// 档次名；未接受时 `None`
    pub rank: Option<String>,
    /// 未接受原因
    pub reason: Option<String>,
}

impl FinalTurnOutput {
    /// 终局帧的 JSON 行（`type = final`）
    #[must_use]
    pub fn json_payload(&self) -> Value {
        OnlineTurnOutput::final_payload(
            self.turn,
            &self.scenario,
            self.accepted,
            self.score,
            self.rank.as_deref(),
            self.reason.as_deref(),
        )
    }
}

/// 启动握手：`--json` 模式的第一行，告诉消费者「已开始接收帧」
#[must_use]
pub fn connected_payload(scenario: &str) -> Value {
    json!({
        "type": "info",
        "event": "connected",
        "scenario": scenario,
    })
}

/// 单帧失败（真机档不终止循环；由 CLI 决定是否继续）
#[must_use]
pub fn error_payload(message: &str) -> Value {
    json!({
        "type": "error",
        "scenario": "matsuri",
        "message": message,
    })
}

/// 训练验算偏差表（`--verify-training`）
#[derive(Debug, Clone, Default)]
pub struct TrainingReport {
    /// 逐条偏差
    pub rows: Vec<TrainingDeviation>,
}

impl TrainingReport {
    /// 是否有偏差
    #[must_use]
    pub fn has_deviation(&self) -> bool {
        self.rows.iter().any(|row| !row.is_match())
    }

    /// 偏差最大的绝对值（给宿主排序 / 阈值判断）
    #[must_use]
    pub fn max_abs_delta(&self) -> i32 {
        self.rows
            .iter()
            .flat_map(|row| row.delta.iter())
            .map(|delta| delta.abs())
            .max()
            .unwrap_or(0)
    }
}

/// 一条训练验算偏差
#[derive(Debug, Clone)]
pub struct TrainingDeviation {
    /// 训练位（0–4）
    pub train: usize,
    /// 是否稽古命令（5101–5105）
    pub keiko: bool,
    /// 帧内指令 id
    pub command_id: i32,
    /// 帧真值（`lower + upper`，7 项）
    pub frame: Vec<i32>,
    /// 还原态预览（7 项）
    pub preview: Vec<i32>,
    /// `preview - frame`（逐项）
    pub delta: Vec<i32>,
    /// 帧失败率
    pub frame_failure_rate: f32,
    /// 预览失败率
    pub preview_failure_rate: f32,
}

impl TrainingDeviation {
    /// 7 项是否逐项相等
    #[must_use]
    pub fn is_match(&self) -> bool {
        self.delta.iter().all(|delta| *delta == 0)
    }
}

/// 搜索摘要投到显示分口径：`E` 与档次都按投影值（终端与 JSON 共用同一份）
///
/// `E = E₀ + (MAX_TURN − turn) × bonus`；档次按投影后的 E 重查 `rank_names`。
/// σ 是确定性平移下的方差，保持不变；`Data` 未初始化时档次沿用原值。
#[must_use]
pub fn projected_summary(summary: &SearchSummary, turn: i32, bonus: i32) -> SearchSummary {
    let score = summary.expected_score + (MAX_TURN - turn) as f64 * f64::from(bonus);
    let rank = Data::get()
        .ok()
        .map(|data| data.consts.get_rank_name(score.round() as i32).to_string())
        .unwrap_or_else(|| summary.expected_rank.clone());
    SearchSummary {
        expected_score: score,
        expected_rank: rank,
        ..summary.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(score: f64, rank: &str) -> SearchSummary {
        SearchSummary {
            expected_score: score,
            expected_rank: rank.to_string(),
            baseline_stdev: 3000.0,
            ..SearchSummary::default()
        }
    }

    /// 投影只平移 E：随回合递减，σ / 基线 / 候选都不变
    #[test]
    fn projected_summary_shifts_score_only() {
        let early = projected_summary(&sample(82140.0, "LG9"), 0, 100);
        assert_eq!(early.expected_score, 89840.0);
        assert_eq!(early.baseline_stdev, 3000.0);
        assert_eq!(early.baseline, 0.0);
        assert!(early.candidates.is_empty());

        let late = projected_summary(&sample(82140.0, "LG9"), 76, 100);
        assert_eq!(late.expected_score, 82240.0);

        let raw = projected_summary(&sample(82140.0, "LG9"), 32, 0);
        assert_eq!(raw.expected_score, 82140.0);
    }

    /// 档次按投影后的 E 重查 rank_names（不再沿用官方口径档次）
    #[test]
    fn projected_summary_relabels_rank() {
        Data::init(matsuri_core::testing::data_dir()).expect("静态数据初始化");
        let data = Data::get().expect("Data 已初始化");
        let raw_rank = data.consts.get_rank_name(82140).to_string();
        let shifted_rank = data.consts.get_rank_name(89840).to_string();
        assert_ne!(raw_rank, shifted_rank, "两档分数应命中不同档次");

        let s = projected_summary(&sample(82140.0, &raw_rank), 0, 100);
        assert_eq!(s.expected_score, 89840.0);
        assert_eq!(s.expected_rank, shifted_rank);
    }
}
