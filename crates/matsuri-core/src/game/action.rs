//! 剧本动作（`Action`）与结算入口
//!
//! `apply` 只负责把动作落到局面上，**取哪条随机流由 `run_stage` 决定**（策略流）。
//! 训练 / 稽古的数值与后处理在 [`train`](super::train)，本文件只放比赛 / 休息 / 出行 / 治病
//! 的结算入口与演目槽入账（目标赛 +4、休息 / 出行 / 非目标赛 +1）。

use std::fmt::Display;

use anyhow::{Result, anyhow};
use rand::Rng;
use umaai_core::Data;
use umasim::game::ActionEnum;
use umasim::game::base::BaseAction;

use super::MatsuriGame;

/// 剧本动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 普通训练（0–4：速 / 耐 / 力 / 根 / 智）
    Train(u8),
    /// 演目稽古（0–4：演目下标 = 训练位下标；合宿回合全部训练变稽古、槽满时对应位变稽古）
    ActTrain(u8),
    /// 比赛（目标赛 / URA 决赛强制，或非比赛回合自选）
    Race,
    /// 休息（普通 / 合宿版由回合决定）
    Rest,
    /// 普通出行
    NormalOuting,
    /// 友人（吾妻）出行
    FriendOuting,
    /// 治病
    Clinic,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Train(idx) => write!(f, "训练{idx}"),
            Self::ActTrain(act) => write!(f, "稽古{act}"),
            Self::Race => write!(f, "比赛"),
            Self::Rest => write!(f, "休息"),
            Self::NormalOuting => write!(f, "普通出行"),
            Self::FriendOuting => write!(f, "友人出行"),
            Self::Clinic => write!(f, "治病"),
        }
    }
}

impl ActionEnum for Action {
    type Game = MatsuriGame;

    fn apply(&self, game: &mut Self::Game, rng: &mut impl Rng) -> Result<()> {
        match self {
            Self::Train(train) => game.do_train(*train as usize, rng),
            Self::ActTrain(act) => game.do_act_train(*act as usize, rng),
            Self::Race => game.do_race(),
            Self::Rest => game.do_rest(rng),
            Self::NormalOuting => game.do_normal_outing(rng),
            Self::FriendOuting => game.do_friend_outing(rng),
            Self::Clinic => game.do_clinic(rng),
        }
    }

    fn as_base_action(&self) -> Option<BaseAction> {
        match self {
            Self::Train(train) => Some(BaseAction::Train(i32::from(*train))),
            Self::Race => Some(BaseAction::Race),
            Self::Rest => Some(BaseAction::Sleep),
            Self::NormalOuting => Some(BaseAction::NormalOuting),
            Self::FriendOuting => Some(BaseAction::FriendOuting),
            Self::Clinic => Some(BaseAction::Clinic),
            // 演目稽古没有等价的基础动作（基础库没有演目概念）
            Self::ActTrain(_) => None,
        }
    }
}

impl MatsuriGame {
    /// 比赛结算：目标赛 / URA 决赛 / 自选比赛统一走 [`BaseGame::push_race_event`]
    ///
    /// `scale = (100 + 支援卡赛后加成 + 発表会 / 本番永久比赛加成) / 100`；取整（floor）在引擎。
    /// 演目槽：目标赛（含 URA 决赛）+4（按 4 次 +1 三则落点）；自选比赛（非目标）+1。
    pub(crate) fn do_race(&mut self) -> Result<()> {
        let goal = self.plan.race;
        let scale = (100 + self.base.uma.race_bonus + self.scenario_race_bonus()) as f64 / 100.0;
        self.base.push_race_event(scale, 1.0)?;
        let gain = if goal {
            self.data.rules.act_gauge_gain.target_race
        } else {
            self.data.rules.act_gauge_gain.other
        };
        self.gain_lowest_act_gauge(gain);
        Ok(())
    }

    /// 休息：普通 / 合宿版走基础库默认（`is_xiahesu` = 原夏合宿窗口 36–39 / 60–63）；演目槽 +1
    pub(crate) fn do_rest(&mut self, rng: &mut impl Rng) -> Result<()> {
        BaseAction::do_sleep(&mut self.base, rng)?;
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        Ok(())
    }

    /// 普通出行：通用出行事件挂入待结算队列（`AfterTrain` 结算）；演目槽 +1
    pub(crate) fn do_normal_outing(&mut self, rng: &mut impl Rng) -> Result<()> {
        BaseAction::do_normal_outing(&mut self.base, rng)?;
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        Ok(())
    }

    /// 治病：基础库默认（+20 体力、清空生病位与不擅长训练）
    pub(crate) fn do_clinic(&mut self, rng: &mut impl Rng) -> Result<()> {
        BaseAction::do_clinic(&mut self.base, rng)
    }

    /// 友人（吾妻）出行：推进 5 段进度 + 按 `outing` 资产给最低演目 Lv+1 + 本段事件入队
    ///
    /// 卡面出行事件数值为 `memo` 中可信度（`matsuri_outing_*` 系统事件，S6 落地真实卡面事件后替换）。
    pub(crate) fn do_friend_outing(&mut self, _rng: &mut impl Rng) -> Result<()> {
        if !self.base.can_friend_outing() {
            return Err(anyhow!("当前不能友人出行（未解锁 / 终盘 / 5 段已用完）"));
        }
        let step = self
            .base
            .friend
            .out_used
            .iter()
            .position(|used| !used)
            .ok_or_else(|| anyhow!("友人出行 5 段已全部用完"))?;
        let card_id = self
            .base
            .friend
            .card_id
            .ok_or_else(|| anyhow!("未带友人卡，不能友人出行"))?;
        let add_level = self
            .data
            .outing
            .iter()
            .find(|row| row.card_id == card_id && row.story_step == step as i32 + 1)
            .ok_or_else(|| anyhow!("出行资产缺行：card_id={card_id}, story_step={}", step + 1))?
            .add_level;
        self.base.friend.out_used[step] = true;
        self.add_lowest_act_level(add_level);
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        // 本段演出事件：数值来源 memo（见 docs/plan/intake.md §3.12），S6 换成真实卡面事件
        let mut event = Data::get()?
            .system_event(&format!("matsuri_outing_{}", step + 1))?
            .clone();
        event.person_index = self.friend_person_index().map(|index| index as i32);
        self.unresolved_events.push(event);
        Ok(())
    }
}
