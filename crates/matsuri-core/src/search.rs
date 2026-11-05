//! FlatSearch 搜索训练员（ramen 式集成层）
//!
//! 决策面三类，全部走引擎 `FlatSearch`（扁平 MC + UCB + CRN），由
//! `matsuri_config [search]` 的 `search_n` 与开关控制：
//! - Train 主动作：`FlatSearch::search`，候选全量（ramen 口径，不剪枝）；
//! - 教诲分配：手写价值矩阵先剪到 top-16，再对子集搜索；
//! - 事件选项：`FlatSearch::search_with` + dummy action token，应用当前选项后
//!   从 `event_queue` 的下一个事件继续（`event_cursor` 留在局面上）。
//!
//! `search_n` 是**每个候选**的 rollout 次数（引擎口径）。搜索 interval 内全部
//! `diag!` 由引擎 `DiagGuard` 静默，结束后只打一行摘要。

use std::time::Instant;

use anyhow::{Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, FlatSearchGame, NoTerminal, SearchConfig};

use crate::config::{MatsuriConfig, SearchScopeConfig};
use crate::diag;
use crate::game::{Action, MatsuriGame};
use crate::trainer::MatsuriTrainer;

/// 教诲候选剪枝宽度：120 排列全搜不可行，按 lab / 本轮定档取 16
const ADVICE_TOPK: usize = 16;
/// 事件搜索最多支持的候选数（dummy token 容量：5 个 Train + 5 个 ActTrain）
const EVENT_MAX_CHOICES: usize = 10;

/// 叶子估值器接口（预留）
///
/// 当前引擎没有截断搜索的 leaf hook；生产 `max_depth = 0`，本接口不进入搜索路径。
/// 后续接截断时，由搜索侧在 depth 截断处调用，返回终局分估计。
pub trait LeafEvaluator: Send + Sync {
    /// 估计当前局面的终局评分
    fn evaluate(&self, game: &MatsuriGame) -> Result<f64>;
}

/// 占位实现：返回当前搜索分（`pt_favor = 1.0`），仅供接口接线与测试
pub struct ScoreLeaf;

impl LeafEvaluator for ScoreLeaf {
    fn evaluate(&self, game: &MatsuriGame) -> Result<f64> {
        game.search_score(1.0)
    }
}

/// FlatSearch 搜索训练员
pub struct MatsuriSearchTrainer {
    /// 引擎扁平搜索内核
    search: FlatSearch<MatsuriGame>,
    /// 手写基策：fallback + rollout 教师（不带任何搜索）
    fallback: MatsuriTrainer,
    /// 搜索作用面（来自 `matsuri_config [search]`）
    scope: SearchScopeConfig,
    /// 搜索排序的 PT 偏好（与手写剪枝同口径）
    pt_favor_rate: f64,
}

impl MatsuriSearchTrainer {
    /// 用剧本配置 + 引擎搜索参数构造；`[search].search_n = 0` / `max_depth > 0` 直接拒绝
    ///
    /// `search_n` 以剧本配置为准（每候选 rollout 次数），入口层只需用 env / 覆盖层
    /// 改写 `matsuri_config.search.search_n`；引擎 `SearchConfig` 只提供内核参数。
    pub fn new(config: MatsuriConfig, mut search: SearchConfig) -> Result<Self> {
        if config.search.search_n == 0 {
            bail!("search.search_n 必须大于 0（0 = 手写档，不应构造搜索训练员）");
        }
        if search.max_depth != 0 {
            bail!("叶子估值器接口已预留，截断搜索未接；max_depth 必须为 0");
        }
        search.search_n = config.search.search_n;
        search.pt_favor_rate = config.trainer.pt_favor_rate;
        let pt_favor_rate = search.pt_favor_rate;
        let scope = config.search.clone();
        let fallback = MatsuriTrainer::new(config.clone());
        let searcher = FlatSearch::<MatsuriGame>::new(search)
            .with_rollout_trainer(MatsuriTrainer::new(config))
            .with_parallel(true);
        Ok(Self {
            search: searcher,
            fallback,
            scope,
            pt_favor_rate,
        })
    }

    /// 候选剪枝：手写分排序取前 K，并强制并入手写 argmax
    fn prune(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        topk: usize,
    ) -> Result<(Vec<Action>, Vec<usize>)> {
        let mut scored = self.fallback.score_actions(game, actions)?;
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        // 保证搜索空间不劣于「本点走纯手写」：手写 argmax 必须在子集里。
        // 本函数只用于教诲分配（候选全是分配方案），手写分支就是同一批方案分取最大，
        // 直接复用排序首项，省掉第二次 Model 与 120 次方案打分。
        // 先取 hand 再滤非有限值，与 choose_action 在未过滤表上取首项的语义一致。
        let hand = scored.first().map_or(0, |(index, _)| *index);
        let mut keep: Vec<usize> = scored
            .into_iter()
            .filter(|(_, value)| value.is_finite())
            .take(topk)
            .map(|(i, _)| i)
            .collect();
        if !keep.contains(&hand)
            && let Some(last) = keep.last_mut()
        {
            *last = hand;
        }
        keep.sort_unstable();
        keep.dedup();
        let subset = keep.iter().map(|&index| actions[index].clone()).collect();
        Ok((subset, keep))
    }

    /// Train 主动作搜索
    fn search_train(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        // Train 全候选（ramen 口径）；候选剪枝不进配置
        let started = Instant::now();
        // 与 ramen 一致：搜索从调用方决策流取种子；局面规则流由 `fork_for_rollout`
        // 独立派生，搜索不读 / 不改真实局面的规则流。
        let out = self
            .search
            .search::<NoTerminal, _>(game, actions, rng, |_| NoTerminal)?;
        let chosen = out.best_action_idx;
        let mean = out.action_results[chosen].weighted_mean(out.radical_factor);
        diag!(
            "【搜索】T{} Train 候选 {} × {} → #{} {} 均值={:.0} 耗时={}ms",
            game.turn(),
            actions.len(),
            self.search.config().search_n,
            chosen,
            actions[chosen],
            mean,
            started.elapsed().as_millis()
        );
        Ok(chosen)
    }

    /// 教诲分配搜索：手写价值矩阵剪枝后交给 FlatSearch
    fn search_advice(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let (subset, map) = self.prune(game, actions, ADVICE_TOPK)?;
        if subset.len() < 2 {
            return self.fallback.select_action(game, actions, rng);
        }
        let started = Instant::now();
        let out = self
            .search
            .search::<NoTerminal, _>(game, &subset, rng, |_| NoTerminal)?;
        let picked = out.best_action_idx;
        let chosen = map[picked];
        let mean = out.action_results[picked].weighted_mean(out.radical_factor);
        diag!(
            "【搜索】T{} 教诲 候选 {}→{} × {} → 方案{} 均值={:.0} 耗时={}ms",
            game.turn(),
            actions.len(),
            subset.len(),
            self.search.config().search_n,
            chosen,
            mean,
            started.elapsed().as_millis()
        );
        Ok(chosen)
    }

    /// 事件选项搜索
    ///
    /// 事件不是 `Action`：用 dummy token 走 `FlatSearch::search_with` 复用同一套
    /// UCB / CRN；闭包内 fork → 应用当前选项 → 从 `event_queue` 下一事件继续 →
    /// 再跑后续 stage 到终局。
    fn search_event(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let tokens: Vec<Action> = (0..choices.len()).map(event_token).collect();
        let started = Instant::now();
        let pt = self.pt_favor_rate;
        let fallback = &self.fallback;
        let out = self
            .search
            .search_with(game, &tokens, rng, |g, token, seed| {
                let choice = choice_of_token(token)?;
                let mut sim = g.fork_for_rollout(seed);
                if sim.event_queue.get(sim.event_cursor).is_none() {
                    bail!("事件搜索缺少事件队列游标");
                }
                let mut streams = sim.base.take_rule_streams()?;
                // 当前事件选项
                sim.apply_event(event, choice, &mut streams.event)?;
                sim.event_cursor += 1;
                // 同一 stage 的剩余事件从下一个继续（fallback 决策 / 结算）
                let mut sim_rng = StdRng::seed_from_u64(seed);
                while sim.event_cursor < sim.event_queue.len() {
                    // 与 `run_event_queue` 同口径：取走当前槽留占位，避免每事件一次深拷贝
                    let next = std::mem::take(&mut sim.event_queue[sim.event_cursor]);
                    sim.run_event(&next, fallback, &mut sim_rng, &mut streams.event)?;
                    sim.event_cursor += 1;
                }
                sim.event_queue.clear();
                sim.event_cursor = 0;
                sim.base.put_rule_streams(streams);
                while sim.next() {
                    sim.run_stage(fallback, &mut sim_rng)?;
                }
                sim.on_simulation_end(fallback, &mut sim_rng)?;
                sim.search_score(pt)
            })?;
        let picked = out.best_action_idx;
        let mean = out.action_results[picked].weighted_mean(out.radical_factor);
        diag!(
            "【搜索】T{} 事件#{} {} 选项{} × {} → 第{}项 均值={:.0} 耗时={}ms",
            game.turn(),
            event.id,
            event.name,
            choices.len(),
            self.search.config().search_n,
            picked,
            mean,
            started.elapsed().as_millis()
        );
        Ok(picked)
    }
}

impl Trainer<MatsuriGame> for MatsuriSearchTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.is_empty() {
            bail!("候选动作为空，无法选择");
        }
        if actions.len() == 1 {
            return self.fallback.select_action(game, actions, rng);
        }
        if actions
            .iter()
            .all(|action| matches!(action, Action::AssignAdvice(_)))
        {
            return if self.scope.advice {
                self.search_advice(game, actions, rng)
            } else {
                self.fallback.select_action(game, actions, rng)
            };
        }
        if self.scope.train {
            self.search_train(game, actions, rng)
        } else {
            self.fallback.select_action(game, actions, rng)
        }
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.fallback.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        if !self.scope.event
            || choices.len() <= 1
            || choices.len() > EVENT_MAX_CHOICES
            || game.event_queue.get(game.event_cursor).is_none()
        {
            return self.fallback.select_event_choice(game, event, choices, rng);
        }
        self.search_event(game, event, choices, rng)
    }
}

/// dummy action token：0–4 用 `Train`，5–9 用 `ActTrain`（仅作候选标识）
fn event_token(index: usize) -> Action {
    if index < 5 {
        Action::Train(index as u8)
    } else {
        Action::ActTrain((index - 5) as u8)
    }
}

/// token → 事件选项下标
fn choice_of_token(token: &Action) -> Result<usize> {
    match token {
        Action::Train(index) => Ok(usize::from(*index)),
        Action::ActTrain(index) => Ok(usize::from(*index) + 5),
        other => bail!("非法事件搜索 token: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;

    #[test]
    fn event_token_roundtrip() {
        for index in 0..EVENT_MAX_CHOICES {
            let token = event_token(index);
            assert_eq!(choice_of_token(&token).expect("token"), index);
        }
    }

    #[test]
    fn score_leaf_returns_search_score() {
        let game = crate::testing::sample_game();
        let leaf = ScoreLeaf;
        let value = leaf.evaluate(&game).expect("叶估");
        assert_eq!(
            value,
            game.search_score(1.0).expect("搜索分"),
            "占位叶估应等于 pt_favor=1.0 的搜索分"
        );
    }

    /// 搜索随机流独立派生：不消费调用方决策流
    #[test]
    fn search_consumes_one_decision_draw() {
        let mut cfg = MatsuriConfig::default();
        cfg.search.search_n = 1;
        let trainer =
            MatsuriSearchTrainer::new(cfg, super::SearchConfig::default().with_search_n(1))
                .expect("搜索训练员");
        let mut game = crate::testing::sample_game();
        game.stage = crate::game::Stage::Train;
        game.plan = game.derive_plan();
        game.base.distribution = vec![Vec::new(); 5];
        let actions = vec![Action::Train(0), Action::Rest];
        let mut rng = StdRng::seed_from_u64(123);
        let mut probe = StdRng::seed_from_u64(123);
        trainer
            .select_action(&game, &actions, &mut rng)
            .expect("搜索决策");
        // 与 ramen 一致：FlatSearch 从调用方决策流取一个 rollout 种子；
        // 局面规则流由 fork_for_rollout 独立派生，不读真实局面的规则流。
        probe.next_u64();
        assert_eq!(
            rng.next_u64(),
            probe.next_u64(),
            "搜索应恰好消费一个决策流随机字"
        );
    }
}
