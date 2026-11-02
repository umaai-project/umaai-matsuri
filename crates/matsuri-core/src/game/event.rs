//! 事件管线（S6）：回合开始事件链 / 合宿抑制 / 随机分布 / 友人链 / 事件扩展
//!
//! 事件来源分四层：① 引擎通用固定事件（`data/events.json` `story_events`）；② 剧本固定事件
//! （`scenario_matsuri.json` `scenario_events`）；③ 随机分布（支援卡连续事件 / 马娘事件 /
//! 掉心情，分布与数值按引擎 `constants.json` + `events.json` 原始值）；④ 友人链（首次点击 /
//! 训练后点击 / 出行解锁 / 正月 / 结局，数据为吾妻 `809052xxx`）。
//!
//! 纪律：事件随机走事件流（`RuleStreams.event`）、选项走决策流、结算走事件流；本模块只做
//! **生成与剧本侧结算**，通用数值入账仍走引擎 `BaseGame::apply_event`。
//! 事件抑制 = 合宿规则层（剧本自持）+ 引擎 `no_event_turns` 基线层，合并为单一判定。

use anyhow::{Result, anyhow};
use rand::Rng;
use rand::seq::IndexedRandom;
use umaai_core::gamedata::{ActionValue, EventData};
use umaai_core::{Data, Ill};
use umasim::game::{FriendOutState, Game, Person, sample_bucket};

use super::{MatsuriGame, PersonOrigin};

/// 研鑽「追加の稽古」事件 ID（`scenario_events` 内定义）
pub(crate) const KENSAN_EVENT_ID: u32 = 400015106;
/// 友人（吾妻）出行解锁事件 ID（`809052004` 求むるは、新たな書）
pub(crate) const FRIEND_OUT_EVENT_ID: u32 = 809052004;

/// 赛后奖励事件 id（生涯 4001 / 自选 G1–OP 4101–4104 / URA 决赛 4105–4107）
///
/// 这些事件的五维收益吃 **1200 软上限**（见 [`softcap`](super::softcap)）；
/// 脚本事件的属性收益**不在**已实证范围内 ⇒ 不套上限。
pub(crate) const RACE_REWARD_EVENT_IDS: [u32; 8] = [4001, 4101, 4102, 4103, 4104, 4105, 4106, 4107];

/// 是否赛后奖励事件（吃属性软上限）
pub(crate) fn is_race_reward_event(id: u32) -> bool {
    RACE_REWARD_EVENT_IDS.contains(&id)
}

impl MatsuriGame {
    /// 回合开始事件链（引擎契约 `Game::generate_events` 的剧本实现）
    ///
    /// 顺序：固定事件（引擎通用 + 剧本）→ 合宿抑制判定 → 友人出行解锁判定 → 随机分布。
    /// 固定事件存在时不抽随机事件（引擎基础局口径）。
    pub(crate) fn begin_events(&self, rng: &mut impl Rng) -> Vec<EventData> {
        let Ok(data) = Data::get() else {
            return vec![];
        };
        // ① 固定回合事件：引擎通用 + 剧本自有
        let mut fixed = self.list_turn_events(&data.events.story_events);
        fixed.extend(self.list_turn_events(&self.data.scenario_events));
        if !fixed.is_empty() {
            return fixed;
        }
        // ② 抑制判定（合宿规则 + 引擎基线）
        if self.random_events_suppressed() {
            return vec![];
        }
        // ③ 友人出行解锁：首次点击后（BeforeUnlock）按羁绊高低取概率，命中后本回合只发生这一件
        if self.base.friend.out_state == FriendOutState::BeforeUnlock {
            let friendship = self
                .friend_person_index()
                .map_or(0, |index| self.persons[index].friendship());
            let key = if friendship < 60 {
                "friend_unlock_low"
            } else {
                "friend_unlock_high"
            };
            if rng.random_bool(data.event_prob(key).unwrap_or(0.0))
                && let Some(event) = data.events.friend_events.get("out")
            {
                return vec![event.clone()];
            }
        }
        // ④ 一般随机事件分布（支援卡连续 / 马娘 / 掉心情）
        let Ok(bucket) = sample_bucket(&data.consts.get_event_distribution(), rng) else {
            return vec![];
        };
        match bucket {
            0 => {
                // 支援卡连续事件：只取可用 **支援卡** 人头（友人 / 团队不计入连续事件）
                let candidates = self.card_event_candidates();
                let Some(&person_index) = candidates.choose(rng) else {
                    return vec![];
                };
                self.base
                    .generate_card_event(person_index as i32, rng)
                    .ok()
                    .flatten()
                    .into_iter()
                    .collect()
            }
            1 => self
                .base
                .random_select_event(&data.events.uma_events, rng)
                .into_iter()
                .collect(),
            2 => {
                // 掉心情只在第 12 回合之后进入分布
                if self.turn() >= 12 {
                    data.system_event("drop_motivation")
                        .ok()
                        .cloned()
                        .into_iter()
                        .collect()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// 随机事件是否抑制：合宿规则层优先，其次引擎 `no_event_turns` 基线层
    ///
    /// 合宿规则（user 2026-10-30）：第 1 年合宿两类随机事件都发生；第 2/3 年常规合宿回合
    /// 两类都不发生；延长段只抑制随机事件、固定事件不经过本判定。引擎基线层为 STUB，
    /// 等剧本专属禁事件回合表出来只替换第二层。
    pub(crate) fn random_events_suppressed(&self) -> bool {
        if self.plan.camp_zone && self.plan.camp_year.is_some_and(|year| year >= 1) {
            return true;
        }
        Data::get()
            .map(|data| data.consts.no_event_turns.contains(&self.turn()))
            .unwrap_or(true)
    }

    /// 回合开始的友人固定事件：正月（turn 24，已解锁时）加入本回合事件队列
    pub(crate) fn append_begin_friend_event(&mut self, events: &mut Vec<EventData>) {
        if self.base.friend.out_state != FriendOutState::AfterUnlock {
            return;
        }
        if self.turn() == 24
            && let Ok(data) = Data::get()
            && let Some(event) = data.events.friend_events.get("newyear")
        {
            events.push(event.clone());
        }
    }

    /// 回合开始的「回合后结算」事件入队（友人结局 / 育成结束 / 抽签；引擎基础局同型）
    pub(crate) fn queue_after_train_events(&mut self) {
        let Ok(data) = Data::get() else {
            return;
        };
        let turn = self.turn();
        if turn == 77 {
            if self.base.friend.out_state == FriendOutState::AfterUnlock
                && let Some(event) = data.events.friend_events.get("end")
            {
                self.unresolved_events.push(event.clone());
            }
            if let Ok(event) = data.system_event("ending") {
                self.unresolved_events.push(event.clone());
            }
        }
        if turn == 48
            && let Ok(event) = data.system_event("ticket")
        {
            self.unresolved_events.push(event.clone());
        }
    }

    /// 疲劳回复（剧本自持 flag）：体力 ≤49 时一次性 +15
    pub(crate) fn recover_fatigue(&mut self) {
        if !self.fatigue_recovery || self.uma().vital > 49 {
            return;
        }
        self.fatigue_recovery = false;
        self.uma_mut().add_value(&ActionValue {
            vital: 15,
            ..Default::default()
        });
    }

    /// 【变胖】回合开始判定：按剧本规则概率解除
    pub(crate) fn roll_fat_recovery(&mut self, rng: &mut impl Rng) {
        if !self.is_fat() {
            return;
        }
        let prob = self.data.rules.fat_recovery_prob;
        if prob > 0 && rng.random_bool(f64::from(prob) / 100.0) {
            self.uma_mut().flags.ill.remove(Ill::FAT);
        }
    }

    /// 【变胖】是否处于变胖状态（训练 / 排练的速度增量归零）
    pub(crate) fn is_fat(&self) -> bool {
        self.uma().flags.ill.contains(Ill::FAT)
    }

    /// 事件额外效果结算（引擎 `ActionValue` 之外的部分，按 `(event_id, choice)` 定位）
    pub(crate) fn apply_event_extra(&mut self, event_id: u32, choice: usize) {
        let Some(extra) = self
            .data
            .event_extras
            .iter()
            .find(|extra| extra.event_id == event_id && extra.choice == choice)
            .cloned()
        else {
            return;
        };
        if extra.act_gauge_lowest > 0 {
            self.gain_lowest_act_gauge(extra.act_gauge_lowest);
        }
        if extra.clear_debuff {
            self.clear_one_debuff();
        }
    }

    /// 解除一个 debuff（优先不擅长训练，其次具名生病位）
    fn clear_one_debuff(&mut self) {
        let flags = &mut self.uma_mut().flags;
        if flags.bad_trainer {
            flags.bad_trainer = false;
            flags.ill.remove(Ill::BAD_TRAINER);
            return;
        }
        for kind in [Ill::FAT, Ill::SKIN, Ill::HEADACHE, Ill::LAZY, Ill::NIGHT] {
            if flags.ill.contains(kind) {
                flags.ill.remove(kind);
                return;
            }
        }
    }

    /// 研鑽「追加の稽古」：推入剧本事件（`400015106`，训练员选择上 / 下档）
    pub(crate) fn trigger_kensan_event(&mut self, train: usize) -> Result<()> {
        let event = self
            .data
            .scenario_events
            .iter()
            .find(|event| event.id == KENSAN_EVENT_ID)
            .cloned()
            .ok_or_else(|| anyhow!("剧本事件缺行: 追加の稽古 {KENSAN_EVENT_ID}"))?;
        let mut event = event;
        // 上档的理事长羁绊 +5 归到理事长人头（引擎按 `person_index` 定位事件归属）
        event.person_index = self
            .person_index_of_type(umasim::game::PersonType::Yayoi)
            .map(|index| index as i32);
        let _ = train;
        self.unresolved_events.push(event);
        Ok(())
    }

    /// turn 66（Senior 10 月前半行动后）进化技能记账：只记 `hint_level + 1`
    pub(crate) fn apply_evolution_hint(&mut self) {
        if self.turn() != 66 {
            return;
        }
        self.uma_mut().add_value(&ActionValue {
            hint_level: 1,
            ..Default::default()
        });
    }

    /// 支援卡事件人头候选：可用支援卡（`PersonOrigin::Card`）人头下标
    pub(crate) fn card_event_candidates(&self) -> Vec<usize> {
        (0..self.persons.len())
            .filter(|index| {
                self.persons[*index].origin == PersonOrigin::Card
                    && self.person_is_available(*index)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use umaai_core::gamedata::EventData;
    use umaai_core::rng::EventRng;
    use umaai_core::{Data, Ill, UmaFlags};
    use umasim::game::{FriendOutState, Game};

    use super::*;
    use crate::testing::sample_game;

    /// 合宿抑制：第 1 年允许；第 2/3 年常规与延长段都抑制；非合宿按引擎基线
    #[test]
    fn camp_rule_suppresses_random_events() {
        let mut game = sample_game();
        game.plan.camp_zone = true;
        game.plan.camp_year = Some(0);
        game.base.turn = 12;
        assert!(
            !game.random_events_suppressed(),
            "第 1 年合宿应允许随机事件"
        );

        game.plan.camp_year = Some(1);
        game.base.turn = 36;
        assert!(game.random_events_suppressed(), "第 2 年合宿应抑制随机事件");

        game.plan.camp_year = Some(2);
        game.plan.camp_extended = true;
        game.base.turn = 64;
        assert!(
            game.random_events_suppressed(),
            "第 3 年延长段仍抑制随机事件"
        );

        game.plan.camp_zone = false;
        game.plan.camp_year = None;
        game.plan.camp_extended = false;
        game.base.turn = 40;
        assert!(
            !game.random_events_suppressed(),
            "非合宿回合按引擎基线（turn 40 不在 no_event_turns）"
        );
    }

    /// 固定事件存在时不抽随机事件；剧本固定事件按回合命中
    #[test]
    fn fixed_scenario_event_preempts_random() {
        let mut game = sample_game();
        game.base.turn = 11;
        let mut rng = StdRng::seed_from_u64(1);
        let events = game.begin_events(&mut rng);
        assert_eq!(events.len(), 1, "turn 11 应只有固定事件");
        assert_eq!(events[0].id, 400015105, "教え見直し事件");

        game.base.turn = 12;
        let events = game.begin_events(&mut rng);
        assert!(
            events.iter().all(|event| event.id != 400015105),
            "非固定回合不应再出现固定事件"
        );
    }

    /// 友人解锁：BeforeUnlock 时按概率抽 `809052004`；解锁后不再抽
    #[test]
    fn friend_unlock_event_by_probability() {
        let mut game = sample_game();
        game.base.turn = 5;
        game.base.friend.out_state = FriendOutState::BeforeUnlock;
        let mut hit = 0;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let events = game.begin_events(&mut rng);
            if events.iter().any(|event| event.id == FRIEND_OUT_EVENT_ID) {
                hit += 1;
            }
        }
        assert!(hit > 0, "200 个种子应至少命中一次出行解锁事件");
        // 解锁后不再抽
        game.base.friend.out_state = FriendOutState::AfterUnlock;
        for seed in 0..50_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let events = game.begin_events(&mut rng);
            assert!(
                events.iter().all(|event| event.id != FRIEND_OUT_EVENT_ID),
                "已解锁后不应再抽出行解锁事件"
            );
        }
    }

    /// 支援卡连续事件候选只含支援卡人头（友人卡不计入）
    #[test]
    fn card_event_candidates_exclude_friend() {
        let game = sample_game();
        let candidates = game.card_event_candidates();
        assert!(!candidates.is_empty(), "应有可用支援卡人头");
        for index in candidates {
            assert_eq!(
                game.persons[index].origin,
                PersonOrigin::Card,
                "候选只应含支援卡人头"
            );
        }
    }

    /// 出行解锁事件：`AfterUnlock` + 疲劳回复 flag；疲劳回复只触发一次
    #[test]
    fn friend_out_unlock_and_fatigue_recovery() {
        let mut game = sample_game();
        assert_eq!(game.base.friend.out_state, FriendOutState::UnClicked);
        let event = Data::get().expect("数据已初始化").events.friend_events["out"].clone();
        let mut rng = StdRng::seed_from_u64(2);
        game.apply_event(&event, 0, &mut rng)
            .expect("结算出行解锁事件");
        assert_eq!(game.base.friend.out_state, FriendOutState::AfterUnlock);
        assert!(game.fatigue_recovery, "解锁出行应获得疲劳回复 flag");
        assert_eq!(game.uma().vital, game.uma().max_vital, "上档 +25 体");

        game.uma_mut().vital = 40;
        game.recover_fatigue();
        assert_eq!(game.uma().vital, 55, "疲劳回复 +15");
        assert!(!game.fatigue_recovery);
        game.recover_fatigue();
        assert_eq!(game.uma().vital, 55, "疲劳回复只回一次");
    }

    /// 事件 `add_flags` 由剧本侧补齐（引擎 `BaseGame::apply_event` 不结算）
    #[test]
    fn event_flags_are_applied() {
        let mut game = sample_game();
        let mut event = EventData::from_action_value(9901, "测试-变胖", &Default::default());
        event.choices[0][0].add_flags = Some(UmaFlags {
            ill: Ill::FAT,
            ..Default::default()
        });
        let mut rng = StdRng::seed_from_u64(3);
        game.apply_event(&event, 0, &mut rng).expect("结算事件");
        assert!(game.is_fat(), "变胖状态应被写入");
    }

    /// 变胖：回合开始按剧本概率解除（20%）
    #[test]
    fn fat_recovery_roll_by_rules_probability() {
        let mut game = sample_game();
        game.uma_mut().flags.ill.insert(Ill::FAT);
        let mut recovered = 0;
        for seed in 0..100_u64 {
            let mut g = game.clone();
            g.uma_mut().flags.ill.insert(Ill::FAT);
            let mut rng = StdRng::seed_from_u64(seed);
            g.roll_fat_recovery(&mut rng);
            if !g.is_fat() {
                recovered += 1;
            }
        }
        let prob = game.data.rules.fat_recovery_prob;
        let expect = f64::from(prob);
        let got = recovered as f64;
        assert!(
            (got - expect).abs() <= 15.0,
            "100 个种子的解除次数 {got} 应接近规则概率 {expect}"
        );
    }

    /// 事件额外效果：最低演目 gauge +1、解一个 debuff
    #[test]
    fn event_extra_gauge_and_debuff() {
        let mut game = sample_game();
        game.base.turn = crate::SCENARIO_START_TURN;
        game.acts.gauge = [0; 5];
        game.acts.level = [3, 3, 1, 3, 3];
        game.uma_mut().flags.bad_trainer = true;
        game.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
        game.apply_event_extra(KENSAN_EVENT_ID, 0);
        assert_eq!(game.acts.gauge[2], 1, "最低演目（act2）gauge +1");
        assert!(!game.uma().flags.bad_trainer, "解一个 debuff：不擅长训练");
        assert!(!game.uma().flags.ill.contains(Ill::BAD_TRAINER));
    }

    /// 追加稽古上档事件完整结算：SP+20 / 直前训练 +20 / 体力-5 / 理事长羁绊 +5 / gauge+1 / 解 debuff
    #[test]
    fn kensan_upper_event_applies_all_effects() {
        let mut game = sample_game();
        game.base.turn = 20;
        game.advice.acquired = true;
        game.advice.slots = [201, 101, 301, 401, 501];
        game.advice.fused = [701, 0, 0, 0, 0];
        game.acts.level = [4; 5];
        game.acts.gauge = [0; 5];
        let person = game
            .persons
            .iter()
            .position(|person| person.act == Some(0))
            .expect("act0 担当");
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        game.kensan_pending = true;
        game.try_kensan(0).expect("推入追加稽古事件");
        let event = game.unresolved_events.pop().expect("事件应已入队");
        assert_eq!(event.id, KENSAN_EVENT_ID);
        game.base.set_last_train(0);
        game.uma_mut().flags.bad_trainer = true;
        game.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
        let yayoi = game
            .person_index_of_type(umasim::game::PersonType::Yayoi)
            .expect("理事长人头");
        let yayoi_friendship = game.persons[yayoi].friendship();
        let sp = game.uma().skill_pt;
        let vital = game.uma().vital;
        let speed = game.uma().five_status[0];

        let trainer = crate::MatsuriTrainer::new(crate::MatsuriConfig::default());
        let mut decision = StdRng::seed_from_u64(4);
        let mut event_rng = EventRng::new(7);
        game.run_event(&event, &trainer, &mut decision, &mut event_rng)
            .expect("结算追加稽古事件");
        assert_eq!(game.uma().skill_pt, sp + 20, "上档 SP+20");
        assert_eq!(game.uma().vital, vital - 5, "上档体力 -5");
        assert_eq!(game.uma().five_status[0], speed + 20, "直前训练属性 +20");
        assert_eq!(
            game.persons[yayoi].friendship(),
            yayoi_friendship + 5,
            "理事长羁绊 +5"
        );
        assert_eq!(game.acts.gauge.iter().sum::<i32>(), 1, "最低演目 gauge +1");
        assert!(!game.uma().flags.bad_trainer, "解一个 debuff");
    }

    /// 事件结算后写入事件记录（id / 名称 / 选项 / 实际到账）；View 快照同步
    #[test]
    fn event_records_are_logged() {
        let mut game = sample_game();
        let event = Data::get().expect("数据已初始化").events.friend_events["click"].clone();
        let mut rng = StdRng::seed_from_u64(9);
        game.apply_event(&event, 0, &mut rng)
            .expect("结算训练后事件");
        let record = game.event_log.last().expect("事件应入记录");
        assert_eq!(record.id, event.id);
        assert_eq!(record.name, event.name);
        assert_eq!(record.turn, game.turn());
        assert!(record.detail.is_some(), "事件应有到账摘要");
        assert_eq!(
            game.view().recent_events.last().map(|record| record.id),
            Some(event.id),
            "View 的最近事件应同步"
        );
        // 容量上限：连续追加不会无限增长
        for _ in 0..crate::game::EVENT_LOG_CAPACITY + 5 {
            game.push_event_log(crate::game::EventRecord {
                seq: 0,
                turn: 0,
                id: 0,
                name: String::new(),
                choice: 0,
                detail: None,
            });
        }
        assert_eq!(game.event_log.len(), crate::game::EVENT_LOG_CAPACITY);
    }

    /// turn 66 进化技能：只记 hint_level +1；其他回合不记
    #[test]
    fn evolution_hint_accounted_at_turn66() {
        let mut game = sample_game();
        let before = game.uma().total_hints;
        game.base.turn = 65;
        game.apply_evolution_hint();
        assert_eq!(game.uma().total_hints, before, "非 turn 66 不记");
        game.base.turn = 66;
        game.apply_evolution_hint();
        assert_eq!(
            game.uma().total_hints,
            before + 1,
            "turn 66 记 hint_level +1"
        );
    }

    /// 回合后事件入队：turn 48 抽签 / turn 77 育成结束 + 友人结局
    #[test]
    fn after_train_events_queued() {
        let mut game = sample_game();
        game.base.friend.out_state = FriendOutState::AfterUnlock;
        game.base.turn = 48;
        game.queue_after_train_events();
        assert!(
            game.unresolved_events.iter().any(|event| event.id == 4011),
            "turn 48 应入队新年抽签"
        );
        game.unresolved_events.clear();
        game.base.turn = 77;
        game.queue_after_train_events();
        let ids: Vec<u32> = game
            .unresolved_events
            .iter()
            .map(|event| event.id)
            .collect();
        assert!(ids.contains(&809052023), "turn 77 应入队友人结局: {ids:?}");
        assert!(ids.contains(&5011), "turn 77 应入队育成结束事件: {ids:?}");
    }
}
