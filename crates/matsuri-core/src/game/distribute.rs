//! 人头分布：亮起锁担当 / 不在率 / 吾妻补位 / 分身
//!
//! 口径见 `docs/mechanism.md` §2.1 与 `docs/matsuri_memo_cn.md` §2.2 / §3.5 / §11.2：
//! - **所有亮起（槽满）演目**的担当卡与担当神驹锁定在担当训练位、**不判不在**；
//!   合宿期全部演目按排练处理 → 全员锁位；
//! - 普通卡按训练类型 + 得意率权重落位；神驹等权随机、只计人头；每位置 ≤5 人；
//!   同一人同一位置不重复、每个位置至多 1 名友人；
//! - 吾妻：普通回合按通用规则（不在率 100）；**排练回合必补 1 个未满排练位**（U16 不可分身）；
//! - 分身（協力 801 / 転成 101 合宿 / URA 本番演出）：同一担当在**另一个训练位**再加一处登场
//!   （不叠在排练位），逐占位计人数与槽。
//!
//! 随机走 `Distribute` 阶段的回合固定流（与引擎基础局同轴）；演目满槽锁位时按
//! 「未锁位时 distribute_person 会抽多少」补消费，避免后续人头抽取游标整体错位。

use anyhow::Result;
use rand::Rng;
use umaai_core::Data;
use umasim::game::{Game, Person, PersonType, sample_bucket};

use super::MatsuriGame;
use super::person::PersonOrigin;
use crate::diag;

/// 每个训练位的人数上限（引擎通用口径）
const TRAIN_CAP: usize = 5;

impl MatsuriGame {
    /// 本回合是否需要分布（比赛 / 强制休息回合不训练，不分布）
    pub(crate) fn needs_distribution(&self) -> bool {
        !self.plan.race && !self.forced_rest
    }

    /// 锁位分支补消费：按未锁位时 `Game::distribute_person` 的抽签次数推进共享随机流
    ///
    /// 只推进随机流、不改分布：让后续人头在「某演目满槽锁位」与「未满槽」两条 rollout
    /// 分支上抽到同一组随机数，恢复 CRN 配对；锁位本身仍由 `place_locked` 固定落位。
    fn burn_locked_person(&mut self, person_index: usize, rng: &mut impl Rng) -> Result<()> {
        let train_type = self.persons[person_index].train_type() as usize;
        let is_friend = self.persons[person_index].is_friend();
        let absent_rate = self.absent_weight(person_index);
        if absent_rate > 0 && rng.random_bool(absent_rate as f64 / (500 + absent_rate) as f64) {
            return Ok(());
        }
        let mut weights = [100, 100, 100, 100, 100];
        if train_type <= 4 {
            weights[train_type] += self.deyilv(person_index as i32) as i32;
        }
        let mut retries = 0;
        while retries < 10 {
            let train = sample_bucket(&weights, rng)?;
            retries += 1;
            // 与 distribute_person 的落位校验同口径；锁位分支只消费随机、不写入分布
            if self.base.distribution[train].len() >= TRAIN_CAP
                || self.base.distribution[train].contains(&(person_index as i32))
            {
                continue;
            }
            if is_friend
                && self.base.distribution[train]
                    .iter()
                    .any(|index| self.persons[*index as usize].is_friend())
            {
                continue;
            }
            break;
        }
        Ok(())
    }

    /// 本回合分布入口（`Distribute` 阶段调用）
    ///
    /// 顺序：锁位 →（有排练时）吾妻补位 → 其余人头 →（无排练时）吾妻通用落位 →
    /// 分身。吾妻先于其余人头补位，保证「有稽古就必定登场」；理事长 / 记者只落非稽古位。
    pub(crate) fn distribute_round(&mut self, rng: &mut impl Rng) -> Result<()> {
        self.reset_distribution();
        if !self.needs_distribution() {
            return Ok(());
        }
        let practice = self.practice_acts();
        let practice_train = self.practice_train_mask(&practice);
        self.place_locked(&practice);
        let practice_any = practice_train.iter().any(|lit| *lit);
        if practice_any {
            self.place_friend_filler(&practice_train, rng)?;
        }
        self.distribute_others(&practice, &practice_train, rng)?;
        if !practice_any {
            self.place_friend_filler(&practice_train, rng)?;
        }
        self.place_splits(&practice, rng)?;
        // 人数 / 闪数只在诊断打开时求值：`diag!` 的参数是惰性的
        diag!(
            "【分布】T{} 人数 {:?} 闪{:?}",
            self.turn(),
            std::array::from_fn::<usize, 5, _>(|train| self.base.distribution[train].len()),
            std::array::from_fn::<usize, 5, _>(|train| self.shining_count(train))
        );
        Ok(())
    }

    /// 本回合按稽古处理的训练位（由练习演目反查训练位；合宿 / URA 全真）
    fn practice_train_mask(&self, practice: &[bool; 5]) -> [bool; 5] {
        std::array::from_fn(|train| practice[self.act_of_train(train)])
    }

    /// 该训练位是否已有「友人 / 团队 / 理事长 / 记者」人头
    ///
    /// 通用规则：**同一训练位最多 1 名**友人 / 团队 / 理事长 / 记者。
    /// 引擎 `Person::is_friend()` 的口径 = `train_type() > 4 || Yayoi || Reporter`，与之一致；
    /// 引擎 `distribute_person` 自带该校验，剧本侧自定义放位（理事长 / 记者 / 吾妻补位）须自己补上。
    fn has_social_person(&self, train: usize) -> bool {
        self.base.distribution[train].iter().any(|index| {
            *index >= 0
                && self
                    .persons
                    .get(*index as usize)
                    .is_some_and(|person| person.is_friend())
        })
    }

    /// 亮起演目的担当（卡 / 神驹）强制锁位（不判不在）
    fn place_locked(&mut self, practice: &[bool; 5]) {
        let mut locked = 0;
        for (act, lit) in practice.iter().enumerate() {
            if !*lit {
                continue;
            }
            let train = self.train_of_act(act);
            for person_index in 0..self.persons.len() {
                if self.persons[person_index].act != Some(act)
                    || !self.person_is_available(person_index)
                {
                    continue;
                }
                if self.push_to_train(train, person_index) {
                    locked += 1;
                }
            }
        }
        if locked > 0 {
            diag!(
                "【分布】T{} 锁定担当 {locked} 人（排练演目 {:?}）",
                self.turn(),
                (0..5).filter(|act| practice[*act]).collect::<Vec<_>>()
            );
        }
    }

    /// 其余人头按引擎分配顺序落位（理事长 → 记者 → 支援卡 → 神驹；友人另走补位）
    ///
    /// 理事长 / 记者：**只落非稽古训练位**；全稽古回合（合宿 / URA /
    /// 全槽满）无普通位 → 本回合完全不出现。
    fn distribute_others(
        &mut self,
        practice: &[bool; 5],
        practice_train: &[bool; 5],
        rng: &mut impl Rng,
    ) -> Result<()> {
        let sequence = [
            PersonType::Yayoi,
            PersonType::Reporter,
            PersonType::Card,
            PersonType::Npc,
        ];
        let friend_index = self.friend_person_index();
        let mut guests_hidden = 0;
        for ty in sequence {
            for person_index in 0..self.persons.len() {
                if self.persons[person_index].person_type() != ty {
                    continue;
                }
                if Some(person_index) == friend_index {
                    continue;
                }
                if !self.person_is_available(person_index) {
                    continue;
                }
                // 亮起演目的担当已锁位，不再参与普通分布
                if let Some(act) = self.persons[person_index].act
                    && practice[act]
                {
                    // 锁位分支补消费：按「未锁位时 distribute_person 会抽多少」推进随机流，
                    // 避免后续人头的抽取游标整体错位（不改变落位结果）
                    self.burn_locked_person(person_index, rng)?;
                    continue;
                }
                if matches!(ty, PersonType::Yayoi | PersonType::Reporter) {
                    if !self.place_guest_outside_practice(person_index, practice_train, rng) {
                        guests_hidden += 1;
                    }
                    continue;
                }
                // 神驹按剧本口径也有不在率（STUB：absent_weight = 50，U29）；其余走各自权重
                Game::distribute_person(self, person_index as i32, true, rng)?;
            }
        }
        if guests_hidden > 0 {
            diag!(
                "【分布】T{} 全稽古回合：理事长 / 记者不出现（{} 人）",
                self.turn(),
                guests_hidden
            );
        }
        Ok(())
    }

    /// 理事长 / 记者落位：只在非稽古训练位里按引擎同型规则（先判不在、再均匀随机）落位
    ///
    /// 返回 `false` = 本回合没有非稽古空位（全稽古回合），该人头不出现、不消耗随机。
    fn place_guest_outside_practice(
        &mut self,
        person_index: usize,
        practice_train: &[bool; 5],
        rng: &mut impl Rng,
    ) -> bool {
        let mut allowed = [0usize; 5];
        let mut len = 0;
        for (train, lit) in practice_train.iter().enumerate() {
            if !*lit
                && self.base.distribution[train].len() < TRAIN_CAP
                && !self.base.distribution[train].contains(&(person_index as i32))
                && !self.has_social_person(train)
            {
                allowed[len] = train;
                len += 1;
            }
        }
        if len == 0 {
            return false;
        }
        // 与引擎 `distribute_person` 同口径：先判不在（理事长 / 记者 absent_weight = 200）
        let absent = self.absent_weight(person_index);
        if absent > 0 && rng.random_bool(f64::from(absent) / f64::from(500 + absent)) {
            return true;
        }
        let pick = rng.random_range(0..len);
        self.base.distribution[allowed[pick]].push(person_index as i32);
        true
    }

    /// 吾妻：有稽古训练时必补一个未满 5 人的稽古位（不判不在、不去普通位）；
    /// 无稽古的普通回合走通用不在判定。
    ///
    /// 唯一稽古位且未满 5 人时必定落在该位（候选只有 1 个）；有多个候选时随机取一。
    fn place_friend_filler(
        &mut self,
        practice_train: &[bool; 5],
        rng: &mut impl Rng,
    ) -> Result<()> {
        let Some(friend_index) = self.friend_person_index() else {
            return Ok(());
        };
        if !self.person_is_available(friend_index) {
            return Ok(());
        }
        if practice_train.iter().any(|lit| *lit) {
            let mut empty = [0usize; 5];
            let mut len = 0;
            for (train, lit) in practice_train.iter().enumerate() {
                if *lit
                    && self.base.distribution[train].len() < TRAIN_CAP
                    && !self.base.distribution[train].contains(&(friend_index as i32))
                    && !self.has_social_person(train)
                {
                    empty[len] = train;
                    len += 1;
                }
            }
            if len == 0 {
                return Ok(());
            }
            let pick = rng.random_range(0..len);
            let train = empty[pick];
            self.base.distribution[train].push(friend_index as i32);
            diag!("【分布】T{} 吾妻补位 → 训练位{}", self.turn(), train);
        } else {
            Game::distribute_person(self, friend_index as i32, true, rng)?;
        }
        Ok(())
    }

    /// 分身落位：協力 801 / 転成 101 合宿 / URA 本番演出
    ///
    /// 分身占**另一种训练种类**、不叠在排练位；每处占位参与人数 / 槽 / 面板。
    fn place_splits(&mut self, practice: &[bool; 5], rng: &mut impl Rng) -> Result<()> {
        // URA 本番演出的「+1 人」只在 URA 训练（72–77）生效；68–71 空窗期不生效
        let finals =
            self.finals_done && (crate::URA_START_TURN..=crate::MAX_TURN).contains(&self.turn);
        let camp = self.plan.camp_train(self.forced_rest);
        for (act, lit) in practice.iter().enumerate() {
            let effect = self.advice_slot_effect(act);
            // 協力 801：该演目每个担当（卡 + 神驹）各 +1 处
            let mut count = effect.cooperate_split;
            // 転成 101：合宿中该演目每个担当各 +1 处
            if *lit && camp {
                count += effect.camp_extra_person;
            }
            // URA 本番演出：该演目稽古的**编成卡**各 +1 人（不含友人 / 团体 / 神驹）
            let finals_extra = if finals && *lit {
                self.data.finals_act.extra_member
            } else {
                0
            };
            if count <= 0 && finals_extra <= 0 {
                continue;
            }
            // 顺序与原「先收集再逐项落位」一致：演目 0→4、同一演目内人头 0→n
            for person_index in 0..self.persons.len() {
                let person = &self.persons[person_index];
                if person.act != Some(act) || !self.person_is_available(person_index) {
                    continue;
                }
                let mut person_extra = count;
                if finals_extra > 0 && person.origin == PersonOrigin::Card {
                    person_extra += finals_extra;
                }
                for _ in 0..person_extra {
                    self.place_split_once(person_index, rng);
                }
            }
        }
        Ok(())
    }

    /// 把一个人头再放到一个「非本担当训练位」的空位上（放不下则跳过）
    fn place_split_once(&mut self, person_index: usize, rng: &mut impl Rng) {
        let Some(act) = self.persons[person_index].act else {
            return;
        };
        let own_train = self.train_of_act(act);
        let mut candidates = [0usize; 5];
        let mut len = 0;
        for (train, slot) in self.base.distribution.iter().enumerate().take(5) {
            if train != own_train
                && slot.len() < TRAIN_CAP
                && !slot.contains(&(person_index as i32))
            {
                candidates[len] = train;
                len += 1;
            }
        }
        if len == 0 {
            return;
        }
        let pick = rng.random_range(0..len);
        let train = candidates[pick];
        self.base.distribution[train].push(person_index as i32);
        diag!(
            "【分布】T{} 分身 人头{} → 训练位{}",
            self.turn(),
            person_index,
            train
        );
    }

    /// 往训练位追加一个人头（容量 / 同位置去重）；放不下返回 false
    fn push_to_train(&mut self, train: usize, person_index: usize) -> bool {
        if self.base.distribution[train].len() >= TRAIN_CAP
            || self.base.distribution[train].contains(&(person_index as i32))
        {
            return false;
        }
        self.base.distribution[train].push(person_index as i32);
        true
    }

    /// 灵感（Hint）分配：基础公式 + 卡面加成 + 剧本加成；排练 5 种卡时全灵感图标
    pub(crate) fn distribute_hints(&mut self, rng: &mut impl Rng) -> Result<()> {
        let data = Data::get()?;
        let base_hint_rate = data.consts.base_hint_rate / 100.0;
        // 剧本灵感率加成：发布会 + 该卡担当演目的教え（逐人现算，避免每回合临时 Vec）
        let presentation = self.presentation_bonus();
        for person_index in 0..self.persons.len() {
            if self.persons[person_index].person_type() != PersonType::Card {
                continue;
            }
            let Some(bonus) = self.persons[person_index].card_id().and_then(|card_id| {
                self.base
                    .deck
                    .iter()
                    .find(|card| card.card_id == card_id)
                    .map(|card| card.card_value().hint_prob_increase)
            }) else {
                continue;
            };
            let scenario = presentation.hint_rate
                + self
                    .persons
                    .get(person_index)
                    .and_then(|person| person.act)
                    .map_or(0, |act| self.advice_slot_effect(act).hint_rate);
            let hint_prob = (base_hint_rate * ((100 + bonus + scenario) as f64 / 100.0)).min(1.0);
            let hit = rng.random_bool(hint_prob);
            self.persons[person_index].set_hint(hit);
        }
        // 排练 + 编成支援卡类型 ≥5：编成卡（除友人 / 团队）全部亮灵感图标
        let practice = self.practice_acts();
        if practice.iter().any(|lit| *lit) && self.deck_has_five_types() {
            for person in &mut self.persons {
                if person.person_type() == PersonType::Card && person.origin != PersonOrigin::Friend
                {
                    person.set_hint(true);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use umasim::game::{Game, Person, PersonType};

    use super::*;

    /// 找指定引擎类型的人头下标
    fn person_of_type(game: &MatsuriGame, ty: PersonType) -> usize {
        game.persons()
            .iter()
            .position(|person| person.person_type() == ty)
            .expect("缺少指定类型的人头")
    }

    #[test]
    fn absent_weight_uses_scenario_values() {
        let game = crate::testing::sample_game();
        let card = person_of_type(&game, PersonType::Card);
        let friend = person_of_type(&game, PersonType::ScenarioCard);
        let npc = person_of_type(&game, PersonType::Npc);
        let yayoi = person_of_type(&game, PersonType::Yayoi);
        assert_eq!(game.absent_weight(card), 50, "支援卡基础不在权重 50");
        assert_eq!(game.absent_weight(friend), 100, "友人基础不在权重 100");
        assert_eq!(
            game.absent_weight(npc),
            50,
            "编成外神驹按剧本口径 50（约 9.1%，STUB U29）"
        );
        assert_eq!(game.absent_weight(yayoi), 200, "理事长固定 200");
    }

    #[test]
    fn deyilv_includes_presentation_baseline() {
        let mut game = crate::testing::sample_game();
        let card_deyilv = game.deck()[0].calc_training_effect(&game, 0).deyilv;
        let got = game.deyilv(0);
        assert_eq!(got, card_deyilv + 20.0, "得意率 = 卡面 + 发布会基线 +20");
    }

    #[test]
    fn distribution_locks_act_members_and_fills_friend() {
        let mut game = crate::testing::sample_game();
        game.turn = 20;
        game.acts.gauge[2] = game.act_gauge_max();
        let mut rng = StdRng::seed_from_u64(1);
        game.distribute_round(&mut rng).expect("分布失败");
        let practice_train = game.train_of_act(2);
        for (index, person) in game.persons.iter().enumerate() {
            if person.act == Some(2) && game.person_is_available(index) {
                assert!(
                    game.distribution()[practice_train].contains(&(index as i32)),
                    "亮起演目的担当应锁定在担当训练位: {index}"
                );
            }
        }
        // 吾妻：排练回合必补 1 个未满排练位
        let friend = game.friend_person_index().expect("带友人卡");
        assert!(
            game.distribution()[practice_train].contains(&(friend as i32)),
            "排练回合吾妻应补位到排练位"
        );
        // 容量 / 去重（无分身时同一人每位置至多 1 次）
        for train in game.distribution() {
            assert!(train.len() <= TRAIN_CAP, "训练位人数不超过 5");
            let mut seen = std::collections::HashSet::new();
            for index in train {
                assert!(seen.insert(*index), "同一训练位不应重复人头");
            }
        }
    }

    /// 理事长 / 记者：稽古回合只落非稽古位；全稽古回合（合宿）完全不出现
    #[test]
    fn yayoi_and_reporter_never_on_practice_positions() {
        // 日常：1 个演目亮起（稽古位），理事长 / 记者只可能出现在其余普通位
        let mut game = crate::testing::sample_game();
        game.turn = 20;
        game.acts.gauge[2] = game.act_gauge_max();
        let mut rng = StdRng::seed_from_u64(7);
        game.distribute_round(&mut rng).expect("分布失败");
        let practice_train = game.train_of_act(2);
        for index in 0..game.persons.len() {
            let ty = game.persons[index].person_type();
            if !matches!(ty, PersonType::Yayoi | PersonType::Reporter) {
                continue;
            }
            for (train, slot) in game.distribution().iter().enumerate() {
                if slot.contains(&(index as i32)) {
                    assert_ne!(train, practice_train, "理事长 / 记者不应出现在稽古位");
                }
            }
        }
        // 合宿（全稽古）：理事长 / 记者完全不出现
        let mut camp = crate::testing::sample_game();
        camp.turn = 12;
        camp.plan = crate::TurnPlan {
            camp_zone: true,
            camp_year: Some(0),
            camp_regular: true,
            ..crate::TurnPlan::default()
        };
        let mut rng = StdRng::seed_from_u64(8);
        camp.distribute_round(&mut rng).expect("分布失败");
        for index in 0..camp.persons.len() {
            let ty = camp.persons[index].person_type();
            if matches!(ty, PersonType::Yayoi | PersonType::Reporter) {
                assert!(
                    camp.distribution()
                        .iter()
                        .all(|slot| !slot.contains(&(index as i32))),
                    "合宿全稽古回合：理事长 / 记者不应出现"
                );
            }
        }
    }

    /// 吾妻：只要有稽古位就必补该位（不判不在、不去普通位）；唯一稽古位必定命中
    #[test]
    fn friend_fills_practice_position_when_any() {
        let mut game = crate::testing::sample_game();
        game.turn = 20;
        game.acts.gauge[2] = game.act_gauge_max();
        let friend = game.friend_person_index().expect("带友人卡");
        let practice_train = game.train_of_act(2);
        for seed in 0..50u64 {
            let mut probe = game.clone();
            probe.acts.gauge[2] = probe.act_gauge_max();
            let mut rng = StdRng::seed_from_u64(seed);
            probe.distribute_round(&mut rng).expect("分布失败");
            let positions: Vec<usize> = probe
                .distribution()
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.contains(&(friend as i32)))
                .map(|(train, _)| train)
                .collect();
            assert_eq!(
                positions,
                vec![practice_train],
                "seed {seed}: 吾妻只应出现在唯一稽古位"
            );
        }
        // 合宿全稽古：吾妻恰好出现 1 处（不可分身），且在某稽古位
        let mut camp = crate::testing::sample_game();
        camp.turn = 12;
        camp.plan = crate::TurnPlan {
            camp_zone: true,
            camp_year: Some(0),
            camp_regular: true,
            ..crate::TurnPlan::default()
        };
        let friend = camp.friend_person_index().expect("带友人卡");
        let mut rng = StdRng::seed_from_u64(9);
        camp.distribute_round(&mut rng).expect("分布失败");
        let positions: Vec<usize> = camp
            .distribution()
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.contains(&(friend as i32)))
            .map(|(train, _)| train)
            .collect();
        assert_eq!(positions.len(), 1, "合宿中吾妻应恰好出现 1 处（不可分身）");
    }

    /// 通用规则：同一训练位最多 1 名「友人 / 团队 / 理事长 / 记者」
    #[test]
    fn at_most_one_social_person_per_position() {
        for seed in 0..40u64 {
            let mut game = crate::testing::sample_game();
            game.turn = 20;
            game.acts.gauge[2] = game.act_gauge_max();
            let mut rng = StdRng::seed_from_u64(seed);
            game.distribute_round(&mut rng).expect("分布失败");
            for (train, slot) in game.distribution().iter().enumerate() {
                let social = slot
                    .iter()
                    .filter(|index| {
                        **index >= 0
                            && game
                                .persons
                                .get(**index as usize)
                                .is_some_and(|person| person.is_friend())
                    })
                    .count();
                assert!(
                    social <= 1,
                    "seed {seed}: 训练位 {train} 有多名友人/理事长/记者: {slot:?}"
                );
            }
        }
    }

    #[test]
    fn cooperate_advice_adds_split_appearance() {
        let mut game = crate::testing::sample_game();
        game.turn = 20;
        game.advice.acquired = true;
        game.advice.slots = [401, 101, 201, 301, 501];
        game.advice.fused = [801, 0, 0, 0, 0];
        game.acts.level = [1; 5];
        game.acts.gauge[0] = game.act_gauge_max();
        let mut rng = StdRng::seed_from_u64(2);
        game.distribute_round(&mut rng).expect("分布失败");
        // 協力 801：act0 每个担当在另一个训练位额外登场 → 出现次数 ≥2 且位置不同
        let mut split_found = false;
        for (index, person) in game.persons.iter().enumerate() {
            if person.act != Some(0) || !game.person_is_available(index) {
                continue;
            }
            let positions: Vec<usize> = game
                .distribution()
                .iter()
                .enumerate()
                .filter(|(_, train)| train.contains(&(index as i32)))
                .map(|(train, _)| train)
                .collect();
            if positions.len() >= 2 {
                split_found = true;
                assert_ne!(positions[0], positions[1], "分身应落在另一个训练位");
            }
        }
        assert!(split_found, "協力教诲应给出至少一处分身");
    }

    #[test]
    fn finals_extra_member_only_for_edition_cards() {
        let mut game = crate::testing::sample_game();
        game.turn = 72;
        game.finals_done = true;
        game.acts.gauge[0] = game.act_gauge_max();
        let mut rng = StdRng::seed_from_u64(4);
        game.distribute_round(&mut rng).expect("分布失败");
        let mut card_split = false;
        for (index, person) in game.persons.iter().enumerate() {
            if person.act != Some(0) || !game.person_is_available(index) {
                continue;
            }
            let count = game
                .distribution()
                .iter()
                .filter(|train| train.contains(&(index as i32)))
                .count();
            if person.origin == PersonOrigin::Card {
                card_split |= count >= 2;
            } else {
                assert!(count <= 1, "URA 本番演出额外登场只加编成支援卡");
            }
        }
        assert!(card_split, "URA 期演目稽古：编成卡应额外登场 1 处");
    }

    #[test]
    fn practice_five_types_lights_all_card_hints() {
        let mut game = crate::testing::sample_game();
        game.turn = 20;
        game.acts.gauge[0] = game.act_gauge_max();
        let mut rng = StdRng::seed_from_u64(3);
        game.distribute_round(&mut rng).expect("分布失败");
        game.distribute_hints(&mut rng).expect("灵感分配失败");
        for person in game.persons() {
            if person.person_type() == PersonType::Card {
                assert!(person.hint(), "排练 + 5 种卡：编成卡应带灵感图标");
            }
        }
        let friend = person_of_type(&game, PersonType::ScenarioCard);
        assert!(!game.persons()[friend].hint(), "友人不产生灵感图标");
        let npc = person_of_type(&game, PersonType::Npc);
        assert!(!game.persons()[npc].hint(), "神驹不产生灵感图标");
    }

    /// 演目满槽锁位不应改变其他人头的随机落位（人头级独立流 / CRN）
    #[test]
    fn locked_act_keeps_other_person_placement_aligned() {
        let mut base = crate::testing::sample_game();
        base.base.turn = 10;
        base.plan = base.derive_plan();
        base.set_rule_master(42);

        let mut locked = base.clone();
        locked.acts.gauge[0] = locked.act_gauge_max();
        let mut open = base.clone();
        open.acts.gauge[0] = 0;

        let mut rng_a = StdRng::seed_from_u64(1);
        let mut rng_b = StdRng::seed_from_u64(1);
        locked.distribute_round(&mut rng_a).expect("锁位分布失败");
        open.distribute_round(&mut rng_b).expect("普通分布失败");

        let person = base
            .persons
            .iter()
            .enumerate()
            .find(|(index, person)| {
                person.person_type() == PersonType::Card
                    && person.act != Some(0)
                    && base.person_is_available(*index)
            })
            .map(|(index, _)| index)
            .expect("非演目 0 的支援卡");
        let slot = |game: &MatsuriGame| {
            game.base
                .distribution
                .iter()
                .position(|row| row.contains(&(person as i32)))
        };
        assert_eq!(
            slot(&locked),
            slot(&open),
            "锁位不应把其他人头的随机落位整体错位"
        );
    }
}
