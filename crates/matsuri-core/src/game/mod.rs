//! 剧本局面（`MatsuriGame`）、资源账本与装配
//!
//! 状态底座复用引擎 [`BaseGame`]（回合 / 马娘 / 卡组 / 继承 / 规则流），剧本只加自己的
//! 阶段机、人头、剧本资源与剧本资产。契约 impl 集中在 [`traits`]，回合结构与阶段机在
//! [`stage`]，回合信息快照（`MatsuriView`）在 `view.rs`，本文件只放 inherent 方法
//! （资源账本 / 建局 / 训练 / 预览）。

pub mod action;
pub mod advice;
pub mod deck;
mod diag;
pub mod distribute;
pub mod event;
pub mod person;
pub mod roster;
pub mod settle;
pub mod stage;
pub mod train;
pub mod traits;
mod view;

use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use umaai::Config;
use umaai_core::gamedata::{ActionValue, EventData};
use umaai_core::rng::StrategyRng;
use umaai_core::{Array5, Data};
use umasim::game::base::BaseGame;
use umasim::game::{Game, InheritInfo, TrainingBasicTable};

pub use action::Action;
pub use deck::{DeckError, DeckReport, DeckWarning};
pub use person::{MatsuriPerson, PersonOrigin};
pub use stage::{CampState, Stage, TurnPlan, URA_START_TURN};
pub use view::{
    BoardView, CampView, CheckPointNeed, EVENT_LOG_CAPACITY, EventRecord, FriendView, MatsuriView,
    MatsuriYear, MemberView, PersonView, ResourceView, ScheduleView, SelfRace, TurnView, UmaView,
};

use crate::data::{ActLevelEffect, CheckPointResult, MatsuriData};

/// 剧本机制启动回合（0 基）：神驹加入 / 首次传授在 turn 2
///
/// 0–1 回合剧本系统还没开始：没有担当分配，训练 / 休息 / 出行 / 事件也都不获取演目槽等剧本资源。
pub const SCENARIO_START_TURN: i32 = 2;

/// 「铁心的挑战者」：生野狄杜斯系马娘（106301 / 106302）的生涯赛比赛加成
///
/// 状态 `31` 目前未建模，先按马娘 ID + 回合窗硬编码：**出道战后（turn 12–71）对生涯比赛**
/// 五维 ×2.5、PT ×0.46；五维与卡面赛后**乘算**、在本番 / 発表会剧本赛后**乘算之前**，
/// PT 折扣由引擎 `pt_rate` 单独缩放（真机口径：属性多、PT 相对少）。
const IRON_HEART_UMA_IDS: [u32; 2] = [106301, 106302];
/// 「铁心的挑战者」五维加成 %（+150 = ×2.5）
const IRON_HEART_STATS_BONUS: i32 = 150;
/// 「铁心的挑战者」PT 倍率 %（46 = ×0.46）
const IRON_HEART_PT_RATE: i32 = 46;
/// 出道战后的首个回合（0 基）
const IRON_HEART_FROM_TURN: i32 = 12;
/// 生涯赛窗口末回合（0 基；其后为 URA）
const IRON_HEART_END_TURN: i32 = 71;

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
    /// 每个演目槽位绑定的**基础教诲** id（101–501；0 = 未分配）。
    /// 転成后仍保留原基础 id，供 `fusion` 升级效果查表。
    pub slots: Array5,
    /// 每个演目槽位叠加的**転成教诲** id（601–1001；0 = 未転成）
    pub fused: Array5,
    /// 本局是否已获得教诲（turn 2 的 `Begin` 首次授予；此前槽位不可用）
    pub acquired: bool,
}

/// 训练 / 演目稽古预览（手动核对与前端面板）
///
/// 成功收益 = 下层（引擎默认公式）+ 上层（剧本来源），已含变胖速度归零 / 本番演出等剧本修正；
/// 失败率 % = 基础值 × 卡面失败率下降 × 剧本失败率下降 + 不擅长偏差（-2 / +2）。
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingPreview {
    /// 成功时的到账值
    pub value: ActionValue,
    /// 失败率（%）
    pub failure_rate: f32,
    /// 是否为演目稽古上下文
    pub keiko: bool,
}

/// 动作的剧本资源预览（演目槽 / 演目 Lv 差值；正 = 增加，负 = 排练成功清槽）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourcePreview {
    /// 各演目槽增量（索引 = 演目 / 训练位下标）
    pub gauge_delta: Array5,
    /// 各演目 Lv 增量
    pub level_delta: Array5,
}

/// 稽古结算上下文
///
/// 预览需要在共享局面（&self）上临时切换该标记，而搜索并行要求局面为 Sync：
/// 因此用原子量而不是可变的 bool。单次决策内只有本线程读写，预览结束由 guard 恢复。
#[derive(Debug, Default)]
struct KeikoSettling(AtomicBool);

impl KeikoSettling {
    /// 当前是否为稽古口径
    fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// 直接设置（正式结算入口用）
    fn set(&self, value: bool) {
        self.0.store(value, Ordering::Relaxed);
    }

    /// 设置并返回旧值（预览临时切换用）
    fn replace(&self, value: bool) -> bool {
        self.0.swap(value, Ordering::Relaxed)
    }
}

/// 稽古上下文恢复 guard（RAII）：正常返回 / 错误 / panic 都恢复进入前的值
struct KeikoGuard<'a> {
    /// 被切换的标记
    flag: &'a KeikoSettling,
    /// 进入前的值
    previous: bool,
}

impl Drop for KeikoGuard<'_> {
    fn drop(&mut self) {
        self.flag.set(self.previous);
    }
}

impl KeikoSettling {
    /// 临时切换并返回恢复 guard
    fn scoped(&self, value: bool) -> KeikoGuard<'_> {
        KeikoGuard {
            flag: self,
            previous: self.replace(value),
        }
    }
}

impl Clone for KeikoSettling {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.get()))
    }
}

impl PartialEq for KeikoSettling {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
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
    /// 演目稽古基础表（Junior 档；`training.keiko` 逐训练位一行，供引擎下层公式复用）
    keiko_table: TrainingBasicTable,
    /// 演目稽古基础表（Classic 及以后档；turn 24 起替换 Junior 档）
    keiko_classic_table: TrainingBasicTable,
    /// 稽古结算上下文：为真时 `training_basic_table()` 返回（按时期挑的）稽古表
    keiko_settling: KeikoSettling,
    /// 演目资源账本（gauge / Lv）
    pub acts: ActLedger,
    /// 教诲槽位账本（5 槽 / 転成 / 待分配）
    pub advice: AdviceLedger,
    /// 合宿进度（回合边界状态）
    pub camp: CampState,
    /// 本回合结构（`Begin` 阶段派生；回合类型的分支来源）
    pub plan: TurnPlan,
    /// 本回合是否被强制休息事件占用（事件管线设置；合宿计数排除用）
    pub forced_rest: bool,
    /// 育成失败标记（自选比赛区间未达标 → 终止；终局跳过奖励）
    pub career_failed: bool,
    /// 友人出行解锁时获得的「疲劳回复」flag（剧本自持；体力 ≤49 时一次 +15）
    pub fatigue_recovery: bool,
    /// 進捗発表会 / 本番结果（3 场；`End` 阶段结算）
    pub check_points: [CheckPointResult; 3],
    /// 本番演出是否已结算
    pub finals_done: bool,
    /// 编成警告项（建局时由 `game/deck.rs` 一次算定；禁止项已拒绝建局）
    pub deck_warnings: Vec<DeckWarning>,
    /// 最近事件记录（手动 / 前端核对用；事件结算后追加，容量上限 [`EVENT_LOG_CAPACITY`]）
    pub event_log: Vec<EventRecord>,
    /// 是否记录事件明细（搜索 rollout 置 `false`：局面跑完即弃，明细无人消费）
    pub record_events: bool,
    /// 当前 stage 待结算事件队列（事件搜索从下一事件继续用；正常推进逐项消费）
    pub event_queue: Vec<EventData>,
    /// 当前事件队列游标（下标 = 正在决策的事件）
    pub event_cursor: usize,
    /// 助力支撑剩余次数（0 = 无；演目稽古获得「助力の支え」时重置为规则上限）
    pub assist_support_left: i32,
    /// 本回合研鑽「追加の稽古」是否掷中（`Begin` 用策略流预掷；本回合未点对应训练则作废）
    pub kensan_pending: bool,
    /// 発表会大成功「全训练 Lv+1」累计（独立于设施计数，不参与「每 N 次 +1」）
    pub train_level_bonus: Array5,
    /// 剧本专属配置（融合预设等；默认层 = 仓内 `matsuri_config.toml`）
    pub config: crate::MatsuriConfig,
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
        let keiko_table = build_keiko_table(&data.training.keiko.junior);
        let keiko_classic_table = build_keiko_table(&data.training.keiko.classic);
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
            keiko_table,
            keiko_classic_table,
            keiko_settling: KeikoSettling::default(),
            acts: ActLedger::default(),
            advice: AdviceLedger::default(),
            camp: CampState::default(),
            plan: TurnPlan::default(),
            forced_rest: false,
            career_failed: false,
            fatigue_recovery: false,
            check_points: [CheckPointResult::NotReached; 3],
            finals_done: false,
            deck_warnings: Vec::new(),
            event_log: Vec::new(),
            record_events: true,
            event_queue: Vec::new(),
            event_cursor: 0,
            assist_support_left: 0,
            kensan_pending: false,
            train_level_bonus: [0; 5],
            config: crate::MatsuriConfig::default(),
        };
        game.init_persons()?;
        Ok(game)
    }

    /// 追加一个人头（按当前人数分配人头序号）
    pub fn add_person(&mut self, mut person: MatsuriPerson) {
        person.set_person_index(self.persons.len() as i32);
        self.persons.push(person);
    }

    /// 注入剧本配置（宿主载入 `matsuri_config.toml` 后调用；未注入时用默认层）
    ///
    /// 育成前状态（`[pre]`）也在这里落到局面上：小切是**局外**状态，须在首回合前注入；
    /// 局内获得切者时由引擎互斥清掉（`gain_qiezhe`），此处不重复判定。
    #[must_use]
    pub fn with_config(mut self, config: crate::MatsuriConfig) -> Self {
        self.config = config;
        if self.config.pre.xiaoqie && !self.base.uma.flags.qiezhe {
            self.base.uma.flags.xiaoqie = true;
        }
        if self.config.pre.positive_thinking > 0 {
            self.base.uma.flags.positive_thinking_count = self.config.pre.positive_thinking;
        }
        self
    }

    /// 演目 Lv 上限（仅支持 SSR 吾妻 → 带友人档 10）
    #[must_use]
    pub fn act_level_cap(&self) -> i32 {
        self.data.rules.act_level_max_with_friend
    }

    /// 当前回合所在「自选比赛义务区间」的达标进度：`(剩余需打场数, 区间内剩余可赛回合数)`
    ///
    /// 返回 `None`：不在任何 `freeRaces` 区间 / 该区间已达标 / 该回合被区间等级门槛挡掉
    /// （载入时按比赛等级表算好的掩码未置位）。
    ///
    /// 口径：**区间最后一回合的 `End` 判定**场数（引擎默认写在
    /// 下一回合开始检查，本仓前移至 End）；未达标 = 育成失败；达标 = 触发一次
    /// 「生涯比赛胜利」奖励事件（吃比赛与剧本赛后加成）。等级门槛只放行**不低于**门槛的比赛。
    /// `left` 统计掩码内 ≥ 当前回合的回合数；未来被强制休息等占用不预扣（保守）。
    pub(crate) fn free_race_progress(&self) -> Option<(u32, u32)> {
        let free = self
            .base
            .uma
            .free_races
            .iter()
            .find(|free| free.mask.contains_turn(self.turn))?;
        let need = free
            .count
            .saturating_sub(self.base.uma.count_free_race(free));
        if need == 0 {
            return None;
        }
        let left = (self.turn..=free.end_turn as i32)
            .filter(|turn| free.mask.contains_turn(*turn))
            .count() as u32;
        Some((need, left))
    }

    /// 普通训练等级上限（剧本资产规则；`train_level` 的夹取来源）
    pub(crate) fn max_normal_train_level(&self) -> usize {
        self.data.rules.normal_train_level_max.max(1) as usize
    }

    /// 友人（剧本友人 = 吾妻）的人头下标；没带友人 / 人头未建时为 `None`
    pub(crate) fn friend_person_index(&self) -> Option<usize> {
        self.base
            .friend
            .card_id
            .and_then(|card_id| self.person_index_of_card(card_id))
    }

    /// 発表会 / 本番的永久 buff（取最新已结算场次的累计行；`schedule 0` 为开局基线）
    pub(crate) fn presentation_bonus(&self) -> PresentationBonus {
        let mut bonus = PresentationBonus::default();
        for row in &self.data.check_points {
            if row.schedule == 0 {
                bonus = PresentationBonus::from_row(row);
                continue;
            }
            let index = row.schedule as usize - 1;
            if self.check_points.get(index).copied() == Some(row.result) {
                bonus = PresentationBonus::from_row(row);
            }
        }
        bonus
    }

    /// 剧本侧比赛加成 %（発表会 / 本番累计值；与支援卡赛后**分开计算**后乘算）
    pub(crate) fn scenario_race_bonus(&self) -> i32 {
        self.presentation_bonus().race_bonus
    }

    /// 比赛奖励实际系数（五维）：**卡面赛后 × 铁心五维倍率 × 剧本赛后**
    ///
    /// 五维与 PT 的倍率不同（铁心 PT ×0.46，由 [`Self::race_pt_rate`] 交给引擎单独缩放）；
    /// 取整（floor）由引擎 [`umasim::game::base::BaseGame::push_race_event`] 在面板乘完后统一做一次。
    pub(crate) fn race_scale(&self) -> f64 {
        let card = (100 + self.base.uma.race_bonus) as f64 / 100.0;
        let stats = (100 + self.special_race_stats_bonus()) as f64 / 100.0;
        let scenario = (100 + self.scenario_race_bonus()) as f64 / 100.0;
        card * stats * scenario
    }

    /// PT 缩放（引擎 `pt_rate`）：铁心生效时 ×0.46，否则 1.0
    pub(crate) fn race_pt_rate(&self) -> f64 {
        if self.iron_heart_active() {
            f64::from(IRON_HEART_PT_RATE) / 100.0
        } else {
            1.0
        }
    }

    /// 特殊状态「铁心的挑战者」五维加成 %（生野狄杜斯 106301 / 106302）
    pub(crate) fn special_race_stats_bonus(&self) -> i32 {
        if self.iron_heart_active() {
            IRON_HEART_STATS_BONUS
        } else {
            0
        }
    }

    /// 状态生效条件：指定马娘 + 出道战后 turn 12–71 + 生涯比赛
    fn iron_heart_active(&self) -> bool {
        IRON_HEART_UMA_IDS.contains(&self.base.uma.uma_id)
            && (IRON_HEART_FROM_TURN..=IRON_HEART_END_TURN).contains(&self.turn())
            && self.plan.race
    }

    /// 判定一场発表会 / 本番结果：`need_act_num` 个演目达到 `need_act_level` → 大成功，否则成功
    pub(crate) fn judge_presentation(&self, index: usize) -> CheckPointResult {
        let Some(schedule) = self.data.schedule.get(index) else {
            return CheckPointResult::NotReached;
        };
        let reached = self
            .acts
            .level
            .iter()
            .filter(|level| **level >= schedule.need_act_level)
            .count() as i32;
        if reached >= schedule.need_act_num {
            CheckPointResult::GreatSuccess
        } else {
            CheckPointResult::Success
        }
    }

    /// 本回合演目稽古是否已切 Classic 档（turn 23 的「演目排练强化」在 `End` 结算，故 turn ≥ 24）
    pub(crate) fn keiko_is_classic(&self) -> bool {
        self.turn >= 24
    }

    /// 本回合使用的演目稽古基础表（按时期两档）
    pub(crate) fn keiko_basic_table(&self) -> &TrainingBasicTable {
        if self.keiko_is_classic() {
            &self.keiko_classic_table
        } else {
            &self.keiko_table
        }
    }

    /// 演目下标 → 训练位下标
    pub(crate) fn train_of_act(&self, act: usize) -> usize {
        self.data.acts[act].train
    }

    /// 训练位下标 → 演目下标（5 演目与 5 训练位一一对应，防御性反查）
    pub(crate) fn act_of_train(&self, train: usize) -> usize {
        self.data
            .acts
            .iter()
            .position(|act| act.train == train)
            .unwrap_or(train)
    }

    /// 演目槽上限
    pub(crate) fn act_gauge_max(&self) -> i32 {
        self.data.rules.act_gauge_max
    }

    /// 剧本机制是否已启动（turn ≥ [`SCENARIO_START_TURN`]）
    ///
    /// 0–1 回合剧本系统还没开始：没有担当分配，训练 / 休息 / 出行 / 事件也都不获取演目槽等剧本资源。
    #[must_use]
    pub fn scenario_started(&self) -> bool {
        self.base.turn >= SCENARIO_START_TURN
    }

    /// 当前「亮起」（演目槽满）的演目集合
    pub(crate) fn lit_acts(&self) -> [bool; 5] {
        let max = self.act_gauge_max();
        let mut lit = [false; 5];
        for (act, gauge) in self.acts.gauge.iter().enumerate() {
            lit[act] = *gauge >= max;
        }
        lit
    }

    /// 本回合按排练处理的演目集合（合宿排练回合 → 全部；否则亮起集合）
    pub(crate) fn practice_acts(&self) -> [bool; 5] {
        if self.all_keiko_round() {
            [true; 5]
        } else {
            self.lit_acts()
        }
    }

    /// 本回合是否「全部训练变演目稽古」：合宿排练回合 / **URA 回合（72–77，剧本结束后的决赛期）**
    ///
    /// URA 期与合宿一样 5 个训练位全按稽古处理（锁担当 / 全灵感 / 编成卡 +1 人等按排练口径）。
    pub(crate) fn all_keiko_round(&self) -> bool {
        self.plan.camp_train(self.forced_rest) || self.plan.ura_period
    }

    /// 给单个演目加槽（满槽吃掉溢出、不叠加），返回实际增加量
    ///
    /// 0–1 回合（剧本机制未开始）不获取演目槽。生产路径走整账本助手，
    /// 单槽入口仅测试使用。
    #[cfg(test)]
    pub(crate) fn gain_act_gauge(&mut self, act: usize, amount: i32) -> i32 {
        let started = self.scenario_started();
        let max = self.act_gauge_max();
        act_gauge_add(&mut self.acts, started, max, act, amount)
    }

    /// 三则落点逐次 +1 槽（`count` 次；满槽跳过、并列逐次分摊），返回各演目实际增加量
    ///
    /// 0–1 回合（剧本机制未开始）不获取演目槽。
    pub(crate) fn gain_lowest_act_gauge(&mut self, count: i32) -> [i32; 5] {
        let started = self.scenario_started();
        let max = self.act_gauge_max();
        act_gauge_add_lowest(&mut self.acts, started, max, count)
    }

    /// 按三则落点（演目 Lv 最低 → 槽最低 → 速/耐/力/毅/智顺序）挑一个演目下标
    pub(crate) fn lowest_act(&self) -> usize {
        lowest_act(&self.acts.level, &self.acts.gauge)
    }

    /// 给三则落点选出的演目 +`amount` Lv（按演目 Lv 上限夹取），返回加到的演目下标
    pub(crate) fn add_lowest_act_level(&mut self, amount: i32) -> usize {
        let act = self.lowest_act();
        self.add_act_level(act, amount);
        act
    }

    /// 给指定演目 +Lv（按演目 Lv 上限夹取）
    pub(crate) fn add_act_level(&mut self, act: usize, amount: i32) {
        let cap = self.act_level_cap();
        act_level_add(&mut self.acts, cap, act, amount)
    }

    /// 演目 Lv 效果行（Lv 夹取到 1–10）
    pub(crate) fn act_effect(&self, act: usize) -> &ActLevelEffect {
        let level = self.acts.level[act].clamp(1, 10) as usize;
        &self.data.act_effects[level - 1]
    }

    /// 编成支援卡类型数 ≥5（含友人卡；演目 Lv 的「5 种卡」条件）
    pub(crate) fn deck_has_five_types(&self) -> bool {
        let mut types = [false; 7];
        for card in &self.base.deck {
            if (0..7).contains(&card.card_type) {
                types[card.card_type as usize] = true;
            }
        }
        types.iter().filter(|present| **present).count() >= 5
    }

    /// 本人头的剧本得意率加成（发布会 + 担当演目的教え / 転成；不含卡面）
    pub(crate) fn scenario_deyilv(&self, person_index: usize) -> i32 {
        let act = match self.persons.get(person_index).and_then(|person| person.act) {
            Some(act) => act,
            None => return self.presentation_bonus().deyilv,
        };
        self.presentation_bonus().deyilv + self.advice_slot_effect(act).deyilv
    }

    /// 本回合是否允许出行（第 2/3 年合宿含延长段不可；第 1 年与普通回合可）
    pub(crate) fn outing_allowed(&self) -> bool {
        !(self.plan.camp_zone && self.plan.camp_year.unwrap_or(0) >= 1)
    }
}

/// 発表会 / 本番永久 buff（累计值）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PresentationBonus {
    /// 友情加成 %
    pub friendship: i32,
    /// 得意率 +N
    pub deyilv: i32,
    /// 灵感发生率 +N%
    pub hint_rate: i32,
    /// 比赛加成 +N%
    pub race_bonus: i32,
}

impl PresentationBonus {
    /// 由资产行构造
    fn from_row(row: &crate::data::CheckPointBuff) -> Self {
        Self {
            friendship: row.friendship,
            deyilv: row.deyilv,
            hint_rate: row.hint_rate,
            race_bonus: row.race_bonus,
        }
    }
}

/// 由剧本资产的稽古档构造「每训练位一行」的基础表（引擎公式按行数夹取等级）
fn build_keiko_table(rows: &[[i32; 7]]) -> TrainingBasicTable {
    rows.iter().map(|row| vec![*row]).collect()
}

/// 演目槽加算（账本级；预览与正式结算共用同一实现）
fn act_gauge_add(acts: &mut ActLedger, started: bool, max: i32, act: usize, amount: i32) -> i32 {
    if !started || amount <= 0 || act >= 5 {
        return 0;
    }
    let old = acts.gauge[act];
    let new = (old + amount).min(max);
    acts.gauge[act] = new;
    new - old
}

/// 未满槽演目里挑「Lv 最低 → 槽最低 → 属性序」（账本级）
fn act_lowest_gain(acts: &ActLedger, max: i32) -> Option<usize> {
    (0..5)
        .filter(|act| acts.gauge[*act] < max)
        .min_by_key(|act| (acts.level[*act], acts.gauge[*act], *act as i32))
}

/// 三则落点逐次 +1 槽（账本级；满槽跳过、并列逐次分摊）
fn act_gauge_add_lowest(acts: &mut ActLedger, started: bool, max: i32, count: i32) -> [i32; 5] {
    let mut gained = [0; 5];
    if !started {
        return gained;
    }
    for _ in 0..count.max(0) {
        let Some(act) = act_lowest_gain(acts, max) else {
            break;
        };
        acts.gauge[act] += 1;
        gained[act] += 1;
    }
    gained
}

/// 演目 +Lv（账本级；按上限夹取）
fn act_level_add(acts: &mut ActLedger, cap: i32, act: usize, amount: i32) {
    acts.level[act] = (acts.level[act] + amount).min(cap);
}

/// 三则落点：演目 Lv 最低 → 槽最低 → 速 / 耐 / 力 / 毅 / 智顺序
fn lowest_act(level: &Array5, gauge: &Array5) -> usize {
    let mut best = 0;
    for index in 1..level.len() {
        let key = (level[index], gauge[index], index as i32);
        let best_key = (level[best], gauge[best], best as i32);
        if key < best_key {
            best = index;
        }
    }
    best
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauge_three_rule_skips_full_and_splits_ties() {
        let mut game = crate::testing::sample_game();
        game.turn = crate::SCENARIO_START_TURN;
        let max = game.act_gauge_max();
        game.acts.level = [1, 2, 2, 3, 4];
        game.acts.gauge = [0, max, 0, 0, 0];
        // 未满槽里 Lv 最低 → act0 连加 5 次（槽满为止；满槽跳过）
        assert_eq!(game.gain_lowest_act_gauge(5), [5, 0, 0, 0, 0]);
        assert_eq!(game.acts.gauge, [5, 5, 0, 0, 0]);
        // act0 槽满：下一档在 Lv2 的 act2（act1 满槽跳过）与其余里取
        assert_eq!(game.gain_lowest_act_gauge(1), [0, 0, 1, 0, 0]);
        assert_eq!(game.acts.gauge, [5, 5, 1, 0, 0]);
    }

    #[test]
    fn gauge_gain_capped_per_act() {
        let mut game = crate::testing::sample_game();
        game.turn = crate::SCENARIO_START_TURN;
        let max = game.act_gauge_max();
        game.acts.gauge[0] = max - 1;
        assert_eq!(game.gain_act_gauge(0, 4), 1, "单演目入槽按上限夹取");
        assert_eq!(game.acts.gauge[0], max);
        // 目标赛 4 次 +1：全满时全部跳过
        game.acts.gauge = [max; 5];
        assert_eq!(game.gain_lowest_act_gauge(4), [0; 5]);
    }

    /// 剧本机制自 turn 2 起：0–1 回合无担当分配，训练等不获取演目槽
    #[test]
    fn scenario_resources_start_at_turn2() {
        let mut game = crate::testing::sample_game();
        assert!(!game.scenario_started(), "turn 0 剧本机制尚未开始");
        assert_eq!(game.gain_act_gauge(0, 3), 0, "0–1 回合不获取演目槽");
        assert_eq!(
            game.gain_lowest_act_gauge(2),
            [0; 5],
            "0–1 回合三则落点也不入槽"
        );
        game.turn = 1;
        assert!(!game.scenario_started());
        assert_eq!(game.gain_act_gauge(0, 3), 0);
        game.turn = crate::SCENARIO_START_TURN;
        assert!(game.scenario_started(), "turn 2 起剧本机制启动");
        assert_eq!(game.gain_act_gauge(0, 3), 3);
    }

    #[test]
    fn act_level_capped_and_three_rule() {
        let mut game = crate::testing::sample_game();
        let cap = game.act_level_cap();
        game.acts.level = [cap, 2, 2, 3, 4];
        // 三则落点不受上限限制的演目 Lv 最低者（act1 / act2 并列，取先头）
        assert_eq!(game.add_lowest_act_level(1), 1);
        assert_eq!(game.acts.level[1], 3);
        // 已到上限的演目不再加
        game.acts.level = [cap; 5];
        game.add_lowest_act_level(1);
        assert_eq!(game.acts.level, [cap; 5]);
    }

    /// 自选比赛义务进度：玉藻十字（102102）第二区间 29–48 需 4 场
    #[test]
    fn free_race_progress_reports_need_and_left() {
        let mut game = crate::testing::sample_game_with_uma(102102);
        game.turn = 29;
        let (need, left) = game.free_race_progress().expect("T29 应在义务区间内");
        assert_eq!(need, 4);
        assert!((1..=20).contains(&left), "剩余可赛回合 {left}");
        // 在区间掩码内记录一场比赛 → 需求减 1；区间外的比赛不计入
        let mask_turn = (29..=48)
            .find(|turn| game.base.uma.free_races[1].mask.contains_turn(*turn))
            .expect("区间内应有可赛回合");
        game.base.uma.set_race(mask_turn);
        assert_eq!(game.free_race_progress().map(|(need, _)| need), Some(3));
        assert!(
            game.base.uma.free_races[1].mask.contains_turn(mask_turn),
            "掩码应命中"
        );
    }

    /// 无 freeRaces 的马娘（100101 特别周）：无义务进度，育成不会被终止
    #[test]
    fn free_race_progress_none_without_free_races() {
        let mut game = crate::testing::sample_game();
        game.turn = 29;
        assert!(game.free_race_progress().is_none());
        game.turn = 28;
        game.stage = Stage::End;
        assert!(game.next(), "无义务区间的马娘应正常推进");
        assert!(!game.career_failed);
    }

    /// 育成失败标记：`on_simulation_end` 跳过终局奖励
    #[test]
    fn career_failed_skips_end_reward() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut game = crate::testing::sample_game_with_uma(102102);
        let before = game.base.uma.skill_pt;
        game.career_failed = true;
        let trainer = crate::MatsuriTrainer::new(crate::MatsuriConfig::default());
        let mut rng = StdRng::seed_from_u64(1);
        game.on_simulation_end(&trainer, &mut rng)
            .expect("终局结算");
        assert_eq!(game.base.uma.skill_pt, before, "育成失败局不应发放终局奖励");
    }

    /// 区间未达标：在区间最后一回合的 End 判定失败（终止），终局跳过奖励
    #[test]
    fn free_race_deadline_failure_terminates_career() {
        let mut game = crate::testing::sample_game_with_uma(102102);
        game.turn = 28;
        game.stage = Stage::End;
        game.settle_turn_end().expect("End 结算");
        assert!(game.career_failed, "T28 End 判定第一区间未达标");
        assert!(!game.next(), "育成失败后应终止");
        assert_eq!(game.turn, 28, "终止发生在区间最后一回合");
    }

    /// 区间达标：End 入队生涯比赛胜利奖励，随后正常推进到下一回合
    #[test]
    fn free_race_deadline_met_continues() {
        let mut game = crate::testing::sample_game_with_uma(102102);
        let mask_turn = (13..=28)
            .find(|turn| game.base.uma.free_races[0].mask.contains_turn(*turn))
            .expect("第一区间应有可赛回合");
        game.base.uma.set_race(mask_turn);
        game.turn = 28;
        game.stage = Stage::End;
        let scale = game.race_scale();
        game.settle_turn_end().expect("End 结算");
        assert!(!game.career_failed);
        assert_eq!(
            game.unresolved_events.len(),
            1,
            "达标应入队一次生涯比赛胜利事件"
        );
        let event = &game.unresolved_events[0];
        assert_eq!(event.id, 4001, "生涯比赛胜利事件 id");
        let value = &event.choices[0][0].value;
        assert_eq!(
            value.status_pt[0],
            (5.0 * scale).floor() as i32,
            "五维吃比赛 / 剧本赛后加成"
        );
        assert_eq!(
            value.status_pt[5],
            (72.0 * scale).floor() as i32,
            "SP 同口径"
        );
        assert!(game.next(), "第一区间已达标，应继续");
        assert_eq!(game.turn, 29);
    }

    #[test]
    fn presentation_bonus_follows_latest_resolved_row() {
        let mut game = crate::testing::sample_game();
        let baseline = game.presentation_bonus();
        assert_eq!(
            (
                baseline.friendship,
                baseline.deyilv,
                baseline.hint_rate,
                baseline.race_bonus
            ),
            (0, 20, 0, 0),
            "开局基线：得意率 +20"
        );
        game.check_points = [
            CheckPointResult::GreatSuccess,
            CheckPointResult::NotReached,
            CheckPointResult::NotReached,
        ];
        let bonus = game.presentation_bonus();
        assert_eq!(
            (
                bonus.friendship,
                bonus.deyilv,
                bonus.hint_rate,
                bonus.race_bonus
            ),
            (5, 40, 50, 0),
            "第 1 回大成功的累计行"
        );
        game.check_points[1] = CheckPointResult::Success;
        let bonus = game.presentation_bonus();
        assert_eq!(
            (
                bonus.friendship,
                bonus.deyilv,
                bonus.hint_rate,
                bonus.race_bonus
            ),
            (5, 40, 50, 0),
            "第 2 回成功行"
        );
    }

    /// 比赛系数：支援卡赛后 × 剧本赛后（乘算）；本番前剧本 0 / 本番大成功 ×2.0
    #[test]
    fn race_scale_multiplies_deck_and_scenario_bonus() {
        let mut game = crate::testing::sample_game();
        let deck = (100 + game.base.uma.race_bonus) as f64 / 100.0;
        assert_eq!(game.scenario_race_bonus(), 0, "本番前剧本赛后为 0");
        assert!((game.race_scale() - deck).abs() < 1e-12);

        game.check_points[2] = CheckPointResult::GreatSuccess;
        assert_eq!(game.scenario_race_bonus(), 100, "本番大成功 +100%");
        assert!(
            (game.race_scale() - deck * 2.0).abs() < 1e-12,
            "剧本赛后应与卡组赛后乘算"
        );
        assert_eq!((5.0 * game.race_scale()).floor() as i32, 15);
        assert_eq!((72.0 * game.race_scale()).floor() as i32, 223);

        game.check_points[2] = CheckPointResult::Success;
        assert!(
            (game.race_scale() - deck * 1.3).abs() < 1e-12,
            "本番成功 ×1.3"
        );
        assert_eq!((5.0 * game.race_scale()).floor() as i32, 10);
        assert_eq!((72.0 * game.race_scale()).floor() as i32, 145);
    }

    /// 「铁心的挑战者」：狄杜斯 106301/106302 出道战后（T12–71）生涯赛 五维×2.5 / PT×0.46
    #[test]
    fn dictus_iron_heart_bonus_window_and_kind() {
        let mut game = crate::testing::sample_game_with_uma(106301);
        game.base.turn = 11;
        game.plan.race = true;
        assert_eq!(game.special_race_stats_bonus(), 0, "出道战 T11 不生效");
        assert!((game.race_pt_rate() - 1.0).abs() < 1e-12, "T11 无 PT 折扣");

        game.base.turn = 12;
        assert_eq!(game.special_race_stats_bonus(), 150, "T12 起五维 +150%");
        assert!(
            (game.race_pt_rate() - 0.46).abs() < 1e-12,
            "T12 起 PT ×0.46"
        );
        game.plan.race = false;
        assert_eq!(game.special_race_stats_bonus(), 0, "自选赛不生效");
        game.plan.race = true;
        game.base.turn = 71;
        assert_eq!(game.special_race_stats_bonus(), 150, "T71 仍生效");
        game.base.turn = 72;
        assert_eq!(game.special_race_stats_bonus(), 0, "URA 期起不生效");

        game.base.turn = 30;
        assert_eq!(
            game.view().uma.special_race_stats_bonus,
            150,
            "View 同步五维加成"
        );
        assert_eq!(
            game.view().uma.special_race_pt_rate,
            46,
            "View 同步 PT 倍率"
        );
        let deck = (100 + game.base.uma.race_bonus) as f64 / 100.0;
        assert!(
            (game.race_scale() - deck * 2.5).abs() < 1e-12,
            "铁心五维 ×2.5 与卡面赛后乘算、在剧本赛后之前"
        );
        assert_eq!((5.0 * game.race_scale()).floor() as i32, 19);
        assert_eq!(
            (72.0 * game.race_scale() * game.race_pt_rate()).floor() as i32,
            128
        );

        game.check_points[2] = CheckPointResult::GreatSuccess;
        assert!((game.race_scale() - deck * 2.5 * 2.0).abs() < 1e-12);
        assert_eq!((5.0 * game.race_scale()).floor() as i32, 38);
        assert_eq!(
            (72.0 * game.race_scale() * game.race_pt_rate()).floor() as i32,
            256
        );

        let other = crate::testing::sample_game();
        assert_eq!(other.special_race_stats_bonus(), 0, "非狄杜斯不生效");
        assert!((other.race_pt_rate() - 1.0).abs() < 1e-12);

        let mut second = crate::testing::sample_game_with_uma(106302);
        second.base.turn = 30;
        second.plan.race = true;
        assert_eq!(second.special_race_stats_bonus(), 150, "106302 同款加成");
    }

    #[test]
    fn judge_presentation_uses_act_levels() {
        let mut game = crate::testing::sample_game();
        game.acts.level = [1; 5];
        assert_eq!(game.judge_presentation(0), CheckPointResult::Success);
        game.acts.level = [3; 5];
        assert_eq!(game.judge_presentation(0), CheckPointResult::GreatSuccess);
        game.acts.level = [3, 3, 3, 3, 2];
        assert_eq!(
            game.judge_presentation(0),
            CheckPointResult::Success,
            "差一个演目未达标即成功档"
        );
    }

    #[test]
    fn keiko_period_switches_at_turn_24() {
        let mut game = crate::testing::sample_game();
        game.turn = 23;
        assert!(!game.keiko_is_classic(), "turn 23 仍是 Junior 档");
        assert_eq!(game.keiko_basic_table()[0][0][0], 16, "Junior 速行");
        game.turn = 24;
        assert!(game.keiko_is_classic(), "turn 24 起切 Classic 档");
        assert_eq!(game.keiko_basic_table()[0][0][0], 21, "Classic 速行");
    }

    #[test]
    fn practice_acts_camp_all_and_gauge_lit() {
        let mut game = crate::testing::sample_game();
        game.plan = TurnPlan {
            camp_zone: true,
            camp_year: Some(0),
            ..TurnPlan::default()
        };
        assert_eq!(game.practice_acts(), [true; 5], "合宿期全部按排练处理");
        game.plan = TurnPlan::default();
        game.acts.gauge[2] = game.act_gauge_max();
        assert_eq!(
            game.practice_acts(),
            [false, false, true, false, false],
            "日常只有槽满演目按排练处理"
        );
        // 比赛 / 强制休息回合不分布人头
        game.plan.race = true;
        assert!(!game.needs_distribution());
        game.plan.race = false;
        game.forced_rest = true;
        assert!(!game.needs_distribution());
    }

    #[test]
    fn train_level_bonus_raises_all_facilities() {
        let mut game = crate::testing::sample_game();
        assert_eq!(game.train_level(0), 1, "开局设施 1 级");
        game.train_level_bonus = [2; 5];
        assert_eq!(game.train_level(0), 3, "发布会大成功累计 +2 级");
        game.train_level_bonus = [9; 5];
        assert_eq!(
            game.train_level(0),
            game.max_normal_train_level(),
            "夹到训练等级上限"
        );
    }

    /// URA 回合（72–77）全变稽古：5 个训练位全部按排练处理（无需槽满）
    #[test]
    fn ura_round_all_keiko() {
        let mut game = crate::testing::sample_game();
        assert!(!game.all_keiko_round(), "日常回合不是全稽古回合");
        game.plan = TurnPlan {
            ura_period: true,
            ..TurnPlan::default()
        };
        assert!(game.all_keiko_round(), "URA 回合属于全稽古回合");
        assert_eq!(game.practice_acts(), [true; 5], "URA 期全部按排练处理");
        // 空窗期（68–71）不属于 URA、不启用全稽古
        game.plan = TurnPlan {
            interlude: true,
            ..TurnPlan::default()
        };
        assert!(!game.all_keiko_round(), "空窗期不是全稽古回合");
        assert_eq!(
            game.practice_acts(),
            game.lit_acts(),
            "空窗期按日常槽满亮起口径"
        );
    }

    /// 通用规则：演目 Lv 已到上限（10）仍可攒 5 槽并排练；排练后槽清空、Lv 保持上限
    #[test]
    fn max_level_act_still_accumulates_and_practices() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut game = crate::testing::sample_game();
        // URA 训练：失败率 -100% → 稽古必成功，便于直接验证结果
        game.turn = crate::URA_START_TURN + 2;
        game.finals_done = true;
        game.acts.level = [10; 5];
        game.acts.gauge = [0; 5];
        // 满级不阻止攒槽：仍可加满 5 格
        let max = game.act_gauge_max();
        game.gain_act_gauge(0, max + 3);
        assert_eq!(game.acts.gauge[0], max, "满级演目仍可把槽攒满");
        assert!(game.practice_acts()[0], "满级 + 满槽仍按排练处理");
        // 排练成功：槽清空、Lv 保持 10（给训练位放一名担当，走完整训练后处理）
        game.base.distribution = vec![vec![0], vec![], vec![], vec![], vec![]];
        let mut rng = StdRng::seed_from_u64(3);
        game.do_act_train(0, &mut rng).expect("稽古结算");
        assert_eq!(game.acts.gauge[0], 0, "排练成功后槽清空");
        assert_eq!(game.acts.level[0], 10, "Lv 已满时排练不再升、保持 10");
    }

    #[test]
    fn deck_five_types() {
        let game = crate::testing::sample_game();
        assert!(
            game.deck_has_five_types(),
            "参考卡组含 5 种类型（速/耐/力/速/智/友）"
        );
    }

    #[test]
    fn lowest_act_follows_three_rule_order() {
        // ① 演目 Lv 最低优先
        assert_eq!(lowest_act(&[1, 2, 2, 2, 2], &[0; 5]), 0);
        // ② Lv 并列时槽最低优先
        assert_eq!(lowest_act(&[2, 2, 2, 2, 2], &[3, 1, 2, 2, 2]), 1);
        // ③ Lv / 槽都并列时按 速→耐→力→毅→智 顺序
        assert_eq!(lowest_act(&[1; 5], &[0; 5]), 0);
        // 组合：Lv 与槽都不同，取元组最小者
        assert_eq!(lowest_act(&[3, 2, 2, 4, 2], &[0, 5, 1, 0, 1]), 2);
    }
}
