//! 剧本局面（`MatsuriGame`）、资源账本、资源快照与装配
//!
//! 状态底座复用引擎 [`BaseGame`]（回合 / 马娘 / 卡组 / 继承 / 规则流），剧本只加自己的
//! 阶段机、人头、剧本资源与剧本资产。契约 impl 集中在 [`traits`]，回合结构与阶段机在
//! [`stage`]，本文件只放 inherent 方法（资源账本 / 建局 / 快照 / 训练）。

pub mod action;
pub mod advice;
pub mod deck;
pub mod distribute;
pub mod event;
pub mod person;
pub mod roster;
pub mod settle;
pub mod stage;
pub mod train;
pub mod traits;

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use umaai::Config;
use umaai_core::gamedata::ActionValue;
use umaai_core::rng::StrategyRng;
use umaai_core::{Array5, Data};
use umasim::game::base::BaseGame;
use umasim::game::{Game, InheritInfo, TrainingBasicTable};

pub use action::Action;
pub use deck::{DeckError, DeckReport, DeckWarning};
pub use person::{MatsuriPerson, PersonOrigin};
pub use stage::{CampState, Stage, TurnPlan};

use crate::data::{ActLevelEffect, CheckPointResult, MatsuriData};

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
    /// 本局是否已获得教诲（turn 2 的 `End` 首次授予；此前槽位不可用）
    pub acquired: bool,
}

/// 事件记录容量（S8 手动 / 前端回显：滚动保留最近 N 条）
pub const EVENT_LOG_CAPACITY: usize = 32;

/// 一次事件结算记录（S8：手动核对 / 前端回显）
///
/// 由 [`MatsuriGame`] 在事件结算后追加：事件 id / 名称 / 选项 / 实际到账数值（含卡面加成）。
/// 属剧本自持的“呈现辅助状态”，不参与机制结算。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    /// 单调递增序号（记录滚动后仍可用于「自上次以来」判断）
    pub seq: u64,
    /// 结算回合（0 基）
    pub turn: i32,
    /// 事件 id
    pub id: u32,
    /// 事件名
    pub name: String,
    /// 选项组下标（0 基）
    pub choice: usize,
    /// 结算摘要（实际到账数值 / 状态变化；无选项或无数值时为空）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 训练 / 演目稽古预览（S8：手动核对与前端面板）
///
/// 成功收益 = 下层（引擎默认公式）+ 上层（剧本来源），已含变胖速度归零 / 本番演出等剧本修正；
/// 失败率 % = 引擎基础公式 + 剧本失败率下降。
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingPreview {
    /// 成功时的到账值
    pub value: ActionValue,
    /// 失败率（%）
    pub failure_rate: f32,
    /// 是否为演目稽古上下文
    pub keiko: bool,
}

/// 剧本资源快照（给前端的纯数据 View）
///
/// 形态 A 五条纪律（见引擎仓 `docs/scenario/contracts.md`）：**单一取数口** / 只放引擎不知道的字段 /
/// 快照语义（幂等、无副作用）/ 可演进（`non_exhaustive`）/ **不进契约 trait**。
/// S3 建底、S5 补全剧本自持资源：演目 gauge / Lv / 教诲槽位与転成 / 合宿状态 / 発表会·本番结果 /
/// 永久 buff / 助力支撑 / 追加稽古 / 訓練 Lv 提升。
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
    /// 発表会 / 本番永久友情 buff %（累计；开局 0）
    pub presentation_friendship: i32,
    /// 発表会 / 本番永久得意率 +N（累计；开局基线 20）
    pub presentation_deyilv: i32,
    /// 発表会 / 本番永久灵感发生率 +N%
    pub presentation_hint_rate: i32,
    /// 発表会 / 本番永久比赛加成 +N%
    pub presentation_race_bonus: i32,
    /// 助力支撑剩余次数（0 = 无）
    pub assist_support_left: i32,
    /// 本回合研鑽「追加の稽古」是否掷中（未点对应训练则作废）
    pub kensan_pending: bool,
    /// 発表会大成功累计的普通训练 Lv 提升
    pub train_level_bonus: Array5,
    /// 编成警告项（建局时一次算定；None 段位表示编成合规）
    pub deck_warnings: Vec<DeckWarning>,

    /// 最近事件记录（S8：手动核对 / 前端回显；容量上限 [`EVENT_LOG_CAPACITY`]）
    pub recent_events: Vec<EventRecord>,
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
    keiko_settling: bool,
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
    /// 友人出行解锁时获得的「疲劳回复」flag（剧本自持；体力 ≤49 时一次 +15，S6）
    pub fatigue_recovery: bool,
    /// 進捗発表会 / 本番结果（3 场；`End` 阶段结算，S5 落地）
    pub check_points: [CheckPointResult; 3],
    /// 本番演出是否已结算（S5 落地）
    pub finals_done: bool,
    /// 编成警告项（建局时由 `game/deck.rs` 一次算定；禁止项已拒绝建局）
    pub deck_warnings: Vec<DeckWarning>,
    /// 最近事件记录（S8：手动 / 前端核对用；事件结算后追加，容量上限 [`EVENT_LOG_CAPACITY`]）
    pub event_log: Vec<EventRecord>,
    /// 助力支撑剩余次数（0 = 无；演目稽古获得「助力の支え」时重置为规则上限，S5）
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
            keiko_settling: false,
            acts: ActLedger::default(),
            advice: AdviceLedger::default(),
            camp: CampState::default(),
            plan: TurnPlan::default(),
            forced_rest: false,
            fatigue_recovery: false,
            check_points: [CheckPointResult::NotReached; 3],
            finals_done: false,
            deck_warnings: Vec::new(),
            event_log: Vec::new(),
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
    #[must_use]
    pub fn with_config(mut self, config: crate::MatsuriConfig) -> Self {
        self.config = config;
        self
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
            presentation_friendship: self.presentation_bonus().friendship,
            presentation_deyilv: self.presentation_bonus().deyilv,
            presentation_hint_rate: self.presentation_bonus().hint_rate,
            presentation_race_bonus: self.presentation_bonus().race_bonus,
            assist_support_left: self.assist_support_left,
            kensan_pending: self.kensan_pending,
            train_level_bonus: self.train_level_bonus,
            deck_warnings: self.deck_warnings.clone(),
            recent_events: self.event_log.clone(),
        }
    }

    /// 追加一条事件记录（S8 手动 / 前端核对；滚动保留最近 [`EVENT_LOG_CAPACITY`] 条）
    ///
    /// 序号由记录自身延续（滚动丢弃旧记录后仍单调递增），供前端判断「自上次以来」的新事件。
    pub(crate) fn push_event_log(&mut self, mut record: EventRecord) {
        record.seq = self.event_log.last().map_or(1, |last| last.seq + 1);
        if self.event_log.len() >= EVENT_LOG_CAPACITY {
            self.event_log.remove(0);
        }
        self.event_log.push(record);
    }

    /// 演目 Lv 上限（仅支持 SSR 吾妻 → 带友人档 10）
    #[must_use]
    pub fn act_level_cap(&self) -> i32 {
        self.data.rules.act_level_max_with_friend
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

    /// 比赛加成 %（支援卡赛后 + 発表会累计 race_bonus，**同表加算**；2026-11-01 拍板）
    pub(crate) fn scenario_race_bonus(&self) -> i32 {
        self.presentation_bonus().race_bonus
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
        if self.plan.camp_train(self.forced_rest) {
            [true; 5]
        } else {
            self.lit_acts()
        }
    }

    /// 给单个演目加槽（满槽吃掉溢出、不叠加），返回实际增加量
    pub(crate) fn gain_act_gauge(&mut self, act: usize, amount: i32) -> i32 {
        if amount <= 0 || act >= 5 {
            return 0;
        }
        let max = self.act_gauge_max();
        let old = self.acts.gauge[act];
        let new = (old + amount).min(max);
        self.acts.gauge[act] = new;
        new - old
    }

    /// 三则落点 + **满槽跳过**：在未满槽的演目里挑「Lv 最低 → 槽最低 → 属性序」
    fn lowest_gain_act(&self) -> Option<usize> {
        let max = self.act_gauge_max();
        (0..5)
            .filter(|act| self.acts.gauge[*act] < max)
            .min_by_key(|act| (self.acts.level[*act], self.acts.gauge[*act], *act as i32))
    }

    /// 三则落点逐次 +1 槽（`count` 次；满槽跳过、并列逐次分摊），返回各演目实际增加量
    pub(crate) fn gain_lowest_act_gauge(&mut self, count: i32) -> [i32; 5] {
        let mut gained = [0; 5];
        for _ in 0..count.max(0) {
            let Some(act) = self.lowest_gain_act() else {
                break;
            };
            self.acts.gauge[act] += 1;
            gained[act] += 1;
        }
        gained
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
        self.acts.level[act] = (self.acts.level[act] + amount).min(cap);
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
        let max = game.act_gauge_max();
        game.acts.gauge[0] = max - 1;
        assert_eq!(game.gain_act_gauge(0, 4), 1, "单演目入槽按上限夹取");
        assert_eq!(game.acts.gauge[0], max);
        // 目标赛 4 次 +1：全满时全部跳过
        game.acts.gauge = [max; 5];
        assert_eq!(game.gain_lowest_act_gauge(4), [0; 5]);
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
