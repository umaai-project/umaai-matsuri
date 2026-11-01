//! 人头分布（S5）：亮起锁担当 / 不在率 / 吾妻补位 / 分身
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
//! 随机走 `Distribute` 阶段的回合固定流（与引擎基础局同轴）。

use anyhow::Result;
use rand::Rng;
use umaai_core::Data;
use umasim::game::{Game, Person, PersonType};

use super::MatsuriGame;
use super::person::PersonOrigin;

/// 每个训练位的人数上限（引擎通用口径）
const TRAIN_CAP: usize = 5;

impl MatsuriGame {
    /// 本回合是否需要分布（比赛 / 强制休息回合不训练，不分布）
    pub(crate) fn needs_distribution(&self) -> bool {
        !self.plan.race && !self.forced_rest
    }

    /// 本回合分布入口（`Distribute` 阶段调用）
    pub(crate) fn distribute_round(&mut self, rng: &mut impl Rng) -> Result<()> {
        self.reset_distribution();
        if !self.needs_distribution() {
            return Ok(());
        }
        let practice = self.practice_acts();
        self.place_locked(&practice);
        self.distribute_others(&practice, rng)?;
        self.place_friend_filler(&practice, rng)?;
        self.place_splits(&practice, rng)?;
        Ok(())
    }

    /// 亮起演目的担当（卡 / 神驹）强制锁位（不判不在）
    fn place_locked(&mut self, practice: &[bool; 5]) {
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
                self.push_to_train(train, person_index);
            }
        }
    }

    /// 其余人头按引擎分配顺序落位（理事长 → 记者 → 支援卡 → 神驹；友人另走补位）
    fn distribute_others(&mut self, practice: &[bool; 5], rng: &mut impl Rng) -> Result<()> {
        let sequence = [
            PersonType::Yayoi,
            PersonType::Reporter,
            PersonType::Card,
            PersonType::Npc,
        ];
        let friend_index = self.friend_person_index();
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
                    continue;
                }
                // 神驹按剧本口径也有不在率（STUB：absent_weight = 50，U29）；其余走各自权重
                Game::distribute_person(self, person_index as i32, true, rng)?;
            }
        }
        Ok(())
    }

    /// 吾妻：排练回合必补一个未满排练位；普通回合走通用不在判定
    fn place_friend_filler(&mut self, practice: &[bool; 5], rng: &mut impl Rng) -> Result<()> {
        let Some(friend_index) = self.friend_person_index() else {
            return Ok(());
        };
        if !self.person_is_available(friend_index) {
            return Ok(());
        }
        let practice_any = practice.iter().any(|lit| *lit);
        if practice_any {
            let mut empty: Vec<usize> = (0..5)
                .filter(|train| {
                    practice[*train]
                        && self.base.distribution[*train].len() < TRAIN_CAP
                        && !self.base.distribution[*train].contains(&(friend_index as i32))
                })
                .collect();
            if empty.is_empty() {
                return Ok(());
            }
            let pick = rng.random_range(0..empty.len());
            let train = empty.swap_remove(pick);
            self.base.distribution[train].push(friend_index as i32);
        } else {
            Game::distribute_person(self, friend_index as i32, true, rng)?;
        }
        Ok(())
    }

    /// 分身落位：協力 801 / 転成 101 合宿 / URA 本番演出
    ///
    /// 分身占**另一种训练种类**、不叠在排练位；每处占位参与人数 / 槽 / 面板。
    fn place_splits(&mut self, practice: &[bool; 5], rng: &mut impl Rng) -> Result<()> {
        // (人头, 额外处数)
        let mut extra: Vec<(usize, i32)> = Vec::new();
        let finals = self.finals_done && (68..=77).contains(&self.turn);
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
            for person_index in 0..self.persons.len() {
                let person = &self.persons[person_index];
                if person.act != Some(act) || !self.person_is_available(person_index) {
                    continue;
                }
                let mut person_extra = count;
                if finals_extra > 0 && person.origin == PersonOrigin::Card {
                    person_extra += finals_extra;
                }
                if person_extra > 0 {
                    extra.push((person_index, person_extra));
                }
            }
        }
        for (person_index, count) in extra {
            for _ in 0..count {
                self.place_split_once(person_index, rng);
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
        let mut candidates: Vec<usize> = (0..5)
            .filter(|train| {
                *train != own_train
                    && self.base.distribution[*train].len() < TRAIN_CAP
                    && !self.base.distribution[*train].contains(&(person_index as i32))
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let pick = rng.random_range(0..candidates.len());
        let train = candidates.swap_remove(pick);
        self.base.distribution[train].push(person_index as i32);
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
        let deck_len = self.base.deck.len();
        // 剧本灵感率加成：发布会 + 该卡担当演目的教え
        let presentation = self.presentation_bonus();
        let mut scenario_bonus = vec![presentation.hint_rate; deck_len];
        for (index, bonus) in scenario_bonus.iter_mut().enumerate() {
            if let Some(act) = self.persons.get(index).and_then(|person| person.act) {
                *bonus += self.advice_slot_effect(act).hint_rate;
            }
        }
        let card_bonus: Vec<(u32, i32)> = self
            .base
            .deck
            .iter()
            .map(|card| (card.card_id, card.card_value().hint_prob_increase))
            .collect();
        for person_index in 0..self.persons.len() {
            if self.persons[person_index].person_type() != PersonType::Card {
                continue;
            }
            let Some((_, bonus)) = self.persons[person_index]
                .card_id()
                .and_then(|card_id| card_bonus.iter().find(|(id, _)| *id == card_id))
            else {
                continue;
            };
            let scenario = scenario_bonus.get(person_index).copied().unwrap_or(0);
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
        game.turn = 70;
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
}
