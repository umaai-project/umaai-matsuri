//! 手写训练员（S7）：守门链 + 打分表 + 事件选项 + 教诲分配择优
//!
//! 目标口径见 [`crate::value`]：官方评分（五维查表 + PT×折算率），PT 再乘配置的
//! `pt_favor_rate`（`1.0` = 官方评分）。所有决策都是**确定性**的（不消耗决策随机流），
//! 保证回放 / CRN / 基线可复现；策略旋钮全部来自 `MatsuriConfig.trainer`（可扫参）。
//!
//! 决策面（引擎契约只给 `Trainer` 三个 hook，教诲分配由剧本在 `Begin` / `End` 主动询问）：
//! - `select_action`：普通动作候选 → 守门链 + 打分表；教诲分配候选 → 价值矩阵择优；
//! - `select_event_choice`：事件选项 → 通用期望值打分（追加稽古「上 / 下」等按语义自动比）；
//! - `select_choice`：旧接口兜底（无事件数据时按通用期望值打分）。

use anyhow::{Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Ill;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, NoTerminal, SearchConfig};

use crate::config::MatsuriConfig;
use crate::diag;
use crate::game::{Action, MatsuriGame};
use crate::value::Model;

/// 手写训练员（无状态：策略只来自随局配置；便于 `FlatSearch` rollout 复用）
pub struct MatsuriTrainer {
    /// 剧本配置（融合预设 + 训练员旋钮）
    config: MatsuriConfig,
    /// 蒙特卡洛搜索配置（引擎 `FlatSearch`：扁平 MC + UCB 动态分配 + CRN）
    ///
    /// `None` = 纯手写打分。**rollout 基策必须是 `None`**：否则搜索里的每次 rollout
    /// 又会在教诲决策点发起搜索，成本按深度指数爆炸。
    search: Option<SearchConfig>,
    /// 教诲分配候选的剪枝宽度（24 个互异落位全搜太贵，只搜手写打分最高的前 K 个）
    search_topk: usize,
}

impl MatsuriTrainer {
    /// 用剧本配置构造训练员（不开蒙特卡洛）
    pub fn new(config: MatsuriConfig) -> Self {
        Self {
            config,
            search: None,
            search_topk: 4,
        }
    }

    /// 打开教诲决策点的蒙特卡洛搜索（`search_n = 0` 视为关闭）
    #[must_use]
    pub fn with_search(mut self, search: SearchConfig, topk: usize) -> Self {
        self.search = (search.search_n > 0).then_some(search);
        self.search_topk = topk.max(1);
        self
    }

    /// 取剧本配置（宿主 / 训练员参数可读）
    #[must_use]
    pub fn config(&self) -> &MatsuriConfig {
        &self.config
    }

    /// 是否需要优先治病（变胖只影响训练速度、每回合 20% 自解，不占动作；不擅长 / 具名病才治）
    fn needs_clinic(game: &MatsuriGame) -> bool {
        let flags = &game.uma().flags;
        flags.bad_trainer || flags.ill.bits() & !Ill::FAT.bits() != 0
    }

    /// 教诲分配的蒙特卡洛择优（引擎 `FlatSearch`：扁平 MC + UCB + CRN 配对种子）
    ///
    /// 手写打分把 24 个互异落位排序后只留前 `search_topk` 个进搜索，每个候选各跑
    /// `search_n` 次 rollout 到终局；中选口径是搜索的排名加权平均分（`radical_factor`
    /// 按剩余回合确定性衰减）。返回 `None` = 剪枝后不足 2 个候选，无需搜索。
    fn search_advice(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        scored: &[(f64, usize)],
        search: &SearchConfig,
    ) -> Result<Option<(usize, f64)>> {
        let cand: Vec<Action> = scored
            .iter()
            .take(self.search_topk)
            .map(|&(_, index)| actions[index].clone())
            .collect();
        if cand.len() < 2 {
            return Ok(None);
        }
        let searcher = FlatSearch::<MatsuriGame>::new(search.clone())
            // rollout 基策 = 同一份配置的手写训练员，且**不再开搜索**（防递归）
            .with_rollout_trainer(MatsuriTrainer::new(self.config.clone()))
            .with_parallel(true);
        // 搜索流独立派生：不吃决策随机流（决策确定性是 CRN / 回放的契约），
        // 按 (主种子, 回合) 定值，同一局面重复搜索逐位一致
        let mut rng = StdRng::seed_from_u64(
            game.rule_master().unwrap_or(0) ^ (game.turn().max(0) as u64) << 40,
        );
        // rollout 里每回合都会打诊断行，几万局下来会淹掉终局块 → 静默
        // （guard 出块即恢复，本决策点自己的诊断行照打）
        let (best, value) = {
            let _quiet = umaai_core::diag::DiagGuard::suppress();
            let out = searcher.search::<NoTerminal, _>(game, &cand, &mut rng, |_| NoTerminal)?;
            (
                out.best_action_idx,
                out.action_results[out.best_action_idx].weighted_mean(out.radical_factor),
            )
        };
        diag!(
            "【搜索】T{} 教诲 MC 候选 {} 个 × {} 次 → 中选第 {} 位 值={:.1}",
            game.turn(),
            cand.len(),
            search.search_n,
            best,
            value
        );
        Ok(Some((scored[best].1, value)))
    }
}

impl Trainer<MatsuriGame> for MatsuriTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.is_empty() {
            bail!("候选动作为空，无法选择");
        }
        if actions.len() == 1 {
            diag!(
                "【决策】T{} 唯一候选 {}（无需比较）",
                game.turn(),
                actions[0]
            );
            return Ok(0);
        }
        // 教诲分配决策点：候选全是分配方案 → 价值矩阵择优（确定性）
        if actions
            .iter()
            .all(|action| matches!(action, Action::AssignAdvice(_)))
        {
            let model = Model::new(game, &self.config.trainer)?;
            // 手写打分先给全部候选排序：既是无搜索时的答案，也是有搜索时的剪枝依据
            let mut scored: Vec<(f64, usize)> = Vec::with_capacity(actions.len());
            for (index, action) in actions.iter().enumerate() {
                let Action::AssignAdvice(assign) = action else {
                    continue;
                };
                scored.push((model.assignment_value(assign), index));
            }
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            let (mut best_value, mut best) = scored
                .first()
                .map_or((f64::NEG_INFINITY, 0), |&(value, index)| (value, index));
            // 蒙特卡洛：中选口径换成「搜索到的终局期望」，手写分只负责剪枝
            if let Some(search) = self.search.as_ref()
                && let Some((index, value)) = self.search_advice(game, actions, &scored, search)?
            {
                best = index;
                best_value = value;
            }
            diag!(
                "【决策】T{} 教诲分配 值={:.1} 方案 {}",
                game.turn(),
                best_value,
                actions
                    .get(best)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
            return Ok(best);
        }

        // 守门 ①：生病 / 不擅长且可选治病时优先治病
        if Self::needs_clinic(game)
            && let Some(index) = actions.iter().position(|action| *action == Action::Clinic)
        {
            diag!("【决策】T{} 守门·治病优先（不擅长 / 生病）", game.turn());
            return Ok(index);
        }

        // 守门 ②：合宿禁令（合宿训练回合内休息 / 比赛 = 浪费训练等级 buff，强制不选）
        // 体力危险线 32 以下仍保命休息
        if game.plan.camp_train(game.forced_rest) && game.uma().vital > 32 {
            let training_actions: Vec<usize> = actions
                .iter()
                .enumerate()
                .filter(|(_, action)| matches!(action, Action::Train(_) | Action::ActTrain(_)))
                .map(|(index, _)| index)
                .collect();
            if !training_actions.is_empty() {
                // 合宿期间只从训练/稽古里选（进打分表但排除休息/比赛/出行）
                let model = Model::new(game, &self.config.trainer)?;
                let mut best = training_actions[0];
                let mut best_value = f64::NEG_INFINITY;
                for &index in &training_actions {
                    let value = model.action_value(&actions[index]);
                    if value.is_nan() {
                        continue;
                    }
                    if value > best_value {
                        best_value = value;
                        best = index;
                    }
                }
                diag!(
                    "【决策】T{} 守门·合宿全练 {} 值={:.1}",
                    game.turn(),
                    actions
                        .get(best)
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    best_value
                );
                return Ok(best);
            }
        }

        // 守门 ③：友人外出截止线（剩余次数 ≥ 剩余可用回合 → 强制友人外出）
        if let Some(index) = actions
            .iter()
            .position(|action| *action == Action::FriendOuting)
        {
            let remaining_uses = game.friend.out_used.iter().filter(|used| !**used).count();
            let remaining_turns = (game.max_turn() - game.turn()).max(0) as usize;
            // 比赛回合不可出行，估算可用回合（排除约 18% 比赛/强制回合）
            let usable_turns = (remaining_turns as f64 * 0.82) as usize;
            if remaining_uses > 0 && remaining_uses >= usable_turns {
                diag!(
                    "【决策】T{} 守门·友人外出截止（剩{}次/可用{}回合）",
                    game.turn(),
                    remaining_uses,
                    usable_turns
                );
                return Ok(index);
            }
        }

        // 守门 ④：体力危险线（vital < 32 且非合宿 → 强制休息）
        if game.uma().vital < 32
            && !game.plan.camp_train(game.forced_rest)
            && let Some(index) = actions.iter().position(|action| *action == Action::Rest)
        {
            diag!(
                "【决策】T{} 守门·体力危险（{}<32）强制休息",
                game.turn(),
                game.uma().vital
            );
            return Ok(index);
        }

        // 守门 ⑤：心情崩坏（干劲≤2 且体力>50 → 强制出行修心情）
        // GA 战报「摆烂病」根因 + 作者确认：祭典杯友人不给心情盾，掉心情影响大
        if game.uma().motivation <= 2
            && game.uma().vital > 50
            && !game.plan.camp_train(game.forced_rest)
        {
            // 优先友人出行（同时修心情 + 演目 Lv）
            if let Some(index) = actions
                .iter()
                .position(|action| *action == Action::FriendOuting)
            {
                diag!("【决策】T{} 守门·心情崩坏→友人出行", game.turn());
                return Ok(index);
            }
            if let Some(index) = actions
                .iter()
                .position(|action| *action == Action::NormalOuting)
            {
                diag!("【决策】T{} 守门·心情崩坏→普通出行", game.turn());
                return Ok(index);
            }
        }

        // 其余全部进入打分表（训练 / 稽古 / 休息 / 出行 / 比赛同权比较）
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, action) in actions.iter().enumerate() {
            let value = model.action_value(action);
            if value.is_nan() {
                continue;
            }
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        diag!(
            "【决策】T{} 动作 {} 值={:.1}",
            game.turn(),
            actions
                .get(best)
                .map(ToString::to_string)
                .unwrap_or_default(),
            best_value
        );
        Ok(best)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        if choices.len() == 1 {
            return Ok(0);
        }
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, group) in choices.iter().enumerate() {
            let value = model.event_choice_value(0, index, group);
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        diag!(
            "【决策】T{} 选项 {best} 期望={:.1}（旧接口）",
            game.turn(),
            best_value
        );
        Ok(best)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        if choices.len() == 1 {
            return Ok(0);
        }
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, group) in choices.iter().enumerate() {
            let value = model.event_choice_value(event.id, index, group);
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        diag!(
            "【决策】T{} 事件#{} {} 选项{} 期望={:.1}",
            game.turn(),
            event.id,
            event.name,
            best,
            best_value
        );
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn trainer_prefers_heal_when_bad_trainer() {
        let mut game = crate::testing::sample_game();
        game.uma_mut().flags.bad_trainer = true;
        game.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
        let trainer = MatsuriTrainer::new(MatsuriConfig::default());
        let mut rng = StdRng::seed_from_u64(1);
        let actions = vec![Action::Train(0), Action::Clinic, Action::Rest];
        let index = trainer
            .select_action(&game, &actions, &mut rng)
            .expect("选择动作");
        assert_eq!(actions[index], Action::Clinic, "不擅长训练时优先治病");
    }

    #[test]
    fn trainer_reads_trainer_knobs() {
        let text = std::fs::read_to_string(crate::testing::data_dir().join("matsuri_config.toml"))
            .expect("仓内配置默认层应存在");
        let config = MatsuriConfig::parse(&text, None).expect("默认层应可解析");
        let trainer = MatsuriTrainer::new(config);
        // 2026-11-03 合并 lab 冠军档后默认值 = 1.7271（见 dev_plan §3）
        assert_eq!(trainer.config().trainer.pt_favor_rate, 1.7271);
    }
}
