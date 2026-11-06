//! 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`）
//!
//! 契约 trait 的 impl 必须整体落在同一文件；可拆的只有 inherent `impl`（见 AGENTS.md）。

use crate::config::MatsuriConfig;
use crate::trainer::MatsuriTrainer;
use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use umaai_core::gamedata::{ActionValue, EventData};
use umaai_core::{AttributeArray, Ill};
use umasim::game::{
    CardTrainingEffect, Game, Person, PersonType, StageId, SupportCard, Trainer,
    TrainingBasicTable, TrainingValueSource, Uma,
};
use umasim::search::FlatSearchGame;

use super::advice::ParticipantAdvice;
use super::person::{MatsuriPerson, PersonOrigin};
use super::{Action, EventRecord, MatsuriGame, Stage};
use crate::MAX_TURN;
use crate::data::CheckPointResult;
use crate::diag;

/// 剧本人头：引擎通用字段逐项委托 [`BasePerson`](umasim::game::base::BasePerson)
///
/// `PersonOrigin` ↔ `PersonType` 的映射与边界见 [`MatsuriPerson`] 文档；引擎默认逻辑只读
/// `PersonType`，剧本逻辑只看 `PersonOrigin`。
impl Person for MatsuriPerson {
    fn person_type(&self) -> PersonType {
        self.base.person_type
    }

    fn person_index(&self) -> i32 {
        self.base.person_index
    }

    fn train_type(&self) -> i32 {
        self.base.train_type
    }

    fn friendship(&self) -> i32 {
        self.base.friendship
    }

    fn hint(&self) -> bool {
        self.base.is_hint
    }

    fn set_hint(&mut self, hint: bool) {
        self.base.is_hint = hint;
    }

    fn set_friendship(&mut self, friendship: i32) {
        self.base.friendship = friendship;
    }

    fn card_id(&self) -> Option<u32> {
        self.base.card_id
    }
}

impl TrainingValueSource for MatsuriGame {
    /// 普通训练返回资产 `training.normal`；**演目稽古结算期间**返回稽古表
    /// （`training.keiko` 逐训练位一行，等级按表行数夹取到 1 档）
    fn training_basic_table(&self) -> &TrainingBasicTable {
        if self.keiko_settling.get() {
            // 稽古档按时期切：turn 23 発表会「演目排练强化」后为 Classic 及以后档
            self.keiko_basic_table()
        } else {
            &self.data.training.normal
        }
    }
}

impl Game for MatsuriGame {
    type Person = MatsuriPerson;
    type Action = Action;

    fn turn(&self) -> i32 {
        self.base.turn
    }

    fn max_turn(&self) -> i32 {
        MAX_TURN
    }

    fn rule_master(&self) -> Option<u64> {
        self.base.master()
    }

    fn set_rule_master(&mut self, master: u64) {
        // 引擎两轴 + 剧本自持的策略轴一起重置（底座入口叫 init_rule_streams，刻意不与契约同名）
        self.base.init_rule_streams(master);
        self.reset_strategy_stream();
    }

    fn stage(&self) -> StageId {
        self.stage.into()
    }

    fn next(&mut self) -> bool {
        // 育成失败（自选比赛区间未达标，在区间最后一回合的 End 判定）→ 终止
        if self.career_failed {
            return false;
        }
        if let Some(stage) = self.stage.next() {
            self.stage = stage;
        } else if self.turn < self.max_turn() {
            self.turn += 1;
            self.stage = Stage::Begin;
            // 新回合：两条引擎流与自持的策略流一起按 `(master, 新回合)` 重新派生（CRN 配对来源）
            self.base.reset_rule_streams();
            self.reset_strategy_stream();
        } else {
            return false;
        }
        true
    }

    fn run_stage<T: Trainer<Self>>(&mut self, trainer: &T, decision: &mut StdRng) -> Result<()> {
        // 三条轴各就各位：局面流 / 事件流从底座取出，策略流本局面自持，决策流只给 Trainer。
        // 取出-放回成对执行（中途出错也先放回再传播），避免推进过的流状态被丢掉。
        let mut streams = self.base.take_rule_streams()?;
        let Some(mut strategy) = self.strategy else {
            return Err(anyhow!(
                "策略流缺失：主种子已注入但策略流未派生，应先调用 set_rule_master"
            ));
        };
        let res = self.stage_body(trainer, decision, &mut streams, &mut strategy);
        self.base.put_rule_streams(streams);
        self.strategy = Some(strategy);
        res
    }

    fn list_actions(&self) -> Result<Vec<Self::Action>> {
        // 强制比赛回合（目标赛 / URA 决赛）：只能比赛
        if self.plan.race && !self.forced_rest {
            return Ok(vec![Action::Race]);
        }
        // 强制休息回合（马娘特殊事件占掉目标赛）：实际动作是休息
        if self.forced_rest {
            return Ok(vec![Action::Rest]);
        }
        // 合宿排练回合：5 个稽古（全部训练变稽古）+ 休息 + 第 1 年出行 + 可自选比赛
        if self.plan.camp_train(self.forced_rest) {
            let mut actions: Vec<Action> = (0..5u8).map(Action::ActTrain).collect();
            actions.push(Action::Rest);
            if self.outing_allowed() {
                actions.push(Action::NormalOuting);
                if self.base.can_friend_outing() {
                    actions.push(Action::FriendOuting);
                }
            }
            if self.base.can_self_race() {
                actions.push(Action::Race);
            }
            if !self.uma().flags.ill.is_empty() {
                actions.push(Action::Clinic);
            }
            return Ok(actions);
        }
        // URA 回合（72–77，剧本结束后的决赛期）：**全部训练变演目稽古**（类似合宿）；
        // 可休息 / 普通出行 / 治病；不可自选赛 / 友人出行（引擎 URA 规则已拦截）
        if self.plan.ura_period {
            let mut actions: Vec<Action> = (0..5u8).map(Action::ActTrain).collect();
            actions.push(Action::Rest);
            actions.push(Action::NormalOuting);
            if !self.uma().flags.ill.is_empty() {
                actions.push(Action::Clinic);
            }
            return Ok(actions);
        }
        // 日常 / 空窗期回合：槽满的演目对应训练位变演目稽古
        let gauge_max = self.data.rules.act_gauge_max;
        let mut actions: Vec<Action> = (0..5u8)
            .map(|train| {
                let act = self.act_of_train(train as usize);
                if self.acts.gauge[act] >= gauge_max {
                    Action::ActTrain(act as u8)
                } else {
                    Action::Train(train)
                }
            })
            .collect();
        actions.push(Action::Rest);
        actions.push(Action::NormalOuting);
        if self.base.can_friend_outing() {
            actions.push(Action::FriendOuting);
        }
        if self.base.can_self_race() {
            actions.push(Action::Race);
        }
        if !self.uma().flags.ill.is_empty() {
            actions.push(Action::Clinic);
        }
        Ok(actions)
    }

    fn generate_events(&self, rng: &mut impl Rng) -> Vec<EventData> {
        // 事件链：固定回合 / 合宿抑制 / 友人解锁 / 随机分布（见 `game/event.rs`）
        self.begin_events(rng)
    }

    fn apply_event(&mut self, event: &EventData, choice: usize, rng: &mut impl Rng) -> Result<()> {
        let before = self.key_state();
        // ① 引擎底座负责通用数值结算（含友人卡事件加成、ExtraStatus）
        let mut result = self.base.apply_event(event, choice, rng);
        // ② 幸运体质（`lucky_count` = 挡生病次数）：剧本侧是 add/remove_flags 的唯一结算点，
        //    在 `update_flags` 前拦截。不变量：`bad_trainer` 与 `Ill::BAD_TRAINER` 同置同清 →
        //    一起拦；**变胖（`Ill::FAT`）也是生病**，全部生病位一起挡（剧本口径）。
        if let Some(r) = &mut result
            && let Some(add) = &mut r.add_flags
            && (add.bad_trainer || !add.ill.is_empty())
            && self.uma().flags.lucky_count > 0
        {
            self.uma_mut().flags.lucky_count -= 1;
            add.bad_trainer = false;
            add.ill = Ill::NONE;
            diag!(
                "【状态】T{} 事件#{} 幸运体质挡下一次生病",
                self.turn(),
                event.id
            );
        }
        if let Some(result) = &result {
            // ③ 引擎 `BaseEvent::apply_event` 不结算 add/remove_flags，剧本侧补齐（变胖等状态）
            self.base.uma.update_flags(result);
            let add_flags = result
                .add_flags
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            let remove_flags = result
                .remove_flags
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            if !add_flags.is_empty() || !remove_flags.is_empty() {
                diag!(
                    "【状态】T{} 事件#{} 状态+[{add_flags}] 状态-[{remove_flags}]",
                    self.turn(),
                    event.id
                );
            }
            // ③ 事件羁绊归属（红点 / 点击 / 出行 / 友人事件等带羁绊值）
            if result.value.friendship != 0
                && let Some(person_index) = self.base.event_person_target(event, &self.persons)
            {
                self.base
                    .add_friendship(&mut self.persons, person_index, result.value.friendship);
                diag!(
                    "【事件羁绊】T{} 事件#{} 人头{person_index} +{}",
                    self.turn(),
                    event.id,
                    result.value.friendship
                );
            }
        }
        // ④ 引擎继承特殊结算（4012 第二年 / 4013 第三年）：事件数据为零值，属性、PT 与
        //    上限由公式结算（蓝因子 × 6 + 剧本因子 × scale；上限 = floor(蓝因子 × 5.33)）。
        //    引擎把这段写在样例局 BasicGame 的 apply_event 里，剧本侧（BaseGame 底座）需自持。
        if matches!(event.id, 4012 | 4013) {
            let value = ActionValue {
                status_pt: self.base.inherit.inherit(rng),
                ..Default::default()
            };
            let limit = self.base.inherit.inherit_limit(rng);
            self.uma_mut().add_value(&value);
            self.uma_mut().five_status_limit.add_eq(&limit);
            diag!(
                "【继承】T{} #{} {}｜属性 {:?} 上限 {:?}",
                self.turn(),
                event.id,
                event.name,
                value.status_pt,
                limit
            );
        }
        // ⑤ 剧本特例：友人出行解锁 + 疲劳回复 flag
        if event.id == super::event::FRIEND_OUT_EVENT_ID {
            self.base.friend.out_state = umasim::game::FriendOutState::AfterUnlock;
            self.fatigue_recovery = true;
            diag!(
                "【状态】T{} 友人出行解锁 → 出行可用 + 疲劳回复 flag",
                self.turn()
            );
        }
        // ⑥ 事件额外效果（演目 gauge / 解 debuff）
        self.apply_event_extra(event.id, choice);
        diag!(
            "【事件结算】T{} #{} {} 选项{}｜变化 {}",
            self.turn(),
            event.id,
            event.name,
            choice,
            before.diff(&self.key_state())
        );
        // ⑦ 事件记录（手动 / 前端核对）：记录实际到账数值与状态变化
        //
        // 搜索 rollout 不记录（`record_events = false`）：明细构造含多次格式化与分配，
        // 而 rollout 局面跑完即弃，`event_log` 无人消费。
        if self.record_events {
            let mut detail = result
                .as_ref()
                .map(|result| result.value.to_string())
                .unwrap_or_default();
            if let Some(result) = &result {
                if let Some(flags) = &result.add_flags
                    && !flags.to_string().is_empty()
                {
                    if !detail.is_empty() {
                        detail.push('；');
                    }
                    detail.push_str("状态+");
                    detail.push_str(&flags.to_string());
                }
                if let Some(flags) = &result.remove_flags
                    && !flags.to_string().is_empty()
                {
                    if !detail.is_empty() {
                        detail.push('；');
                    }
                    detail.push_str("状态-");
                    detail.push_str(&flags.to_string());
                }
            }
            self.push_event_log(EventRecord {
                seq: 0,
                turn: self.turn(),
                id: event.id,
                name: event.name.clone(),
                choice,
                detail: (!detail.is_empty()).then_some(detail),
            });
        }
        Ok(())
    }

    fn persons(&self) -> &[Self::Person] {
        &self.persons
    }

    fn persons_mut(&mut self) -> &mut [Self::Person] {
        &mut self.persons
    }

    fn init_persons(&mut self) -> Result<()> {
        // 卡组人头（前缀同序）+ 理事长 / 记者 + 编成外神驹；担当分配在这里一次性完成
        self.init_roster()
    }

    fn person_is_available(&self, person_index: usize) -> bool {
        let Some(person) = self.persons.get(person_index) else {
            return false;
        };
        match person.origin {
            PersonOrigin::Card => true,
            PersonOrigin::Friend => self.turn() >= 2,
            PersonOrigin::Guest => {
                self.turn() >= person.avail_turn
                    && (!person.needs_great_success
                        || self.check_points[1] == CheckPointResult::GreatSuccess)
            }
            // 理事长：本剧本 T1 起才出现（真机口径；引擎基础局默认 T0 起）
            PersonOrigin::Yayoi => self.turn() >= 1,
            PersonOrigin::Reporter => self.turn() >= 13,
        }
    }

    fn distribution(&self) -> &Vec<Vec<i32>> {
        &self.base.distribution
    }

    fn distribution_mut(&mut self) -> &mut Vec<Vec<i32>> {
        &mut self.base.distribution
    }

    fn absent_rate_drop(&self) -> i32 {
        self.base.absent_rate_drop
    }

    /// 人头「不在」权重：卡 50 / 友人 100 / 理事长·记者 200（引擎基础局口径）；
    /// **编成外神驹按剧本口径 50（约 9.1%，STUB U29）**——排练锁位时不判不在。
    fn absent_weight(&self, person_index: usize) -> i32 {
        let drop = self.absent_rate_drop();
        match self.persons[person_index].person_type() {
            PersonType::Card => 50 - drop,
            PersonType::Yayoi | PersonType::Reporter => 200,
            PersonType::ScenarioCard | PersonType::OtherFriend | PersonType::TeamCard => 100 - drop,
            PersonType::Npc => 50,
        }
    }

    /// 人头得意率：卡面效果 + 剧本加成（発表会 deyilv / 该卡担当演目的教え得意率 / 転成）
    fn deyilv(&mut self, person_index: i32) -> f32 {
        if person_index < 0 {
            return 0.0;
        }
        let index = person_index as usize;
        // 卡片人头占 persons 前缀、与 deck 同序；前缀之外（神驹 / 理事长 / 记者）没有得意率
        if index >= self.base.deck.len() {
            return 0.0;
        }
        let card_deyilv = self.base.deck[index].calc_training_effect(self, 0).deyilv;
        card_deyilv + self.scenario_deyilv(index) as f32
    }

    /// 重新分配所有人头（剧本口径：亮起锁担当 / NPC 不在率 / 吾妻补位 / 分身）
    fn distribute_all(&mut self, rng: &mut impl Rng) -> Result<()> {
        self.distribute_round(rng)
    }

    /// 灵感分配（剧本口径：基础公式 + 卡面加成 + 剧本加成；排练 5 种卡全灵感图标）
    fn distribute_hint(&mut self, rng: &mut impl Rng) -> Result<()> {
        self.distribute_hints(rng)
    }

    fn has_group_buff(&self) -> bool {
        self.base.friend.group_buff_turn > 0
    }

    fn train_level(&self, train: usize) -> usize {
        let up_count = self.data.rules.train_level_up_count.max(1) as usize;
        let max_level = self.max_normal_train_level();
        let bonus = self.train_level_bonus[train].max(0) as usize;
        (self.base.train_level_count[train] as usize / up_count + 1 + bonus).clamp(1, max_level)
    }

    /// 训练值 = 下层（引擎默认公式）+ 上层（剧本来源：发布会 / 教え / 演目 Lv / 稽古 +10）
    fn calc_training_value(&self, buffs: &CardTrainingEffect, train: usize) -> Result<ActionValue> {
        let advice = self.participant_advice(train);
        self.calc_training_value_inner(buffs, train, &advice)
    }

    /// 训练失败率：引擎基础局公式（口径同 §5.6）+ 剧本失败率下降（不屈 / 本番演出 -100%）
    fn calc_training_failure_rate(&self, buffs: &CardTrainingEffect, train: usize) -> Result<f32> {
        let advice = self.participant_advice(train);
        self.calc_training_failure_rate_inner(buffs, train, &advice)
    }

    /// 终局结算（育成终了奖励；技能灵感 / 粉丝不建模）
    fn on_simulation_end<T: Trainer<Self>>(
        &mut self,
        _trainer: &T,
        _rng: &mut StdRng,
    ) -> Result<()> {
        // 育成失败（自选比赛未达标）：按真机口径不发育成终了奖励，直接结算当前状态
        if self.career_failed {
            diag!("【终局】育成失败（自选比赛未达标）→ 跳过终局奖励");
            return Ok(());
        }
        self.apply_career_end_reward();
        Ok(())
    }

    fn uma(&self) -> &Uma {
        &self.base.uma
    }

    fn uma_mut(&mut self) -> &mut Uma {
        &mut self.base.uma
    }

    fn deck(&self) -> &Vec<SupportCard> {
        &self.base.deck
    }
}

impl MatsuriGame {
    /// [`Game::calc_training_value`] 的 advice 复用版（同一次预览 / 结算只算一次参与教诲）
    pub(crate) fn calc_training_value_inner(
        &self,
        buffs: &CardTrainingEffect,
        train: usize,
        advice: &ParticipantAdvice,
    ) -> Result<ActionValue> {
        let mut value = self.default_calc_training_value(buffs, train)?;
        let keiko = self.keiko_settling.get();
        let upper = self.training_upper_with(train, &value, keiko, advice);
        for status in 0..5 {
            value.status_pt[status] += upper.status[status];
            if keiko {
                value.status_pt[status] += upper.keiko_flat;
            }
        }
        value.status_pt[5] += upper.skill_pt;
        value.vital = upper.vital;
        value.motivation += upper.motivation;
        Ok(value)
    }

    /// [`Game::calc_training_failure_rate`] 的 advice 复用版
    pub(crate) fn calc_training_failure_rate_inner(
        &self,
        buffs: &CardTrainingEffect,
        train: usize,
        advice: &ParticipantAdvice,
    ) -> Result<f32> {
        let data = umaai_core::Data::get()?;
        let thresholds = &data.consts.training_vital_threshold;
        let row = thresholds.get(train).ok_or_else(|| {
            anyhow!(
                "训练类型错误: {train}（阈值表内训练数 {}）",
                thresholds.len()
            )
        })?;
        if row.is_empty() {
            return Err(anyhow!("训练 {train} 的失败率阈值表为空"));
        }
        let level = self.train_level(train).clamp(1, row.len()) - 1;
        let x0 = row[level];
        let vital = self.uma().vital as f32;
        let bias = if self.uma().flags.good_trainer {
            -2.0
        } else if self.uma().flags.bad_trainer {
            2.0
        } else {
            0.0
        };
        let base = if vital < x0 {
            (100.0 - vital) * (x0 - vital) / 40.0
        } else {
            0.0
        }
        .clamp(0.0, 99.0);
        let scenario_drop = self.scenario_fail_rate_drop_with(advice);
        let card_factor = (100.0 - buffs.fail_rate_drop) / 100.0;
        let scenario_factor = (100.0 - scenario_drop as f32) / 100.0;
        let f = if scenario_drop >= 100 {
            0.0
        } else {
            base * card_factor * scenario_factor + bias
        };
        Ok(f.clamp(0.0, 100.0))
    }
}

impl MatsuriGame {
    /// 执行已选动作（走本局面自持的策略流，与 `run_stage` 的 `Train` 阶段同一条流）
    ///
    /// 搜索评估根动作时必须走这里，不能用调用方的决策流：否则根动作的随机来源与
    /// 真实对局不一致，CRN 在根节点分叉。
    pub(crate) fn apply_selected_action(&mut self, action: &Action) -> Result<()> {
        let Some(mut strategy) = self.strategy else {
            return Err(anyhow!(
                "策略流缺失：主种子已注入但策略流未派生，应先调用 set_rule_master"
            ));
        };
        let res = self.apply_action(action, &mut strategy);
        self.strategy = Some(strategy);
        res
    }
}

impl FlatSearchGame for MatsuriGame {
    /// rollout 基策 = 本剧本的手写训练员（随机基策搜出来的动作价值没有可用口径）
    type RolloutTrainer = MatsuriTrainer;

    /// 没有 leaf 估值器时声明 `false`：搜索入口会直接拒绝 `max_depth > 0`
    const SUPPORTS_TRUNCATED_LEAF: bool = false;

    fn default_rollout_trainer() -> Self::RolloutTrainer {
        // 兜底用仓内默认档；调用方应经 `with_rollout_trainer` 注入随局配置
        MatsuriTrainer::new(MatsuriConfig::default())
    }

    fn apply_root_action(&mut self, action: &Self::Action, _rng: &mut StdRng) -> Result<()> {
        // 必须与 `run_stage` 内执行动作的方式逐字一致：走本局面自持的策略流，
        // 不能用调用方的决策流（否则根动作随机来源不一致，CRN 在根节点分叉）
        self.apply_selected_action(action)
    }

    fn fork_for_rollout(&self, rollout_seed: u64) -> Self {
        // 分支 = 克隆 + 注入 rollout 主种子；任何不由传入 rng 驱动的流都必须在这里重置
        let mut game = self.clone();
        game.set_rule_master(rollout_seed);
        // rollout 局面跑完即弃：事件明细只给手动面板 / 前端核对，搜索分支不记录
        game.record_events = false;
        game
    }

    /// 搜索口径 = 引擎默认单轴：技能 + PT×`pt_score_rate`×`pt_favor_rate` + 五维公式
    ///
    /// `pt_favor_rate = 1.0` 即官方评分；生产取 `[trainer].pt_favor_rate`，与手写剪枝
    /// 的目标函数同口径。引擎默认实现会先乘 `pt_score_rate` 再乘 `pt_favor_rate`，
    /// 这里显式展开，避免不同调用方对「PT 二次乘算」的理解分叉。
    fn search_score(&self, pt_favor_rate: f64) -> Result<f64> {
        let parts = self.uma().score_parts()?;
        let pt = (parts.pt as f32 * pt_favor_rate as f32) as i32;
        Ok((parts.skill + pt + parts.five_status.iter().sum::<i32>()) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_game;
    use rand::{RngCore, SeedableRng};

    #[test]
    fn apply_root_action_uses_strategy_stream() {
        let mut a = sample_game();
        // 训练后处理按训练位读取分布；搜索只在 Distribute 之后的决策点调用
        a.base.distribution = vec![Vec::new(); 5];
        a.set_rule_master(7);
        let mut b = a.clone();
        let mut rng1 = StdRng::seed_from_u64(1);
        let mut rng2 = StdRng::seed_from_u64(999);
        a.apply_root_action(&Action::Train(0), &mut rng1)
            .expect("根动作");
        b.apply_root_action(&Action::Train(0), &mut rng2)
            .expect("根动作");
        assert_eq!(a.key_state(), b.key_state(), "根动作不应取决于调用方 rng");
        let mut probe = StdRng::seed_from_u64(1);
        assert_eq!(rng1.next_u64(), probe.next_u64(), "调用方 rng 不应被消费");
    }

    /// 搜索口径：1.0 = 官方评分；PT 偏好越大，PT 项越值钱；失败分支按实际分
    #[test]
    fn search_score_uses_pt_favor_knob_and_actual_failed_score() {
        let mut game = sample_game();
        let official = f64::from(game.uma().calc_score().expect("官方评分"));
        assert_eq!(
            game.search_score(1.0).expect("搜索评分"),
            official,
            "pt_favor=1.0 应等于官方评分"
        );
        if game.uma().total_pt().expect("PT") > 0 {
            assert!(
                game.search_score(2.0).expect("搜索评分") > official,
                "PT 偏好 > 1 时应放大 PT 项"
            );
        }
        game.career_failed = true;
        assert_eq!(
            game.search_score(1.0).expect("失败分支实际分"),
            official,
            "失败分支不额外罚，按实际 calc_score"
        );
    }
}
