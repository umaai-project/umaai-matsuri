//! 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`）
//!
//! 契约 trait 的 impl 必须整体落在同一文件；可拆的只有 inherent `impl`（见 AGENTS.md）。

use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use umaai_core::gamedata::EventData;
use umasim::game::{
    Game, Person, PersonType, StageId, SupportCard, Trainer, TrainingBasicTable,
    TrainingValueSource, Uma,
};
use umasim::search::FlatSearchGame;
use umasim::trainer::RandomTrainer;

use super::person::{MatsuriPerson, PersonOrigin};
use super::{Action, MatsuriGame, Stage};
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
    /// 剧本资产里的普通训练表（`training.normal`：5 训练 × 等级行）
    fn training_basic_table(&self) -> &TrainingBasicTable {
        &self.data.training.normal
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
        // S1：五个训练 + 休息；后续实现在这里列自己的动作（比赛 / 出行 / 剧本机制）
        let mut actions: Vec<Action> = (0..5u8).map(Action::Train).collect();
        actions.push(Action::Rest);
        Ok(actions)
    }

    fn generate_events(&self, _rng: &mut impl Rng) -> Vec<EventData> {
        // S1 占位：后续实现在这里生成回合事件（固定回合 / 概率 / 剧本条件）
        Vec::new()
    }

    fn apply_event(&mut self, event: &EventData, choice: usize, rng: &mut impl Rng) -> Result<()> {
        // 引擎底座负责通用事件结算；剧本自己的特殊事件在这里补分支
        let _ = self.base.apply_event(event, choice, rng);
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

    fn deyilv(&mut self, _person_index: i32) -> f32 {
        // S1 占位：后续实现按自己的卡组效果算「人头得意率」
        1.0
    }

    fn has_group_buff(&self) -> bool {
        self.base.friend.group_buff_turn > 0
    }

    fn train_level(&self, train: usize) -> usize {
        let up_count = self.data.rules.train_level_up_count.max(1) as usize;
        let max_level = self.max_normal_train_level();
        (self.base.train_level_count[train] as usize / up_count + 1).clamp(1, max_level)
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
