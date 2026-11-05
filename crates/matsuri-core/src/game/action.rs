//! 剧本动作（`Action`）与结算入口
//!
//! `apply` 只负责把动作落到局面上，**取哪条随机流由 `run_stage` 决定**（策略流）。
//! 训练 / 稽古的数值与后处理在 [`train`](super::train)，本文件只放比赛 / 休息 / 出行 / 治病
//! 的结算入口与演目槽入账（目标赛 +4、休息 / 出行 / 非目标赛 +1）。

use std::fmt::Display;

use anyhow::{Result, anyhow};
use rand::Rng;
use umaai_core::Data;
use umasim::game::base::BaseAction;
use umasim::game::{ActionEnum, Game};

use super::MatsuriGame;
use super::advice::AdviceAssignment;
use crate::diag;

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
    /// 教诲分配方案（**只作为剧本决策点的候选出现**，不在 `list_actions` 动作集里）
    ///
    /// 每个教诲窗口由 `stage_body` 构造 120 个候选、交给 `Trainer::select_action` 选；
    /// 应用时只做槽位重排，不消耗随机流。
    AssignAdvice(AdviceAssignment),
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
            Self::AssignAdvice(assign) => write!(f, "教诲分配{assign}"),
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
            Self::AssignAdvice(assign) => game.apply_assignment(assign),
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
            // 演目稽古 / 教诲分配没有等价的基础动作
            Self::ActTrain(_) | Self::AssignAdvice(_) => None,
        }
    }
}

/// 友人出行段标签（诊断用；只在日志打开时格式化）
struct FriendOutingLabel(usize);

impl std::fmt::Display for FriendOutingLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "友人出行第{}段", self.0)
    }
}

/// 比赛 PT 倍率备注（诊断用；只在日志打开时格式化）
struct PtNote(f64);

impl std::fmt::Display for PtNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if (self.0 - 1.0).abs() > f64::EPSILON {
            write!(f, " PT×{:.2}", self.0)
        } else {
            Ok(())
        }
    }
}

impl MatsuriGame {
    /// 诊断：演目槽入账行（`label` = 动作名；`before` = 入账前槽位；无入账不输出）
    ///
    /// 摘要文本由 `diag!` 惰性求值：日志关闭时只做一次槽位数组比较。
    fn diag_gauge_gain(&self, label: impl std::fmt::Display, before: &[i32; 5]) {
        if self.acts.gauge != *before {
            diag!(
                "【演目槽】T{} {label} 入槽 {}",
                self.turn(),
                crate::game::diag::gauge_changes(before, &self.acts.gauge)
            );
        }
    }

    /// 比赛结算：目标赛 / URA 决赛 / 自选比赛统一走 [`BaseGame::push_race_event`]
    ///
    /// 五维系 = [`MatsuriGame::race_scale`]（卡面赛后 × 铁心五维倍率 × 剧本赛后）；
    /// PT 另按 [`MatsuriGame::race_pt_rate`] 缩放。取整（floor）由引擎统一做一次。
    /// 演目槽：目标赛（含 URA 决赛）+4（按 4 次 +1 三则落点）；自选比赛（非目标）+1。
    pub(crate) fn do_race(&mut self) -> Result<()> {
        let before = self.key_state();
        let goal = self.plan.race;
        let scale = self.race_scale();
        let pt_rate = self.race_pt_rate();
        self.base.push_race_event(scale, pt_rate)?;
        let gain = if goal {
            self.data.rules.act_gauge_gain.target_race
        } else {
            self.data.rules.act_gauge_gain.other
        };
        let kind = if self.plan.ura_race {
            "URA决赛"
        } else if goal {
            "目标赛"
        } else {
            "自选比赛"
        };
        self.gain_lowest_act_gauge(gain);
        self.diag_gauge_gain(kind, &before.gauge);
        diag!(
            "【动作】T{} {kind} 加成×{scale:.2}{}｜变化 {}",
            self.turn(),
            PtNote(pt_rate),
            before.diff(&self.key_state())
        );
        Ok(())
    }

    /// 休息：普通 / 合宿版走基础库默认（`is_xiahesu` = 原夏合宿窗口 36–39 / 60–63）；演目槽 +1
    ///
    /// 合宿版休息额外解除【变胖】（剧本口径）。
    pub(crate) fn do_rest(&mut self, rng: &mut impl Rng) -> Result<()> {
        let before = self.key_state();
        BaseAction::do_sleep(&mut self.base, rng)?;
        if self.plan.camp_rest {
            self.uma_mut().flags.ill.remove(umaai_core::Ill::FAT);
        }
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        self.diag_gauge_gain(
            if self.plan.camp_rest {
                "合宿版休息"
            } else {
                "休息"
            },
            &before.gauge,
        );
        diag!(
            "【动作】T{} 休息（{}）｜变化 {}",
            self.turn(),
            if self.plan.camp_rest {
                "合宿版"
            } else {
                "普通"
            },
            before.diff(&self.key_state())
        );
        Ok(())
    }

    /// 普通出行：通用出行事件挂入待结算队列（`AfterTrain` 结算）；演目槽 +1
    pub(crate) fn do_normal_outing(&mut self, rng: &mut impl Rng) -> Result<()> {
        let before = self.key_state();
        BaseAction::do_normal_outing(&mut self.base, rng)?;
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        self.diag_gauge_gain("普通出行", &before.gauge);
        diag!(
            "【动作】T{} 普通出行｜变化 {}",
            self.turn(),
            before.diff(&self.key_state())
        );
        Ok(())
    }

    /// 治病：基础库默认（+20 体力、清空生病位与不擅长训练）
    pub(crate) fn do_clinic(&mut self, rng: &mut impl Rng) -> Result<()> {
        let before = self.key_state();
        BaseAction::do_clinic(&mut self.base, rng)?;
        diag!(
            "【动作】T{} 治病｜变化 {}",
            self.turn(),
            before.diff(&self.key_state())
        );
        Ok(())
    }

    /// 友人（吾妻）出行：推进 5 段进度 + 按 `outing` 资产给最低演目 Lv+1 + 本段真实卡面事件入队
    ///
    /// 段事件 = `events.friend_events["1".."5"]`（吾妻 `809052011–015`）；数值为 memo 基础值，
    /// 结算时由引擎叠卡面「事件效果 / 回复量」加成。
    pub(crate) fn do_friend_outing(&mut self, _rng: &mut impl Rng) -> Result<()> {
        let before = self.key_state();
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
            .outing_add_level(card_id, step)
            .ok_or_else(|| anyhow!("出行资产缺行：card_id={card_id}, story_step={}", step + 1))?;
        // 先完成全部校验，再写状态：缺行时不留下「已用一段 / 已加 Lv」的半态
        let key = (step + 1).to_string();
        let event = Data::get()?
            .events
            .friend_events
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("友人出行段事件缺行: friend_events[{key}]"))?;
        self.base.friend.out_used[step] = true;
        self.add_lowest_act_level(add_level);
        self.gain_lowest_act_gauge(self.data.rules.act_gauge_gain.other);
        self.diag_gauge_gain(FriendOutingLabel(step + 1), &before.gauge);
        diag!(
            "【动作】T{} 友人出行 第{}段 演出目Lv+{} 事件#{}（已用 {}/5 段）",
            self.turn(),
            step + 1,
            add_level,
            event.id,
            self.base
                .friend
                .out_used
                .iter()
                .filter(|used| **used)
                .count()
        );
        diag!(
            "【动作】T{} 友人出行｜变化 {}",
            self.turn(),
            before.diff(&self.key_state())
        );
        self.unresolved_events.push(event);
        Ok(())
    }

    /// 当前友人出行段的最低演目 Lv 增加量（无友人 / 5 段用完 / 资产缺行 → `None`）
    pub(crate) fn friend_outing_add_level(&self) -> Option<i32> {
        let step = self.base.friend.out_used.iter().position(|used| !used)?;
        let card_id = self.base.friend.card_id?;
        self.outing_add_level(card_id, step)
    }

    /// 指定友人卡与段位的最低演目 Lv 增加量（`outing` 资产查表）
    fn outing_add_level(&self, card_id: u32, step: usize) -> Option<i32> {
        self.data
            .outing
            .iter()
            .find(|row| row.card_id == card_id && row.story_step == step as i32 + 1)
            .map(|row| row.add_level)
    }
}
