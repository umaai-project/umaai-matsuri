//! 训练结算（S5）：下层（引擎默认公式）+ 上层（剧本来源）+ 失败率 + 训练后处理
//!
//! 两层口径（2026-10-31 / 2026-11-01 拍板）：
//! - **下层** = 引擎 `default_calc_training_value`（卡面友情 / 训练效果 / 人数 / 干劲 / 成长率都在下层）；
//! - **上层** = 仅剧本来源（发布会 buff、教え、演目 Lv 效果、稽古 +10、特化 / 両道 / 絆 获得量等），
//!   按 `memo` §3.3 的主 / 副 / SP 三式叠加，乘算后 floor，按「上昇量上限」封顶；
//! - 稽古「全基础 +10」（act_lv `all_status_bonus`）= 上层乘区之后**全五维 flat 加**（2026-11-01 拍板）；
//! - 失败率：引擎公式 + 不屈 / 本番演出 -100%；失败不给属性 / SP、不增设施计数、排练槽不动。
//!
//! 训练后处理改用剧本口径（羁绊基础值 / 絆教え平摊 / 灵感事件 / 加练 / 点击 / 支援卡类型条件）。

use anyhow::Result;
use rand::Rng;
use rand::seq::IndexedRandom;
use umaai_core::gamedata::ActionValue;
use umaai_core::{Data, Ill};
use umasim::game::{FriendOutState, Game, Person, PersonType};

use super::person::PersonOrigin;
use super::{Action, MatsuriGame, ResourcePreview, TrainingPreview};
use crate::diag;

/// 上层训练数值（剧本来源增量；体力 / 干劲改动也在这里汇总）
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TrainingUpper {
    /// 五维增量（上层，已封顶；不含稽古 +10 flat）
    pub status: [i32; 5],
    /// 技能点增量
    pub skill_pt: i32,
    /// 稽古「全基础 +10」flat 加（五维各 +N）
    pub keiko_flat: i32,
    /// 体力增量（已含稽古体力消耗下降 / 本番演出 +30）
    pub vital: i32,
    /// 干劲增量（本番演出 +1）
    pub motivation: i32,
}

impl MatsuriGame {
    /// 训练 / 演目稽古预览（S8 手动核对 / 前端面板）
    ///
    /// 在局面的**克隆**上临时切稽古上下文计算，不改动原局面、不消耗任何随机流；
    /// 预览期间静默诊断日志（不把「如果选这个」的模拟结果混进真实状态变化日志）。
    pub fn preview_training(&self, train: usize, keiko: bool) -> Result<TrainingPreview> {
        if train >= 5 {
            return Err(anyhow::anyhow!("普通训练下标越界: {train}（训练位 0–4）"));
        }
        let _quiet = umaai_core::diag::DiagGuard::suppress();
        let mut probe = self.clone();
        probe.keiko_settling = keiko;
        let buffs = probe.calc_training_buff(train)?;
        let failure_rate = probe.calc_training_failure_rate(&buffs, train)?;
        let mut value = probe.calc_training_value(&buffs, train)?;
        // 【变胖】预览与正式结算同口径：训练 / 排练速度增量归零
        if probe.is_fat() {
            value.status_pt[0] = 0;
        }
        Ok(TrainingPreview {
            value,
            failure_rate,
            keiko,
        })
    }

    /// 动作的剧本资源预览：演目槽 / 演目 Lv 差值（正 = 增加，负 = 排练成功清槽）
    ///
    /// 在局面克隆上按「成功」口径模拟资源侧（不耗随机流、不改原局面）：普通训练按参与担当入槽；
    /// 稽古成功清槽 + Lv+1（合宿中不升不清）；比赛 / 休息 / 出行按三则入槽；友人出行额外最低演目
    /// Lv+1。返回与当前局面的差值；预览期间静默诊断日志。
    pub fn preview_action_resources(&self, action: &Action) -> ResourcePreview {
        let _quiet = umaai_core::diag::DiagGuard::suppress();
        let mut probe = self.clone();
        match action {
            Action::Train(train) => probe.gain_training_gauge(usize::from(*train)),
            Action::ActTrain(act) => {
                let train = probe
                    .data
                    .acts
                    .get(usize::from(*act))
                    .map_or(usize::from(*act), |row| row.train);
                let act = probe.act_of_train(train);
                // 稽古成功：非合宿 → Lv+1、槽清零；合宿中不升不清
                if !probe.plan.camp_train(probe.forced_rest) {
                    probe.add_act_level(act, 1);
                    probe.acts.gauge[act] = 0;
                }
            }
            Action::Race => {
                let gain = if probe.plan.race {
                    probe.data.rules.act_gauge_gain.target_race
                } else {
                    probe.data.rules.act_gauge_gain.other
                };
                probe.gain_lowest_act_gauge(gain);
            }
            Action::Rest | Action::NormalOuting => {
                probe.gain_lowest_act_gauge(probe.data.rules.act_gauge_gain.other);
            }
            Action::FriendOuting => {
                // 与 do_friend_outing 同口径：当前段最低演目 Lv+1 + 槽 +1
                if let Some(add_level) = probe.friend_outing_add_level() {
                    probe.add_lowest_act_level(add_level);
                }
                probe.gain_lowest_act_gauge(probe.data.rules.act_gauge_gain.other);
            }
            Action::Clinic | Action::AssignAdvice(_) => {}
        }
        let mut preview = ResourcePreview::default();
        for act in 0..5 {
            preview.gauge_delta[act] = probe.acts.gauge[act] - self.acts.gauge[act];
            preview.level_delta[act] = probe.acts.level[act] - self.acts.level[act];
        }
        preview
    }

    /// 普通训练结算
    pub(crate) fn do_train(&mut self, train: usize, rng: &mut impl Rng) -> Result<()> {
        if train >= 5 {
            return Err(anyhow::anyhow!("普通训练下标越界: {train}（训练位 0–4）"));
        }
        self.settle_training(train, false, rng)
    }

    /// 演目稽古结算（成功才升 Lv / 清槽；合宿中不升不清，Lv 由合宿结束统一 +1）
    pub(crate) fn do_act_train(&mut self, act: usize, rng: &mut impl Rng) -> Result<()> {
        let train = self
            .data
            .acts
            .get(act)
            .ok_or_else(|| anyhow::anyhow!("演目下标越界: {act}（演目 0–4）"))?
            .train;
        // 稽古结算期间让 `training_basic_table` 返回稽古表；无论成败都恢复
        self.keiko_settling = true;
        let result = self.settle_training(train, true, rng);
        self.keiko_settling = false;
        result
    }

    /// 一次训练结算（普通 / 稽古共用）：失败判定 → 数值入账 → 设施计数 → 剧本后处理
    fn settle_training(&mut self, train: usize, keiko: bool, rng: &mut impl Rng) -> Result<()> {
        // 诊断：结算前快照（成功 / 失败都打一行「动作 + 变化」）
        let before = self.key_state();
        let label = if keiko {
            format!("稽古#{}", self.act_of_train(train))
        } else {
            format!("训练#{train}")
        };
        let buffs = self.calc_training_buff(train)?;
        let failure_rate = self.calc_training_failure_rate(&buffs, train)?;
        let fail_prob = f64::from(failure_rate) / 100.0;
        if rng.random_bool(fail_prob) {
            diag!(
                "【动作】T{} {label} 失败（失败率 {:.1}%）｜变化 {}",
                self.turn(),
                failure_rate,
                before.diff(&self.key_state())
            );
            return self.apply_training_failure(fail_prob, rng);
        }
        let mut value = self.calc_training_value(&buffs, train)?;
        // 【变胖】训练 / 排练的速度增量归零；训练后事件 / 休息 / 出行 / 比赛 / 事件仍可加速度
        if self.is_fat() {
            value.status_pt[0] = 0;
        }
        self.uma_mut().add_value(&value);
        if !keiko {
            self.base.train_level_count[train] += 1;
        }
        // 助カ支撑：本次训练若已生效则消耗 1 次（稽古获得后重置，见下）
        if self.assist_support_left > 0 {
            self.assist_support_left -= 1;
            diag!(
                "【助力支撑】T{} 训练#{} 消耗 剩 {}",
                self.turn(),
                train,
                self.assist_support_left
            );
        }
        self.after_train(train, keiko, rng)?;
        // 演目槽：普通训练按参与担当入账（闪彩 +2）；排练回合不涨
        if !keiko {
            self.gain_training_gauge(train);
        }
        // 演目稽古成功：非合宿 → 该演目 Lv+1、槽清零；合宿中不升不清
        if keiko {
            let act = self.act_of_train(train);
            if !self.plan.camp_train(self.forced_rest) {
                self.add_act_level(act, 1);
                self.acts.gauge[act] = 0;
            }
            // 助カの支え：稽古获得（重置次数）
            if self.participant_advice(train).effect.support_assist {
                self.assist_support_left = self.data.rules.assist_support.count;
                diag!(
                    "【助力支撑】T{} 稽古#{} 获得 重置为 {}",
                    self.turn(),
                    act,
                    self.assist_support_left
                );
            }
        }
        // 研鑽「追加の稽古」：普通训练 / 稽古成功都触发（绑定演目担当参与即可）
        self.try_kensan(train)?;
        diag!(
            "【动作】T{} {label} 成功（失败率 {:.1}%）｜变化 {}",
            self.turn(),
            failure_rate,
            before.diff(&self.key_state())
        );
        Ok(())
    }

    /// 训练失败：二次判定大失败（生病 / 不擅长），按基础局口径直接结算失败事件
    fn apply_training_failure(&mut self, failure_rate: f64, rng: &mut impl Rng) -> Result<()> {
        let event = if rng.random_bool(failure_rate) {
            self.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
            self.uma_mut().flags.bad_trainer = true;
            diag!("【训练失败】T{} 大失败 → 生病 / 不擅长训练", self.turn());
            Data::get()?.system_event("training_fail_low")?.clone()
        } else {
            diag!("【训练失败】T{} 普通失败", self.turn());
            Data::get()?.system_event("training_fail")?.clone()
        };
        self.apply_event(&event, 0, rng)
    }

    /// 上层训练数值（剧本来源；`lower` 为下层值，`keiko` 为排练上下文）
    pub(crate) fn training_upper(
        &self,
        train: usize,
        lower: &ActionValue,
        keiko: bool,
    ) -> TrainingUpper {
        let advice = self.participant_advice(train).effect;
        let presentation = self.presentation_bonus();
        let five_types = self.deck_has_five_types();
        let friend_training = self.shining_count(train) > 0;
        let act = self.act_of_train(train);
        let act_effect = keiko.then(|| self.act_effect(act));

        // 友情合计（仅友情训练）：发布会 + 協力教え +（稽古：演目 Lv 友情 + 転成稽古友情）
        let mut friendship = 0;
        if friend_training {
            friendship += presentation.friendship;
            friendship += advice.cooperate_friendship;
            if let Some(effect) = act_effect {
                friendship += effect.friendship;
                friendship += advice.keiko_friendship;
            }
        }
        // 训练效果合计：助力（非友情）+ 助力支撑 +（稽古：両道 / 不屈 稽古效果）
        let mut training_effect = 0;
        if !friend_training {
            training_effect += advice.non_friend_training_effect;
        }
        if self.assist_support_left > 0 {
            training_effect += self.data.rules.assist_support.effect;
        }
        if keiko {
            training_effect += advice.keiko_training_effect;
        }
        // 主获得合计：特化 +（稽古：絆教え / 絆転成 稽古）
        let mut main_gain = advice.main_gain;
        if keiko {
            main_gain += advice.keiko_main_bonus;
        }
        // 副获得合计：両道（SP 不吃）
        let sub_gain = advice.dual_sub_gain;
        // SP 训练效果（稽古且 5 种卡：演目 Lv sp_training）
        let mut sp_training = 0;
        // 上层上限：基础上限 + 特化 status_limit +（稽古且 5 种卡：演目 Lv status_limit）
        let mut status_cap = self.data.rules.training_upper_cap + advice.status_limit;
        let mut sp_cap = self.data.rules.training_upper_cap + advice.status_limit;
        let mut keiko_flat = 0;
        if let Some(effect) = act_effect {
            keiko_flat = effect.all_status_bonus;
            if five_types {
                sp_training += effect.sp_training;
                status_cap += effect.status_limit;
                sp_cap += effect.sp_limit;
            }
        }

        let mut upper = TrainingUpper {
            keiko_flat,
            ..Default::default()
        };
        let friend_pct = 100 + friendship;
        for axis in 0..5 {
            let base = lower.status_pt[axis];
            if base <= 0 {
                continue;
            }
            let (rate1, rate2, cap) = if axis == train {
                (100 + training_effect, 100 + main_gain, status_cap)
            } else {
                (100 + training_effect + sub_gain, 100, status_cap)
            };
            let real =
                (i64::from(base) * i64::from(friend_pct) * i64::from(rate1) * i64::from(rate2)
                    / 1_000_000) as i32;
            let gain = (real - base).min(cap).max(0);
            upper.status[axis] = gain;
        }
        // SP：不吃副获得；吃 SP 训练效果 + SP 上限
        let sp_base = lower.status_pt[5];
        if sp_base > 0 {
            let rate1 = 100 + training_effect + sp_training;
            let real =
                (i64::from(sp_base) * i64::from(friend_pct) * i64::from(rate1) / 10_000) as i32;
            upper.skill_pt = (real - sp_base).min(sp_cap).max(0);
        }
        // 体力 / 干劲：稽古体力消耗下降 + 本番演出（URA 训练：体力 +30 / 干劲 +1）
        let mut vital = lower.vital;
        if keiko && vital < 0 {
            let drop = advice.keiko_vital_cost_drop.min(100);
            vital = -(i64::from(-vital) * i64::from(100 - drop) / 100) as i32;
        }
        if self.ura_training_active() {
            vital += self.data.finals_act.vital;
            upper.motivation += self.data.finals_act.motivation;
        }
        upper.vital = vital;
        upper
    }

    /// 剧本失败率下降 %（不屈 / 本番演出；100 = 必定成功）
    pub(crate) fn scenario_fail_rate_drop(&self, train: usize) -> i32 {
        let mut drop = 0;
        if self.ura_training_active() {
            drop = drop.max(self.data.finals_act.fail_rate_drop);
        }
        if let Some(threshold) = self.participant_advice(train).effect.fail_vital_threshold
            && (threshold == 0 || self.uma().vital >= threshold)
        {
            drop = 100;
        }
        drop
    }

    /// 本番演出是否在 URA 训练中生效（**turn 72–77**；68–71 空窗期不生效）
    pub(crate) fn ura_training_active(&self) -> bool {
        self.finals_done && (crate::URA_START_TURN..=crate::MAX_TURN).contains(&self.turn)
    }

    /// 训练成功后的剧本后处理：羁绊 / 灵感事件 / 加练 / 点击（替换引擎默认实现）
    fn after_train(&mut self, train: usize, keiko: bool, rng: &mut impl Rng) -> Result<()> {
        self.base.set_last_train(train);
        let queued_before = self.unresolved_events.len();
        let mut friendship_sum = 0;
        let mut friendship_count = 0;
        let advice = self.participant_advice(train);
        let lesson = advice.relationship_gain;
        let touches = self.base.distribution[train].clone();
        let mut hint_persons: Vec<usize> = Vec::new();
        let mut friend_clicked = false;
        let mut yayoi_clicked = false;
        let mut reporter_clicked = false;
        for index in touches {
            if index < 0 {
                continue;
            }
            let person_index = index as usize;
            let person = &self.persons[person_index];
            let origin = person.origin;
            let person_type = person.person_type();
            let mut base = match person_type {
                // 理事长 / 记者：引擎基础局口径 +5（剧本资产未单列）
                PersonType::Yayoi | PersonType::Reporter => 5,
                _ => match origin {
                    PersonOrigin::Card => self.data.rules.friendship_gain.card,
                    PersonOrigin::Friend => self.data.rules.friendship_gain.friend,
                    // 编成外神驹只计人头，不给羁绊
                    PersonOrigin::Guest => 0,
                    _ => 0,
                },
            };
            // 絆の教え：全编组卡 +3（平摊，不按参与人数叠加；爱娇由 add_friendship 统一施加一次）
            if lesson > 0 && matches!(origin, PersonOrigin::Card | PersonOrigin::Friend) {
                base += lesson;
            }
            if base > 0 {
                let before = self.persons[person_index].friendship();
                self.base
                    .add_friendship(&mut self.persons, person_index, base);
                // 摘要按**实际到账**（引擎侧封顶 + 爱娇加成）计
                friendship_sum += self.persons[person_index].friendship() - before;
                friendship_count += 1;
            }
            if self.persons[person_index].hint() {
                hint_persons.push(person_index);
            }
            match person_type {
                PersonType::Yayoi => yayoi_clicked = true,
                PersonType::Reporter => reporter_clicked = true,
                _ => {}
            }
            if matches!(origin, PersonOrigin::Friend) {
                friend_clicked = true;
            }
        }
        // 灵感事件：默认随机 1 人；排练 + 5 种卡 + 演目 Lv≥4 → 全灵感事件发动
        let all_hint = keiko
            && self.deck_has_five_types()
            && self.act_effect(self.act_of_train(train)).all_hint;
        let hint_level_bonus = if keiko {
            self.participant_advice(train).effect.keiko_hint_level
        } else {
            0
        };
        if all_hint {
            for person_index in &hint_persons {
                self.do_hint_with_bonus(*person_index, hint_level_bonus, rng)?;
            }
        } else if let Some(&person_index) = hint_persons.choose(rng) {
            self.do_hint_with_bonus(person_index, hint_level_bonus, rng)?;
        }
        // 加练（非夏合宿；引擎基础局口径）
        let extra_train_prob = Data::get()?.event_prob("extra_train")?;
        if !self.is_xiahesu() && rng.random_bool(extra_train_prob) {
            let mut event = umasim::game::extra_training_event(train)?;
            if let Some(yayoi) = self.person_index_of_type(PersonType::Yayoi) {
                event.person_index = Some(yayoi as i32);
            }
            self.unresolved_events.push(event);
        }
        // 点击事件（理事长 / 记者；友人点击与真实吾妻事件留 S6，暂不推温泉事件）
        if yayoi_clicked {
            let mut event = Data::get()?.system_event("yayoi_click")?.clone();
            if let Some(index) = self.person_index_of_type(PersonType::Yayoi) {
                event.person_index = Some(index as i32);
            }
            self.unresolved_events.push(event);
        }
        if reporter_clicked {
            let mut event = Data::get()?.system_event("reporter_click")?.clone();
            event.choices[0][0].value.status_pt[train] = 2;
            if let Some(index) = self.person_index_of_type(PersonType::Reporter) {
                event.person_index = Some(index as i32);
            }
            self.unresolved_events.push(event);
        }
        // 友人点击事件（Q4 已定：与追加稽古预掷相互独立）：
        // 首次点击 → 首次事件并转入 BeforeUnlock；之后每次点击按引擎常量概率（默认 40%）推训练后事件。
        if friend_clicked {
            match self.base.friend.out_state {
                FriendOutState::UnClicked => {
                    self.base.friend.out_state = FriendOutState::BeforeUnlock;
                    if let Some(event) = Data::get()?.events.friend_events.get("first") {
                        self.unresolved_events.push(event.clone());
                    }
                }
                _ => {
                    let prob = Data::get()?.event_prob("friend_click")?;
                    if rng.random_bool(prob)
                        && let Some(event) = Data::get()?.events.friend_events.get("click")
                    {
                        self.unresolved_events.push(event.clone());
                    }
                }
            }
        }
        let mut parts = vec![format!("羁绊+{friendship_sum}（{friendship_count} 人）")];
        if !hint_persons.is_empty() {
            parts.push(format!("灵感 {} 人", hint_persons.len()));
        }
        let queued = self.unresolved_events.len() - queued_before;
        if queued > 0 {
            parts.push(format!("追加事件 {queued}"));
        }
        if friend_clicked {
            parts.push("友人点击".to_string());
        }
        if yayoi_clicked {
            parts.push("理事长点击".to_string());
        }
        if reporter_clicked {
            parts.push("记者点击".to_string());
        }
        let label = if keiko {
            format!("稽古#{}", self.act_of_train(train))
        } else {
            format!("训练#{train}")
        };
        diag!("【训练后】T{} {label} {}", self.turn(), parts.join(" "));
        Ok(())
    }

    /// 灵感事件（可叠加稽古灵感等级 +N；口径同引擎基础局，含单卡累计上限 / 次数）
    pub(crate) fn do_hint_with_bonus(
        &mut self,
        person_index: usize,
        level_bonus: i32,
        rng: &mut impl Rng,
    ) -> Result<()> {
        let data = Data::get()?;
        let attr_prob = data.event_prob("hint_attr")?;
        let max_hint = data.consts.max_hint_per_card;
        let card = self.persons[person_index].card_id().and_then(|card_id| {
            self.base
                .deck
                .iter()
                .position(|card| card.card_id == card_id)
        });
        let (hint_level, hint_count) = match card {
            Some(index) => {
                let level = (1 + self.base.deck[index].card_value().hint_level + level_bonus)
                    .min(5)
                    .min(max_hint - self.base.deck[index].total_hints)
                    .max(0);
                let count = 1 + self.base.deck[index].effect.hint_count_bonus;
                (level, count)
            }
            None => (1 + level_bonus, 1),
        };
        for _ in 0..hint_count {
            let mut event = if rng.random_bool(attr_prob) || hint_level == 0 {
                umasim::game::hint_attr_event(
                    self.persons[person_index].train_type() as usize,
                    person_index,
                )?
            } else {
                if let Some(index) = card {
                    self.base.deck[index].total_hints += hint_level;
                }
                umasim::game::hint_skill_event(hint_level, person_index)
            };
            if let Some(index) = card {
                event.name = format!("{} - {}", event.name, self.base.deck[index].short_name());
            }
            self.unresolved_events.push(event);
        }
        Ok(())
    }

    /// 普通训练成功后按参与担当入账演目槽（担当归集；闪彩 +2、其余 +1）
    fn gain_training_gauge(&mut self, train: usize) {
        let gauge_before = self.acts.gauge;
        let touches = self.base.distribution[train].clone();
        for index in touches {
            if index < 0 {
                continue;
            }
            let person_index = index as usize;
            let Some(act) = self.persons.get(person_index).and_then(|person| person.act) else {
                continue;
            };
            let amount = if self.is_shining_at(person_index, train) {
                self.data.rules.act_gauge_gain.friendship
            } else {
                self.data.rules.act_gauge_gain.member
            };
            self.gain_act_gauge(act, amount);
        }
        if gauge_before != self.acts.gauge {
            let mut changes: Vec<String> = Vec::new();
            for (act, (before, after)) in
                gauge_before.iter().zip(self.acts.gauge.iter()).enumerate()
            {
                if before != after {
                    changes.push(format!("演目{act}槽{before}→{after}"));
                }
            }
            diag!(
                "【演目槽】T{} 训练#{} 入槽 {}",
                self.turn(),
                train,
                changes.join(" ")
            );
        }
    }

    /// 研鑽「追加の稽古」：掷中且绑定演目担当参与成功训练时，推入二选一事件（训练员选择上 / 下档）
    ///
    /// 事件数据 = 剧本 `scenario_events` 的 `400015106`：上档 = 直前训练属性 +20 / SP+20 / 体力-5 /
    /// 理事长羁绊+5 / 最低演目 gauge+1 / 解一个 debuff；下档 = 直前训练 +5 / 体力+5。
    pub(crate) fn try_kensan(&mut self, train: usize) -> Result<()> {
        if !self.kensan_pending {
            return Ok(());
        }
        let Some(kensan_act) = self.kensan_act() else {
            return Ok(());
        };
        let participated = self.base.distribution[train]
            .iter()
            .filter(|index| **index >= 0)
            .any(|index| {
                self.persons
                    .get(*index as usize)
                    .and_then(|person| person.act)
                    == Some(kensan_act)
            });
        if !participated {
            return Ok(());
        }
        self.kensan_pending = false;
        diag!(
            "【追加稽古】T{} 训练#{} 掷中 → 事件 #{KENSAN_EVENT}",
            self.turn(),
            train,
            KENSAN_EVENT = crate::game::event::KENSAN_EVENT_ID
        );
        self.trigger_kensan_event(train)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use umaai_core::gamedata::ActionValue;
    use umasim::game::{Game, Person, PersonType};

    use super::*;

    /// 合成下层值（只给速 / SP / 体力，其余 0）
    fn lower_value(speed: i32, sp: i32, vital: i32) -> ActionValue {
        let mut value = ActionValue::default();
        value.status_pt[0] = speed;
        value.status_pt[5] = sp;
        value.vital = vital;
        value
    }

    /// 把一个指定训练类型的支援卡放进该训练位并置为闪彩（友情训练）
    fn make_shining(game: &mut MatsuriGame, train: usize) -> usize {
        let index = game
            .persons()
            .iter()
            .position(|person| {
                person.person_type() == PersonType::Card && person.train_type() == train as i32
            })
            .expect("缺少该训练类型的支援卡");
        game.base.distribution = vec![vec![]; 5];
        game.base.distribution[train] = vec![index as i32];
        game.persons[index].set_friendship(80);
        index
    }

    /// 把一个指定演目的担当放进训练位（用于教え参与聚合）
    fn person_of_act(game: &MatsuriGame, act: usize) -> usize {
        game.persons
            .iter()
            .position(|person| person.act == Some(act))
            .expect("该演目没有担当")
    }

    #[test]
    fn upper_zero_without_scenario_sources() {
        let game = crate::testing::sample_game();
        let upper = game.training_upper(0, &lower_value(100, 100, -20), false);
        assert_eq!(upper.status, [0; 5], "无发布会 / 教え时上层为 0");
        assert_eq!(upper.skill_pt, 0);
        assert_eq!(upper.keiko_flat, 0);
        assert_eq!(upper.vital, -20, "普通训练体力不变");
        assert_eq!(upper.motivation, 0);
    }

    #[test]
    fn upper_presentation_friendship_applies_on_friend_training() {
        let mut game = crate::testing::sample_game();
        game.check_points[0] = crate::data::CheckPointResult::GreatSuccess;
        make_shining(&mut game, 0);
        let upper = game.training_upper(0, &lower_value(100, 100, -20), false);
        assert_eq!(upper.status[0], 5, "发布会友情 +5% → 速 +5");
        assert_eq!(upper.skill_pt, 5, "同一乘区作用于 SP");
        assert_eq!(upper.status[1], 0, "非主属性无下层基数");
    }

    #[test]
    fn upper_keiko_flat_and_act_effects() {
        let mut game = crate::testing::sample_game();
        game.acts.level[0] = 1;
        make_shining(&mut game, 0);
        let upper = game.training_upper(0, &lower_value(16, 9, -25), true);
        assert_eq!(upper.keiko_flat, 10, "稽古全基础 +10（全五维 flat）");
        // 演目 Lv1 友情 2%：16 × 1.02 = 16.32 → 16，增量为 0；SP 吃 sp_training（5 种卡 → 20%）
        assert_eq!(upper.status[0], 0);
        assert_eq!(
            upper.skill_pt, 2,
            "SP 训练效果：9 × 1.02 × 1.2 = 11 → 增量 2"
        );
        assert_eq!(upper.vital, -25, "未带体力教え时体力不变");
    }

    #[test]
    fn upper_special_caps_and_main_gain() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [601, 201, 301, 401, 501];
        game.acts.level = [10, 1, 1, 1, 1];
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        // 无闪彩 → 非友情：特化主获得 +25%、基础上限 +25
        let upper = game.training_upper(0, &lower_value(100, 100, -20), false);
        assert_eq!(upper.status[0], 25, "100 × 1.25 = 125 → 增量 25");
        assert_eq!(upper.skill_pt, 0, "特化 main_gain 不吃 SP");
    }

    #[test]
    fn upper_non_friend_assist_training_effect() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [401, 201, 301, 501, 101];
        game.acts.level = [10, 1, 1, 1, 1];
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let upper = game.training_upper(0, &lower_value(100, 0, -20), false);
        assert_eq!(upper.status[0], 40, "助力 Lv10 非友情 +40%");
    }

    #[test]
    fn upper_keiko_vital_cost_drop() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.acts.level = [1, 1, 1, 1, 10];
        let person = person_of_act(&game, 4);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let upper = game.training_upper(0, &lower_value(16, 9, -25), true);
        assert_eq!(upper.vital, -21, "稽古体力消耗 -15%：-25 → -21");
    }

    #[test]
    fn upper_sp_training_five_types() {
        let mut game = crate::testing::sample_game();
        game.acts.level[0] = 10;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let upper = game.training_upper(0, &lower_value(16, 100, -25), true);
        assert_eq!(
            upper.skill_pt, 170,
            "演目 Lv10 SP 排练 +170%（上限 120 不截增量）"
        );
    }

    #[test]
    fn ura_training_effects() {
        let mut game = crate::testing::sample_game();
        game.finals_done = true;
        // 空窗期 68–71：本番演出**不生效**（训练扣体 / 失败率照常）
        for turn in [68, 70, 71] {
            game.turn = turn;
            assert!(
                !game.ura_training_active(),
                "T{turn} 空窗期不应用 URA 训练效果"
            );
            let upper = game.training_upper(0, &lower_value(100, 0, -20), false);
            assert_eq!(upper.vital, -20, "T{turn} 空窗期训练不额外回体");
            assert_eq!(upper.motivation, 0, "T{turn} 空窗期训练不额外加干劲");
            assert_eq!(
                game.scenario_fail_rate_drop(0),
                0,
                "T{turn} 空窗期无失败率下降"
            );
        }
        // URA 72–77：本番演出生效（先扣体 / 失败率下降，再回体 → 净回体）
        for turn in [72, 74, 77] {
            game.turn = turn;
            assert!(game.ura_training_active(), "T{turn} URA 训练生效");
            let upper = game.training_upper(0, &lower_value(100, 0, -20), false);
            assert_eq!(
                upper.vital, 10,
                "T{turn} URA 训练体力 +30（-20+30 净回 10）"
            );
            assert_eq!(upper.motivation, 1, "T{turn} URA 训练干劲 +1");
            assert_eq!(
                game.scenario_fail_rate_drop(0),
                100,
                "T{turn} 本番演出失败率 -100%"
            );
        }
    }

    #[test]
    fn unyielding_drops_failure_to_zero() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [901, 201, 301, 401, 501];
        game.acts.level[0] = 10;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        game.uma_mut().vital = 10;
        assert_eq!(
            game.scenario_fail_rate_drop(0),
            100,
            "不屈 Lv10 体力无关 -100%"
        );
        game.acts.level[0] = 1;
        assert_eq!(game.scenario_fail_rate_drop(0), 0, "不屈 Lv1 需体力 ≥45");
    }

    #[test]
    fn kensan_pushes_choice_event() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [201, 101, 301, 401, 501];
        game.advice.fused = [701, 0, 0, 0, 0];
        game.acts.level[0] = 10;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        game.kensan_pending = true;
        game.try_kensan(0).expect("推入追加稽古事件");
        assert!(!game.kensan_pending, "触发后消耗本回合追加稽古");
        let event = game.unresolved_events.last().expect("应有待结算事件");
        assert_eq!(event.id, 400015106, "追加稽古事件 id");
        assert!(event.player_select, "追加稽古应由训练员选择上 / 下档");
        assert_eq!(event.choices.len(), 2);
    }

    #[test]
    fn gauge_gain_by_participants_with_shining() {
        let mut game = crate::testing::sample_game();
        game.turn = crate::SCENARIO_START_TURN;
        game.acts.gauge = [0; 5];
        let shining = make_shining(&mut game, 0); // act0 担当、速类型、闪彩
        let other = person_of_act(&game, 1);
        let npc = game
            .persons()
            .iter()
            .position(|person| person.act == Some(2) && person.person_type() == PersonType::Npc)
            .expect("act2 的神驹");
        game.base.distribution = vec![
            vec![shining as i32, other as i32, npc as i32],
            vec![],
            vec![],
            vec![],
            vec![],
        ];
        game.gain_training_gauge(0);
        assert_eq!(
            game.acts.gauge,
            [2, 1, 1, 0, 0],
            "参与担当各给自己担当演目 +1；闪彩担当 +2"
        );
    }

    /// 动作资源预览：与实结算一致、不改原局面；排练预览清槽 + Lv+1
    #[test]
    fn preview_action_resources_matches_actual() {
        let mut game = crate::testing::sample_game();
        game.turn = crate::SCENARIO_START_TURN;
        game.acts.gauge = [0; 5];
        let shining = make_shining(&mut game, 0);
        let other = person_of_act(&game, 1);
        game.base.distribution = vec![
            vec![shining as i32, other as i32],
            vec![],
            vec![],
            vec![],
            vec![],
        ];
        let preview = game.preview_action_resources(&Action::Train(0));
        assert_eq!(game.acts.gauge, [0; 5], "预览不得改原局面");
        let mut actual = game.clone();
        actual.gain_training_gauge(0);
        for act in 0..5 {
            assert_eq!(
                preview.gauge_delta[act],
                actual.acts.gauge[act] - game.acts.gauge[act],
                "预览槽增量应与实结算一致（act {act}）"
            );
        }
        // 排练：成功预览 = 该演目槽清零 + Lv+1；合宿中不升不清
        let mut normal = crate::testing::sample_game();
        normal.turn = 20;
        normal.acts.gauge[2] = normal.act_gauge_max();
        let preview = normal.preview_action_resources(&Action::ActTrain(2));
        assert_eq!(
            preview.gauge_delta[2],
            -normal.act_gauge_max(),
            "排练成功清槽"
        );
        assert_eq!(preview.level_delta[2], 1, "排练成功 Lv+1");
        // 合宿中排练不升 Lv、不清槽
        let mut camp = crate::testing::sample_game();
        camp.turn = 20;
        camp.plan.camp_zone = true;
        camp.plan.camp_year = Some(0);
        camp.acts.gauge[0] = camp.act_gauge_max();
        let camp_preview = camp.preview_action_resources(&Action::ActTrain(0));
        assert_eq!(camp_preview.level_delta, [0; 5], "合宿中排练不升 Lv");
        assert_eq!(camp_preview.gauge_delta, [0; 5], "合宿中排练不清槽");
        // 休息：三则 +1
        let rest = normal.preview_action_resources(&Action::Rest);
        assert_eq!(rest.gauge_delta.iter().sum::<i32>(), 1, "休息按三则入槽 +1");
    }

    #[test]
    fn kensan_triggers_on_normal_training_too() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [201, 101, 301, 401, 501];
        game.advice.fused = [701, 0, 0, 0, 0];
        game.acts.level[0] = 10;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        game.kensan_pending = true;
        game.try_kensan(0).expect("推入追加稽古事件");
        assert!(!game.kensan_pending, "普通训练成功也消耗追加稽古");
    }

    #[test]
    fn keiko_flat_applies_to_all_five_stats() {
        let mut game = crate::testing::sample_game();
        game.keiko_settling = true;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let buffs = game.calc_training_buff(0).expect("训练 buff");
        let value = game.calc_training_value(&buffs, 0).expect("训练值");
        assert_eq!(
            value.status_pt[1], 10,
            "速度稽古的耐力基础为 0 → 只吃 +10 flat"
        );
        assert!(value.status_pt[0] >= 26, "速度 = 下层 + 10 flat");
    }

    /// 友人点击链：首次点击 → 首次事件 + `BeforeUnlock`；之后每次点击 40% 触发训练后事件
    #[test]
    fn friend_click_pushes_first_then_click_by_probability() {
        let mut game = crate::testing::sample_game();
        game.base.turn = 5;
        let friend = game
            .persons
            .iter()
            .position(|person| person.origin == PersonOrigin::Friend)
            .expect("应带 SSR 吾妻");
        game.base.distribution = vec![vec![friend as i32], vec![], vec![], vec![], vec![]];
        let mut rng = StdRng::seed_from_u64(5);
        game.after_train(0, false, &mut rng).expect("训练后处理");
        assert_eq!(
            game.base.friend.out_state,
            FriendOutState::BeforeUnlock,
            "首次点击应转 BeforeUnlock"
        );
        let first_id = Data::get().expect("数据已初始化").events.friend_events["first"].id;
        assert!(
            game.unresolved_events
                .iter()
                .any(|event| event.id == first_id),
            "首次点击应推首次事件"
        );

        let click_id = Data::get().expect("数据已初始化").events.friend_events["click"].id;
        let mut hits = 0;
        for _ in 0..200 {
            game.unresolved_events.clear();
            game.after_train(0, false, &mut rng).expect("训练后处理");
            if game
                .unresolved_events
                .iter()
                .any(|event| event.id == click_id)
            {
                hits += 1;
            }
        }
        // 引擎常量 friend_click = 40%
        assert!(
            (60..=120).contains(&hits),
            "200 次点击应约 80 次触发训练后事件，实际 {hits}"
        );
    }

    /// 预览 = 正式结算（同一局面、成功的普通训练逐项一致）
    #[test]
    fn preview_matches_actual_training() {
        let mut base = crate::testing::sample_game();
        base.base.turn = 20;
        base.uma_mut().vital = 100;
        let person = person_of_act(&base, 0);
        base.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let preview = base.preview_training(0, false).expect("预览");
        let mut matched = false;
        for seed in 0..20_u64 {
            let mut game = base.clone();
            let before = game.uma().clone();
            let mut rng = StdRng::seed_from_u64(seed);
            game.do_train(0, &mut rng).expect("训练结算");
            let after = game.uma();
            let five: [i32; 5] =
                std::array::from_fn(|axis| after.five_status[axis] - before.five_status[axis]);
            if five == [0; 5] {
                continue; // 训练失败，换种子
            }
            let delta = [
                five[0],
                five[1],
                five[2],
                five[3],
                five[4],
                after.skill_pt - before.skill_pt,
            ];
            assert_eq!(delta, preview.value.status_pt, "预览与实结算属性 / SP 一致");
            assert_eq!(
                after.vital - before.vital,
                preview.value.vital,
                "预览与实结算体力一致"
            );
            matched = true;
            break;
        }
        assert!(matched, "20 个种子内应有成功的训练");
    }

    /// 变胖：训练 / 排练的速度增量归零，其他轴与事件收益不受影响
    #[test]
    fn fat_zeroes_training_speed_only() {
        let base = crate::testing::sample_game();
        let mut game = base.clone();
        game.base.turn = 20;
        game.uma_mut().vital = 100;
        let person = person_of_act(&game, 0);
        game.base.distribution = vec![vec![person as i32], vec![], vec![], vec![], vec![]];
        let mut fat = game.clone();
        fat.uma_mut().flags.ill.insert(Ill::FAT);
        let mut ok = false;
        for seed in 0..20_u64 {
            let mut a = game.clone();
            let mut b = fat.clone();
            a.uma_mut().flags.ill = Ill::NONE;
            b.uma_mut().flags.ill.insert(Ill::FAT);
            let before = a.uma().five_status;
            let mut rng_a = StdRng::seed_from_u64(seed);
            let mut rng_b = StdRng::seed_from_u64(seed);
            a.do_train(0, &mut rng_a).expect("训练结算");
            b.do_train(0, &mut rng_b).expect("训练结算");
            let d_normal = std::array::from_fn::<i32, 5, _>(|i| a.uma().five_status[i] - before[i]);
            let d_fat = std::array::from_fn::<i32, 5, _>(|i| b.uma().five_status[i] - before[i]);
            if d_normal == [0; 5] {
                continue; // 训练失败，换种子
            }
            ok = true;
            assert_eq!(d_fat[0], 0, "变胖时速度增量应为 0");
            assert_eq!(
                [d_fat[1], d_fat[2], d_fat[3], d_fat[4]],
                [d_normal[1], d_normal[2], d_normal[3], d_normal[4]],
                "变胖不影响其他轴的训练增量"
            );
            break;
        }
        assert!(ok, "20 个种子内应有成功的速度训练");
    }

    /// 变胖：治病 / 合宿版休息解除；普通休息不解除
    #[test]
    fn fat_cleared_by_clinic_and_camp_rest() {
        let mut game = crate::testing::sample_game();
        let mut rng = StdRng::seed_from_u64(1);
        game.uma_mut().flags.ill.insert(Ill::FAT);
        game.do_clinic(&mut rng).expect("治病");
        assert!(!game.is_fat(), "治病应清空生病位");

        // 普通休息不解除
        game.base.turn = 20;
        game.plan.camp_rest = false;
        game.uma_mut().flags.ill.insert(Ill::FAT);
        game.do_rest(&mut rng).expect("普通休息");
        assert!(game.is_fat(), "普通休息不解除变胖");

        // 合宿版休息解除
        game.base.turn = 36;
        game.plan.camp_rest = true;
        game.do_rest(&mut rng).expect("合宿版休息");
        assert!(!game.is_fat(), "合宿版休息解除变胖");
    }
}
