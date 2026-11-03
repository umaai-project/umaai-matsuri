//! 剧本回合信息快照（`MatsuriView`）：宿主呈现 / 在线外壳 / 前端的**单一数据源**
//!
//! 形态：值快照（owned、`Clone`、无借用生命周期，跨线程 / 跨宿主传递安全；序列化由剧本提供）。
//! 口径（S8 2026-11-02 拍板）：**完整回合快照**——手动核对面板已显示的全部状态与派生数据都从
//! 本结构取；名称文本不进入（呈现层按引擎静态表查），动作候选 / 预览不进入（决策上下文，
//! 由 `Trainer` 入参与 `preview_*` 只读 API 提供），建局育成配置不进入（不是回合信息）。
//!
//! 分节与手动面板分区一一对应：[`TurnView`]（回合结构）/ [`ResourceView`]（剧本资源）/
//! [`UmaView`]（数值状态）/ [`BoardView`]（人头与训练）/ [`ScheduleView`]（赛程）/
//! [`FriendView`]（友人链）/ [`CampView`]（合宿）。
//!
//! 纪律：**单一取数口**（何时取由宿主决定）/ **快照语义**（幂等、只读、无副作用）/
//! **可演进**（`non_exhaustive`）/ **不进契约 trait**。事件记录 `EventRecord` 也在这里（滚动保留）。

use serde::{Deserialize, Serialize};
use umaai_core::{Array5, Data, UmaFlags};
use umasim::game::{FriendOutState, Game, Person, StageId};

use super::MatsuriGame;
use super::deck::DeckWarning;
use super::person::PersonOrigin;
use crate::MAX_TURN;
use crate::data::CheckPointResult;

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

/// 剧本回合信息快照（宿主呈现 / 在线外壳 / 前端单一数据源）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MatsuriView {
    /// 回合与回合结构
    pub turn: TurnView,
    /// 剧本自持资源（演目 / 教诲 / 発表会 / 助力 / 追加稽古 / 訓練 Lv 加成）
    pub resources: ResourceView,
    /// 马娘数值与状态（面板「数值 / 状态」）
    pub uma: UmaView,
    /// 人头与训练位（面板「资源·担当」「训练」）
    pub board: BoardView,
    /// 赛程（下次発表会门槛 / 目标赛 / 自选赛）
    pub schedule: ScheduleView,
    /// 友人链（吾妻）
    pub friend: FriendView,
    /// 合宿进度
    pub camp: CampView,
    /// 编成警告项（建局时一次算定；空 = 编成合规）
    pub deck_warnings: Vec<DeckWarning>,
    /// 最近事件记录（容量上限 [`EVENT_LOG_CAPACITY`]）
    pub recent_events: Vec<EventRecord>,
}

/// 回合 / 阶段 / 回合结构（`TurnPlan` 全量投影 + 派生的排练集合）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnView {
    /// 当前回合（0 基）
    pub turn: i32,
    /// 最大回合
    pub max_turn: i32,
    /// 阶段号（`StageId` 显式赋号，稳定契约）
    pub stage: StageId,
    /// 年次（Junior / Classic / Senior）
    pub year: MatsuriYear,
    /// 剧本机制是否已启动（turn ≥ 2；此前的担当 / 演目资源均不可用）
    pub scenario_started: bool,
    /// 本回合是否强制比赛（目标赛 / URA 决赛）
    pub race: bool,
    /// 本回合是否 URA 决赛（加赛）
    pub ura_race: bool,
    /// 本回合是否本番（第 3 次発表会）
    pub finals: bool,
    /// 本回合進捗発表会 / 本番场次（0–2；非结算回合为 `None`）
    pub check_point: Option<usize>,
    /// 本回合是否教诲可调整窗口
    pub advice_window: bool,
    /// 本回合是否 URA 回合（turn 72–77；73/75/77 为固定决赛）
    pub ura_period: bool,
    /// 本回合是否剧本结束后的空窗期（本番后、URA 前，turn 68–71；不属于 URA 回合）
    pub interlude: bool,
    /// 本回合是否因马娘特殊事件改为强制休息（目标赛回合仍按赛程分类）
    pub forced_rest: bool,
    /// 本回合是否可自选比赛（自选赛窗口内）
    pub self_race_available: bool,
    /// 本回合按排练处理的演目集合（合宿排练 / URA 回合 → 全部；否则槽满亮起集合）
    pub practice: [bool; 5],
}

/// 年次（剧本回合边界：0–23 Junior / 24–47 Classic / 48– Senior）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatsuriYear {
    /// 第一年
    Junior,
    /// 第二年
    Classic,
    /// 第三年
    Senior,
}

impl MatsuriYear {
    /// 由回合（0 基）派生年次
    #[must_use]
    pub fn from_turn(turn: i32) -> Self {
        match turn {
            0..=23 => Self::Junior,
            24..=47 => Self::Classic,
            _ => Self::Senior,
        }
    }

    /// 年次英文名（呈现 / 诊断日志用）
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Junior => "Junior",
            Self::Classic => "Classic",
            Self::Senior => "Senior",
        }
    }
}

/// 剧本自持资源（演目账本 / 教诲 / 発表会累计 / 助力 / 追加稽古 / 訓練 Lv 加成）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceView {
    /// 5 条演目 gauge（索引 = 演目 / 训练位下标）
    pub act_gauge: Array5,
    /// 5 个演目 Lv（1 级起）
    pub act_level: Array5,
    /// 演目 gauge 上限
    pub act_gauge_max: i32,
    /// 演目 Lv 上限（SSR 吾妻 → 10）
    pub act_level_max: i32,
    /// 演目下标 → 训练位下标（展示映射；静态资产派生）
    pub act_trains: [usize; 5],
    /// 教诲槽位：每个演目绑定的基础教诲 id（0 = 未分配）
    pub advice_slots: Array5,
    /// 転成：每个演目槽位叠加的教诲 id（0 = 未転成）
    pub advice_fused: Array5,
    /// 本局是否已获得教诲（T2 Begin 首次授予）
    pub advice_acquired: bool,
    /// 助力支撑剩余次数（0 = 无）
    pub assist_support_left: i32,
    /// 本回合研鑽「追加の稽古」是否掷中（未点对应训练则作废）
    pub kensan_pending: bool,
    /// 研鑽（追加稽古）绑定演目（未获得 / 未転成时为 `None`）
    pub kensan_act: Option<usize>,
    /// 追加稽古当前发生率 %（研鑽绑定演目的 Lv 档；无研鑽时 0）
    pub kensan_rate: i32,
    /// 助力支撑触发演目（该演目担当参与稽古成功时获得 / 重置；无助力教え为 `None`）
    pub assist_act: Option<usize>,
    /// 発表会大成功累计的普通训练 Lv 提升
    pub train_level_bonus: Array5,
    /// 発表会 / 本番永久友情 buff %（累计；开局 0）
    pub presentation_friendship: i32,
    /// 発表会 / 本番永久得意率 +N（累计；开局基线 20）
    pub presentation_deyilv: i32,
    /// 発表会 / 本番永久灵感发生率 +N%
    pub presentation_hint_rate: i32,
    /// 発表会 / 本番永久比赛加成 +N%
    pub presentation_race_bonus: i32,
    /// 進捗発表会 / 本番结果（3 场）
    pub check_points: [CheckPointResult; 3],
    /// 本番演出是否已结算
    pub finals_done: bool,
}

/// 马娘数值与状态（面板「数值 / 状态」行）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UmaView {
    /// 马娘编号
    pub uma_id: u32,
    /// 体力
    pub vital: i32,
    /// 体力上限
    pub max_vital: i32,
    /// 干劲 [1, 5]
    pub motivation: i32,
    /// 五维 [速, 耐, 力, 根, 智]
    pub five_status: Array5,
    /// 五维本局动态上限
    pub five_status_limit: Array5,
    /// 技能点
    pub skill_pt: i32,
    /// 累计灵感等级（面板「灵感合计」）
    pub total_hints: i32,
    /// 比赛加成 %（支援卡赛后）
    pub race_bonus: i32,
    /// 特殊状态五维加成 %（「铁心的挑战者」+150 = ×2.5；0 = 无）
    pub special_race_stats_bonus: i32,
    /// 特殊状态 PT 倍率 %（「铁心的挑战者」46 = ×0.46；100 = 无折扣）
    pub special_race_pt_rate: i32,
    /// Buff / 生病状态（Display 含「变胖」等病名）
    pub flags: UmaFlags,
    /// 最近一次训练的训练下标（0–4）；未训练过为 `None`
    pub last_train: Option<usize>,
}

/// 人头与训练位（面板「资源·担当」+「训练」）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardView {
    /// 普通训练设施等级（1 基；索引 = 训练位）
    pub train_levels: [usize; 5],
    /// 人头（Vec 下标 = 人头下标）
    pub persons: Vec<PersonView>,
    /// 各演目担当人头下标（可用者；0–1 回合机制未启动时为空）
    pub act_members: [Vec<usize>; 5],
    /// 各训练位成员（按占位，含分身；带闪彩标记）
    pub distribution: [Vec<MemberView>; 5],
}

/// 单个人头的呈现数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonView {
    /// 人头来源（卡 / 友人 / 编成外神驹 / 理事长 / 记者）
    pub origin: PersonOrigin,
    /// 担当演目下标（不参与担当为 `None`）
    pub act: Option<usize>,
    /// 角色 ID
    pub chara_id: u32,
    /// 支援卡 ID（非卡人头为 `None`）
    pub card_id: Option<u32>,
    /// 羁绊
    pub friendship: i32,
    /// 灵感图标（Hint）
    pub hint: bool,
    /// 本回合是否可用（加入回合 / 大成功条件等）
    pub available: bool,
}

/// 训练位上的一个占位（同一人头可能出现于多个训练位 = 分身）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberView {
    /// 人头下标（`BoardView::persons` 下标）
    pub person: usize,
    /// 该占位是否闪彩（彩圈）
    pub shining: bool,
}

/// 赛程（下次発表会门槛 / 目标赛 / 自选赛窗口）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleView {
    /// 下次進捗発表会 / 本番门槛（无后续场次为 `None`）
    pub next_check_point: Option<CheckPointNeed>,
    /// 后续目标赛回合（升序；含当前回合）
    pub goal_races: Vec<i32>,
    /// 后续自选赛窗口（升序；不含目标赛回合）
    pub self_races: Vec<SelfRace>,
}

/// 下次進捗発表会 / 本番门槛
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPointNeed {
    /// 结算回合（0 基）
    pub turn: i32,
    /// 各演目需要的 Lv
    pub need_act_level: i32,
    /// 需要达标的演目数
    pub need_act_num: i32,
    /// 是否本番（最后一次発表会）
    pub is_finals: bool,
}

/// 一个自选赛窗口
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfRace {
    /// 回合（0 基）
    pub turn: i32,
    /// 比赛等级（1 = G1 … 4 = OP）
    pub grade: i32,
}

/// 友人（吾妻）链状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FriendView {
    /// 友人出行状态（未点击 / 已点击·未解锁 / 已解锁 / 离开）
    pub out_state: FriendOutState,
    /// 5 段出行各自是否已用
    pub out_used: [bool; 5],
    /// 下一段出行下标（0 基；`None` = 未解锁或 5 段已用完）
    pub next_step: Option<usize>,
    /// 下一段出行的段事件 id（名称由呈现层按引擎静态表解析）
    pub next_event_id: Option<u32>,
    /// 解除友人出行时获得的「疲劳回复」flag（体力 ≤49 时一次性 +15）
    pub fatigue_recovery: bool,
}

/// 合宿进度（含窗口内 / 延长段 / 结算点 / 已计入回合数）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampView {
    /// 本回合是否在合宿区（含被比赛 / 强制休息占用的回合）
    pub active: bool,
    /// 本回合合宿年次（0–2；不在合宿为 `None`）
    pub year: Option<usize>,
    /// 本回合是否原窗口内合宿（非延长期）
    pub regular: bool,
    /// 本回合是否延长期合宿（窗口外顺延）
    pub extended: bool,
    /// 本回合是否合宿版休息（Classic / Senior 原窗口）
    pub rest: bool,
    /// 本回合是否合宿结束的全演目 Lv+1 结算点
    pub level_up: bool,
    /// 本回合训练是否全变演目稽古（合宿区且未被比赛 / 强制休息占用）
    pub training: bool,
    /// 本年度已计入的合宿回合数（比赛 / 强制休息不计）
    pub counted: usize,
    /// 本年度合宿窗口总回合数（非合宿为 `None`）
    pub total: Option<usize>,
}

impl MatsuriGame {
    /// 取剧本回合信息快照（**单一取数口**；何时取由宿主决定）
    ///
    /// 纯只读：不消耗任何随机流、不改局面；同一局面连续调用结果相等。
    #[must_use]
    pub fn view(&self) -> MatsuriView {
        let turn = self.turn();
        let presentation = self.presentation_bonus();
        // 教诲派生：研鑽（追加稽古）绑定演目与发生率、助力支撑触发演目（纯只读）
        let kensan_act = self.kensan_act();
        let kensan_rate = kensan_act.map_or(0, |act| self.advice_slot_effect(act).kensan_rate);
        let assist_act = (0..5).find(|act| self.advice_slot_effect(*act).support_assist);
        MatsuriView {
            turn: TurnView {
                turn,
                max_turn: self.max_turn(),
                stage: self.stage(),
                year: MatsuriYear::from_turn(turn),
                scenario_started: self.scenario_started(),
                race: self.plan.race,
                ura_race: self.plan.ura_race,
                finals: self.plan.finals(),
                check_point: self.plan.check_point,
                advice_window: self.plan.advice_window.is_some(),
                ura_period: self.plan.ura_period,
                interlude: self.plan.interlude,
                forced_rest: self.forced_rest,
                self_race_available: self.base.can_self_race(),
                practice: self.practice_acts(),
            },
            resources: ResourceView {
                act_gauge: self.acts.gauge,
                act_level: self.acts.level,
                act_gauge_max: self.act_gauge_max(),
                act_level_max: self.act_level_cap(),
                act_trains: std::array::from_fn(|act| {
                    self.data.acts.get(act).map_or(act, |row| row.train)
                }),
                advice_slots: self.advice.slots,
                advice_fused: self.advice.fused,
                advice_acquired: self.advice.acquired,
                assist_support_left: self.assist_support_left,
                kensan_pending: self.kensan_pending,
                kensan_act,
                kensan_rate,
                assist_act,
                train_level_bonus: self.train_level_bonus,
                presentation_friendship: presentation.friendship,
                presentation_deyilv: presentation.deyilv,
                presentation_hint_rate: presentation.hint_rate,
                presentation_race_bonus: presentation.race_bonus,
                check_points: self.check_points,
                finals_done: self.finals_done,
            },
            uma: UmaView {
                uma_id: self.uma().uma_id,
                vital: self.uma().vital,
                max_vital: self.uma().max_vital,
                motivation: self.uma().motivation,
                five_status: self.uma().five_status,
                five_status_limit: self.uma().five_status_limit,
                skill_pt: self.uma().skill_pt,
                total_hints: self.uma().total_hints,
                race_bonus: self.uma().race_bonus,
                special_race_stats_bonus: self.special_race_stats_bonus(),
                special_race_pt_rate: (self.race_pt_rate() * 100.0).round() as i32,
                flags: self.uma().flags.clone(),
                last_train: self.base.last_train,
            },
            board: self.board_view(),
            schedule: self.schedule_view(turn),
            friend: self.friend_view(),
            camp: CampView {
                active: self.plan.camp_zone,
                year: self.plan.camp_year,
                regular: self.plan.camp_regular,
                extended: self.plan.camp_extended,
                rest: self.plan.camp_rest,
                level_up: self.plan.camp_level_up,
                training: self.plan.camp_train(self.forced_rest),
                counted: self.camp.counted,
                total: self.plan.camp_year.and_then(|year| {
                    self.data
                        .camp
                        .windows
                        .get(year)
                        .map(|window| (window.end - window.start + 1).max(0) as usize)
                }),
            },
            deck_warnings: self.deck_warnings.clone(),
            recent_events: self.event_log.clone(),
        }
    }

    /// 人头与训练位快照
    fn board_view(&self) -> BoardView {
        let persons = self
            .persons
            .iter()
            .enumerate()
            .map(|(index, person)| PersonView {
                origin: person.origin,
                act: person.act,
                chara_id: person.chara_id(),
                card_id: person.card_id(),
                friendship: person.friendship(),
                hint: person.hint(),
                available: self.person_is_available(index),
            })
            .collect();
        // 担当：机制启动前不分配；只列当前可用者
        let mut act_members: [Vec<usize>; 5] = std::array::from_fn(|_| Vec::new());
        if self.scenario_started() {
            for (index, person) in self.persons.iter().enumerate() {
                if let Some(act) = person.act
                    && act < 5
                    && self.person_is_available(index)
                {
                    act_members[act].push(index);
                }
            }
        }
        // 训练位占位：按分布原序；负数（不在）跳过；分身按占位逐个计入
        let mut distribution: [Vec<MemberView>; 5] = std::array::from_fn(|_| Vec::new());
        for (train, slot) in distribution.iter_mut().enumerate() {
            let Some(occupied) = self.base.distribution.get(train) else {
                continue;
            };
            for person in occupied {
                if *person < 0 {
                    continue;
                }
                let person = *person as usize;
                slot.push(MemberView {
                    person,
                    shining: self.is_shining_at(person, train),
                });
            }
        }
        BoardView {
            train_levels: std::array::from_fn(|train| self.train_level(train)),
            persons,
            act_members,
            distribution,
        }
    }

    /// 赛程快照（门槛 / 目标赛 / 自选赛窗口）
    fn schedule_view(&self, turn: i32) -> ScheduleView {
        let last_check_turn = self.data.schedule.last().map(|row| row.turn);
        let next_check_point = self
            .data
            .schedule
            .iter()
            .find(|row| row.turn >= turn)
            .map(|row| CheckPointNeed {
                turn: row.turn,
                need_act_level: row.need_act_level,
                need_act_num: row.need_act_num,
                is_finals: Some(row.turn) == last_check_turn,
            });
        let goal_races = (turn..=MAX_TURN)
            .filter(|turn| self.uma().is_race_turn(*turn))
            .collect();
        let self_races = (turn..=MAX_TURN)
            .filter(|turn| !self.uma().is_race_turn(*turn))
            .filter_map(|turn| {
                let grade = *self.base.race_grades.get(turn as usize)?;
                (grade > 0).then_some(SelfRace { turn, grade })
            })
            .collect();
        ScheduleView {
            next_check_point,
            goal_races,
            self_races,
        }
    }

    /// 友人链快照
    fn friend_view(&self) -> FriendView {
        let out_used: [bool; 5] = std::array::from_fn(|index| {
            self.base
                .friend
                .out_used
                .get(index)
                .copied()
                .unwrap_or(false)
        });
        let next_step = self.base.friend.out_used.iter().position(|used| !used);
        // 段位（1 基）→ 引擎静态表里的段事件；名称由呈现层解析
        let next_event_id = next_step.and_then(|step| {
            Data::get()
                .ok()?
                .events
                .friend_events
                .get(&(step + 1).to_string())
                .map(|event| event.id)
        });
        FriendView {
            out_state: self.base.friend.out_state,
            out_used,
            next_step,
            next_event_id,
            fatigue_recovery: self.fatigue_recovery,
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
}

#[cfg(test)]
mod tests {
    use umasim::game::Game;

    use super::*;
    use crate::data::CheckPointResult;
    use crate::game::TurnPlan;

    /// 视图与局面逐项一致（分节字段全部对得上）
    #[test]
    fn view_mirrors_game_state() {
        let mut game = crate::testing::sample_game();
        game.turn = 36;
        game.plan = TurnPlan {
            race: false,
            ura_race: false,
            ura_period: false,
            interlude: false,
            camp_year: Some(1),
            camp_zone: true,
            camp_regular: true,
            camp_extended: false,
            camp_rest: true,
            camp_level_up: false,
            check_point: None,
            advice_window: Some(3),
        };
        game.acts.gauge = [1, 2, 3, 4, 5];
        game.acts.level = [2, 3, 4, 5, 6];
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.advice.fused = [0, 601, 0, 0, 0];
        game.camp.counted = 2;
        game.assist_support_left = 3;
        game.kensan_pending = true;
        game.train_level_bonus = [1, 2, 0, 0, 1];
        game.check_points = [
            CheckPointResult::GreatSuccess,
            CheckPointResult::Success,
            CheckPointResult::NotReached,
        ];
        game.finals_done = true;
        game.fatigue_recovery = true;
        game.base.last_train = Some(2);
        game.base.uma.vital = 55;
        game.base.uma.skill_pt = 321;
        game.base.uma.flags.ill = umaai_core::Ill::FAT;
        game.base.friend.out_state = FriendOutState::AfterUnlock;
        game.base.friend.out_used = vec![true, true, false, false, false];
        game.base.distribution = vec![vec![0, 1], vec![2], vec![], vec![3, 4], vec![5]];

        let view = game.view();
        // 回合 / 阶段
        assert_eq!(view.turn.turn, 36);
        assert_eq!(view.turn.max_turn, crate::MAX_TURN);
        assert_eq!(view.turn.stage, game.stage());
        assert_eq!(view.turn.year, MatsuriYear::Classic);
        assert!(view.turn.scenario_started);
        assert!(view.turn.advice_window);
        assert!(view.camp.regular && !view.camp.extended);
        // 资源
        assert_eq!(view.resources.act_gauge, game.acts.gauge);
        assert_eq!(view.resources.act_level, game.acts.level);
        assert_eq!(view.resources.act_gauge_max, game.act_gauge_max());
        assert_eq!(view.resources.act_level_max, game.act_level_cap());
        assert_eq!(view.resources.advice_slots, game.advice.slots);
        assert_eq!(view.resources.advice_fused, game.advice.fused);
        assert!(view.resources.advice_acquired);
        assert_eq!(view.resources.kensan_act, None, "未転成 701 时无研鑽演目");
        assert_eq!(view.resources.kensan_rate, 0);
        assert_eq!(
            view.resources.assist_act,
            Some(3),
            "助力教え（401）绑定演目 3"
        );
        assert_eq!(
            view.resources.act_trains,
            std::array::from_fn(|act| game.data.acts[act].train)
        );
        // 数值
        assert_eq!(view.uma.uma_id, game.uma().uma_id);
        assert_eq!(view.uma.vital, game.uma().vital);
        assert_eq!(view.uma.five_status, game.uma().five_status);
        assert_eq!(view.uma.flags.ill, umaai_core::Ill::FAT);
        assert_eq!(view.uma.last_train, Some(2));
        assert!(view.friend.fatigue_recovery);
        // 人头 / 训练
        assert_eq!(view.board.persons.len(), game.persons().len());
        for (index, person) in game.persons().iter().enumerate() {
            let snapshot = &view.board.persons[index];
            assert_eq!(snapshot.origin, person.origin, "人头 {index} 来源");
            assert_eq!(snapshot.act, person.act, "人头 {index} 担当");
            assert_eq!(snapshot.chara_id, person.chara_id());
            assert_eq!(snapshot.card_id, person.card_id());
            assert_eq!(snapshot.friendship, person.friendship());
            assert_eq!(snapshot.hint, person.hint());
            assert_eq!(snapshot.available, game.person_is_available(index));
        }
        assert_eq!(
            view.board.train_levels,
            std::array::from_fn(|t| game.train_level(t))
        );
        assert_eq!(view.board.distribution.len(), 5);
        assert_eq!(view.board.distribution[0].len(), 2);
        assert_eq!(view.board.distribution[0][0].person, 0);
        assert_eq!(view.board.distribution[3][1].person, 4);
        // 赛程 / 合宿 / 友人
        assert_eq!(
            view.friend.next_step,
            Some(2),
            "前两段已用 → 下一段 0 基下标 2"
        );
        let expected_event = Data::get()
            .ok()
            .and_then(|data| data.events.friend_events.get("3"))
            .map(|event| event.id);
        assert_eq!(
            view.friend.next_event_id, expected_event,
            "下一段事件 id 应与引擎静态表一致"
        );
        assert_eq!(view.camp.counted, 2);
        assert_eq!(view.camp.total, Some(4), "Classic 合宿窗口 4 回合");
        assert!(view.camp.training, "合宿区且非比赛 / 强制休息 → 全排练");
        let expected_next = game
            .data
            .schedule
            .iter()
            .find(|row| row.turn >= 36)
            .expect("36 回合后应有発表会");
        assert_eq!(
            view.schedule.next_check_point,
            Some(CheckPointNeed {
                turn: expected_next.turn,
                need_act_level: expected_next.need_act_level,
                need_act_num: expected_next.need_act_num,
                is_finals: expected_next.turn == game.data.schedule.last().unwrap().turn,
            })
        );
        // 事件 / 编成
        assert_eq!(view.deck_warnings, game.deck_warnings);
        assert_eq!(view.recent_events, game.event_log);
    }

    /// 研鑽 / 助力支撑的派生演目跟随教诲槽与転成（视图只读）
    #[test]
    fn view_exposes_kensan_and_assist_acts() {
        let mut game = crate::testing::sample_game();
        game.acquire_advice();
        game.advice.fused = [0, 701, 0, 0, 0];
        game.acts.level = [3, 7, 3, 3, 3];
        game.kensan_pending = true;
        let view = game.view();
        assert_eq!(view.resources.kensan_act, Some(1), "701 転成在演目 1");
        assert_eq!(view.resources.kensan_rate, 40, "研鑽 Lv7 发生率 40%");
        assert_eq!(view.resources.assist_act, Some(3), "401 助力绑定演目 3");
        // 未获得教诲：两类演目均为 None
        let fresh = crate::testing::sample_game();
        let fresh_view = fresh.view();
        assert_eq!(fresh_view.resources.kensan_act, None);
        assert_eq!(fresh_view.resources.assist_act, None);
        assert_eq!(fresh_view.resources.kensan_rate, 0);
    }

    /// 视图是纯只读快照：连续调用相等，且不改动局面
    #[test]
    fn view_is_pure_and_idempotent() {
        let mut game = crate::testing::sample_game();
        game.turn = 12;
        let before = game.clone();
        let first = game.view();
        let second = game.view();
        assert_eq!(first, second, "同一局面连续取快照必须相等");
        assert_eq!(game, before, "取快照不得改动局面");
    }

    /// 赛程派生：発表会门槛 / 合宿窗口总长 / 自选赛窗口
    #[test]
    fn view_schedule_derivations() {
        let mut game = crate::testing::sample_game();
        // 発表会门槛：T23 / T47 / T67，Lv 3 / 5 / 6，需 5 个演目；T67 为本番
        for (turn, expect) in [
            (0, Some((23, 3, 5, false))),
            (22, Some((23, 3, 5, false))),
            (23, Some((23, 3, 5, false))),
            (24, Some((47, 5, 5, false))),
            (47, Some((47, 5, 5, false))),
            (48, Some((67, 6, 5, true))),
            (67, Some((67, 6, 5, true))),
            (68, None),
        ] {
            game.turn = turn;
            let need = game.view().schedule.next_check_point;
            assert_eq!(
                need,
                expect.map(|(turn, level, count, finals)| CheckPointNeed {
                    turn,
                    need_act_level: level,
                    need_act_num: count,
                    is_finals: finals,
                }),
                "T{turn} 下次発表会门槛"
            );
        }
        // 合宿窗口总长
        game.plan.camp_year = Some(0);
        assert_eq!(game.view().camp.total, Some(4), "Junior 合宿窗口 4 回合");
        game.plan.camp_year = Some(2);
        assert_eq!(game.view().camp.total, Some(4), "Senior 合宿窗口 4 回合");
        game.plan.camp_year = None;
        assert_eq!(game.view().camp.total, None);
        // 自选赛：升序、全在当前回合之后、等级 > 0、且不是目标赛回合
        game.turn = 0;
        let view = game.view();
        assert!(view.schedule.self_races.iter().all(|race| {
            race.turn >= 0 && race.grade > 0 && !game.uma().is_race_turn(race.turn)
        }));
        assert!(
            view.schedule
                .self_races
                .windows(2)
                .all(|pair| pair[0].turn < pair[1].turn),
            "自选赛窗口必须升序"
        );
        assert!(
            view.schedule
                .goal_races
                .iter()
                .all(|turn| game.uma().is_race_turn(*turn)),
            "目标赛列表只含目标赛回合"
        );
    }

    /// 年次边界
    #[test]
    fn year_boundaries() {
        assert_eq!(MatsuriYear::from_turn(0), MatsuriYear::Junior);
        assert_eq!(MatsuriYear::from_turn(23), MatsuriYear::Junior);
        assert_eq!(MatsuriYear::from_turn(24), MatsuriYear::Classic);
        assert_eq!(MatsuriYear::from_turn(47), MatsuriYear::Classic);
        assert_eq!(MatsuriYear::from_turn(48), MatsuriYear::Senior);
        assert_eq!(MatsuriYear::from_turn(77), MatsuriYear::Senior);
    }

    /// 事件记录滚动保留最近 N 条、序号单调
    #[test]
    fn event_log_keeps_recent_and_monotonic_seq() {
        let mut game = crate::testing::sample_game();
        for id in 0..(EVENT_LOG_CAPACITY as u32 + 3) {
            game.push_event_log(EventRecord {
                seq: 0,
                turn: 0,
                id,
                name: format!("事件{id}"),
                choice: 0,
                detail: None,
            });
        }
        assert_eq!(game.event_log.len(), EVENT_LOG_CAPACITY);
        assert_eq!(game.event_log.last().map(|record| record.id), Some(34));
        assert_eq!(
            game.event_log.last().map(|record| record.seq),
            Some((EVENT_LOG_CAPACITY as u64) + 3)
        );
    }

    /// 快照可序列化往返（宿主传输格式由剧本提供）
    #[test]
    fn view_roundtrip() {
        let mut game = crate::testing::sample_game();
        game.turn = 12;
        let view = game.view();
        let text = serde_json::to_string(&view).expect("序列化 View 失败");
        let back: MatsuriView = serde_json::from_str(&text).expect("反序列化 View 失败");
        assert_eq!(back, view);
    }
}
