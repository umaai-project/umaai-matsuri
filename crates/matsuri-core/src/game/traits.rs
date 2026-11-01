//! 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`）
//!
//! 契约 trait 的 impl 必须整体落在同一文件；可拆的只有 inherent `impl`（见 AGENTS.md）。

use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use umaai_core::gamedata::{ActionValue, EventData};
use umasim::game::{
    CardTrainingEffect, Game, Person, PersonType, StageId, SupportCard, Trainer,
    TrainingBasicTable, TrainingValueSource, Uma,
};
use umasim::search::FlatSearchGame;
use umasim::trainer::RandomTrainer;

use super::person::{MatsuriPerson, PersonOrigin};
use super::{Action, EventRecord, MatsuriGame, Stage};
use crate::MAX_TURN;
use crate::data::CheckPointResult;

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
        if self.keiko_settling {
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
        // 日常 / URA 收尾回合：槽满的演目对应训练位变演目稽古
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
        // S6 事件链：固定回合 / 合宿抑制 / 友人解锁 / 随机分布（见 `game/event.rs`）
        self.begin_events(rng)
    }

    fn apply_event(&mut self, event: &EventData, choice: usize, rng: &mut impl Rng) -> Result<()> {
        // ① 引擎底座负责通用数值结算（含友人卡事件加成、ExtraStatus）
        let result = self.base.apply_event(event, choice, rng);
        if let Some(result) = &result {
            // ② 引擎 `BaseEvent::apply_event` 不结算 add/remove_flags，剧本侧补齐（变胖等状态）
            self.base.uma.update_flags(result);
            // ③ 事件羁绊归属（红点 / 点击 / 出行 / 友人事件等带羁绊值）
            if result.value.friendship != 0
                && let Some(person_index) = self.base.event_person_target(event, &self.persons)
            {
                self.base
                    .add_friendship(&mut self.persons, person_index, result.value.friendship);
            }
        }
        // ④ 剧本特例：友人出行解锁 + 疲劳回复 flag
        if event.id == super::event::FRIEND_OUT_EVENT_ID {
            self.base.friend.out_state = umasim::game::FriendOutState::AfterUnlock;
            self.fatigue_recovery = true;
        }
        // ⑤ 事件额外效果（演目 gauge / 解 debuff）
        self.apply_event_extra(event.id, choice);
        // ⑥ 事件记录（S8 手动 / 前端核对）：记录实际到账数值与状态变化
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
            // 理事长 / 记者按引擎基础局默认
            PersonOrigin::Yayoi => true,
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

    /// 重新分配所有人头（S5 剧本口径：亮起锁担当 / NPC 不在率 / 吾妻补位 / 分身）
    fn distribute_all(&mut self, rng: &mut impl Rng) -> Result<()> {
        self.distribute_round(rng)
    }

    /// 灵感分配（S5 剧本口径：基础公式 + 卡面加成 + 剧本加成；排练 5 种卡全灵感图标）
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
        let mut value = self.default_calc_training_value(buffs, train)?;
        let keiko = self.keiko_settling;
        let upper = self.training_upper(train, &value, keiko);
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

    /// 训练失败率：引擎基础局公式（口径同 §5.6）+ 剧本失败率下降（不屈 / 本番演出 -100%）
    fn calc_training_failure_rate(&self, buffs: &CardTrainingEffect, train: usize) -> Result<f32> {
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
        let scenario_drop = self.scenario_fail_rate_drop(train);
        let card_factor = (100.0 - buffs.fail_rate_drop) / 100.0;
        let scenario_factor = (100.0 - scenario_drop as f32) / 100.0;
        let f = if scenario_drop >= 100 {
            0.0
        } else {
            base * card_factor * scenario_factor + bias
        };
        Ok(f.clamp(0.0, 100.0))
    }

    /// 终局结算（育成终了奖励；技能灵感 / 粉丝不建模）
    fn on_simulation_end<T: Trainer<Self>>(
        &mut self,
        _trainer: &T,
        _rng: &mut StdRng,
    ) -> Result<()> {
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

impl FlatSearchGame for MatsuriGame {
    /// rollout 基策（S1 用契约参考实现；后续实现换成自己的策略）
    type RolloutTrainer = RandomTrainer;

    /// 没有 leaf 估值器时声明 `false`：搜索入口会直接拒绝 `max_depth > 0`
    const SUPPORTS_TRUNCATED_LEAF: bool = false;

    fn default_rollout_trainer() -> Self::RolloutTrainer {
        RandomTrainer
    }

    fn apply_root_action(&mut self, action: &Self::Action, rng: &mut StdRng) -> Result<()> {
        // 必须与 `run_stage` 内执行动作的方式逐字一致，否则被评估的动作会用上另一条流
        self.apply_action(action, rng)
    }

    fn fork_for_rollout(&self, rollout_seed: u64) -> Self {
        // 分支 = 克隆 + 注入 rollout 主种子；任何不由传入 rng 驱动的流都必须在这里重置
        let mut game = self.clone();
        game.set_rule_master(rollout_seed);
        game
    }
}
