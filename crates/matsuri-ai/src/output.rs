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

use matsuri_core::{Action, MatsuriGame, MatsuriView};
use serde_json::{Value, json};
use umaai::DecisionRecord;

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
