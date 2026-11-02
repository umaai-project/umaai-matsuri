//! 手写训练员估值层（S7）：局面 / 动作 / 事件 / 教诲分配 → 统一评分单位
//!
//! 目标口径：官方评分（`consts.five_status_final_score` 查表 + PT × `pt_score_rate`），
//! PT 再乘训练员配置的 `pt_favor_rate`（`1.0` = 官方评分，引擎既有定档 `2.0`）。
//! 全部估值都是**评分点**，训练员只比较同一量纲的分数，不加第二个评分轴。
//!
//! 纪律：只读局面（`preview_*` 走克隆、不耗随机流）；纯函数式打分，供手写训练员与
//! 手动面板复用。后续扫参只改 `[trainer]` 配置段，不改这里的公式骨架。

use anyhow::Result;
use umaai_core::gamedata::{ActionValue, EventChoice, ExtraStatus, GameConstants};
use umaai_core::{Data, Ill, UmaFlags};
use umasim::game::{Game, Person, PersonType};

use crate::config::{AdviceWeights, TrainerConfig, ValueWeights};
use crate::game::advice::{AdviceAssignment, AdviceSlotEffect};
use crate::game::event::KENSAN_EVENT_ID;
use crate::game::{Action, MatsuriGame, PersonOrigin};

/// 训练失败的机会成本（评分点；含失败后事件损失）
const FAIL_PENALTY_TRAIN: f64 = 120.0;
/// 演目稽古失败的机会成本（排练失败完全无效，损失更大）
const FAIL_PENALTY_KEIKO: f64 = 150.0;
/// 一次闪彩（友情训练）的额外价值
const SHINING_BONUS: f64 = 15.0;
/// 兑现「追加稽古」掷中的额外价值
const KENSAN_TRIGGER_BONUS: f64 = 200.0;
/// 多亮起囤积的惩罚（亮起会锁人、降低其他位置期望人头）
const LIT_HOARD_PENALTY: f64 = 60.0;
/// 友人外出「尽早用完」策略的加分
const FRIEND_OUTING_EARLY_BONUS: f64 = 120.0;
/// 治病的保底价值（生病 / 不擅长时）
const CLINIC_VALUE: f64 = 400.0;
/// 非变胖病位的惩罚
const OTHER_ILL_PENALTY: f64 = 40.0;
/// 单次比赛的保底价值（强制比赛回合只有比赛一个候选）
const FORCED_RACE_BASE: f64 = 500.0;

/// 估算上下文（构造时一次算定，避免每次动作重复遍历人头）
struct Context {
    /// 预计剩余普通训练回合
    train_turns: f64,
    /// 预计剩余演目稽古回合
    keiko_turns: f64,
    /// 预计剩余合宿回合
    camp_turns: f64,
    /// 距 Senior 合宿（第三次合宿窗口起点）还有多少回合
    senior_camp_left: f64,
    /// 各演目担当数（卡 + 神驹；可用人头）
    persons: [f64; 5],
    /// 各演目支援卡数
    cards: [f64; 5],
    /// 各演目支援卡平均羁绊
    avg_friend: [f64; 5],
    /// 各演目担当占比（训练频率估计）
    share: [f64; 5],
    /// 下一次発表会 / 本番
    checkpoint: Option<CheckpointCtx>,
}

/// 下一次発表会 / 本番的达标压力
struct CheckpointCtx {
    /// 需要的演目 Lv
    required: i32,
    /// 距该回合还剩多少回合
    turns_left: f64,
}

/// 估值模型（借用局面；构造一次、给一批候选打分）
pub(crate) struct Model<'a> {
    game: &'a MatsuriGame,
    consts: &'static GameConstants,
    cfg: &'a TrainerConfig,
    /// 每点 PT 的评分（含 `pt_favor_rate`）
    pt_value: f64,
    weights: ValueWeights,
    advice: AdviceWeights,
    status_weights: [f64; 5],
    ctx: Context,
}

impl<'a> Model<'a> {
    /// 构造模型（预计算局面上下文；需要静态数据已初始化）
    ///
    /// 训练员旋钮取**训练员自己的配置**（`TrainerConfig`），不退化成随局默认值——
    /// 自定义训练员 / rollout 基策可以各带一套旋钮。
    pub(crate) fn new(game: &'a MatsuriGame, cfg: &'a TrainerConfig) -> Result<Self> {
        let consts = &Data::get()?.consts;
        let turn = game.turn();
        let remaining = f64::from((game.max_turn() - turn).max(0));

        // 合宿窗口剩余回合（含延长段的余量不估，取窗口本身）
        let mut camp_turns = 0.0;
        let mut senior_camp_left = 0.0;
        for (index, window) in game.data.camp.windows.iter().enumerate() {
            if index == 2 {
                senior_camp_left = f64::from((window.start - turn).max(0));
            }
            let start = window.start.max(turn + 1);
            if window.end >= start {
                camp_turns += f64::from(window.end - start + 1);
            }
        }
        let keiko_turns = camp_turns + remaining * 0.08;
        let train_turns = (remaining - camp_turns - remaining * 0.18).max(remaining * 0.35);

        // 各演目担当构成
        let mut persons = [0.0f64; 5];
        let mut cards = [0.0f64; 5];
        let mut friend_sum = [0.0f64; 5];
        for (index, person) in game.persons.iter().enumerate() {
            let Some(act) = person.act else { continue };
            if act >= 5 || !game.person_is_available(index) {
                continue;
            }
            persons[act] += 1.0;
            if person.origin == PersonOrigin::Card {
                cards[act] += 1.0;
                friend_sum[act] += f64::from(person.friendship());
            }
        }
        let total: f64 = persons.iter().sum();
        let share: [f64; 5] = if total > 0.0 {
            persons.map(|count| count / total)
        } else {
            [0.2; 5]
        };
        let avg_friend = std::array::from_fn(|act| {
            if cards[act] > 0.0 {
                friend_sum[act] / cards[act]
            } else {
                0.0
            }
        });

        let checkpoint = game
            .data
            .schedule
            .iter()
            .find(|schedule| schedule.turn >= turn)
            .map(|schedule| CheckpointCtx {
                required: schedule.need_act_level,
                turns_left: f64::from((schedule.turn - turn).max(0)),
            });

        Ok(Self {
            game,
            consts,
            cfg,
            pt_value: f64::from(consts.pt_score_rate) * cfg.pt_favor_rate,
            weights: cfg.weights.clone(),
            advice: cfg.advice.clone(),
            status_weights: cfg.status_weights,
            ctx: Context {
                train_turns,
                keiko_turns,
                camp_turns,
                senior_camp_left,
                persons,
                cards,
                avg_friend,
                share,
                checkpoint,
            },
        })
    }

    /// 单动作估值（评分点）
    pub(crate) fn action_value(&self, action: &Action) -> f64 {
        match action {
            Action::Train(train) => self.train_value(usize::from(*train), false, action),
            Action::ActTrain(act) => {
                let act = usize::from(*act);
                let Some(row) = self.game.data.acts.get(act) else {
                    return f64::NEG_INFINITY;
                };
                self.train_value(row.train, true, action)
            }
            Action::Race => self.race_value(),
            Action::Rest => self.rest_value(),
            Action::NormalOuting => self.normal_outing_value(),
            Action::FriendOuting => self.friend_outing_value(),
            Action::Clinic => self.clinic_value(),
            // 分配方案不是动作集候选；训练员在分配决策点单独择优
            Action::AssignAdvice(_) => f64::NEG_INFINITY,
        }
    }

    /// 事件选项组估值（组内结果行按 `prob` 加权；`choice_index` 用于剧本额外效果）
    pub(crate) fn event_choice_value(
        &self,
        event_id: u32,
        choice_index: usize,
        group: &[EventChoice],
    ) -> f64 {
        if group.is_empty() {
            return 0.0;
        }
        let total_prob: i32 = group.iter().map(|row| row.prob.max(0)).sum();
        let mut total = 0.0;
        for row in group {
            let weight = if total_prob > 0 {
                f64::from(row.prob.max(0)) / f64::from(total_prob)
            } else {
                1.0 / group.len() as f64
            };
            total += weight * self.event_row_value(event_id, row);
        }
        // 剧本额外效果（引擎 ActionValue 表达不了的部分）
        if event_id == KENSAN_EVENT_ID && choice_index == 0 {
            // 上档：最低演目槽 +1（三则、满槽跳过）+ 解一个 debuff
            total += self.weights.gauge;
            total += self.debuff_clear_value();
        }
        total
    }

    /// 教诲分配方案估值（与 `apply_assignment` 的 801 硬约束同口径）
    pub(crate) fn assignment_value(&self, assign: &AdviceAssignment) -> f64 {
        let effective = self.effective_assignment(assign);
        let mut total = 0.0;
        for (act, source) in effective.iter().enumerate() {
            let source = usize::from(*source);
            let level = self.game.acts.level[act].clamp(1, 10) as usize;
            let effect = self.game.advice_effect_of(source, level);
            total += self.slot_value(&effect, act);
        }
        total
    }

    // ------------------------------------------------------------------
    // 动作估值
    // ------------------------------------------------------------------

    /// 训练 / 演目稽古估值（成功收益 × (1-p) + 失败惩罚 × p）
    fn train_value(&self, train: usize, keiko: bool, action: &Action) -> f64 {
        let Ok(preview) = self.game.preview_training(train, keiko) else {
            return f64::NEG_INFINITY;
        };
        let fail_prob = (f64::from(preview.failure_rate) / 100.0).clamp(0.0, 1.0);
        let success = self.value_of(&preview.value)
            + self.resource_value(action)
            + self.training_friendship(train)
            + self.kensan_bonus(action);
        let fail = if keiko {
            -FAIL_PENALTY_KEIKO
        } else {
            -FAIL_PENALTY_TRAIN
        };
        success * (1.0 - fail_prob) + fail * fail_prob
    }

    /// 休息估值（普通 / 合宿版；含演目槽 +1）
    fn rest_value(&self) -> f64 {
        let vital = self.game.uma().vital;
        let mut value = if self.game.plan.camp_rest {
            self.vital_gain(vital, 40) + self.weights.motivation
        } else {
            let probs = &self.consts.rest_probs;
            let expected = if probs.len() >= 3 {
                let total: f64 = probs
                    .iter()
                    .map(|prob| f64::from(*prob))
                    .sum::<f64>()
                    .max(1.0);
                (f64::from(probs[0]) * 30.0
                    + f64::from(probs[1]) * 50.0
                    + f64::from(probs[2]) * 70.0)
                    / total
            } else {
                48.0
            };
            self.vital_gain(vital, expected as i32)
        };
        value += self.resource_value(&Action::Rest);
        // 低于休息阈值时再加权（阈值本身不硬守，仍由打分对比）
        let rest_vital = self.cfg.rest_vital;
        if vital < rest_vital {
            value += f64::from(rest_vital - vital) * self.weights.vital * 0.5;
        }
        value
    }

    /// 普通出行估值（出行事件期望 + 演目槽 +1）
    fn normal_outing_value(&self) -> f64 {
        let data = Data::get().ok();
        let events: Vec<&umaai_core::gamedata::EventData> = data
            .iter()
            .flat_map(|data| data.events.system_events.iter())
            .filter(|(key, _)| key.starts_with("normal_outing"))
            .map(|(_, event)| event)
            .collect();
        let expected = if events.is_empty() {
            60.0
        } else {
            events
                .iter()
                .filter_map(|event| event.choices.first())
                .map(|group| self.event_choice_value(0, 0, group))
                .sum::<f64>()
                / events.len() as f64
        };
        expected + self.resource_value(&Action::NormalOuting)
    }

    /// 友人出行估值（段事件期望 + 最低演目 Lv+1 + 槽 +1；攻略「尽早用完」加分）
    fn friend_outing_value(&self) -> f64 {
        if self.game.friend_outing_add_level().is_none() {
            return f64::NEG_INFINITY;
        }
        let step = self
            .game
            .friend
            .out_used
            .iter()
            .position(|used| !*used)
            .map(|index| index + 1)
            .unwrap_or(1);
        let expected = Data::get()
            .ok()
            .and_then(|data| data.events.friend_events.get(&step.to_string()).cloned())
            .map(|event| {
                event
                    .choices
                    .first()
                    .map_or(0.0, |group| self.event_choice_value(event.id, 0, group))
            })
            .unwrap_or(50.0);
        let mut value = expected + self.resource_value(&Action::FriendOuting);
        if self.cfg.friend_outing_early {
            value += FRIEND_OUTING_EARLY_BONUS;
        }
        value
    }

    /// 比赛估值（强制比赛回合给保底；自选比赛受 `self_race` 开关控制）
    fn race_value(&self) -> f64 {
        let forced = self.game.plan.race;
        if !forced && !self.cfg.self_race {
            return f64::NEG_INFINITY;
        }
        let mut value = self.estimated_race_reward() + self.resource_value(&Action::Race);
        if forced {
            value += FORCED_RACE_BASE;
        }
        value
    }

    /// 治病估值（生病 / 不擅长时才有价值）
    fn clinic_value(&self) -> f64 {
        let flags = &self.game.uma().flags;
        if flags.ill.is_empty() && !flags.bad_trainer {
            return 0.0;
        }
        let mut value = CLINIC_VALUE;
        if flags.ill.contains(Ill::FAT) {
            value += self.weights.fat;
        }
        value
    }

    /// 比赛奖励期望（目标赛 / URA 决赛 / 自选赛按等级取事件；吃比赛加成 scale）
    fn estimated_race_reward(&self) -> f64 {
        let key = if self.game.plan.ura_race {
            match self.game.turn() {
                73 => "race_ura_qualifier",
                75 => "race_ura_semifinal",
                _ => "race_ura_final",
            }
        } else if self.game.plan.race {
            "race_career"
        } else {
            match self.game.race_grade().unwrap_or(4) {
                1 => "race_g1",
                2 => "race_g2",
                3 => "race_g3",
                _ => "race_g4",
            }
        };
        let Some(event) = Data::get()
            .ok()
            .and_then(|data| data.events.system_events.get(key).cloned())
        else {
            return 80.0;
        };
        let base = event
            .choices
            .first()
            .map_or(0.0, |group| self.event_choice_value(event.id, 0, group));
        let scale = (100.0
            + f64::from(self.game.uma().race_bonus)
            + f64::from(self.game.scenario_race_bonus()))
            / 100.0;
        // 属性 / PT 吃 scale，体力不吃；体力项占比小，统一乘只影响排序而不改变量纲
        base * scale
    }

    // ------------------------------------------------------------------
    // 基础折算
    // ------------------------------------------------------------------

    /// 五维查表评分（先按本局上限夹取，再查 `five_status_final_score`）
    fn status_score(&self, axis: usize, value: i32) -> f64 {
        let table = &self.consts.five_status_final_score;
        let Some(&last) = table.last() else {
            return 0.0;
        };
        let limit = self.game.uma().five_status_limit[axis];
        let value = value.min(limit).max(0) as usize;
        let index = value.min(table.len() - 1);
        if index == table.len() - 1 {
            f64::from(last)
        } else {
            f64::from(table[index])
        }
    }

    /// 五维增量评分（含策略权重 `status_weights`）
    fn status_gain(&self, gains: &[i32; 5]) -> f64 {
        let uma = self.game.uma();
        let mut total = 0.0;
        for (axis, gain) in gains.iter().enumerate() {
            if *gain == 0 {
                continue;
            }
            let current = uma.five_status[axis];
            let after = (current + *gain).min(uma.five_status_limit[axis]);
            total += (self.status_score(axis, after) - self.status_score(axis, current))
                * self.status_weights[axis];
        }
        total
    }

    /// 单轴增量评分（含策略权重）
    fn status_gain_axis(&self, axis: usize, gain: i32) -> f64 {
        let mut gains = [0i32; 5];
        gains[axis] = gain;
        self.status_gain(&gains)
    }

    /// PT 增量评分（`pt_score_rate × pt_favor_rate`）
    fn pt_gain(&self, pt: i32) -> f64 {
        f64::from(pt) * self.pt_value
    }

    /// 体力价值曲线（分段递减；再乘配置权重）
    fn vital_curve(&self, vital: i32) -> f64 {
        let value = f64::from(vital.max(0));
        let raw = if value <= 50.0 {
            2.5 * value
        } else if value <= 70.0 {
            125.0 + 1.5 * (value - 50.0)
        } else {
            155.0 + (value - 70.0)
        };
        raw * self.weights.vital
    }

    /// 体力增量价值（按上限夹取）
    fn vital_gain(&self, current: i32, delta: i32) -> f64 {
        if delta == 0 {
            return 0.0;
        }
        let max = self.game.uma().max_vital.max(1);
        let current = current.clamp(0, max);
        let after = (current + delta).clamp(0, max);
        self.vital_curve(after) - self.vital_curve(current)
    }

    /// 一次 `ActionValue` 的评分折算（属性 + PT + 体力 + 干劲 + 羁绊 + 灵感）
    fn value_of(&self, value: &ActionValue) -> f64 {
        let gains: [i32; 5] = std::array::from_fn(|axis| value.status_pt[axis]);
        self.status_gain(&gains)
            + self.pt_gain(value.status_pt[5])
            + self.vital_gain(self.game.uma().vital, value.vital)
            + f64::from(value.motivation) * self.weights.motivation
            + f64::from(value.friendship) * self.weights.friendship
            + f64::from(value.hint_level) * self.weights.hint
    }

    /// 一次训练的羁绊 / 灵感 / 闪彩估值（引擎训练值不含这些，训练员侧补）
    fn training_friendship(&self, train: usize) -> f64 {
        let rules = &self.game.data.rules.friendship_gain;
        let lesson = self.game.participant_advice(train).relationship_gain;
        let mut total = 0.0;
        if let Some(slot) = self.game.base.distribution.get(train) {
            for &index in slot {
                if index < 0 {
                    continue;
                }
                let Some(person) = self.game.persons.get(index as usize) else {
                    continue;
                };
                let mut gain = match person.person_type() {
                    PersonType::Yayoi | PersonType::Reporter => 5,
                    _ => match person.origin {
                        PersonOrigin::Card => rules.card,
                        PersonOrigin::Friend => rules.friend,
                        _ => 0,
                    },
                };
                if lesson > 0 && matches!(person.origin, PersonOrigin::Card | PersonOrigin::Friend)
                {
                    gain += lesson;
                }
                if gain > 0 {
                    // 未满 80 的羁绊更值钱（离闪彩门槛更近）
                    let factor = if person.friendship() < 80 { 1.5 } else { 0.5 };
                    total += f64::from(gain) * self.weights.friendship * factor;
                }
                if person.hint() {
                    total += self.weights.hint;
                }
            }
        }
        total += self.game.shining_count(train) as f64 * SHINING_BONUS;
        total
    }

    /// 动作的演目资源价值（槽 / Lv / 达标急迫 / 亮起囤积惩罚）
    fn resource_value(&self, action: &Action) -> f64 {
        let preview = self.game.preview_action_resources(action);
        let lit = self.game.lit_acts();
        let lit_count = lit.iter().filter(|flag| **flag).count();
        let gauge_max = self.game.act_gauge_max();
        let mut total = 0.0;
        for (act, lit_act) in lit.iter().enumerate() {
            let level_gain = f64::from(preview.level_delta[act]);
            total += f64::from(preview.gauge_delta[act]) * self.weights.gauge;
            total += level_gain * self.level_value(act);
            // 多亮起囤积惩罚：本动作把某演目推满、且已有其他亮起时
            let after_gauge = self.game.acts.gauge[act] + preview.gauge_delta[act];
            if preview.gauge_delta[act] > 0
                && !lit_act
                && after_gauge >= gauge_max
                && lit_count >= 1
            {
                total -= LIT_HOARD_PENALTY;
            }
        }
        total
    }

    /// 演目 +1 Lv 的估值（普通价值 + 発表会急迫 + Senior 合宿前目标）
    fn level_value(&self, act: usize) -> f64 {
        let mut value = self.weights.act_level;
        if let Some(checkpoint) = &self.ctx.checkpoint
            && self.game.acts.level[act] < checkpoint.required
        {
            let margin = f64::from(self.cfg.checkpoint_margin).max(1.0);
            let urgency =
                ((margin * 2.0 - checkpoint.turns_left).max(0.0) / (margin * 2.0)).min(1.0);
            value += self.weights.checkpoint * (0.5 + urgency);
        }
        if self.cfg.act_level_target > 0
            && self.ctx.cards[act] > 0.0
            && self.ctx.senior_camp_left > 0.0
            && self.game.acts.level[act] < self.cfg.act_level_target
        {
            let urgency = ((30.0 - self.ctx.senior_camp_left).max(0.0) / 30.0).min(1.0);
            value += self.weights.act_level * (0.5 + urgency);
        }
        value
    }

    /// 兑现「追加稽古」掷中的额外价值（该演目担当参与本次训练）
    fn kensan_bonus(&self, action: &Action) -> f64 {
        if !self.game.kensan_pending {
            return 0.0;
        }
        let Some(kensan_act) = self.game.kensan_act() else {
            return 0.0;
        };
        let hit = match action {
            Action::Train(train) => self
                .game
                .base
                .distribution
                .get(usize::from(*train))
                .is_some_and(|slot| {
                    slot.iter().any(|index| {
                        *index >= 0
                            && self
                                .game
                                .persons
                                .get(*index as usize)
                                .is_some_and(|person| person.act == Some(kensan_act))
                    })
                }),
            Action::ActTrain(act) => usize::from(*act) == kensan_act,
            _ => false,
        };
        if hit { KENSAN_TRIGGER_BONUS } else { 0.0 }
    }

    // ------------------------------------------------------------------
    // 事件估值
    // ------------------------------------------------------------------

    /// 单行事件结果估值（含友人卡加成 / ExtraStatus / 状态增删）
    fn event_row_value(&self, event_id: u32, row: &EventChoice) -> f64 {
        let mut value = row.value.clone();
        if self.game.base.friend_event_ids.contains(&event_id) {
            let bonus = self.game.friend.event_bonus;
            if bonus != 0 {
                for slot in &mut value.status_pt {
                    if *slot != 0 {
                        *slot = *slot * (100 + bonus) / 100;
                    }
                }
            }
            let vital_bonus = self.game.friend.vital_bonus;
            if vital_bonus != 0 && value.vital > 0 {
                value.vital = value.vital * (100 + vital_bonus) / 100;
            }
        }
        self.value_of(&value)
            + self.extra_status_value(row.extra_status.as_ref())
            + self.flags_value(row.add_flags.as_ref(), -1.0)
            + self.flags_value(row.remove_flags.as_ref(), 1.0)
    }

    /// `ExtraStatus` 的期望价值（随机项按当前五维平均边际；上次训练项取 `last_train` 轴）
    fn extra_status_value(&self, extra: Option<&ExtraStatus>) -> f64 {
        match extra {
            Some(ExtraStatus::Random { count, value }) => {
                let count = usize::from(*count).min(5) as f64;
                let value = f64::from(*value);
                let average: f64 = (0..5)
                    .map(|axis| self.status_gain_axis(axis, 1))
                    .sum::<f64>()
                    / 5.0;
                count * value * average
            }
            Some(ExtraStatus::LastTrain { value }) => match self.game.base.last_train {
                Some(train) if train < 5 => self.status_gain_axis(train, *value),
                _ => 0.0,
            },
            None => 0.0,
        }
    }

    /// 状态增删的估值（`sign`：+1 = 移除（收益），-1 = 添加（惩罚））
    fn flags_value(&self, flags: Option<&UmaFlags>, sign: f64) -> f64 {
        let Some(flags) = flags else {
            return 0.0;
        };
        let mut magnitude = 0.0;
        if flags.ill.contains(Ill::FAT) {
            magnitude += self.weights.fat;
        }
        for kind in [
            Ill::SKIN,
            Ill::HEADACHE,
            Ill::LAZY,
            Ill::NIGHT,
            Ill::BAD_TRAINER,
        ] {
            if flags.ill.contains(kind) {
                magnitude += OTHER_ILL_PENALTY;
            }
        }
        if flags.bad_trainer {
            magnitude += OTHER_ILL_PENALTY;
        }
        if flags.aijiao || flags.good_trainer {
            magnitude += 30.0;
        }
        if flags.refresh_mind > 0 || flags.lucky_count > 0 {
            magnitude += 20.0;
        }
        magnitude * sign
    }

    /// 追加稽古上档「解一个 debuff」的估值
    fn debuff_clear_value(&self) -> f64 {
        let flags = &self.game.uma().flags;
        if flags.ill.contains(Ill::FAT) {
            return self.weights.fat;
        }
        if flags.bad_trainer || !flags.ill.is_empty() {
            return 60.0;
        }
        0.0
    }

    // ------------------------------------------------------------------
    // 教诲分配估值
    // ------------------------------------------------------------------

    /// 应用 `apply_assignment` 的 801 硬约束后的有效分配（估值与落地同口径）
    fn effective_assignment(&self, assign: &AdviceAssignment) -> [u8; 5] {
        let mut effective = assign.0;
        let Some(source) = (0..5).find(|slot| self.game.advice.fused[*slot] == 801) else {
            return effective;
        };
        let target = (0..5)
            .max_by_key(|act| {
                (
                    self.game
                        .persons
                        .iter()
                        .filter(|person| person.act == Some(*act))
                        .count(),
                    usize::MAX - *act,
                )
            })
            .unwrap_or(0);
        if usize::from(effective[target]) == source {
            return effective;
        }
        let Some(position) = (0..5).find(|act| usize::from(effective[*act]) == source) else {
            return effective;
        };
        effective.swap(target, position);
        effective
    }

    /// 单个教诲槽在指定演目上的估值
    fn slot_value(&self, effect: &AdviceSlotEffect, act: usize) -> f64 {
        let train = self.act_train_est(act);
        let keiko = self.act_keiko_est(act);
        let friend = self.friend_training_prob(act);
        let non_friend = 1.0 - friend;
        let persons = self.ctx.persons[act];
        let cards = self.ctx.cards[act];
        // 训练值类（吃「预计训练次数」）
        let mut train_value = 0.0;
        train_value += f64::from(effect.main_gain) * 0.6 * train;
        train_value +=
            f64::from(effect.non_friend_training_effect) * 0.6 * (train + keiko) * non_friend;
        train_value += f64::from(effect.dual_sub_gain) * 0.3 * train;
        train_value += f64::from(effect.deyilv) / 50.0 * 3.0 * train;
        train_value += f64::from(effect.hint_rate) / 80.0 * 0.5 * train * cards;
        let mut value = train_value * self.advice.train;
        // 稽古类（吃「预计稽古次数」）
        let mut keiko_value = 0.0;
        keiko_value += f64::from(effect.keiko_main_bonus) * 0.6 * keiko;
        keiko_value += f64::from(effect.keiko_training_effect) * 0.6 * keiko;
        keiko_value += f64::from(effect.keiko_vital_cost_drop) * 0.4 * keiko;
        keiko_value += f64::from(effect.keiko_friendship) * 0.6 * keiko * friend;
        keiko_value += f64::from(effect.keiko_hint_level) * 4.0 * keiko;
        value += keiko_value * self.advice.keiko;
        // 羁绊 / 协力（吃卡数权重）
        value += (f64::from(effect.relationship_gain) * 2.0 * (train + keiko)
            + f64::from(effect.cooperate_friendship) * 0.6 * (train + keiko) * friend)
            * self.advice.card;
        // 分身 / 合宿额外人（吃担当数权重）
        value += (f64::from(effect.cooperate_split) * persons * 4.0
            + f64::from(effect.camp_extra_person) * persons * self.ctx.camp_turns * 1.5)
            * self.advice.npc;
        // 研钻（追加稽古；吃演目 Lv 权重）
        value +=
            f64::from(effect.kensan_rate) / 100.0 * (train + keiko) * 150.0 * self.advice.level;
        // 不屈（吃达标 / 风险权重）
        if let Some(threshold) = effect.fail_vital_threshold {
            let vital = self.game.uma().vital;
            let reliability = if threshold == 0 {
                1.0
            } else {
                ((f64::from(vital - threshold)) / 20.0).clamp(0.0, 1.0)
            };
            value += reliability * 0.04 * (train + keiko) * 200.0 * self.advice.milestone;
        }
        value
    }

    /// 预计该演目训练位上的普通训练次数（含达标急迫加权）
    fn act_train_est(&self, act: usize) -> f64 {
        let urgency = self.act_urgency(act);
        self.ctx.train_turns * self.ctx.share[act] * (1.0 + self.advice.milestone * urgency)
    }

    /// 预计该演目的稽古次数（合宿均摊 + 亮起排练）
    fn act_keiko_est(&self, act: usize) -> f64 {
        let urgency = self.act_urgency(act);
        self.ctx.keiko_turns / 5.0 * (1.0 + self.advice.milestone * urgency)
    }

    /// 该演目训练触发友情（彩圈）的粗略概率（用于「非友情」条件的期望）
    fn friend_training_prob(&self, act: usize) -> f64 {
        if self.ctx.cards[act] <= 0.0 {
            return 0.0;
        }
        let average = self.ctx.avg_friend[act] / 100.0;
        (self.ctx.cards[act] * average / 3.0).clamp(0.0, 0.85)
    }

    /// 达标急迫度（0–1；発表会缺 Lv / Senior 合宿前目标）
    fn act_urgency(&self, act: usize) -> f64 {
        let mut urgency = 0.0f64;
        if let Some(checkpoint) = &self.ctx.checkpoint
            && self.game.acts.level[act] < checkpoint.required
        {
            let margin = f64::from(self.cfg.checkpoint_margin).max(1.0);
            urgency = urgency
                .max(((margin * 2.0 - checkpoint.turns_left).max(0.0) / (margin * 2.0)).min(1.0));
        }
        if self.cfg.act_level_target > 0
            && self.ctx.cards[act] > 0.0
            && self.ctx.senior_camp_left > 0.0
            && self.game.acts.level[act] < self.cfg.act_level_target
        {
            urgency = urgency.max(((30.0 - self.ctx.senior_camp_left).max(0.0) / 30.0).min(1.0));
        }
        urgency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_game;

    /// 大胃王：体力充足时避开变胖风险；体力见底时吃大份
    #[test]
    fn big_eater_choice_follows_vital() {
        let mut game = sample_game();
        let event = Data::get()
            .expect("数据已初始化")
            .events
            .uma_events
            .iter()
            .find(|event| event.id == 5010)
            .expect("大胃王事件")
            .clone();
        game.uma_mut().vital = 90;
        let model = Model::new(&game, &game.config.trainer).expect("模型");
        let high_small = model.event_choice_value(5010, 0, &event.choices[0]);
        let high_big = model.event_choice_value(5010, 1, &event.choices[1]);
        assert!(high_small > high_big, "体力充足时避免变胖风险");
        game.uma_mut().vital = 5;
        let model = Model::new(&game, &game.config.trainer).expect("模型");
        let low_small = model.event_choice_value(5010, 0, &event.choices[0]);
        let low_big = model.event_choice_value(5010, 1, &event.choices[1]);
        assert!(low_big > low_small, "体力见底时吃大份");
    }

    /// 教诲分配：价值矩阵会把 801 槽放到担当数最多的演目（与 apply_assignment 硬约束同口径）
    #[test]
    fn assignment_puts_801_on_most_assigned_act() {
        let mut game = sample_game();
        game.acquire_advice();
        game.advice.fused = [801, 0, 0, 0, 0];
        let model = Model::new(&game, &game.config.trainer).expect("模型");
        let candidates = game.assignment_candidates();
        let best = candidates
            .iter()
            .max_by(|left, right| {
                model
                    .assignment_value(left)
                    .partial_cmp(&model.assignment_value(right))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("120 个候选")
            .to_owned();
        let mut probe = game.clone();
        probe.apply_assignment(&best).expect("合法分配");
        let counts: Vec<usize> = (0..5)
            .map(|act| {
                probe
                    .persons
                    .iter()
                    .filter(|person| person.act == Some(act))
                    .count()
            })
            .collect();
        let target = (0..5)
            .max_by_key(|act| (counts[*act], usize::MAX - *act))
            .expect("5 演目");
        let slot = (0..5)
            .find(|act| probe.advice.fused[*act] == 801)
            .expect("801 已分配");
        assert_eq!(slot, target, "801 应落在担当数最多的演目（{counts:?}）");
    }

    /// 五维边际评分：正增量有分、到上限后为 0
    #[test]
    fn status_marginal_saturates_at_limit() {
        let mut game = sample_game();
        let model = Model::new(&game, &game.config.trainer).expect("模型");
        assert!(model.status_gain(&[50, 0, 0, 0, 0]) > 0.0, "速度增量应有分");
        let limit = game.uma().five_status_limit[0];
        game.uma_mut().five_status[0] = limit;
        let model = Model::new(&game, &game.config.trainer).expect("模型");
        assert_eq!(
            model.status_gain(&[50, 0, 0, 0, 0]),
            0.0,
            "到上限后边际为 0"
        );
    }
}
