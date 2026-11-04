//! 回合末结算（S5）：発表会 / 本番 / 合宿终了 / 教诲窗口 / 育成终了
//!
//! 结算顺序（`End` 阶段）：**剧本结算（発表会 / 本番 → 合宿 +1）→ 教诲分配 / 融合**，
//! 与 `mechanism.md` §5.2 / §6.1 一致；発表会 buff 由 `check_points` 派生（累计行），
//! 一次性奖励直接在 `End` 入账（技能灵感不建模，登记 TBD）。

use anyhow::Result;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::ActionValue;
use umasim::game::{Game, PersonType, Trainer};

use super::MatsuriGame;
use crate::config::FusionPair;
use crate::data::{CheckPointResult, Reward, RewardResult};
use crate::diag;

impl MatsuriGame {
    /// 回合末剧本结算（在合宿计数之后、求解器推进回合之前调用）
    pub(crate) fn settle_turn_end(&mut self) -> Result<()> {
        // 1) 発表会 / 本番（turn 23 / 47 / 67 的 End）
        if let Some(index) = self.plan.check_point {
            self.settle_check_point(index);
        }
        // 2) 合宿结束：全演目 Lv +1（turn 15 / 39 / 63）
        if self.plan.camp_level_up {
            let before = self.acts.level;
            for act in 0..5 {
                self.add_act_level(act, 1);
            }
            diag!(
                "【合宿】T{} 结束 全演目 Lv+1 {before:?}→{:?}",
                self.turn(),
                self.acts.level
            );
        }
        // 3) 自选比赛义务区间：在区间最后一回合的 End 判定
        self.settle_free_race_deadline()?;
        Ok(())
    }

    /// 自选比赛义务区间判定（**区间最后一回合的 End**，2026-11-04 真机修正）
    ///
    /// - 达标：入队「生涯比赛胜利」奖励事件（同目标赛：五维 / PT 吃比赛与剧本赛后加成）；
    /// - 未达标：置 `career_failed`，`next()` 终止育成（终局跳过奖励）。
    fn settle_free_race_deadline(&mut self) -> Result<()> {
        let turn = self.turn();
        let mut failed = false;
        let mut met = 0usize;
        for free in &self.base.uma.free_races {
            if free.end_turn as i32 != turn {
                continue;
            }
            let won = self.base.uma.count_free_race(free);
            if won >= free.count {
                met += 1;
            } else {
                failed = true;
                diag!(
                    "【自选比赛】T{} 义务区间未达标（需 {} 场，已 {} 场）→ 育成失败（终止）",
                    turn,
                    free.count,
                    won
                );
            }
        }
        if failed {
            self.career_failed = true;
            return Ok(());
        }
        if met == 0 {
            return Ok(());
        }
        let data = Data::get()?;
        let scale = self.race_scale();
        let pt_rate = self.race_pt_rate();
        for _ in 0..met {
            let mut event = data.system_event("race_career")?.clone();
            event.map_status(|x| (f64::from(x) * scale).floor() as i32);
            if (pt_rate - 1.0).abs() > f64::EPSILON {
                let value = &mut event.choices[0][0].value;
                value.status_pt[5] = (f64::from(value.status_pt[5]) * pt_rate).floor() as i32;
            }
            diag!(
                "【自选比赛】T{} 义务区间达标 → 生涯比赛胜利奖励（scale={scale:.2} pt×{pt_rate:.2}）",
                turn
            );
            self.base.unresolved_events.push(event);
        }
        Ok(())
    }

    /// 进场（発表会 / 本番）结算：判定结果 → 永久 buff（派生）→ 一次性奖励
    fn settle_check_point(&mut self, index: usize) {
        if index >= self.check_points.len()
            || self.check_points[index] != CheckPointResult::NotReached
        {
            return;
        }
        let result = self.judge_presentation(index);
        self.check_points[index] = result;
        if index == 2 {
            self.finals_done = true;
        }
        if let Some(schedule) = self.data.schedule.get(index) {
            let reached = self
                .acts
                .level
                .iter()
                .filter(|level| **level >= schedule.need_act_level)
                .count();
            diag!(
                "【発表会】T{} 第{}回 {} 达标 {reached}/{} 演目 Lv≥{}",
                self.turn(),
                index + 1,
                check_point_result_name(result),
                schedule.need_act_num,
                schedule.need_act_level
            );
        }
        let schedule = index as i32 + 1;
        let Some(reward_result) = reward_result(result) else {
            return;
        };
        if let Some(reward) = self
            .data
            .check_point_rewards
            .iter()
            .find(|reward| reward.schedule == Some(schedule) && reward.result == reward_result)
            .cloned()
        {
            self.apply_reward(&reward);
        }
    }

    /// 一次性奖励入账（属性 / SP / 体力 / 干劲 / 训练等级 / 理事长羁绊；
    /// 技能灵感与粉丝不建模，见 `stubs.md`）
    fn apply_reward(&mut self, reward: &Reward) {
        let mut value = ActionValue::default();
        for status in 0..5 {
            value.status_pt[status] += reward.status;
        }
        value.status_pt[5] += reward.skill_pt;
        value.vital += reward.vital;
        value.motivation += reward.motivation;
        self.uma_mut().add_value(&value);
        if reward.training_level_up > 0 {
            for bonus in &mut self.train_level_bonus {
                *bonus += reward.training_level_up;
            }
        }
        if reward.president_friendship > 0
            && let Some(index) = self.person_index_of_type(PersonType::Yayoi)
        {
            self.base
                .add_friendship(&mut self.persons, index, reward.president_friendship);
        }
        // `act_member_add`：第 2 批神驹的登场条件本身即「Classic 進捗発表会大成功」
        // （建局时已按 `needs_great_success` 门禁），此处不需要再补人头（U27 口径）。
        let _ = reward.act_member_add;
        diag!(
            "【奖励】T{} 五维+{} SP+{} 体力+{} 干劲+{} 訓練Lv+{} 理事长羁绊+{}",
            self.turn(),
            reward.status,
            reward.skill_pt,
            reward.vital,
            reward.motivation,
            reward.training_level_up,
            reward.president_friendship
        );
    }

    /// 教诲窗口（`End` 阶段、剧本结算之后）：融合（配置预设）→ 分配（交给 Trainer）
    ///
    /// 窗口表 = turn 2 / 11 / 23 / 35 / 47 / 59；turn 2 的首次授予与分配在 `Begin`
    /// （`stage.rs`），End 的窗口 0 不再重复处理。
    pub(crate) fn settle_advice_window<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
    ) -> Result<()> {
        let Some(window) = self.plan.advice_window else {
            return Ok(());
        };
        if window == 0 {
            return Ok(());
        }
        // 先融合（turn 23 / 47），再让 Trainer 选 5 槽分配；其他窗口只做分配
        let pairs = self.fusion_plan(window);
        if !pairs.is_empty() {
            self.apply_fusion_pairs(&pairs)?;
        }
        self.decide_advice_assignment(trainer, decision)
    }

    /// 本窗口的融合配对来源（**当前 = 配置预设**；将来若交给 Trainer，只改这一个函数）
    fn fusion_plan(&self, window: usize) -> Vec<FusionPair> {
        match window {
            2 => self.config.fusion.year2.clone(),
            4 => self.config.fusion.year3.clone(),
            _ => Vec::new(),
        }
    }

    /// 育成终了奖励（`on_simulation_end`）：三场全大成功 → 大成功档，否则成功档
    pub(crate) fn apply_career_end_reward(&mut self) {
        let all_great = self
            .check_points
            .iter()
            .all(|result| *result == CheckPointResult::GreatSuccess);
        let result = if all_great {
            RewardResult::GreatSuccess
        } else {
            RewardResult::Success
        };
        diag!(
            "【终局】T{} 三场结果 {:?} → {}档奖励｜演目Lv {:?}（上限 {}）",
            self.turn(),
            self.check_points,
            match result {
                RewardResult::GreatSuccess => "大成功",
                RewardResult::Success => "成功",
            },
            self.acts.level,
            self.act_level_cap()
        );
        if let Some(reward) = self
            .data
            .career_end_rewards
            .iter()
            .find(|reward| reward.result == result)
            .cloned()
        {
            self.apply_reward(&reward);
        }
    }
}

/// 発表会结果名（诊断日志用）
fn check_point_result_name(result: CheckPointResult) -> &'static str {
    match result {
        CheckPointResult::NotReached => "未达成",
        CheckPointResult::Success => "成功",
        CheckPointResult::GreatSuccess => "大成功",
    }
}

/// 発表会结果 → 奖励档（未达成无奖励）
fn reward_result(result: CheckPointResult) -> Option<RewardResult> {
    match result {
        CheckPointResult::NotReached => None,
        CheckPointResult::Success => Some(RewardResult::Success),
        CheckPointResult::GreatSuccess => Some(RewardResult::GreatSuccess),
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use umasim::game::{Game, Person, PersonType, Trainer};

    use super::*;

    /// 保持现状的训练员（教诲分配返回 0 = 恒等排列）
    struct KeepTrainer;

    impl Trainer<MatsuriGame> for KeepTrainer {
        fn select_action(
            &self,
            _game: &MatsuriGame,
            _actions: &[crate::game::Action],
            _rng: &mut StdRng,
        ) -> Result<usize> {
            Ok(0)
        }

        fn select_choice(
            &self,
            _game: &MatsuriGame,
            _choices: &[Vec<umaai_core::gamedata::EventChoice>],
            _rng: &mut StdRng,
        ) -> Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn first_check_point_great_success_applies_reward() {
        let mut game = crate::testing::sample_game();
        game.acts.level = [3; 5];
        game.plan.check_point = Some(0);
        game.turn = 23;
        let before = game.uma().clone();
        game.settle_turn_end().expect("结算失败");
        assert_eq!(game.check_points[0], CheckPointResult::GreatSuccess);
        for axis in 0..5 {
            assert_eq!(
                game.uma().five_status[axis] - before.five_status[axis],
                5,
                "第 1 回大成功：五维各 +5（axis {axis}）"
            );
        }
        assert_eq!(game.uma().skill_pt - before.skill_pt, 200, "SP +200");
        assert_eq!(game.train_level_bonus, [1; 5], "全部普通训练 Lv+1");
        let yayoi = game
            .persons()
            .iter()
            .position(|person| person.person_type() == PersonType::Yayoi)
            .expect("理事长人头");
        assert_eq!(game.persons()[yayoi].friendship(), 15, "理事长羁绊 +15");
        let bonus = game.presentation_bonus();
        assert_eq!(
            (bonus.friendship, bonus.deyilv, bonus.hint_rate),
            (5, 40, 50),
            "永久 buff 换到第 1 回大成功行"
        );
    }

    #[test]
    fn check_point_success_when_one_act_short() {
        let mut game = crate::testing::sample_game();
        game.acts.level = [3, 3, 3, 3, 2];
        game.plan.check_point = Some(0);
        let before = game.uma().clone();
        game.settle_turn_end().expect("结算失败");
        assert_eq!(game.check_points[0], CheckPointResult::Success);
        assert_eq!(
            game.uma().five_status[0] - before.five_status[0],
            3,
            "成功档五维各 +3"
        );
        assert_eq!(game.uma().skill_pt - before.skill_pt, 100, "成功档 SP +100");
        assert_eq!(game.train_level_bonus, [0; 5], "成功档不提升训练 Lv");
    }

    #[test]
    fn finals_sets_finals_done_and_reward() {
        let mut game = crate::testing::sample_game();
        game.acts.level = [6; 5];
        game.plan.check_point = Some(2);
        game.turn = 67;
        game.uma_mut().vital = 50;
        let before = game.uma().clone();
        game.settle_turn_end().expect("结算失败");
        assert_eq!(game.check_points[2], CheckPointResult::GreatSuccess);
        assert!(game.finals_done, "本番结算后置本番演出标记");
        assert_eq!(game.uma().five_status[0] - before.five_status[0], 15);
        assert_eq!(game.uma().skill_pt - before.skill_pt, 400);
        assert_eq!(game.uma().vital - before.vital, 30, "本番大成功体力 +30");
        assert_eq!(game.uma().motivation, before.motivation + 1);
        let bonus = game.presentation_bonus();
        assert_eq!(bonus.race_bonus, 100, "本番大成功比赛加成 +100%");
    }

    #[test]
    fn camp_level_up_raises_all_acts() {
        let mut game = crate::testing::sample_game();
        game.acts.level = [1, 2, 3, 4, 5];
        game.plan.camp_level_up = true;
        game.settle_turn_end().expect("结算失败");
        assert_eq!(game.acts.level, [2, 3, 4, 5, 6], "合宿结束全演目 Lv+1");
    }

    /// turn 2 的首次授予在 Begin 阶段完成，End 的窗口 0 不再授予（保持训练员 = 不重排）
    #[test]
    fn advice_window_zero_does_not_acquire_at_end() {
        let mut game = crate::testing::sample_game();
        game.plan.advice_window = Some(0);
        let mut decision = StdRng::seed_from_u64(3);
        game.settle_advice_window(&KeepTrainer, &mut decision)
            .expect("结算失败");
        assert!(!game.advice.acquired, "T2 首次授予在 Begin，End 不再授予");
    }

    #[test]
    fn advice_fusion_after_begin_acquire() {
        let mut game = crate::testing::sample_game();
        // T2 Begin 已完成首次授予（此处直接构造授予后的局面）
        game.acquire_advice();
        assert!(game.advice.acquired);
        assert_eq!(game.advice.slots, [101, 201, 301, 401, 501]);
        assert_eq!(game.advice.fused, [0; 5]);

        let mut decision = StdRng::seed_from_u64(3);
        game.plan.advice_window = Some(2);
        game.settle_advice_window(&KeepTrainer, &mut decision)
            .expect("结算失败");
        assert_eq!(game.advice.fused[0], 601, "第 2 年融合 101 → 601");
        assert_eq!(game.advice.fused[1], 701, "第 2 年融合 201 → 701");

        game.plan.advice_window = Some(4);
        game.settle_advice_window(&KeepTrainer, &mut decision)
            .expect("结算失败");
        assert!(
            game.advice.fused.iter().all(|fused| *fused != 0),
            "第 3 年全部融合"
        );
        let counts: Vec<usize> = (0..5)
            .map(|act| {
                game.persons
                    .iter()
                    .filter(|person| person.act == Some(act))
                    .count()
            })
            .collect();
        let target = (0..5)
            .max_by_key(|act| (counts[*act], usize::MAX - *act))
            .expect("5 演目");
        let slot = (0..5)
            .find(|act| game.advice.fused[*act] == 801)
            .expect("801 已融合");
        assert_eq!(
            slot, target,
            "401+801 硬约束到担当数最多的演目（{counts:?}）"
        );
    }

    #[test]
    fn career_end_rewards_by_presentation_results() {
        let mut game = crate::testing::sample_game();
        game.check_points = [CheckPointResult::GreatSuccess; 3];
        let before = game.uma().clone();
        game.apply_career_end_reward();
        for axis in 0..5 {
            assert_eq!(game.uma().five_status[axis] - before.five_status[axis], 30);
        }
        assert_eq!(game.uma().skill_pt - before.skill_pt, 350);

        let mut game = crate::testing::sample_game();
        game.check_points = [
            CheckPointResult::GreatSuccess,
            CheckPointResult::Success,
            CheckPointResult::Success,
        ];
        let before = game.uma().clone();
        game.apply_career_end_reward();
        assert_eq!(
            game.uma().five_status[0] - before.five_status[0],
            15,
            "非全大成功 → 成功档"
        );
        assert_eq!(game.uma().skill_pt - before.skill_pt, 150);
    }
}
