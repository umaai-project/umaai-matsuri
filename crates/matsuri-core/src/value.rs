//! 手写训练员估值层：局面 / 动作 / 事件 / 教诲分配 → 统一评分单位
//!
//! 目标口径：官方评分（`umaai_core::score::status_point` 单项属性公式 + PT × `pt_score_rate`），
//! PT 再乘训练员配置的 `pt_favor_rate`（`1.0` = 官方评分，引擎既有定档 `2.0`）。
//! 五维公式以原始属性为输入、没有属性上限；`data/constants.json` 已删除
//! `five_status_final_score` 表（该表在引擎侧仅为旧数据兼容保留）。
//! 全部估值都是**评分点**，训练员只比较同一量纲的分数，不加第二个评分轴。
//!
//! 纪律：只读局面（`preview_*` 不耗随机流、不写局面）；纯函数式打分，供手写训练员与
//! 手动面板复用。后续扫参只改 `[trainer]` 配置段，不改这里的公式骨架。

use std::cell::OnceCell;

use anyhow::Result;
use umaai_core::gamedata::{ActionValue, EventChoice, ExtraStatus, GameConstants};
use umaai_core::{Data, Ill, UmaFlags};
use umasim::game::{Game, Person, PersonType};

use crate::deck_config::{AdviceWeights, TrainerConfig, ValueWeights};
use crate::game::advice::{AdviceAssignment, AdviceSlotEffect};
use crate::game::event::{EXTRA_TRAIN_EVENT_ID, KENSAN_EVENT_ID};
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
/// 「解一个 debuff」在非变胖目标上的价值（追加稽古上档）
const DEBUFF_CLEAR_VALUE: f64 = 60.0;
/// 单次比赛的保底价值（强制比赛回合只有比赛一个候选）
const FORCED_RACE_BASE: f64 = 500.0;
/// 满体力闲逛惩罚（vital ≥ 70 时休息 / 普通出行扣分；GA 验证行为修正）
const HIGH_VITAL_IDLE_PENALTY: f64 = 200.0;
/// 满体力阈值
const HIGH_VITAL_THRESHOLD: i32 = 70;
/// 演目 Lv 上限（有 SSR 吾妻时可到 10）
const ACT_LEVEL_CAP: i32 = 10;
/// 第三年合宿结束后的稽古兑现窗口末回合（本番后空窗期结束、URA 前）
const ENDGAME_LAST_TURN: i32 = 71;

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
    /// 五维动态权重乘子（`status_catchup`；全 1 = 关闭）
    dyn_status: [f64; 5],
    /// 演目 Lv 动态权重乘子（`act_catchup`；全 1 = 关闭）
    dyn_act: [f64; 5],
    /// 体力影子价格乘子（`vital_decay`；1 = 关闭）
    vital_scale: f64,
    /// 稽古节奏乘子（`keiko_tempo`；1 = 关闭，>1 落后要加速，<1 领先可卡回合）
    tempo_scale: f64,
    /// 近顶轴标记（按**生效上限**判定；`overflow.margin`）
    near_cap: [bool; 5],
    /// 非近顶轴的溢出放大因子（全 1 = 关闭）
    overflow_factor: [f64; 5],
    /// 第三年合宿结束～T71 的普通回合窗口
    endgame: bool,
    /// 满槽数超过剩余普通回合的比例（0–1）
    endgame_pressure: f64,
}

/// 下一次発表会 / 本番的达标压力
struct CheckpointCtx {
    /// 需要的演目 Lv
    required: i32,
    /// 距该回合还剩多少回合
    turns_left: f64,
}

/// 教诲分配缓存
///
/// 120 个候选只由 5 个槽下标×5 个演目组合而成：槽位价值与候选无关，801 目标演目也与
/// 候选无关。首次分配打分时算一次，之后查表，结果与逐候选重算逐位一致。
struct AdviceCache {
    /// 槽位价值：[act][source]
    slot: [[f64; 5]; 5],
    /// 801 硬约束的目标演目（无 801 时为 None）
    target_801: Option<usize>,
    /// 落位先验命中表：[act][source]（先验只依赖槽内容 × 演目，候选侧只查表计数）
    route_hit: [[bool; 5]; 5],
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
    /// 教诲分配查表缓存（首次分配打分时惰性建立）
    advice_cache: OnceCell<AdviceCache>,
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

        // ---- 动态权重（全部默认 0 → 乘子恒为 1，逐位退化成静态打分）----
        let dynamic = &cfg.dynamic;
        let max_turn = f64::from(game.max_turn().max(1));
        let phase = (f64::from(turn) / max_turn).clamp(0.0, 1.0);

        // ① 五维追赶：剩余空间占比高于均值的轴加权（速先满 → 把回合让给根 / 智）
        let dyn_status = if dynamic.status_catchup > 0.0 {
            let uma = game.uma();
            let head: [f64; 5] = std::array::from_fn(|axis| {
                let limit = uma.five_status_limit[axis].max(1);
                let value = uma.five_status[axis].clamp(0, limit);
                f64::from(limit - value) / f64::from(limit)
            });
            let mean = head.iter().sum::<f64>() / 5.0;
            std::array::from_fn(|axis| {
                (1.0 + dynamic.status_catchup * 2.0 * (head[axis] - mean)).max(0.0)
            })
        } else {
            [1.0; 5]
        };

        // ② 演目 Lv 追赶：落后于全队均值的演目升级更值钱（治 10/10/10/8/8 长尾）
        let level = game.acts.level;
        let dyn_act = if dynamic.act_catchup > 0.0 {
            let mean = level.iter().map(|value| f64::from(*value)).sum::<f64>() / 5.0;
            std::array::from_fn(|act| {
                let gap = (mean - f64::from(level[act])).max(0.0);
                (1.0 + dynamic.act_catchup * gap).clamp(0.0, 4.0)
            })
        } else {
            [1.0; 5]
        };

        // ③ 体力影子价格随回合衰减：末期每点体力换不到几次训练，不值钱了
        let vital_scale = 1.0 - dynamic.vital_decay * phase;

        // ④ 稽古节奏：与「达标线」比短板 Lv。落后 → 加速升级；领先 → 压低升级价值，
        //    让休息 / 出行有机会赢，即「卡回合」把回合留给更好的机会。
        //    达标线口径：Lv1 起，Senior 合宿前推到 `act_level_target`，收尾推到 Lv 上限。
        let tempo_scale = if dynamic.keiko_tempo > 0.0 {
            let target = f64::from(cfg.act_level_target.clamp(1, ACT_LEVEL_CAP));
            let cap = f64::from(ACT_LEVEL_CAP);
            let camp_turn = f64::from(game.data.camp.windows.get(2).map_or(60, |w| w.start));
            let now = f64::from(turn);
            let line = if now <= camp_turn {
                1.0 + (target - 1.0) * (now / camp_turn.max(1.0))
            } else {
                let end = target.max(cap);
                target + (end - target) * (now - camp_turn) / (max_turn - camp_turn).max(1.0)
            };
            let lead = f64::from(level.iter().min().copied().unwrap_or(1)) - line;
            (1.0 - dynamic.keiko_tempo * lead / 3.0).clamp(0.25, 4.0)
        } else {
            1.0
        };

        // ⑤ 溢出：按**生效上限**标记近顶轴，只放大非近顶轴；主属性近顶时另外保留
        //    副属性 / PT 的价值（见 `train_value` 的 `keep`）。
        let overflow = &cfg.overflow;
        let mut near_cap = [false; 5];
        let mut near_count = 0usize;
        if overflow.margin > 0.0 || overflow.tilt > 0.0 || overflow.keep > 0.0 {
            for (axis, near) in near_cap.iter_mut().enumerate() {
                let limit = game.uma().five_status_limit[axis].max(1);
                let value = game.uma().five_status[axis].clamp(0, limit);
                let ratio = f64::from(limit - value) / f64::from(limit);
                *near = ratio <= overflow.margin;
                if *near {
                    near_count += 1;
                }
            }
        }
        let overflow_factor: [f64; 5] = std::array::from_fn(|axis| {
            if near_count > 0 && !near_cap[axis] {
                1.0 + overflow.tilt * near_count as f64 / 5.0
            } else {
                1.0
            }
        });

        // ⑥ 第三年合宿结束～T71：只在普通回合统计「满槽数 vs 剩余普通回合」。
        //    URA（72–77）全稽古、每回合都能选一个，不进入本窗口。
        let camp_end = game
            .data
            .camp
            .windows
            .get(2)
            .map_or(63, |window| window.end);
        let endgame = turn > camp_end
            && turn <= ENDGAME_LAST_TURN
            && !game.plan.camp_zone
            && !game.plan.ura_period
            && !game.plan.race
            && !game.forced_rest;
        let endgame_pressure = if endgame {
            let pending = game.lit_acts().iter().filter(|lit| **lit).count();
            let normal_turns = (turn..=ENDGAME_LAST_TURN)
                .filter(|t| !game.uma().is_race_turn(*t))
                .count();
            if pending > normal_turns {
                (pending - normal_turns) as f64 / pending as f64
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(Self {
            game,
            consts,
            cfg,
            pt_value: f64::from(consts.pt_score_rate) * cfg.pt_favor_rate,
            weights: cfg.weights.clone(),
            advice: cfg.advice.clone(),
            status_weights: cfg.status_weights,
            advice_cache: OnceCell::new(),
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
                dyn_status,
                dyn_act,
                vital_scale,
                tempo_scale,
                near_cap,
                overflow_factor,
                endgame,
                endgame_pressure,
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
        // 「追加訓練」上档（体力 −5）：附带定点治疗变胖（`event.rs::apply_event_extra` 同口径）
        if event_id == EXTRA_TRAIN_EVENT_ID && choice_index == 0 && self.is_fat() {
            total += self.weights.fat;
        }
        total
    }

    /// 当前是否变胖（训练速度分量归零的状态）
    fn is_fat(&self) -> bool {
        self.game.uma().flags.ill.contains(Ill::FAT)
    }

    /// 教诲分配方案估值（与 `apply_assignment` 的 801 硬约束同口径）
    pub(crate) fn assignment_value(&self, assign: &AdviceAssignment) -> f64 {
        if assign.0.iter().any(|slot| *slot >= 5) {
            return f64::NEG_INFINITY;
        }
        let cache = self.advice_cache.get_or_init(|| self.build_advice_cache());
        let effective = self.effective_assignment(assign, cache.target_801);
        let mut total = 0.0;
        for (act, source) in effective.iter().enumerate() {
            total += cache.slot[act][usize::from(*source)];
        }
        if self.advice.route > 0.0 {
            let hits = effective
                .iter()
                .enumerate()
                .filter(|&(act, source)| cache.route_hit[act][usize::from(*source)])
                .count();
            total += hits as f64 * self.advice.route;
        }
        total
    }

    /// 建立教诲分配查表（槽位价值 5×5 + 801 目标演目）
    fn build_advice_cache(&self) -> AdviceCache {
        let mut slot = [[0.0f64; 5]; 5];
        for (act, row) in slot.iter_mut().enumerate() {
            let level = self.game.acts.level[act].clamp(1, 10) as usize;
            for (source, cell) in row.iter_mut().enumerate() {
                let effect = self.game.advice_effect_of(source, level);
                *cell = self.slot_value(&effect, act);
            }
        }
        let target_801 = self.game.advice.fused.contains(&801).then(|| {
            (0..5)
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
                .unwrap_or(0)
        });
        // 落位先验：先验只依赖「槽内容 × 演目」，候选无关 → 一次查表
        let mut route_hit = [[false; 5]; 5];
        if self.advice.route > 0.0
            && let Some(prior) = self
                .game
                .plan
                .advice_window
                .and_then(|window| self.game.config.deck.advice.window(window))
        {
            for (slot_index, slot) in self.game.advice.slots.iter().enumerate().take(5) {
                for (act, expected) in prior.iter().enumerate() {
                    route_hit[act][slot_index] = *slot == *expected;
                }
            }
        }
        AdviceCache {
            slot,
            target_801,
            route_hit,
        }
    }

    // ------------------------------------------------------------------
    // 动作估值
    // ------------------------------------------------------------------

    /// 训练 / 演目稽古估值（成功收益 × (1-p) + 失败惩罚 × p）
    fn train_value(&self, train: usize, keiko: bool, action: &Action) -> f64 {
        // 预览同时返回参与教诲与闪彩人数，供羁绊估值复用（同一候选不重算）
        let Ok((preview, advice, shining)) = self.game.preview_training_full(train, keiko) else {
            return f64::NEG_INFINITY;
        };
        let fail_prob = (f64::from(preview.failure_rate) / 100.0).clamp(0.0, 1.0);
        let mut success = self.value_of_with(&preview.value, true)
            + self.resource_value(action)
            + self.training_friendship(train, &advice, shining)
            + self.kensan_bonus(action);
        // 主属性近顶时，副属性 / PT 仍是速训练的价值来源：给一个小幅保留加分，
        // 避免溢出层把「速训练 → 力量 / PT」完全倾斜掉。
        if let Some(main) = self.train_axis(action)
            && self.ctx.near_cap[main]
            && self.cfg.overflow.keep > 0.0
        {
            let gains: [i32; 5] = std::array::from_fn(|axis| {
                if axis == main {
                    0
                } else {
                    preview.value.status_pt[axis]
                }
            });
            success += self.cfg.overflow.keep
                * (self.status_gain(&gains) + self.pt_gain(preview.value.status_pt[5]));
        }
        if keiko {
            success += self.endgame_keiko_bonus(action);
        }
        let fail = if keiko {
            -FAIL_PENALTY_KEIKO
        } else {
            -FAIL_PENALTY_TRAIN
        };
        let mut expected = success * (1.0 - fail_prob) + fail * fail_prob;
        // 末段普通回合：满槽 > 可兑现回合时，普通训练让位（只扣 Train，不扣休息 / 治病）。
        if !keiko
            && self.ctx.endgame
            && self.ctx.endgame_pressure > 0.0
            && matches!(action, Action::Train(_))
        {
            expected -= self.cfg.endgame.normal_penalty * self.ctx.endgame_pressure;
        }
        expected
    }

    /// 训练动作对应的主属性轴（ActionResult 的下标）
    fn train_axis(&self, action: &Action) -> Option<usize> {
        match action {
            Action::Train(train) => Some(usize::from(*train)),
            Action::ActTrain(act) => self
                .game
                .data
                .acts
                .get(usize::from(*act))
                .map(|row| row.train),
            _ => None,
        }
    }

    /// 第三年合宿结束～T71 的满槽稽古加分（URA 不进入 `ctx.endgame`）
    fn endgame_keiko_bonus(&self, action: &Action) -> f64 {
        if !self.ctx.endgame || self.ctx.endgame_pressure <= 0.0 {
            return 0.0;
        }
        let Action::ActTrain(act) = action else {
            return 0.0;
        };
        let act = usize::from(*act);
        let Some(gauge) = self.game.acts.gauge.get(act) else {
            return 0.0;
        };
        let mut bonus = 0.0;
        if *gauge >= self.game.act_gauge_max() {
            bonus += self.cfg.endgame.keiko
                * (1.0 + self.cfg.endgame.scarcity * self.ctx.endgame_pressure);
        }
        let level = self.game.acts.level[act];
        if level < ACT_LEVEL_CAP {
            bonus += self.cfg.endgame.level * f64::from((ACT_LEVEL_CAP - level).min(2));
        }
        bonus
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
        // 满体力闲逛惩罚（GA 验证：vital ≥ 70 时休息是浪费）
        value -= self.idle_penalty(vital);
        value
    }

    /// 普通出行估值（出行事件期望 + 演目槽 +1）
    fn normal_outing_value(&self) -> f64 {
        // 单遍累加：原实现先把 22 条系统事件过滤进临时 Vec，再取长度与求和
        let mut sum = 0.0;
        let mut count = 0usize;
        if let Ok(data) = Data::get() {
            for (key, event) in &data.events.system_events {
                if !key.starts_with("normal_outing") {
                    continue;
                }
                count += 1;
                if let Some(group) = event.choices.first() {
                    sum += self.event_choice_value(0, 0, group);
                }
            }
        }
        let expected = if count == 0 { 60.0 } else { sum / count as f64 };
        let mut value = expected + self.resource_value(&Action::NormalOuting);
        // 满体力闲逛惩罚（GA 验证：vital ≥ 70 时普通出行是浪费）
        value -= self.idle_penalty(self.game.uma().vital);
        value
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
        // 段键是固定 1–5 的字面量：直接借用查表，避免 `step.to_string()` 与整条事件深拷贝
        const STEP_KEYS: [&str; 5] = ["1", "2", "3", "4", "5"];
        let expected = Data::get()
            .ok()
            .and_then(|data| {
                STEP_KEYS
                    .get(step.saturating_sub(1))
                    .and_then(|key| data.events.friend_events.get(*key))
            })
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

    /// 自选比赛义务紧迫度（0–1）：区间剩余需打场数 / 剩余可赛回合数
    ///
    /// `1.0` = 再不打赢不了（训练员另有硬保底守门）；不在义务区间 / 已达标 → `0`。
    fn free_race_urgency(&self) -> f64 {
        self.game.free_race_progress().map_or(0.0, |(need, left)| {
            if need >= left {
                1.0
            } else {
                f64::from(need) / f64::from(left.max(1))
            }
        })
    }

    /// 比赛估值（强制比赛回合给保底；自选比赛受 `self_race` 开关控制）
    ///
    /// 义务自选赛（`freeRaces` 区间）另加紧迫度软加分：
    /// `free_race_urgency × urgency × 保底分`；`self_race = false` 时只保留义务补足相关价值，
    /// 纯额外自由赛不参与比较。
    fn race_value(&self) -> f64 {
        let forced = self.game.plan.race;
        let urgency = if forced {
            0.0
        } else {
            self.free_race_urgency()
        };
        if !forced && !self.cfg.self_race && urgency <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let mut value = self.estimated_race_reward() + self.resource_value(&Action::Race);
        if forced {
            value += FORCED_RACE_BASE;
        }
        value += self.cfg.free_race_urgency * urgency * FORCED_RACE_BASE;
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

    /// 比赛奖励期望（目标赛 / URA 决赛 / 自选赛按等级取事件；五维与 PT 分别吃倍率）
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
        let Some(mut event) = Data::get()
            .ok()
            .and_then(|data| data.events.system_events.get(key).cloned())
        else {
            return 80.0;
        };
        // 五维与 PT 倍率不同（铁心 PT ×0.46）：先按结算口径缩放面板，再估值
        let stats_scale = self.game.race_scale();
        let pt_scale = stats_scale * self.game.race_pt_rate();
        for group in &mut event.choices {
            for row in group {
                for point in &mut row.value.status_pt[..5] {
                    *point = (f64::from(*point) * stats_scale).floor() as i32;
                }
                row.value.status_pt[5] =
                    (f64::from(row.value.status_pt[5]) * pt_scale).floor() as i32;
            }
        }
        event
            .choices
            .first()
            .map_or(0.0, |group| self.event_choice_value(event.id, 0, group))
    }

    // ------------------------------------------------------------------
    // 基础折算
    // ------------------------------------------------------------------

    /// 五维公式评分（先按本局上限夹取，再代入单项属性公式）
    fn status_score(&self, axis: usize, value: i32) -> f64 {
        let limit = self.game.uma().five_status_limit[axis];
        let value = value.min(limit).max(0);
        f64::from(umaai_core::score::status_point(value))
    }

    /// 五维增量评分（含策略权重 `status_weights`）
    fn status_gain(&self, gains: &[i32; 5]) -> f64 {
        self.status_gain_with(gains, false)
    }

    /// 五维增量评分；`training` = 是否套用溢出层的非近顶轴放大
    ///
    /// 事件 / 比赛 / 教诲分配走 `status_gain`（不放大），只有训练估值走 `training=true`，
    /// 避免溢出层污染事件与比赛口径。
    fn status_gain_with(&self, gains: &[i32; 5], training: bool) -> f64 {
        let uma = self.game.uma();
        let mut total = 0.0;
        for (axis, gain) in gains.iter().enumerate() {
            if *gain == 0 {
                continue;
            }
            let current = uma.five_status[axis];
            let after = (current + *gain).min(uma.five_status_limit[axis]);
            let overflow = if training {
                self.ctx.overflow_factor[axis]
            } else {
                1.0
            };
            total += (self.status_score(axis, after) - self.status_score(axis, current))
                * self.status_weights[axis]
                * self.ctx.dyn_status[axis]
                * overflow;
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
        raw * self.weights.vital * self.ctx.vital_scale
    }

    /// 满体力闲置惩罚（休息 / 普通出行；`vital_idle` 让它随超出量线性放大）
    fn idle_penalty(&self, vital: i32) -> f64 {
        if vital < HIGH_VITAL_THRESHOLD {
            return 0.0;
        }
        let over = f64::from(vital - HIGH_VITAL_THRESHOLD);
        HIGH_VITAL_IDLE_PENALTY * (1.0 + self.cfg.dynamic.vital_idle * over / 30.0)
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
        self.value_of_with(value, false)
    }

    /// `ActionValue` 折算；`training` = 是否套用溢出层的非近顶轴放大
    fn value_of_with(&self, value: &ActionValue, training: bool) -> f64 {
        let gains: [i32; 5] = std::array::from_fn(|axis| value.status_pt[axis]);
        self.status_gain_with(&gains, training)
            + self.pt_gain(value.status_pt[5])
            + self.vital_gain(self.game.uma().vital, value.vital)
            + f64::from(value.motivation) * self.weights.motivation
            + f64::from(value.friendship) * self.weights.friendship
            + f64::from(value.hint_level) * self.weights.hint
    }

    /// 一次训练的羁绊 / 灵感 / 闪彩估值（引擎训练值不含这些，训练员侧补）
    fn training_friendship(
        &self,
        train: usize,
        advice: &crate::game::advice::ParticipantAdvice,
        shining: usize,
    ) -> f64 {
        let rules = &self.game.data.rules.friendship_gain;
        let lesson = advice.relationship_gain;
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
        total += shining as f64 * SHINING_BONUS;
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
        // 动态调制：演目 Lv 追赶 × 稽古节奏（落后加速 / 领先卡回合）
        value * self.ctx.dyn_act.get(act).copied().unwrap_or(1.0) * self.ctx.tempo_scale
    }

    /// 兑现「追加稽古」掷中的额外价值（该训练位参与者含研鑽绑定演目担当）
    ///
    /// 判定与 `train.rs::try_kensan` 同口径：普通训练与稽古都按**训练位占位**里的
    /// 「担当 = 研鑽绑定演目」判断，不能只比较稽古下标（稽古位与担当归属不一致）。
    fn kensan_bonus(&self, action: &Action) -> f64 {
        if !self.game.kensan_pending {
            return 0.0;
        }
        let Some(kensan_act) = self.game.kensan_act() else {
            return 0.0;
        };
        let train = match action {
            Action::Train(train) => usize::from(*train),
            Action::ActTrain(act) => self
                .game
                .data
                .acts
                .get(usize::from(*act))
                .map_or(usize::from(*act), |row| row.train),
            _ => return 0.0,
        };
        let hit = self.game.base.distribution.get(train).is_some_and(|slot| {
            slot.iter().any(|index| {
                *index >= 0
                    && self
                        .game
                        .persons
                        .get(*index as usize)
                        .is_some_and(|person| person.act == Some(kensan_act))
            })
        });
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
            + self.flags_value(row.add_flags.as_ref(), 1.0)
            + self.flags_value(row.remove_flags.as_ref(), -1.0)
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
        let mut harm = 0.0;
        let mut benefit = 0.0;
        if flags.ill.contains(Ill::FAT) {
            harm += self.weights.fat;
        }
        for kind in [Ill::SKIN, Ill::HEADACHE, Ill::LAZY, Ill::NIGHT] {
            if flags.ill.contains(kind) {
                harm += OTHER_ILL_PENALTY;
            }
        }
        // `bad_trainer` 与 `Ill::BAD_TRAINER` 同置同清，只计一次
        if flags.bad_trainer || flags.ill.contains(Ill::BAD_TRAINER) {
            harm += OTHER_ILL_PENALTY;
        }
        if flags.aijiao || flags.good_trainer {
            benefit += 30.0;
        }
        if flags.refresh_mind > 0 || flags.lucky_count > 0 {
            benefit += 20.0;
        }
        (benefit - harm) * sign
    }

    /// 追加稽古上档「解一个 debuff」的估值
    ///
    /// 与 `event.rs::clear_one_debuff` 的清除顺序同口径：**不擅长训练 → 变胖 → 其余具名病位**，
    /// 取实际会被清掉那个的价值；只带 `UNKNOWN`（线上病种未辨）时无人可清 → 0。
    fn debuff_clear_value(&self) -> f64 {
        const OTHERS: Ill = Ill::from_bits(
            Ill::SKIN.bits() | Ill::HEADACHE.bits() | Ill::LAZY.bits() | Ill::NIGHT.bits(),
        );
        let flags = &self.game.uma().flags;
        if flags.bad_trainer || flags.ill.contains(Ill::BAD_TRAINER) {
            return DEBUFF_CLEAR_VALUE;
        }
        if flags.ill.contains(Ill::FAT) {
            return self.weights.fat;
        }
        if flags.ill.bits() & OTHERS.bits() != 0 {
            return DEBUFF_CLEAR_VALUE;
        }
        0.0
    }

    // ------------------------------------------------------------------
    // 教诲分配估值
    // ------------------------------------------------------------------

    /// 应用 `apply_assignment` 的 801 硬约束后的有效分配（估值与落地同口径）
    ///
    /// 目标演目由查表缓存给出（与逐候选重算的 max_by_key 同值）。
    fn effective_assignment(
        &self,
        assign: &AdviceAssignment,
        target_801: Option<usize>,
    ) -> [u8; 5] {
        let mut effective = assign.0;
        let Some(target) = target_801 else {
            return effective;
        };
        let Some(source) = (0..5).find(|slot| self.game.advice.fused[*slot] == 801) else {
            return effective;
        };
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
        // 特化上限（601 特別稽古）：抬高该演目的训练基础上限 `status_limit` 点
        // （满级 +25；基础上限 3200 到硬顶 3800 只剩 600 余量，属末期稀缺资源）
        value += f64::from(effect.status_limit) * (train + keiko) * self.advice.status_limit;
        // 助力の支え（401）：非友情训练的保底收益，是个开关量，不按数值放大
        if effect.support_assist {
            value += (train + keiko) * non_friend * self.advice.assist;
        }
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

    #[test]
    fn flags_value_signs_benefit_and_harm() {
        let game = sample_game();
        let cfg = TrainerConfig::default();
        let model = Model::new(&game, &cfg).expect("模型");
        let good = UmaFlags {
            good_trainer: true,
            ..Default::default()
        };
        let bad = UmaFlags {
            bad_trainer: true,
            ill: Ill::BAD_TRAINER,
            ..Default::default()
        };
        assert!(
            model.flags_value(Some(&good), 1.0) > 0.0,
            "加好调应为正收益"
        );
        assert!(model.flags_value(Some(&bad), 1.0) < 0.0, "加不调应为负收益");
        assert_eq!(
            model.flags_value(Some(&bad), -1.0),
            -model.flags_value(Some(&bad), 1.0),
            "移除与添加应反号"
        );
        assert_eq!(
            model.flags_value(Some(&bad), 1.0),
            -40.0,
            "BAD_TRAINER 不应双计"
        );
    }

    #[test]
    fn assignment_value_rejects_out_of_range_slots() {
        let game = sample_game();
        let cfg = TrainerConfig::default();
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(
            model
                .assignment_value(&AdviceAssignment([9, 0, 0, 0, 0]))
                .is_infinite()
        );
    }

    /// 义务自选赛参与打分：有 freeRaces 的马娘在区间内 Race 可比较；无义务 + self_race=false 时不参与
    #[test]
    fn race_value_uses_free_race_urgency() {
        let mut game = crate::testing::sample_game_with_uma(102102);
        game.turn = 47;
        game.plan = game.derive_plan();
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        assert!(
            model.action_value(&Action::Race).is_finite(),
            "义务补足时自选赛应参与打分"
        );

        // 100101（特别周）无义务自选赛；显式指定，避免受默认测试马娘切换影响
        let mut plain = crate::testing::sample_game_with_uma(100101);
        plain.turn = 47;
        plain.plan = plain.derive_plan();
        let model = Model::new(&plain, &plain.config.deck.trainer).expect("模型");
        assert!(
            model.action_value(&Action::Race).is_infinite(),
            "无义务 + self_race=false 时自选赛不参与比较"
        );
    }

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
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        let high_small = model.event_choice_value(5010, 0, &event.choices[0]);
        let high_big = model.event_choice_value(5010, 1, &event.choices[1]);
        assert!(high_small > high_big, "体力充足时避免变胖风险");
        game.uma_mut().vital = 5;
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        let low_small = model.event_choice_value(5010, 0, &event.choices[0]);
        let low_big = model.event_choice_value(5010, 1, &event.choices[1]);
        assert!(low_big > low_small, "体力见底时吃大份");
    }

    /// 追加稽古兑现判断：稽古也按「训练位参与者」判定（与 `try_kensan` 同口径）
    #[test]
    fn kensan_bonus_uses_participants_for_keiko() {
        let mut game = sample_game();
        game.acquire_advice();
        game.advice.fused = [701, 0, 0, 0, 0];
        game.acts.level = [5; 5];
        game.kensan_pending = true;
        let person = game
            .persons
            .iter()
            .position(|person| person.act == Some(0))
            .expect("演目 0 应有担当");
        // 研鑽担当放在训练位 2（非它自己的演目位）
        game.base.distribution = vec![vec![], vec![], vec![person as i32], vec![], vec![]];
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        assert!(
            model.kensan_bonus(&Action::ActTrain(2)) > 0.0,
            "稽古训练位的参与者含研鑽担当 → 应兑现追加稽古"
        );
        assert_eq!(
            model.kensan_bonus(&Action::ActTrain(0)),
            0.0,
            "研鑽演目自己的稽古位没有担当 → 不应兑现"
        );
        assert!(
            model.kensan_bonus(&Action::Train(2)) > 0.0,
            "普通训练与稽古同口径"
        );
        assert_eq!(model.kensan_bonus(&Action::Race), 0.0);
    }

    /// 教诲分配：价值矩阵会把 801 槽放到担当数最多的演目（与 apply_assignment 硬约束同口径）
    #[test]
    fn assignment_puts_801_on_most_assigned_act() {
        let mut game = sample_game();
        game.acquire_advice();
        game.advice.fused = [801, 0, 0, 0, 0];
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
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
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        assert!(model.status_gain(&[50, 0, 0, 0, 0]) > 0.0, "速度增量应有分");
        let limit = game.uma().five_status_limit[0];
        game.uma_mut().five_status[0] = limit;
        let model = Model::new(&game, &game.config.deck.trainer).expect("模型");
        assert_eq!(
            model.status_gain(&[50, 0, 0, 0, 0]),
            0.0,
            "到上限后边际为 0"
        );
    }

    /// 溢出层：按生效上限标记近顶轴，只放大非近顶轴；全 0 时因子恒 1
    #[test]
    fn overflow_tilts_away_from_capped_axis() {
        let mut game = sample_game();
        let limit = game.uma().five_status_limit[0];
        game.uma_mut().five_status[0] = limit;
        let mut cfg = game.config.deck.trainer.clone();
        cfg.overflow = crate::deck_config::OverflowConfig {
            margin: 0.01,
            tilt: 0.5,
            keep: 0.0,
        };
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(model.ctx.near_cap[0], "速顶满应标记近顶");
        assert_eq!(model.ctx.overflow_factor[0], 1.0, "近顶轴不再额外压低");
        assert!(
            model.ctx.overflow_factor[3] > 1.0,
            "非近顶轴应被放大: {:?}",
            model.ctx.overflow_factor
        );

        cfg.overflow = crate::deck_config::OverflowConfig::default();
        let model = Model::new(&game, &cfg).expect("模型");
        assert_eq!(model.ctx.overflow_factor, [1.0; 5], "全 0 = 关闭");
    }

    /// 溢出保留：主属性顶满时，速训练的副属性 / PT 仍给 `keep` 加分
    #[test]
    fn overflow_keep_preserves_secondary_value_when_main_capped() {
        let mut game = sample_game();
        game.uma_mut().five_status[0] = game.uma().five_status_limit[0];
        let mut cfg = game.config.deck.trainer.clone();
        cfg.overflow = crate::deck_config::OverflowConfig {
            margin: 0.0,
            tilt: 0.0,
            keep: 0.0,
        };
        let without = Model::new(&game, &cfg)
            .expect("模型")
            .action_value(&Action::Train(0));
        cfg.overflow.keep = 0.2;
        let with = Model::new(&game, &cfg)
            .expect("模型")
            .action_value(&Action::Train(0));
        assert!(
            with > without,
            "keep 应保留速训练的副属性 / PT 价值: {without} -> {with}"
        );
    }

    /// 末段稽古：第三年合宿后～T71 的普通回合，满槽数超过剩余普通回合时给满槽加分
    #[test]
    fn endgame_keiko_pressure_boosts_full_gauges() {
        let mut game = crate::testing::sample_game_with_uma(106301);
        game.turn = 70;
        game.plan = game.derive_plan();
        game.acts.gauge = [5; 5];
        game.acts.level = [9; 5];
        let mut cfg = game.config.deck.trainer.clone();
        cfg.endgame = crate::deck_config::EndgameConfig {
            keiko: 250.0,
            scarcity: 1.0,
            level: 80.0,
            normal_penalty: 120.0,
        };
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(model.ctx.endgame, "T70 应进入末段窗口");
        assert!(
            model.ctx.endgame_pressure > 0.0,
            "满槽 5 > 剩余普通回合 1，应有超额压力"
        );
        assert!(
            model.endgame_keiko_bonus(&Action::ActTrain(3)) > 0.0,
            "满槽稽古应有加分"
        );
        assert_eq!(
            model.endgame_keiko_bonus(&Action::Train(3)),
            0.0,
            "普通训练不吃稽古加分"
        );

        // 普通训练的机会成本：pressure 越高扣得越多
        let before = model.action_value(&Action::Train(3));
        cfg.endgame.normal_penalty = 0.0;
        let model = Model::new(&game, &cfg).expect("模型");
        let after = model.action_value(&Action::Train(3));
        assert!(
            before < after,
            "普通训练应被末段机会成本压低: {before} vs {after}"
        );
    }

    /// 末段窗口不含 URA：T72 起不进入，避免了把全稽古回合当成“浪费”
    #[test]
    fn endgame_window_excludes_ura() {
        let mut game = crate::testing::sample_game_with_uma(106301);
        game.turn = 72;
        game.plan = game.derive_plan();
        game.acts.gauge = [5; 5];
        let mut cfg = game.config.deck.trainer.clone();
        cfg.endgame = crate::deck_config::EndgameConfig {
            keiko: 250.0,
            scarcity: 1.0,
            level: 80.0,
            normal_penalty: 120.0,
        };
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(!model.ctx.endgame, "URA 回合不进入末段窗口");
        assert_eq!(model.ctx.endgame_pressure, 0.0);
        assert_eq!(model.endgame_keiko_bonus(&Action::ActTrain(3)), 0.0);
    }

    /// 末段窗口从第三年合宿结束时开始：T63（合宿中）不进入，T64 进入
    #[test]
    fn endgame_window_starts_after_third_camp() {
        let mut game = crate::testing::sample_game_with_uma(106301);
        game.turn = 63;
        game.plan.camp_zone = true;
        game.plan.race = false;
        game.plan.ura_period = false;
        game.forced_rest = false;
        let cfg = game.config.deck.trainer.clone();
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(!model.ctx.endgame, "合宿中不进入末段窗口");

        game.turn = 64;
        game.plan.camp_zone = false;
        let model = Model::new(&game, &cfg).expect("模型");
        assert!(model.ctx.endgame, "第三年合宿结束时起进入末段窗口");
    }
}
