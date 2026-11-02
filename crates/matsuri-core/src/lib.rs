//! # 骏大祭剧本（`matsuri` / scenarioId 15）
//!
//! 「朋、史解き ―午式駿大祭―」的剧本 workspace 骨架，给出「引擎契约怎么接」的完整清单；
//! 当前为 S8 View 补全 + diag 诊断态：五阶段机（Begin / Distribute / Train / AfterTrain / End）、
//! 回合结构 `TurnPlan`、回合信息快照 `MatsuriView`（分节）、动作全集（训练 / 演目稽古 / 比赛 /
//! 休息 / 出行 / 治病）、人头分布、训练两层公式、演目 gauge / Lv / 稽古两档、教诲、発表会 / 本番 /
//! 合宿 +1 / URA / 终局奖励、事件链（固定 / 随机分布 / 合宿抑制 / 友人链 / 追加稽古事件化 / 变胖）、
//! 手写训练员（S7）、事件记录 / 训练预览 / 诊断日志（`diag!`）已落地。
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
//! - **回合信息取数**：宿主 / 面板 / 前端一律经 [`MatsuriGame::view`]（[`MatsuriView`]）取状态，
//!   不直读局面字段；诊断日志走 [`diag!`]（引擎宏，预览 / rollout 自动静默）。

pub mod config;
pub mod data;
pub mod game;
pub mod testing;
pub mod trainer;
mod value;

pub use config::MatsuriConfig;
pub use data::MatsuriData;
pub use game::{
    ActLedger, Action, AdviceLedger, BoardView, CampState, CampView, CheckPointNeed, DeckError,
    DeckReport, DeckWarning, EVENT_LOG_CAPACITY, EventRecord, FriendView, MatsuriGame,
    MatsuriPerson, MatsuriView, MatsuriYear, MemberView, PersonOrigin, PersonView, ResourcePreview,
    ResourceView, SCENARIO_START_TURN, ScheduleView, SelfRace, Stage, TrainingPreview, TurnPlan,
    TurnView, URA_START_TURN, UmaView, build_game,
};
pub use trainer::MatsuriTrainer;

/// 诊断日志宏（引擎 `umaai_core::diag!` 的统一入口）
///
/// 本仓埋点统一写 `use crate::diag;` + `diag!(...)`：target = `diagnostic`，编译期 feature +
/// 运行时开关（`DiagGuard` 供搜索 rollout 静默）。日志器由宿主入口安装（`matsuri-cli` 消费
/// `output.log_level`）；未装日志器时记录被 `log` 门面静默丢弃。
pub use umaai_core::diag;

/// 育成最大回合（0 基，78 回合）；后续实现按自己的赛程改
pub const MAX_TURN: i32 = 77;
