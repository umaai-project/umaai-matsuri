//! 事件管线：回合开始事件链 / 合宿抑制 / 随机分布 / 友人链 / 事件扩展
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
use crate::diag;

/// 研鑽「追加の稽古」事件 ID（`scenario_events` 内定义）
pub(crate) const KENSAN_EVENT_ID: u32 = 400015106;
/// 友人（吾妻）出行解锁事件 ID（`809052004` 求むるは、新たな書）
pub(crate) const FRIEND_OUT_EVENT_ID: u32 = 809052004;

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
            diag!("【事件链】T{} 固定事件 {} 件", self.turn(), fixed.len());
            return fixed;
        }
        // ② 抑制判定（合宿规则 + 引擎基线）
        if self.random_events_suppressed() {
            diag!(
                "【事件链】T{} 随机事件抑制（合宿 / 引擎禁事件回合）",
                self.turn()
            );
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
                diag!("【事件链】T{} 友人出行解锁 #{}", self.turn(), event.id);
                return vec![event.clone()];
            }
        }
        // ④ 一般随机事件分布（支援卡连续 / 马娘 / 掉心情）
        let Ok(bucket) = sample_bucket(&data.consts.get_event_distribution(), rng) else {
            return vec![];
        };
        diag!("【事件链】T{} 随机事件桶 {bucket}", self.turn());
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
    /// 合宿规则：第 1 年合宿两类随机事件都发生；
    /// 第 2/3 年**常规**合宿回合两类都不发生；**延长段事件轴等同正常回合**——支援卡连续 /
    /// 普通马娘 / 掉心情等随机事件均可触发（仅训练全变稽古），故不经过合宿抑制层、只按引擎
    /// 基线判定。固定事件不经过本判定。引擎基线层为 STUB，等剧本专属禁事件回合表出来只替换第二层。
    pub(crate) fn random_events_suppressed(&self) -> bool {
        if self.plan.camp_zone
            && self.plan.camp_regular
            && self.plan.camp_year.is_some_and(|year| year >= 1)
        {
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
            diag!("【事件链】T{} 友人正月 #{}", self.turn(), event.id);
            events.push(event.clone());
        }
    }

    /// 回合开始的「回合后结算」事件入队（友人结局 / 育成结束 / 抽签；引擎基础局同型）
    pub(crate) fn queue_after_train_events(&mut self) {
        let Ok(data) = Data::get() else {
            return;
        };
        let turn = self.turn();
        let queued_before = self.unresolved_events.len();
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
        let queued = self.unresolved_events.len() - queued_before;
        if queued > 0 {
            diag!("【事件链】T{turn} 回合后事件入队 {queued} 件");
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
        diag!("【状态】T{} 疲劳回复 体力+15", self.turn());
    }

    /// 【变胖】回合开始判定：按剧本规则概率解除
    pub(crate) fn roll_fat_recovery(&mut self, rng: &mut impl Rng) {
        if !self.is_fat() {
            return;
        }
        let prob = self.data.rules.fat_recovery_prob;
        if prob > 0 && rng.random_bool(f64::from(prob) / 100.0) {
            self.uma_mut().flags.ill.remove(Ill::FAT);
            diag!("【状态】T{} 变胖解除（{}% 判定命中）", self.turn(), prob);
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
        let gauge_before = self.acts.gauge;
        if extra.act_gauge_lowest > 0 {
            self.gain_lowest_act_gauge(extra.act_gauge_lowest);
        }
        let cleared = if extra.clear_debuff {
            self.clear_one_debuff()
        } else {
            None
        };
        let mut parts: Vec<String> = Vec::new();
        if extra.act_gauge_lowest > 0 {
            for (act, (before, after)) in
                gauge_before.iter().zip(self.acts.gauge.iter()).enumerate()
            {
                if before != after {
                    parts.push(format!("演目{act}槽{before}→{after}"));
                }
            }
        }
        if let Some(name) = cleared {
            parts.push(format!("解除{name}"));
        }
        if !parts.is_empty() {
            diag!(
                "【事件附加】T{} 事件#{} 选项{} {}",
                self.turn(),
                event_id,
                choice,
                parts.join(" ")
            );
        }
    }

    /// 解除一个 debuff（优先不擅长训练，其次具名生病位）；返回被解除的状态名
    fn clear_one_debuff(&mut self) -> Option<&'static str> {
        let flags = &mut self.uma_mut().flags;
        if flags.bad_trainer {
            flags.bad_trainer = false;
            flags.ill.remove(Ill::BAD_TRAINER);
            return Some("不擅长训练");
        }
        for (kind, name) in [
            (Ill::FAT, "变胖"),
            (Ill::SKIN, "皮肤差"),
            (Ill::HEADACHE, "偏头痛"),
            (Ill::LAZY, "摸鱼"),
            (Ill::NIGHT, "熬夜"),
        ] {
            if flags.ill.contains(kind) {
                flags.ill.remove(kind);
                return Some(name);
            }
        }
        None
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
        diag!(
            "【追加稽古】T{} 训练#{} 推入事件 #{}",
            self.turn(),
            train,
            event.id
        );
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
        diag!("【状态】T{} 进化技能 hint_level+1", self.turn());
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

    /// 合宿抑制：第 1 年允许；第 2/3 年**常规**抑制；**延长段等同正常回合**；非合宿按引擎基线
    #[test]
    fn camp_rule_suppresses_random_events() {
        let mut game = sample_game();
        game.plan.camp_zone = true;
        game.plan.camp_regular = true;
        game.plan.camp_year = Some(0);
        game.base.turn = 12;
        assert!(
            !game.random_events_suppressed(),
            "第 1 年合宿应允许随机事件"
        );

        game.plan.camp_year = Some(1);
        game.base.turn = 36;
        assert!(
            game.random_events_suppressed(),
            "第 2 年常规合宿应抑制随机事件"
        );

        game.plan.camp_year = Some(2);
        game.plan.camp_regular = false;
        game.plan.camp_extended = true;
        game.base.turn = 64;
        assert!(
            !game.random_events_suppressed(),
            "第 3 年延长段按正常回合（不抑制随机事件）"
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

    /// 延长段事件轴 = 正常回合：随机分布可抽出支援卡连续事件等；常规合宿仍抑制
    #[test]
    fn extended_camp_runs_normal_event_axis() {
        let mut game = sample_game();
        game.base.friend.out_state = FriendOutState::AfterUnlock;
        game.plan.camp_zone = true;
        game.plan.camp_year = Some(1);
        game.plan.camp_regular = false;
        game.plan.camp_extended = true;
        game.base.turn = 40;

        let mut non_empty = 0;
        let mut card = 0;
        for seed in 0..200_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let events = game.begin_events(&mut rng);
            if !events.is_empty() {
                non_empty += 1;
            }
            if events.iter().any(|event| (8001..=8003).contains(&event.id)) {
                card += 1;
            }
        }
        assert!(
            non_empty > 0,
            "延长段应能触发随机事件（实际 {non_empty}/200）"
        );
        assert!(card > 0, "延长段应能触发支援卡连续事件（实际 {card}/200）");

        game.plan.camp_regular = true;
        game.plan.camp_extended = false;
        game.base.turn = 39;
        for seed in 0..50_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            assert!(
                game.begin_events(&mut rng).is_empty(),
                "第 2 年常规合宿回合仍应抑制随机事件"
            );
        }
    }

    /// 固定事件存在时不抽随机事件；剧本固定事件按回合命中
    #[test]
    fn fixed_scenario_event_preempts_random() {
        let mut game = sample_game();
        game.base.turn = 11;
        let mut rng = StdRng::seed_from_u64(1);
        let events = game.begin_events(&mut rng);
        assert_eq!(events.len(), 1, "turn 11 应只有固定事件");
        assert_eq!(events[0].id, 400015104, "駿大祭合宿の申し出事件");

        game.base.turn = 12;
        let events = game.begin_events(&mut rng);
        assert!(
            events.iter().all(|event| event.id != 400015104),
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

    /// 事件 `hint_level` 收益必须计入 `total_hints`（通用事件；URA 事件本身无 Hint）
    #[test]
    fn event_hint_level_is_counted() {
        let mut game = sample_game();
        // 友人正月事件在 turn 24 真实发生，hint_level = 3
        game.turn = 24;
        let event = Data::get()
            .expect("静态数据")
            .events
            .friend_events
            .get("newyear")
            .cloned()
            .expect("友人正月事件");
        let before = game.uma().total_hints;
        let mut rng = StdRng::seed_from_u64(11);
        game.apply_event(&event, 0, &mut rng).expect("事件结算");
        assert_eq!(
            game.uma().total_hints,
            before + 3,
            "事件 hint_level 应计入 total_hints"
        );
        assert_eq!(
            game.view().uma.total_hints,
            before + 3,
            "View 的灵感合计同步"
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

    /// 幸运体质（生病盾）：事件 `add_flags` 的生病位（含变胖）被挡下且盾扣一层；非生病状态照常生效
    #[test]
    fn lucky_body_blocks_event_illness_including_fat() {
        let mut game = sample_game();
        game.uma_mut().flags.lucky_count = 1;
        let mut event = EventData::from_action_value(9902, "测试-生病事件", &Default::default());
        event.choices[0][0].add_flags = Some(UmaFlags {
            ill: Ill::FAT,
            good_trainer: true,
            ..Default::default()
        });
        let mut rng = StdRng::seed_from_u64(3);
        game.apply_event(&event, 0, &mut rng).expect("结算事件");
        assert_eq!(game.uma().flags.lucky_count, 0, "盾应扣一层");
        assert!(
            !game.uma().flags.ill.contains(Ill::FAT),
            "变胖也是生病，应被盾挡下"
        );
        assert!(game.uma().flags.good_trainer, "非生病状态照常生效");
    }

    /// 幸运体质：`bad_trainer` 与 `Ill::BAD_TRAINER` 同置同清 → 盾一起拦（都不生效）
    #[test]
    fn lucky_body_blocks_bad_trainer_atomically() {
        let mut game = sample_game();
        game.uma_mut().flags.lucky_count = 1;
        let mut event = EventData::from_action_value(9903, "测试-不擅长事件", &Default::default());
        event.choices[0][0].add_flags = Some(UmaFlags {
            bad_trainer: true,
            ill: Ill::BAD_TRAINER,
            ..Default::default()
        });
        let mut rng = StdRng::seed_from_u64(3);
        game.apply_event(&event, 0, &mut rng).expect("结算事件");
        assert!(!game.uma().flags.bad_trainer, "盾应挡下 bad_trainer");
        assert!(
            !game.uma().flags.ill.contains(Ill::BAD_TRAINER),
            "盾应挡下 Ill::BAD_TRAINER"
        );
        assert_eq!(game.uma().flags.lucky_count, 0, "一次生病只扣一层");
    }

    /// 幸运体质：没有生病位的事件不消耗盾
    #[test]
    fn lucky_body_not_consumed_without_illness() {
        let mut game = sample_game();
        game.uma_mut().flags.lucky_count = 2;
        let mut event = EventData::from_action_value(9904, "测试-无生病事件", &Default::default());
        event.choices[0][0].add_flags = Some(UmaFlags {
            good_trainer: true,
            ..Default::default()
        });
        let mut rng = StdRng::seed_from_u64(3);
        game.apply_event(&event, 0, &mut rng).expect("结算事件");
        assert_eq!(game.uma().flags.lucky_count, 2, "无生病不消耗盾");
    }
}
