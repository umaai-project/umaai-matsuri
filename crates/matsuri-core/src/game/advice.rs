//! 教诲系统（S5）：槽效果聚合 / 融合（転成）/ 分配枚举
//!
//! 数量与流程口径见 `docs/mechanism.md` §6.1 与 `docs/matsuri_memo_cn.md` §4：
//! 永远 5 槽（一槽一演目）；教え效果等级 = **绑定演目的 Lv**；效果只在该演目担当**参与**
//! 该训练时生效，且**按参与人数叠加**（絆 +3 平摊是例外，不按人数叠加）。
//! 融合（転成）= **新教诲效果（按绑定演目 Lv）+ 被消耗基础教诲的固定升级效果**（`fusion` 资产）。
//!
//! 本阶段按用户 2026-11-01 拍板：**只落接口与机械默认**（120 全排列 + apply），不做策略估值 /
//! 自动择优（留 S7 训练员）；默认行为 = 顺序分配、不变更现有排列、融合就地、401+801 硬约束。

use anyhow::{Result, bail};
use umasim::game::Game;

use crate::config::FusionPair;
use crate::data::{AdviceEffect, FusionEffectKind};
use crate::diag;
use crate::game::MatsuriGame;

/// 单槽聚合后的教诲数值（已按绑定演目 Lv 取级；尚未按参与人数加权）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AdviceSlotEffect {
    /// 絆の教え：全编组卡 +N（平摊，不按人数叠加）
    pub relationship_gain: i32,
    /// 得意率 +N
    pub deyilv: i32,
    /// 灵感发生率 +N%
    pub hint_rate: i32,
    /// 助力：非友情训练效果 +N%
    pub non_friend_training_effect: i32,
    /// 助力：是否获得「助力の支え」
    pub support_assist: bool,
    /// 体力：演目稽古体力消耗 -N%
    pub keiko_vital_cost_drop: i32,
    /// 特化：主基础获得量 +N%
    pub main_gain: i32,
    /// 特化：训练上层基础上限 +N
    pub status_limit: i32,
    /// 研鑽：追加稽古发生率 +N%
    pub kensan_rate: i32,
    /// 協力：友情 +N%
    pub cooperate_friendship: i32,
    /// 協力：担当额外登场处数
    pub cooperate_split: i32,
    /// 不屈：体力 ≥N 时失败率 -100%（`None` = 无此教诲）
    pub fail_vital_threshold: Option<i32>,
    /// 演目稽古训练效果 +N%
    pub keiko_training_effect: i32,
    /// 両道：副基础获得量 +N%
    pub dual_sub_gain: i32,
    /// 転成 101：稽古主基础 +N%（参与人数加权）
    pub keiko_main_bonus: i32,
    /// 転成 101：合宿中每个担当额外登场处数
    pub camp_extra_person: i32,
    /// 転成 201：稽古友情 +N%
    pub keiko_friendship: i32,
    /// 転成 301：稽古灵感 Lv +N
    pub keiko_hint_level: i32,
}

impl AdviceSlotEffect {
    /// 叠加另一槽的效果（参与人数加权时逐参与者累加）
    fn add_assign(&mut self, other: &Self) {
        self.relationship_gain = self.relationship_gain.max(other.relationship_gain);
        self.deyilv += other.deyilv;
        self.hint_rate += other.hint_rate;
        self.non_friend_training_effect += other.non_friend_training_effect;
        self.support_assist |= other.support_assist;
        self.keiko_vital_cost_drop += other.keiko_vital_cost_drop;
        self.main_gain += other.main_gain;
        self.status_limit += other.status_limit;
        self.kensan_rate += other.kensan_rate;
        self.cooperate_friendship += other.cooperate_friendship;
        self.cooperate_split = self.cooperate_split.max(other.cooperate_split);
        self.fail_vital_threshold = match (self.fail_vital_threshold, other.fail_vital_threshold) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (None, b) => b,
            (a, None) => a,
        };
        self.keiko_training_effect += other.keiko_training_effect;
        self.dual_sub_gain += other.dual_sub_gain;
        self.keiko_main_bonus += other.keiko_main_bonus;
        self.camp_extra_person = self.camp_extra_person.max(other.camp_extra_person);
        self.keiko_friendship += other.keiko_friendship;
        self.keiko_hint_level += other.keiko_hint_level;
    }
}

/// 一次训练参与者的教诲聚合（按参与担当人数加权；絆 +N 平摊只取一次）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ParticipantAdvice {
    /// 已按参与人数累加的效果
    pub effect: AdviceSlotEffect,
    /// 絆の教え +N（平摊；全编组卡，不按人数叠加；无则 0）
    pub relationship_gain: i32,
    /// 参与的担当人数（含分身占位；不含吾妻 / 理事长 / 记者 / NPC 之外的无关人头）
    pub participant_count: i32,
}

/// 教诲分配方案：`assign[act]` = 把「当前第几号教诲槽」分给该演目（0–4 的排列）
///
/// 决策接口：每个窗口把 120 个排列交给 `Trainer` 选（S7，用户拍板「合并 120 候选」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdviceAssignment(pub [u8; 5]);

impl AdviceAssignment {
    /// 展开成槽位下标数组（`[u8; 5]` → `[usize; 5]`）
    #[must_use]
    pub fn slots(self) -> [usize; 5] {
        self.0.map(usize::from)
    }
}

impl std::fmt::Display for AdviceAssignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for slot in self.0 {
            write!(f, "{}", char::from(b'0' + slot))?;
        }
        Ok(())
    }
}

impl MatsuriGame {
    /// 某演目槽位的聚合效果（已按绑定演目 Lv 取级；未融合时 = 基础教诲效果）
    pub(crate) fn advice_slot_effect(&self, act: usize) -> AdviceSlotEffect {
        if act >= 5 {
            return AdviceSlotEffect::default();
        }
        let level = self.acts.level[act].clamp(1, 10) as usize;
        self.advice_effect_of(act, level)
    }

    /// 任意槽位（`source` = 第几号槽）在指定「绑定演目 Lv」下的聚合效果
    ///
    /// 教诲分配估值用：槽内容与槽目前落在哪个演目无关，只看 `slots/fused` 与目标 Lv。
    pub(crate) fn advice_effect_of(&self, source: usize, level: usize) -> AdviceSlotEffect {
        let mut out = AdviceSlotEffect::default();
        if !self.advice.acquired || source >= 5 {
            return out;
        }
        let level = level.clamp(1, 10);
        let base = self.advice.slots[source];
        let fused = self.advice.fused[source];
        if fused != 0 {
            if let Some(advice) = self.data.advices.iter().find(|advice| advice.id == fused) {
                apply_advice(&mut out, &advice.effect, level);
            }
            if let Some(fusion) = self
                .data
                .fusion
                .iter()
                .find(|fusion| fusion.advice_id == base)
            {
                apply_fusion(&mut out, &fusion.effect);
            }
        } else if base != 0
            && let Some(advice) = self.data.advices.iter().find(|advice| advice.id == base)
        {
            apply_advice(&mut out, &advice.effect, level);
        }
        out
    }

    /// 一次训练参与者的教诲聚合（遍历该训练位占位；按担当归集）
    pub(crate) fn participant_advice(&self, train: usize) -> ParticipantAdvice {
        let mut out = ParticipantAdvice::default();
        let Some(slot) = self.base.distribution.get(train) else {
            return out;
        };
        for &index in slot {
            if index < 0 {
                continue;
            }
            let Some(person) = self.persons.get(index as usize) else {
                continue;
            };
            let Some(act) = person.act else {
                continue;
            };
            let effect = self.advice_slot_effect(act);
            out.effect.add_assign(&effect);
            out.relationship_gain = out.relationship_gain.max(effect.relationship_gain);
            out.participant_count += 1;
        }
        out
    }

    /// turn 2：首次获得 5 个基础教诲，默认按「101→act0 … 501→act4」顺序落位
    pub(crate) fn acquire_advice(&mut self) {
        if self.advice.acquired {
            return;
        }
        self.advice.acquired = true;
        self.advice.slots = [101, 201, 301, 401, 501];
        self.advice.fused = [0; 5];
        diag!(
            "【教诲】T{} 首次授予 5 槽初始化 slots={:?}",
            self.turn(),
            self.advice.slots
        );
    }

    /// 融合（転成）就地生效：把 `base` 所在槽的教诲换成 `target`，保留基础 id 供升级效果查表
    pub(crate) fn apply_fusion_pairs(&mut self, pairs: &[FusionPair]) -> Result<()> {
        if !self.advice.acquired {
            bail!("尚未获得教诲，不能融合");
        }
        for pair in pairs {
            let Some(act) = (0..5).find(|act| self.advice.slots[*act] == pair.base) else {
                bail!("融合失败：基础教诲 {} 不在任何槽位", pair.base);
            };
            if self.advice.fused[act] != 0 {
                bail!("融合失败：演目 {act} 的槽位已融合过");
            }
            self.advice.fused[act] = pair.target;
            diag!(
                "【教诲】T{} 融合 演目{}: {}→{}",
                self.turn(),
                act,
                pair.base,
                pair.target
            );
        }
        self.enforce_advice_constraints();
        Ok(())
    }

    /// 分配硬约束（2026-10-31 口径）：401+801 必须落在**担当数最多**的演目（并列取先头）
    pub(crate) fn enforce_advice_constraints(&mut self) {
        let Some(source) = (0..5).find(|act| self.advice.fused[*act] == 801) else {
            return;
        };
        let counts: [usize; 5] = std::array::from_fn(|act| {
            self.persons
                .iter()
                .filter(|person| person.act == Some(act))
                .count()
        });
        let Some(target) = (0..5).max_by_key(|act| (counts[*act], usize::MAX - *act)) else {
            return;
        };
        if target != source {
            self.swap_advice_slots(source, target);
            diag!(
                "【教诲】T{} 401+801 硬约束 交换演目 {source}↔{target}",
                self.turn()
            );
        }
    }

    /// 交换两个演目的教诲槽（槽内容 = base + fused 成对移动）
    fn swap_advice_slots(&mut self, a: usize, b: usize) {
        self.advice.slots.swap(a, b);
        self.advice.fused.swap(a, b);
    }

    /// 枚举当前 5 个教诲槽在 5 个演目上的**全排列**（5! = 120；首个 = 保持现状）
    ///
    /// S7 决策接口：每个窗口把这 120 个候选交给 `Trainer`（合并一次决策）。
    pub(crate) fn assignment_candidates(&self) -> Vec<AdviceAssignment> {
        let mut out = Vec::with_capacity(120);
        let mut order = [0usize, 1, 2, 3, 4];
        permute(&mut order, 0, &mut out);
        out
    }

    /// 应用一个分配方案（`assign[act]` = 当前第几号教诲槽；必须是 0–4 的排列）
    pub(crate) fn apply_assignment(&mut self, assign: &AdviceAssignment) -> Result<()> {
        let mut seen = [false; 5];
        for source in assign.slots() {
            if source >= 5 {
                bail!("分配方案槽位越界: {source}（应为 0–4）");
            }
            if std::mem::replace(&mut seen[source], true) {
                bail!("分配方案槽位重复: {source}");
            }
        }
        let slots = self.advice.slots;
        let fused = self.advice.fused;
        for (act, source) in assign.slots().into_iter().enumerate() {
            self.advice.slots[act] = slots[source];
            self.advice.fused[act] = fused[source];
        }
        self.enforce_advice_constraints();
        diag!(
            "【教诲】T{} 分配 slots={:?} fused={:?}",
            self.turn(),
            self.advice.slots,
            self.advice.fused
        );
        Ok(())
    }

    /// 教诲分配决策：把 120 个候选交给 `Trainer`，应用选中方案（S7）
    ///
    /// 决策只走决策流（`decision`），不消耗结算随机；`Trainer` 确定性选择时回放可复现。
    pub(crate) fn decide_advice_assignment<T: umasim::game::Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut rand::rngs::StdRng,
    ) -> Result<()> {
        let candidates = self.assignment_candidates();
        let actions: Vec<super::Action> = candidates
            .iter()
            .map(|assign| super::Action::AssignAdvice(*assign))
            .collect();
        let index = trainer.select_action(self, &actions, decision)?;
        let Some(super::Action::AssignAdvice(assign)) = actions.get(index) else {
            bail!("教诲分配决策返回了下标 {index}，但候选不是分配方案");
        };
        diag!(
            "【教诲】T{} 分配决策 方案 {}",
            self.turn(),
            assign.slots().map(|slot| slot.to_string()).join("")
        );
        self.apply_assignment(assign)
    }

    /// 找「研鑽（701）」当前所在演目（至多一个）
    pub(crate) fn kensan_act(&self) -> Option<usize> {
        if !self.advice.acquired {
            return None;
        }
        (0..5).find(|act| self.advice.fused[*act] == 701)
    }

    /// 本回合研鑽追加稽古是否掷中（`Begin` 用策略流预掷；掷中后未点对应训练即作废）
    pub(crate) fn roll_kensan(&mut self, rng: &mut impl rand::Rng) {
        self.kensan_pending = false;
        let Some(act) = self.kensan_act() else {
            return;
        };
        let rate = self.advice_slot_effect(act).kensan_rate;
        if rate > 0 {
            self.kensan_pending = rng.random_bool(rate as f64 / 100.0);
            // 只记掷中（未中是每回合例行判定，记了反而埋没关键变化）
            if self.kensan_pending {
                diag!("【追加稽古】T{} 演目{} 掷中（{}%）", self.turn(), act, rate);
            }
        }
    }
}

/// 把一档教诲效果并入槽聚合
fn apply_advice(out: &mut AdviceSlotEffect, effect: &AdviceEffect, level: usize) {
    let index = level.saturating_sub(1);
    match effect {
        AdviceEffect::Friendship { levels } => {
            let row = &levels[index];
            out.relationship_gain = out.relationship_gain.max(row.gauge);
            out.keiko_main_bonus += row.keiko_main_bonus;
        }
        AdviceEffect::Deyilv { levels } => out.deyilv += levels[index].deyilv,
        AdviceEffect::Hint { levels } => out.hint_rate += levels[index].hint_rate,
        AdviceEffect::Assist { levels } => {
            let row = &levels[index];
            out.non_friend_training_effect += row.training_effect;
            out.support_assist |= row.support_assist;
        }
        AdviceEffect::Vital { levels } => {
            out.keiko_vital_cost_drop += levels[index].keiko_vital_cost_drop;
        }
        AdviceEffect::Special { levels } => {
            let row = &levels[index];
            out.main_gain += row.main_gain;
            out.status_limit += row.status_limit;
        }
        AdviceEffect::Kensan { levels } => out.kensan_rate += levels[index].rate,
        AdviceEffect::Cooperate { levels } => {
            let row = &levels[index];
            out.cooperate_friendship += row.friendship;
            out.cooperate_split = out.cooperate_split.max(row.split);
        }
        AdviceEffect::Unyielding { levels } => {
            let row = &levels[index];
            out.fail_vital_threshold = match out.fail_vital_threshold {
                Some(threshold) => Some(threshold.min(row.fail_vital_threshold)),
                None => Some(row.fail_vital_threshold),
            };
            out.keiko_training_effect += row.keiko_training_effect;
        }
        AdviceEffect::Dual { levels } => {
            let row = &levels[index];
            out.dual_sub_gain += row.sub_gain;
            out.keiko_training_effect += row.keiko_training_effect;
        }
    }
}

/// 把被消耗基础教诲的固定転成升级效果并入槽聚合
fn apply_fusion(out: &mut AdviceSlotEffect, effect: &FusionEffectKind) {
    match effect {
        FusionEffectKind::Friendship {
            keiko_main_bonus,
            camp_extra_person,
        } => {
            out.keiko_main_bonus += keiko_main_bonus;
            out.camp_extra_person = out.camp_extra_person.max(*camp_extra_person);
        }
        FusionEffectKind::Deyilv {
            deyilv,
            keiko_friendship,
        } => {
            out.deyilv += deyilv;
            out.keiko_friendship += keiko_friendship;
        }
        FusionEffectKind::Hint {
            hint_rate,
            hint_level,
        } => {
            out.hint_rate += hint_rate;
            out.keiko_hint_level += hint_level;
        }
        FusionEffectKind::Assist {
            training_effect,
            support_assist,
        } => {
            out.non_friend_training_effect += training_effect;
            out.support_assist |= support_assist;
        }
        FusionEffectKind::Vital {
            keiko_vital_cost_drop,
        } => out.keiko_vital_cost_drop += keiko_vital_cost_drop,
    }
}

/// 生成 `order` 从 `k` 起的全排列（5! = 120；由 `assignment_candidates` 使用）
fn permute(order: &mut [usize; 5], k: usize, out: &mut Vec<AdviceAssignment>) {
    if k == order.len() {
        out.push(AdviceAssignment(order.map(|slot| slot as u8)));
        return;
    }
    for i in k..order.len() {
        order.swap(k, i);
        permute(order, k + 1, out);
        order.swap(k, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_count_and_uniqueness() {
        let mut order = [0usize, 1, 2, 3, 4];
        let mut out = Vec::new();
        permute(&mut order, 0, &mut out);
        assert_eq!(out.len(), 120, "5 槽全排列应为 120");
        let mut sorted = out.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 120, "120 个排列应互不相同");
        assert!(out.contains(&AdviceAssignment([0, 1, 2, 3, 4])));
        assert!(out.contains(&AdviceAssignment([4, 3, 2, 1, 0])));
    }

    /// 找一个人头下标（按担当演目 + 来源过滤）
    fn person_of_act(game: &MatsuriGame, act: usize, nth: usize) -> usize {
        game.persons
            .iter()
            .enumerate()
            .filter(|(_, person)| person.act == Some(act))
            .map(|(index, _)| index)
            .nth(nth)
            .expect("该演目的担当人数不足")
    }

    #[test]
    fn slot_effect_uses_bound_act_level() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.acts.level = [1, 3, 5, 7, 10];
        let slot0 = game.advice_slot_effect(0);
        assert_eq!(slot0.relationship_gain, 3, "絆の教え Lv1 的 +3");
        assert_eq!(slot0.keiko_main_bonus, 3);
        let expect_deyilv = match &game.data.advices[1].effect {
            AdviceEffect::Deyilv { levels } => levels[2].deyilv,
            other => panic!("201 应为得意率教诲: {other:?}"),
        };
        assert_eq!(
            game.advice_slot_effect(1).deyilv,
            expect_deyilv,
            "得意率效果按绑定演目 Lv 取级"
        );
        let slot4 = game.advice_slot_effect(4);
        assert!(
            slot4.keiko_vital_cost_drop > 0,
            "体力教え Lv10 应有稽古体力下降"
        );
    }

    #[test]
    fn fusion_adds_new_and_upgraded_effects() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.advice.fused = [601, 0, 0, 0, 0];
        game.acts.level = [1; 5];
        let slot0 = game.advice_slot_effect(0);
        let expect_main = match &game.data.advices[5].effect {
            AdviceEffect::Special { levels } => levels[0].main_gain,
            other => panic!("601 应为特化教诲: {other:?}"),
        };
        assert_eq!(slot0.main_gain, expect_main, "転成后新教诲（特化 Lv1）");
        assert_eq!(slot0.keiko_main_bonus, 15, "転成 101 的固定升级效果");
        assert_eq!(slot0.camp_extra_person, 1, "転成 101 的合宿额外人数");
        assert_eq!(
            slot0.relationship_gain, 0,
            "基础教诲 101 已被消耗，不再给 +3"
        );
    }

    #[test]
    fn participant_advice_stacks_by_count() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.acts.level = [1; 5];
        let first = person_of_act(&game, 0, 0);
        let second = person_of_act(&game, 0, 1);
        let other = person_of_act(&game, 1, 0);
        game.base.distribution = vec![
            vec![first as i32, second as i32, other as i32],
            vec![],
            vec![],
            vec![],
            vec![],
        ];
        let advice = game.participant_advice(0);
        assert_eq!(advice.participant_count, 3);
        assert_eq!(advice.relationship_gain, 3, "絆 +3 平摊、不按人数叠加");
        assert_eq!(
            advice.effect.keiko_main_bonus, 6,
            "絆的稽古主基础按参与人数叠加（2 名 act0 担当 × 3）"
        );
        assert_eq!(
            advice.effect.non_friend_training_effect, 0,
            "参与的训练非友情时才有助力"
        );
    }

    #[test]
    fn assignment_candidates_enumerate_and_apply() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.advice.fused = [0; 5];
        let candidates = game.assignment_candidates();
        assert_eq!(candidates.len(), 120, "5 槽全排列 120");
        assert_eq!(
            candidates[0],
            AdviceAssignment([0, 1, 2, 3, 4]),
            "首个排列 = 保持现状"
        );
        game.apply_assignment(&AdviceAssignment([4, 3, 2, 1, 0]))
            .expect("合法排列");
        assert!(
            game.apply_assignment(&AdviceAssignment([0, 0, 1, 2, 3]))
                .is_err(),
            "重复槽位应报错"
        );
        assert_eq!(game.advice.slots, [501, 401, 301, 201, 101]);
    }

    #[test]
    fn enforce_801_on_most_assigned_act() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.advice.fused = [0, 0, 801, 0, 0];
        game.enforce_advice_constraints();
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
            .unwrap();
        let slot = (0..5).find(|act| game.advice.fused[*act] == 801).unwrap();
        assert_eq!(
            slot, target,
            "401+801 必须落在担当数最多的演目（{counts:?}）"
        );
    }

    #[test]
    fn kensan_act_found_after_fusion() {
        let mut game = crate::testing::sample_game();
        game.advice.acquired = true;
        game.advice.slots = [101, 201, 301, 401, 501];
        game.advice.fused = [0, 701, 0, 0, 0];
        assert_eq!(game.kensan_act(), Some(1));
    }

    #[test]
    fn advice_effect_aggregation() {
        let levels = vec![
            crate::data::FriendshipLevel {
                gauge: 3,
                keiko_main_bonus: 3
            };
            10
        ];
        let mut out = AdviceSlotEffect::default();
        apply_advice(&mut out, &AdviceEffect::Friendship { levels }, 1);
        assert_eq!(out.relationship_gain, 3);
        assert_eq!(out.keiko_main_bonus, 3);
        // 参与人数加权：两槽叠加
        let mut total = AdviceSlotEffect::default();
        total.add_assign(&out);
        total.add_assign(&out);
        assert_eq!(total.relationship_gain, 3, "絆 +3 不按人数叠加");
        assert_eq!(total.keiko_main_bonus, 6, "稽古主基础按参与人数叠加");
    }
}
