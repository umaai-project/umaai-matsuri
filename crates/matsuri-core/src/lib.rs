//! # 骏大祭剧本（`matsuri` / scenarioId 15）
//!
//! 「朋、史解き ―午式駿大祭―」的剧本实现：五阶段机、`TurnPlan`、`MatsuriView`、动作全集、
//! 人头分布、训练两层公式、演目 gauge / Lv、教诲与融合、発表会 / 本番 / 合宿 / URA、事件链、
//! 手写训练员与诊断日志；`matsuri-sim` / `matsuri-ai` / `matsuri-tools` 为骨架，终端入口在 `matsuri-cli`。
//!
//! 纪律（详见引擎仓 `docs/scenario/`）：
//! - 阶段自持：内部强类型，对外只投影 `StageId`（号是契约，改号 = 旧种子失效）；
//! - 三条随机轴（局面 / 事件 / 策略）+ 决策流只给 `Trainer`，新回合按 `(master, 回合)` 一起派生；
//! - 数据与配置由宿主注入，本 crate 不写死路径；搜索分支只经 `fork_for_rollout` 建；
//! - 回合信息一律经 `MatsuriGame::view()` 取；诊断日志统一走 `diag!`。

pub mod config;
pub mod data;
pub mod deck_config;
pub mod game;
pub mod search;
pub mod testing;
pub mod trainer;
mod value;

pub use config::{MatsuriConfig, Qie};
pub use data::MatsuriData;
pub use deck_config::{AdvicePreset, DEFAULT_DECK, DeckParams, DeckTable, deck_comp};
pub use game::{
    ActLedger, Action, AdviceLedger, BoardView, CampState, CampView, CheckPointNeed,
    DISPLAY_HALF_FROM_STATUS, DeckError, DeckReport, DeckWarning, EVENT_LOG_CAPACITY, EventRecord,
    FriendView, MatsuriGame, MatsuriPerson, MatsuriView, MatsuriYear, MemberView, PersonOrigin,
    PersonView, ResourcePreview, ResourceView, SCENARIO_START_TURN, ScheduleView, SelfRace, Stage,
    TrainingBreakdown, TrainingLayers, TrainingPreview, TurnPlan, TurnView, URA_START_TURN,
    UmaView, build_game, restore_displayed_gain,
};
pub use search::{
    LeafEvaluator, MatsuriSearchTrainer, RolloutHealth, ScoreLeaf, SearchCandidate, SearchSummary,
};
pub use trainer::{ActionExplanation, MatsuriTrainer};

/// 诊断日志宏（引擎 `umaai_core::diag!` 的统一入口）
///
/// 本仓埋点统一写 `use crate::diag;` + `diag!(...)`：target = `diagnostic`，编译期 feature +
/// 运行时开关（`DiagGuard` 供搜索 rollout 静默）。日志器由宿主入口安装（`matsuri-cli` 消费
/// `output.log_level`）；未装日志器时记录被 `log` 门面静默丢弃。
pub use umaai_core::diag;

/// 育成最大回合（0 基，78 回合）；后续实现按自己的赛程改
pub const MAX_TURN: i32 = 77;
