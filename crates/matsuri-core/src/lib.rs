//! # 骏大祭剧本（`matsuri` / scenarioId 15）
//!
//! 「朋、史解き ―午式駿大祭―」的剧本 workspace 骨架，给出「引擎契约怎么接」的完整清单；
//! 当前为 S3 回合结构态：五阶段机（Begin / Distribute / Train / AfterTrain / End）、回合结构
//! `TurnPlan`（强制比赛 / 合宿窗口与延长 / 発表会 / 本番 / URA 收尾）与资源快照 `MatsuriView`
//! 已落地；演目 / gauge / 教え / 合宿结算等机制在 S4+ 落地。
//!
//! ## 结构（与 `umaai-onsen` 同构）
//!
//! - `matsuri-core`：局面 / 动作 / 数据 / 配置 / 训练员 / 装配（宿主稳定面）；
//! - `matsuri-sim`：模拟器与基线裁判（骨架）；
//! - `matsuri-ai`：在线决策核心（骨架）；
//! - `matsuri-cli`：终端跑局外壳（thin bin）；
//! - `matsuri-tools`：测卡 / 复盘等外围工具（骨架）。
//!
//! ## 纪律（详见引擎仓 `docs/scenario/README.md` 与 `docs/scenario/contracts.md`）
//!
//! - **阶段自持**：内部用强类型枚举，对外只投影 [`StageId`]（显式赋号，改号等于旧种子失效）。
//! - **三条随机轴**：局面流 / 事件流从 [`BaseGame`] 取出（`take_rule_streams`），策略流本局面
//!   自持，**决策流只给 `Trainer`**；每进入新回合三条轴一起按 `(master, 回合)` 重新派生。
//! - **注入点**：`set_rule_master` 装主种子并派生各轴，未注入就推进会报错（不静默回退）。
//! - **数据与配置由宿主注入**：本 crate 不写死路径；引擎静态表走 `umaai_core::Data`，
//!   剧本资产与配置由 [`MatsuriData`] / [`MatsuriConfig`] 载入。
//! - **搜索接入**：实现 [`FlatSearchGame`]，分支只能经 `fork_for_rollout` 建（克隆 + 注入 rollout 种子）。

pub mod config;
pub mod data;
pub mod game;
pub mod testing;
pub mod trainer;

pub use config::MatsuriConfig;
pub use data::MatsuriData;
pub use game::{
    ActLedger, Action, AdviceLedger, CampState, DeckError, DeckReport, DeckWarning, MatsuriGame,
    MatsuriPerson, MatsuriView, PersonOrigin, Stage, TurnPlan, build_game,
};
pub use trainer::MatsuriTrainer;

/// 育成最大回合（0 基，78 回合）；后续实现按自己的赛程改
pub const MAX_TURN: i32 = 77;
