//! # 剧本在线决策核心（`matsuri-ai`）
//!
//! 骏大祭（`scenarioId = 15`）的在线档：真机帧 → 局面还原 → 一帧一决策。
//!
//! ```text
//! FrameSource::next_frame            → RawFrame
//!   → frame::ParsedFrame::parse      → BaseStatus（引擎通用段）+ FestaState（剧本段）
//!   → BaseStatus::to_base_game       → BaseGame
//!   → MatsuriGame::restore           → 可继续决策的局面（Train / AfterTrain）
//!   → set_rule_master → 路由 → 决策   → OnlineTurnOutput
//! ```
//!
//! ## 分层纪律
//!
//! 本 crate **只出 lib**：不打印、不解析命令行参数、不落盘。终端面板 / JSON / 参数全在
//! `matsuri-cli`；项目里没有第二份「在线编排」。
//!
//! ## 为什么不接 `umaai::Session`
//!
//! `Session::step_turn` 的 `build` 只拿得到 `BaseGame`，路由是 `source == "event" ? 事件 : 训练`
//! 的二分，覆盖不了骏大祭的 `special` 中间态、同回合重复帧与「等级本帧无值」。
//! 所以只复用引擎的 pub 零件（帧通道 / 通用段解析 / 终局帧 DTO / 评分），编排在剧本侧。
//!
//! ## 模块
//!
//! - [`frame`]：帧信封解析（剧本段 + `scenarioId` 校验 + 训练预览真值）
//! - [`final_score`]：终局帧解析与剧本门禁
//! - [`urafile`]：帧来源（真机目录 / 离线回放）
//! - [`session`]：装配与「一帧一决策」主循环
//! - [`online`]：路由、决策记录构造、`--verify-training` 验算
//! - [`output`]：输出 DTO 与 JSON 行（10 字段，沿用旧 schema）

pub mod final_score;
pub mod frame;
pub mod online;
pub mod output;
pub mod session;
pub mod urafile;

pub use final_score::parse_final_score;
pub use frame::{MATSURI_SCENARIO_ID, ParsedFrame, TrainingFrame};
pub use online::Route;
pub use output::{
    FinalTurnOutput, OnlineTurnOutput, TrainingDeviation, TrainingReport, TurnKind,
    connected_payload, error_payload,
};
pub use session::{DeckProfile, FRIEND_FIRST_EVENT, OnlineSession, TrainerKind};
pub use urafile::{FrameSource, ROOT_ENV, ReplaySource, resolve_live_root};
