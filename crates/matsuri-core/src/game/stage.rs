//! 回合结构与阶段机（内部强类型；对外只投影 [`StageId`]）
//!
//! 纪律：**无数据、无引用、可 `Copy`**；中间状态（gauge / 合宿进度 / 教诲槽）放局面字段。
//! 阶段只表示回合内的**执行位置**；比赛 / 合宿 / 発表会 / 本番 / 空窗期 / URA 回合是回合类型或自动
//! 结算（由 [`TurnPlan`] 与局面字段表达），不各开一个阶段。比赛 / 稽古 / 出行是 `Train` 阶段
//! 里的动作（S4 落）；発表会 / 本番 / 合宿 +1 与教诲分配 / 调整都是 `End` 阶段的自动结算 /
//! 决策（S5 落，其中教诲在剧本结算之后处理）。

use anyhow::{Result, anyhow};
use rand::rngs::StdRng;
use umaai_core::rng::{StrategyRng, StreamTag, derive_seed};
use umasim::game::base::RuleStreams;
use umasim::game::{Game, StageId, Trainer};

use super::MatsuriGame;
use super::view::MatsuriYear;
use crate::data::{CampWindow, CheckPointSchedule};
use crate::diag;

/// URA 回合起始（0 基）：**URA 期 = turn 72–77**（与引擎同口径；73/75/77 为固定决赛）
///
/// 本番（最后一次発表会 T67）后先经过 **68–71 的空窗期**（剧本已结束、尚未进 URA），
/// 空窗期不属于 URA 回合：本番演出的训练限定效果不在空窗期生效。
pub const URA_START_TURN: i32 = 72;

/// 剧本阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// 回合开始（回合结构派生 / 事件链）
    #[default]
    Begin,
    /// 人头分配
    Distribute,
    /// 训练 / 比赛决策点
    Train,
    /// 回合后事件（训练后处理 / 失败事件）
    AfterTrain,
    /// 回合结算（合宿计数 / 発表会 / 本番）
    End,
}

impl Stage {
    /// 下一个阶段（`None` = 本回合走完）
    pub(crate) fn next(self) -> Option<Self> {
        match self {
            Self::Begin => Some(Self::Distribute),
            Self::Distribute => Some(Self::Train),
            Self::Train => Some(Self::AfterTrain),
            Self::AfterTrain => Some(Self::End),
            Self::End => None,
        }
    }
}

impl From<Stage> for StageId {
    /// 显式赋号：**号是稳定契约**，改号等于旧种子与基线失效；**只增不改**
    fn from(stage: Stage) -> Self {
        Self::new(match stage {
            Stage::Begin => 0,
            Stage::Train => 1,
            Stage::End => 2,
            Stage::Distribute => 3,
            Stage::AfterTrain => 4,
        })
    }
}

/// 单回合结构（回合开始时派生一次）
///
/// 「回合类型」全在这里：强制比赛 / 合宿（含延长段）/ 発表会 / 本番 / 空窗期 / URA 回合；阶段机
/// 不再按回合类型分叉。S4 / S5 / S6 从这里取本回合该走哪条分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TurnPlan {
    /// 本回合是否强制比赛（马娘固有目标赛事 / URA 决赛）
    pub race: bool,
    /// 本回合是否 URA 决赛（turn 73 预选 / 75 准决赛 / 77 决赛）
    pub ura_race: bool,
    /// 本回合是否 URA 回合（turn 72–77；73/75/77 为固定决赛）
    pub ura_period: bool,
    /// 本回合是否剧本结束后的空窗期（本番后、URA 前，turn 68–71；不属于 URA 回合）
    pub interlude: bool,
    /// 本回合合宿年次（0–2；`None` = 不在合宿期）
    pub camp_year: Option<usize>,
    /// 本回合是否在合宿区（含被比赛 / 强制休息占用的回合）
    pub camp_zone: bool,
    /// 本回合是否原窗口内合宿（非延长期）
    pub camp_regular: bool,
    /// 本回合是否延长期合宿（窗口外顺延的回合）
    pub camp_extended: bool,
    /// 本回合是否合宿版休息（Classic / Senior 原窗口；Junior 与延长段为普通休息）
    pub camp_rest: bool,
    /// 本回合是否合宿结束的全演目 Lv+1 结算点（turn 15 / 39 / 63）
    pub camp_level_up: bool,
    /// 本回合進捗発表会 / 本番场次（0–2；`None` = 非结算回合）
    pub check_point: Option<usize>,
    /// 本回合教诲可调整窗口（`data.advice_windows` 下标；`None` = 非窗口回合）
    pub advice_window: Option<usize>,
}

impl TurnPlan {
    /// 是否本番回合（第 3 次発表会）
    #[must_use]
    pub fn finals(&self) -> bool {
        self.check_point == Some(2)
    }

    /// 是否合宿排练回合（合宿区且未被比赛 / 强制休息占用；训练全变演目稽古）
    #[must_use]
    pub fn camp_train(&self, forced_rest: bool) -> bool {
        self.camp_zone && !self.race && !forced_rest
    }
}

/// 合宿进度（跨回合状态，回合边界推进）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CampState {
    /// 当前合宿年次（0–2；`None` = 不在合宿期）
    pub year: Option<usize>,
    /// 本年度已计入的合宿回合数（比赛 / 强制休息回合不计）
    pub counted: usize,
}

/// 马娘特殊事件记录（**临时**）：目标赛回合被固定事件打断 → 实际执行休息
///
/// 目前只登记凯斯奇迹 T65（0 基，`chara_id = uma_id / 100`）。该回合仍按**目标赛回合**
/// 分类（`TurnPlan::race` 为真），但实际动作是**退赛休息**；不产生比赛事件 / 奖励。
/// 等引擎「马娘特殊事件记录」数据结构落地后，这里改为读引擎数据（见 `docs/mechanism.md` §1）。
const FORCED_REST_TURNS: &[(u32, i32)] = &[(1093, 65)];

/// 本回合是否因马娘特殊事件改为强制休息（分类不变，只覆盖实际动作）
fn is_forced_rest_turn(chara_id: u32, turn: i32) -> bool {
    FORCED_REST_TURNS.contains(&(chara_id, turn))
}

/// 单回合合宿区信息（[`CampState::begin_turn`] 的返回值）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CampTurn {
    /// 合宿年次（0–2；`None` = 不在合宿期）
    pub(crate) year: Option<usize>,
    /// 是否合宿区（含被比赛 / 强制休息占用的回合）
    pub(crate) zone: bool,
    /// 是否原窗口内合宿（非延长期）
    pub(crate) regular: bool,
    /// 是否延长期合宿（窗口外顺延）
    pub(crate) extended: bool,
    /// 是否合宿版休息（Classic / Senior 原窗口）
    pub(crate) rest: bool,
}

impl CampState {
    /// 回合开始：进入合宿窗口或延续延长段，返回本回合合宿区信息
    pub(crate) fn begin_turn(&mut self, turn: i32, windows: &[CampWindow]) -> CampTurn {
        if self.year.is_none()
            && let Some(year) = camp_window_start(turn, windows)
        {
            self.year = Some(year);
            self.counted = 0;
        }
        let Some(year) = self.year else {
            return CampTurn::default();
        };
        let Some(window) = windows.get(year) else {
            return CampTurn::default();
        };
        let regular = turn <= window.end;
        CampTurn {
            year: Some(year),
            zone: true,
            regular,
            extended: !regular,
            rest: regular && year >= 1,
        }
    }

    /// 回合结束：比赛 / 强制休息回合不计入；计满窗口长度后本次合宿结束
    pub(crate) fn end_turn(&mut self, counted: bool, windows: &[CampWindow]) {
        let Some(year) = self.year else {
            return;
        };
        let Some(window) = windows.get(year) else {
            return;
        };
        if counted {
            self.counted = self.counted.saturating_add(1);
        }
        let required = (window.end - window.start + 1) as usize;
        if self.counted >= required {
            self.year = None;
            self.counted = 0;
        }
    }
}

/// 本回合是否某个合宿窗口的起始回合（返回年次 0–2）
pub(crate) fn camp_window_start(turn: i32, windows: &[CampWindow]) -> Option<usize> {
    windows.iter().position(|window| window.start == turn)
}

/// 進捗発表会 / 本番场次（0–2；`None` = 非结算回合）
pub(crate) fn check_point_index(turn: i32, schedule: &[CheckPointSchedule]) -> Option<usize> {
    schedule.iter().position(|item| item.turn == turn)
}

/// 教诲可调整窗口下标（`data.advice_windows` 下标；`None` = 非窗口回合）
pub(crate) fn advice_window_index(turn: i32, windows: &[i32]) -> Option<usize> {
    windows.iter().position(|item| *item == turn)
}

/// 剧本结束后的空窗期起点（本番 = 最后一次発表会之后的回合，0 基 68；缺资产时退化为不过渡）
pub(crate) fn post_scenario_start(schedule: &[CheckPointSchedule]) -> i32 {
    schedule
        .last()
        .map_or(crate::MAX_TURN + 1, |item| item.turn + 1)
}

/// 是否 URA 决赛回合（73 预选 / 75 准决赛 / 77 决赛）
pub(crate) fn is_ura_race_turn(turn: i32) -> bool {
    matches!(turn, 73 | 75 | 77)
}

impl MatsuriGame {
    /// 派生本局面自持的策略流（注入点与回合边界调用）
    pub(crate) fn reset_strategy_stream(&mut self) {
        let turn = self.base.turn as u64;
        self.strategy = self.base.master().map(|master| {
            StrategyRng::new(derive_seed(master, &[turn, StreamTag::Strategy.tag()]))
        });
    }

    /// 派生本回合结构（回合开始调用一次；合宿窗口 / 延长段在此推进）
    pub(crate) fn derive_plan(&mut self) -> TurnPlan {
        let turn = self.base.turn;
        let camp = self.camp.begin_turn(turn, &self.data.camp.windows);
        let post_scenario = post_scenario_start(&self.data.schedule);
        TurnPlan {
            race: self.base.uma.is_race_turn(turn),
            ura_race: is_ura_race_turn(turn),
            ura_period: turn >= URA_START_TURN,
            interlude: turn >= post_scenario && turn < URA_START_TURN,
            camp_year: camp.year,
            camp_zone: camp.zone,
            camp_regular: camp.regular,
            camp_extended: camp.extended,
            camp_rest: camp.rest,
            camp_level_up: self.data.camp.level_up_turns.contains(&turn),
            check_point: check_point_index(turn, &self.data.schedule),
            advice_window: advice_window_index(turn, &self.data.advice_windows),
        }
    }

    /// 单阶段推进
    pub(crate) fn stage_body<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
        streams: &mut RuleStreams,
        strategy: &mut StrategyRng,
    ) -> Result<()> {
        match self.stage {
            Stage::Begin => {
                self.plan = self.derive_plan();
                // 目标赛回合仍按赛程分类（`plan.race`），马娘特殊事件可把本回合实际动作改为休息；
                // 目前只登记凯斯奇迹 T65（临时表，引擎「马娘特殊事件记录」改造后迁移）。
                self.forced_rest = is_forced_rest_turn(self.base.uma.chara_id(), self.base.turn);
                diag!(
                    "【回合】T{} Begin {} 类型: {}",
                    self.base.turn,
                    MatsuriYear::from_turn(self.base.turn).as_str(),
                    self.diag_turn_kind()
                );
                // 回合开始的状态判定：友人疲劳回复（一次性）、变胖解除概率（剧本规则）
                self.recover_fatigue();
                self.roll_fat_recovery(strategy);
                // 回合开始事件链：固定事件 + 友人解锁 + 随机分布（事件流）；正月事件随本回合跑，
                // 结局 / 抽签 / 育成结束入队到 `AfterTrain`（引擎基础局同型）。
                let mut events = self.generate_events(&mut streams.event);
                self.append_begin_friend_event(&mut events);
                self.queue_after_train_events();
                for event in &events {
                    self.run_event(event, trainer, decision, &mut streams.event)?;
                }
                // turn 2：NPC / 友人加入（本回合起可用）后立即授予 5 个基础教诲；
                // 教诲效果须从本回合的分布 / 训练起生效（memo §4）。
                if self.base.turn == crate::SCENARIO_START_TURN {
                    self.acquire_advice();
                    // S7：首次分配也交给 Trainer（合并 120 候选；确定性、不耗结算随机）
                    self.decide_advice_assignment(trainer, decision)?;
                }
                // 研鑽「追加の稽古」：回合开始用策略流预掷（掷中后未点对应训练则作废）
                self.roll_kensan(strategy);
            }
            Stage::Distribute => {
                // 真实人头分布（亮起锁担当 / NPC 不在率 / 吾妻补位 / 分身）+ 灵感图标
                self.distribute_all(&mut streams.turn_fixed)?;
                self.distribute_hint(&mut streams.turn_fixed)?;
            }
            Stage::Train => {
                let actions = self.list_actions()?;
                let selection = trainer.select_action(self, &actions, decision)?;
                let action = actions.get(selection).ok_or_else(|| {
                    anyhow!(
                        "训练员返回的动作下标越界: {selection}（候选 {} 个）",
                        actions.len()
                    )
                })?;
                // 动作结算走**策略流**：与决策抽签分开，改决策流消耗不会位移结算随机
                self.apply_action(action, strategy)?;
            }
            Stage::AfterTrain => {
                // 本回合动作与训练后处理挂出的事件统一在这里结算：
                // 比赛奖励 / 出行 / 红点 / 友人点击 / 加练 / 失败事件等。
                let after_events = std::mem::take(&mut self.unresolved_events);
                for event in &after_events {
                    self.run_event(event, trainer, decision, &mut streams.event)?;
                }
            }
            Stage::End => {
                // 合宿计数：比赛 / 强制休息回合不计入，计满窗口长度后本次合宿结束。
                let counted = self.plan.camp_zone && !self.plan.race && !self.forced_rest;
                let camp_before = self.camp.counted;
                self.camp.end_turn(counted, &self.data.camp.windows);
                if self.plan.camp_zone {
                    diag!(
                        "【合宿】T{} End 计入={} 已计入 {}→{}",
                        self.base.turn,
                        counted,
                        camp_before,
                        self.camp.counted
                    );
                }
                // 剧本结算：発表会 / 本番 → 合宿结束全演目 Lv+1 → 自选比赛区间判定
                self.settle_turn_end()?;
                // End 阶段挂出的事件（自选比赛达标的生涯比赛胜利奖励）在本回合内结算
                let end_events = std::mem::take(&mut self.unresolved_events);
                for event in &end_events {
                    self.run_event(event, trainer, decision, &mut streams.event)?;
                }
                // 教诲窗口：先应用融合（配置预设），再把 120 个分配候选交给 Trainer（S7）
                self.settle_advice_window(trainer, decision)?;
                // turn 66（Senior 10 月前半行动后）进化技能：只记 hint_level + 1
                self.apply_evolution_hint();
                // 研鑽「追加の稽古」：本回合掷中但未点到含绑定演目担当的训练 → 作废
                if self.kensan_pending {
                    diag!(
                        "【追加稽古】T{} 掷中未兑现（未选择追加稽古训练）",
                        self.base.turn
                    );
                    self.kensan_pending = false;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仓内资产的三个合宿窗口（12–15 / 36–39 / 60–63）
    fn camp_windows() -> Vec<CampWindow> {
        vec![
            CampWindow { start: 12, end: 15 },
            CampWindow { start: 36, end: 39 },
            CampWindow { start: 60, end: 63 },
        ]
    }

    /// 从 turn 0 逐回合推进到 `last_turn`，返回每回合的合宿区信息
    fn drive_camp(last_turn: i32, occupied: impl Fn(i32) -> bool) -> Vec<CampTurn> {
        let windows = camp_windows();
        let mut camp = CampState::default();
        let mut out = Vec::new();
        for turn in 0..=last_turn {
            let info = camp.begin_turn(turn, &windows);
            camp.end_turn(info.zone && !occupied(turn), &windows);
            out.push(info);
        }
        out
    }

    #[test]
    fn next_chain() {
        assert_eq!(Stage::Begin.next(), Some(Stage::Distribute));
        assert_eq!(Stage::Distribute.next(), Some(Stage::Train));
        assert_eq!(Stage::Train.next(), Some(Stage::AfterTrain));
        assert_eq!(Stage::AfterTrain.next(), Some(Stage::End));
        assert_eq!(Stage::End.next(), None);
    }

    #[test]
    fn stage_ids_are_stable() {
        assert_eq!(StageId::from(Stage::Begin), StageId::new(0));
        assert_eq!(StageId::from(Stage::Train), StageId::new(1));
        assert_eq!(StageId::from(Stage::End), StageId::new(2));
        assert_eq!(StageId::from(Stage::Distribute), StageId::new(3));
        assert_eq!(StageId::from(Stage::AfterTrain), StageId::new(4));
    }

    #[test]
    fn camp_starts_only_at_window_start() {
        let windows = camp_windows();
        assert_eq!(camp_window_start(12, &windows), Some(0));
        assert_eq!(camp_window_start(13, &windows), None);
        assert_eq!(camp_window_start(36, &windows), Some(1));
        assert_eq!(camp_window_start(60, &windows), Some(2));
    }

    #[test]
    fn camp_basic_window() {
        let turns = drive_camp(40, |_| false);
        for turn in 12..=15 {
            let info = turns[turn as usize];
            assert_eq!(info.year, Some(0), "turn {turn} 应属第 1 次合宿");
            assert!(info.zone && info.regular && !info.extended);
            assert!(!info.rest, "Junior 合宿休息为普通休息");
        }
        assert!(!turns[16].zone, "基础 4 回合结束后不再算合宿");
        for turn in 36..=39 {
            let info = turns[turn as usize];
            assert_eq!(info.year, Some(1), "turn {turn} 应属第 2 次合宿");
            assert!(
                info.zone && info.regular && info.rest,
                "Classic 合宿为合宿版休息"
            );
        }
        assert!(!turns[40].zone);
    }

    #[test]
    fn camp_race_turn_extends() {
        // 合宿窗口内目标比赛（turn 13）不计入 4 个回合，合宿顺延到 turn 16
        let turns = drive_camp(17, |turn| turn == 13);
        for turn in 12..=16 {
            assert!(turns[turn as usize].zone, "turn {turn} 应仍在合宿区");
        }
        assert!(turns[16].extended && !turns[16].regular);
        assert!(!turns[17].zone);
    }

    #[test]
    fn camp_race_at_window_start_extends() {
        // 窗口起始回合就是比赛：turn 12 不计入，合宿顺延到 turn 16
        let turns = drive_camp(17, |turn| turn == 12);
        assert!(turns[12].zone && turns[12].regular);
        assert!(turns[16].zone && turns[16].extended);
        assert!(!turns[17].zone);
    }

    #[test]
    fn camp_forced_rest_turn_extends() {
        // 强制休息占用原窗口最后一回合（turn 15）：不计入，合宿顺延到 turn 16
        let turns = drive_camp(17, |turn| turn == 15);
        assert!(turns[15].zone && turns[15].regular);
        assert!(turns[16].zone && turns[16].extended);
        assert!(!turns[17].zone);
    }

    #[test]
    fn camp_consecutive_races_keep_extending() {
        // 窗口首两回合都是目标比赛：12/13 不计入，合宿顺延到 turn 17
        let turns = drive_camp(18, |turn| turn == 12 || turn == 13);
        for turn in 12..=17 {
            assert!(turns[turn as usize].zone, "turn {turn} 应仍在合宿区");
        }
        assert!(!turns[18].zone);
    }

    #[test]
    fn camp_rest_version_only_in_original_window() {
        // Senior 合宿被比赛占用一回合：原窗口仍是合宿版休息，延长期回到普通休息
        let turns = drive_camp(65, |turn| turn == 63);
        assert!(turns[60].zone && turns[60].regular && turns[60].rest);
        assert!(
            turns[63].zone && turns[63].regular,
            "被比赛占用的回合仍在原窗口内"
        );
        assert!(turns[64].zone && turns[64].extended && !turns[64].rest);
        assert!(!turns[65].zone);
    }

    #[test]
    fn check_point_index_matches_schedule() {
        let schedule = vec![
            CheckPointSchedule {
                turn: 23,
                need_act_num: 5,
                need_act_level: 3,
            },
            CheckPointSchedule {
                turn: 47,
                need_act_num: 5,
                need_act_level: 5,
            },
            CheckPointSchedule {
                turn: 67,
                need_act_num: 5,
                need_act_level: 6,
            },
        ];
        assert_eq!(check_point_index(23, &schedule), Some(0));
        assert_eq!(check_point_index(47, &schedule), Some(1));
        assert_eq!(check_point_index(67, &schedule), Some(2));
        assert_eq!(check_point_index(0, &schedule), None);
        assert_eq!(check_point_index(24, &schedule), None);
        assert_eq!(post_scenario_start(&schedule), 68);
    }

    #[test]
    fn advice_window_index_matches() {
        let windows = vec![2, 11, 23, 35, 47, 59];
        assert_eq!(advice_window_index(2, &windows), Some(0));
        assert_eq!(advice_window_index(23, &windows), Some(2));
        assert_eq!(advice_window_index(59, &windows), Some(5));
        assert_eq!(advice_window_index(3, &windows), None);
    }

    /// URA 边界：68–71 空窗期（不属于 URA），72–77 URA 回合；决赛回合 73/75/77
    #[test]
    fn ura_boundaries_and_interlude() {
        let schedule = vec![CheckPointSchedule {
            turn: 67,
            need_act_num: 5,
            need_act_level: 6,
        }];
        let post = post_scenario_start(&schedule);
        assert_eq!(post, 68, "空窗期自本番后一回合（68）起");
        // 把「是否空窗 / 是否 URA」按 derive_plan 同口径复算
        let classify = |turn: i32| {
            (
                turn >= post && turn < URA_START_TURN,
                turn >= URA_START_TURN,
            )
        };
        assert_eq!(classify(67), (false, false), "本番回合既非空窗也非 URA");
        assert_eq!(classify(68), (true, false), "68 空窗期");
        assert_eq!(classify(71), (true, false), "71 空窗期末");
        assert_eq!(classify(72), (false, true), "72 起进 URA");
        assert_eq!(classify(77), (false, true), "77 URA 末回合");
        assert_eq!(URA_START_TURN, 72);
    }

    #[test]
    fn ura_race_turns() {
        assert!(is_ura_race_turn(73));
        assert!(is_ura_race_turn(75));
        assert!(is_ura_race_turn(77));
        for turn in [72, 74, 76, 71, 0] {
            assert!(!is_ura_race_turn(turn), "turn {turn} 不是 URA 决赛");
        }
    }
}
