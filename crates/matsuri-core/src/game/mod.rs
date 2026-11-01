//! 剧本局面（`MatsuriGame`）、资源账本、资源快照与装配
//!
//! 状态底座复用引擎 [`BaseGame`]（回合 / 马娘 / 卡组 / 继承 / 规则流），剧本只加自己的
//! 阶段机、人头、剧本资源与剧本资产。契约 impl 集中在 [`traits`]，回合结构与阶段机在
//! [`stage`]，本文件只放 inherent 方法（资源账本 / 建局 / 快照 / 训练）。

pub mod action;
pub mod deck;
pub mod person;
pub mod roster;
pub mod stage;
pub mod traits;

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use rand::Rng;
use serde::{Deserialize, Serialize};
use umaai::Config;
use umaai_core::gamedata::ActionValue;
use umaai_core::rng::StrategyRng;
use umaai_core::{Array5, Data};
use umasim::game::base::BaseGame;
use umasim::game::{Game, InheritInfo, TrainingValueSource};

pub use action::Action;
pub use deck::{DeckError, DeckReport, DeckWarning};
pub use person::{MatsuriPerson, PersonOrigin};
pub use stage::{CampState, Stage, TurnPlan};

use crate::data::{CheckPointResult, MatsuriData};

/// 演目资源账本（5 条演目的 gauge 与等级）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActLedger {
    /// 演出目 gauge（0..=剧本规则上限）
    pub gauge: Array5,
    /// 演目 Lv（1 级起）
    pub level: Array5,
}

impl Default for ActLedger {
    /// 开局账本：gauge 清零、演目 Lv 从 1 起（Lv1 即有效果档）
    fn default() -> Self {
        Self {
            gauge: [0; 5],
            level: [1; 5],
        }
    }
}

/// 教诲槽位账本（永远 5 槽 = 5 个演目）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdviceLedger {
    /// 每个演目槽位绑定的基础教诲 id（101–501；0 = 未分配）
    pub slots: Array5,
    /// 每个演目槽位叠加的転成教诲 id（601–1001；0 = 未転成）
    pub fused: Array5,
}

/// 剧本资源快照（给前端的纯数据 View）
///
/// 形态 A 五条纪律（见引擎仓 `docs/scenario/contracts.md`）：**单一取数口** / 只放引擎不知道的字段 /
/// 快照语义（幂等、无副作用）/ 可演进（`non_exhaustive`）/ **不进契约 trait**。
/// S3 覆盖剧本自持资源全集：演目 gauge / Lv / 教诲槽位 / 合宿状态 / 発表会结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MatsuriView {
    /// 5 条演目 gauge（索引 = 演目 / 训练位下标）
    pub act_gauge: Array5,
    /// 5 个演目 Lv（1 级起）
    pub act_level: Array5,
    /// 演目 Lv 上限（SSR 吾妻 → 10）
    pub act_level_max: i32,
    /// 教诲槽位：每个演目绑定的基础教诲 id（0 = 未分配）
    pub advice_slots: Array5,
    /// 転成：每个演目槽位叠加的教诲 id（0 = 未転成）
    pub advice_fused: Array5,
    /// 本回合是否强制比赛（目标赛 / URA 决赛；被强制休息覆盖时分类仍为 true）
    pub race: bool,
    /// 本回合是否本番（第 3 次発表会）
    pub finals: bool,
    /// 本回合進捗発表会 / 本番场次（0–2；非结算回合为 `None`）
    pub check_point: Option<usize>,
    /// 本回合是否教诲可调整窗口（在 `End` 阶段、剧本结算之后处理）
    pub advice_window: bool,
    /// 本回合是否在合宿区（含被比赛 / 强制休息占用的回合）
    pub camp_active: bool,
    /// 本回合训练是否全变演目稽古（合宿排练回合）
    pub camp_training: bool,
    /// 本回合合宿年次（0–2；不在合宿为 `None`）
    pub camp_year: Option<usize>,
    /// 本回合是否延长期合宿
    pub camp_extended: bool,
    /// 本年度已计入的合宿回合数（比赛 / 强制休息不计）
    pub camp_counted: usize,
    /// 本回合是否因马娘特殊事件改为强制休息（目标赛回合仍按赛程分类）
    pub forced_rest: bool,
    /// 進捗発表会 / 本番结果（3 场）
    pub check_points: [CheckPointResult; 3],
    /// 本番演出是否已结算（S5 落地）
    pub finals_done: bool,
    /// 是否 URA 收尾期（本番后 turn 68–77）
    pub ura_period: bool,
    /// 编成警告项（建局时一次算定；None 段位表示编成合规）
    pub deck_warnings: Vec<DeckWarning>,
}

/// 剧本局面
///
/// 状态底座复用引擎的 [`BaseGame`]（回合 / 马娘 / 卡组 / 继承 / 规则流），剧本只加自己的
/// 阶段机、人头、剧本资源与剧本资产。
#[derive(Debug, Clone, PartialEq)]
pub struct MatsuriGame {
    /// 引擎状态底座（经 `Deref` 取用；底座方法名不得与契约方法同名，见 AGENTS.md）
    pub base: BaseGame,
    /// 人头（剧本人头：引擎通用字段 + 担当 / 来源 / 可用回合）
    pub persons: Vec<MatsuriPerson>,
    /// 剧本自持阶段机
    pub stage: Stage,
    /// 剧本自持策略流（动作结算与剧本机制随机；从规则主种子派生）
    pub strategy: Option<StrategyRng>,
    /// 剧本自持资产（宿主载入后注入；训练表等数值都取自它）
    pub data: Arc<MatsuriData>,
    /// 演目资源账本（gauge / Lv）
    pub acts: ActLedger,
    /// 教诲槽位账本（5 槽 / 転成 / 待分配）
    pub advice: AdviceLedger,
    /// 合宿进度（回合边界状态）
    pub camp: CampState,
    /// 本回合结构（`Begin` 阶段派生；S4/S5/S6 的分支来源）
    pub plan: TurnPlan,
    /// 本回合是否被强制休息事件占用（S6 事件管线设置；合宿计数排除用）
    pub forced_rest: bool,
    /// 進捗発表会 / 本番结果（3 场；`End` 阶段结算，S5 落地）
    pub check_points: [CheckPointResult; 3],
    /// 本番演出是否已结算（S5 落地）
    pub finals_done: bool,
    /// 编成警告项（建局时由 `game/deck.rs` 一次算定；禁止项已拒绝建局）
    pub deck_warnings: Vec<DeckWarning>,
}

impl Deref for MatsuriGame {
    type Target = BaseGame;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for MatsuriGame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl MatsuriGame {
    /// 建一局（要求宿主已初始化静态数据：`umaai_core::Data::init`）
    ///
    /// `data` 是剧本自持资产（宿主载入后注入）：训练基础值表等数值都取自它。
    pub fn newgame(
        uma_id: u32,
        deck_ids: &[u32],
        inherit: InheritInfo,
        race_grades: &[i32],
        data: Arc<MatsuriData>,
    ) -> Result<Self> {
        let mut game = Self {
            base: BaseGame::new(
                uma_id,
                deck_ids,
                inherit,
                Data::get()?.consts.five_status_limit_base,
                race_grades,
            )?,
            persons: Vec::new(),
            stage: Stage::Begin,
            strategy: None,
            data,
            acts: ActLedger::default(),
            advice: AdviceLedger::default(),
            camp: CampState::default(),
            plan: TurnPlan::default(),
            forced_rest: false,
            check_points: [CheckPointResult::NotReached; 3],
            finals_done: false,
            deck_warnings: Vec::new(),
        };
        game.init_persons()?;
        Ok(game)
    }

    /// 追加一个人头（按当前人数分配人头序号）
    pub fn add_person(&mut self, mut person: MatsuriPerson) {
        person.set_person_index(self.persons.len() as i32);
        self.persons.push(person);
    }

    /// 取剧本资源快照（**单一取数口**；何时取由宿主决定）
    #[must_use]
    pub fn view(&self) -> MatsuriView {
        MatsuriView {
            act_gauge: self.acts.gauge,
            act_level: self.acts.level,
            act_level_max: self.act_level_cap(),
            advice_slots: self.advice.slots,
            advice_fused: self.advice.fused,
            race: self.plan.race,
            finals: self.plan.finals(),
            check_point: self.plan.check_point,
            advice_window: self.plan.advice_window.is_some(),
            camp_active: self.plan.camp_zone,
            camp_training: self.plan.camp_train(self.forced_rest),
            camp_year: self.plan.camp_year,
            camp_extended: self.plan.camp_extended,
            camp_counted: self.camp.counted,
            forced_rest: self.forced_rest,
            check_points: self.check_points,
            finals_done: self.finals_done,
            ura_period: self.plan.ura_period,
            deck_warnings: self.deck_warnings.clone(),
        }
    }

    /// 演目 Lv 上限（仅支持 SSR 吾妻 → 带友人档 10）
    #[must_use]
    pub fn act_level_cap(&self) -> i32 {
        self.data.rules.act_level_max_with_friend
    }

    /// 训练结算（S2 版：按真实普通训练表入账 + 设施计数；失败判定 / 羁绊 / hint 留给后续阶段）
    pub(crate) fn do_train(&mut self, train: usize, _rng: &mut impl Rng) -> Result<()> {
        let level = self
            .train_level(train)
            .clamp(1, self.max_normal_train_level())
            - 1;
        let row = self
            .training_basic_table()
            .get(train)
            .and_then(|levels| levels.get(level))
            .copied()
            .ok_or_else(|| anyhow!("普通训练表缺行: train={train}, level={level}"))?;
        let value = ActionValue {
            status_pt: [row[0], row[1], row[2], row[3], row[4], row[5]],
            vital: row[6],
            ..Default::default()
        };
        self.uma_mut().add_value(&value);
        self.base.train_level_count[train] += 1;
        Ok(())
    }

    /// 普通训练等级上限（剧本资产规则；`train_level` 的夹取来源）
    pub(crate) fn max_normal_train_level(&self) -> usize {
        self.data.rules.normal_train_level_max.max(1) as usize
    }
}

/// 从引擎配置与剧本资产装配一局剧本局面（终端样例外壳与剧本侧测试共用）
///
/// 数据根与配置由宿主注入：本函数只读宿主给的 [`Config`] 与 [`MatsuriData`]，不碰任何路径。
pub fn build_game(cfg: &Config, data: Arc<MatsuriData>) -> Result<MatsuriGame> {
    MatsuriGame::newgame(
        cfg.sim.uma,
        &cfg.sim.cards,
        cfg.inherit(),
        cfg.race_grades()?,
        data,
    )
}
