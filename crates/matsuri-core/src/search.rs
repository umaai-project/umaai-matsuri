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

use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, FlatSearchGame, RolloutOutcome, SearchConfig, SearchOutput};

use crate::config::{MatsuriConfig, SearchScopeConfig};
use crate::diag;
use crate::game::{Action, MatsuriGame, Stage};
use crate::trainer::MatsuriTrainer;

/// 教诲候选剪枝宽度：120 排列全搜不可行，按 lab / 本轮定档取 16
const ADVICE_TOPK: usize = 16;
/// 事件搜索最多支持的候选数（dummy token 容量：5 个 Train + 5 个 ActTrain）
const EVENT_MAX_CHOICES: usize = 10;

umasim::define_terminal_record!(
    /// 一次 rollout 的终局五维 / PT（只做展示，不参与搜索排序）
    MatsuriTerminal,
    /// 逐候选终局统计
    MatsuriTerminalStats {
        speed => { key: "speed", label: "速度", unit: "status" },
        stamina => { key: "stamina", label: "耐力", unit: "status" },
        power => { key: "power", label: "力量", unit: "status" },
        guts => { key: "guts", label: "根性", unit: "status" },
        wisdom => { key: "wisdom", label: "智力", unit: "status" },
        pt => { key: "pt", label: "PT", unit: "pt" },
    }
);

impl MatsuriTerminal {
    /// 从终局局面提取展示用事实（`total_pt` 失败只影响展示，记 0）
    fn of(game: &MatsuriGame) -> Self {
        let five = game.uma().five_status;
        Self {
            speed: f64::from(five[0]),
            stamina: f64::from(five[1]),
            power: f64::from(five[2]),
            guts: f64::from(five[3]),
            wisdom: f64::from(five[4]),
            pt: f64::from(game.uma().total_pt().unwrap_or(0)),
        }
    }
}

/// 候选按搜索次数加权的期望分（与 `DecisionRecord::baseline_score` 同口径）
fn weighted_baseline(candidates: &[SearchCandidate]) -> f64 {
    let (sum_score, sum_score_n, total_n) = candidates.iter().fold((0.0, 0.0, 0u32), |acc, c| {
        (
            acc.0 + c.score,
            acc.1 + c.score * f64::from(c.n),
            acc.2 + c.n,
        )
    });
    if candidates.is_empty() {
        return 0.0;
    }
    if total_n == 0 {
        sum_score / candidates.len() as f64
    } else {
        sum_score_n / f64::from(total_n)
    }
}

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

/// 一次搜索里的单个候选（供在线输出 / 面板使用）
#[derive(Debug, Clone, PartialEq)]
pub struct SearchCandidate {
    /// 候选在下标空间里的原始位置（动作下标 / 事件选项下标）
    pub index: usize,
    /// 人类可读描述
    pub description: String,
    /// 排名加权均分（与搜索排序同口径）
    pub score: f64,
    /// 单候选均分样本标准差（展示分辨率用）
    pub stdev: f64,
    /// rollout 样本数
    pub n: u32,
    /// 终局五维均值（速 / 耐 / 力 / 根 / 智）
    pub five: [f64; 5],
    /// 终局 PT 均值
    pub pt: f64,
}

/// 最近一次搜索的摘要
///
/// `chosen` 与 `candidates[].index` 都在**调用方传入的完整候选空间**里：
/// 教诲分配会在剪枝后搜索，本结构负责把子集下标映射回原动作下标。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchSummary {
    /// 全部候选（按搜索输入顺序）
    pub candidates: Vec<SearchCandidate>,
    /// 中选候选在完整候选空间里的下标
    pub chosen: usize,
    /// 本决策开始的期望分基线（候选按样本数加权；与运气分同口径）
    pub baseline: f64,
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
    /// 最近一次搜索摘要（在线档取候选分；`None` = 本次走了手写 fallback）
    last: Mutex<Option<SearchSummary>>,
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
        search.pt_favor_rate = config.deck.trainer.pt_favor_rate;
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
            last: Mutex::new(None),
        })
    }

    /// 最近一次搜索摘要（在线档在 `select_action` / `select_event_choice` 之后取）
    ///
    /// 走手写 fallback 时返回 `None`（不构造伪分数，保持「0 = 无搜索分」口径）。
    #[must_use]
    pub fn last_search(&self) -> Option<SearchSummary> {
        self.last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 写入最近一次搜索摘要
    fn set_last(&self, summary: SearchSummary) {
        *self
            .last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(summary);
    }

    /// 清空上一次摘要（本次走 fallback 时必须调用，避免把旧候选分贴到新决策上）
    fn clear_last(&self) {
        *self
            .last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    /// 搜索结果 → 摘要（`index_map` 把搜索输入下标映射回完整候选空间）
    fn summary_of(
        out: &SearchOutput<Action, MatsuriTerminalStats>,
        index_map: impl Fn(usize) -> usize,
        describe: impl Fn(usize) -> String,
        chosen: usize,
    ) -> SearchSummary {
        let candidates: Vec<SearchCandidate> = out
            .action_results
            .iter()
            .enumerate()
            .map(|(position, result)| {
                let terminal = out
                    .terminal_results
                    .get(position)
                    .copied()
                    .unwrap_or_default();
                let index = index_map(position);
                SearchCandidate {
                    index,
                    description: describe(index),
                    score: result.weighted_mean(out.radical_factor),
                    stdev: result.stdev(),
                    n: result.count(),
                    five: [
                        terminal.speed.mean(),
                        terminal.stamina.mean(),
                        terminal.power.mean(),
                        terminal.guts.mean(),
                        terminal.wisdom.mean(),
                    ],
                    pt: terminal.pt.mean(),
                }
            })
            .collect();
        let baseline = weighted_baseline(&candidates);
        SearchSummary {
            candidates,
            chosen,
            baseline,
        }
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
        let out =
            self.search
                .search::<MatsuriTerminal, _>(game, actions, rng, MatsuriTerminal::of)?;
        let chosen = out.best_action_idx;
        let mean = out.action_results[chosen].weighted_mean(out.radical_factor);
        self.set_last(Self::summary_of(
            &out,
            |index| index,
            |index| actions[index].decision_label(),
            chosen,
        ));
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
        let out =
            self.search
                .search::<MatsuriTerminal, _>(game, &subset, rng, MatsuriTerminal::of)?;
        let picked = out.best_action_idx;
        let chosen = map[picked];
        let mean = out.action_results[picked].weighted_mean(out.radical_factor);
        self.set_last(Self::summary_of(
            &out,
            |position| map[position],
            |index| match &actions[index] {
                Action::AssignAdvice(assign) => {
                    assign.describe(&game.advice.slots, &game.advice.fused)
                }
                other => other.decision_label(),
            },
            chosen,
        ));
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
            .search_with_terminal(game, &tokens, rng, |g, token, seed| {
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
                // 事件在 Begin 内部结算：`next()` 会直接进入下一阶段，不补跑 Begin 尾部
                // 就会丢掉 turn 2 的教诲授予与首分配，rollout 到融合窗口即报「尚未获得
                // 教诲」→ 该候选全部 rollout 失败 → 整次搜索 bail。
                if sim.stage == Stage::Begin {
                    let mut strategy = sim.strategy.take().ok_or_else(|| {
                        anyhow::anyhow!("策略流缺失：rollout 分支应由 fork_for_rollout 派生")
                    })?;
                    let tail = sim.begin_tail(fallback, &mut sim_rng, &mut strategy);
                    sim.strategy = Some(strategy);
                    tail?;
                }
                while sim.next() {
                    sim.run_stage(fallback, &mut sim_rng)?;
                }
                sim.on_simulation_end(fallback, &mut sim_rng)?;
                let score = sim.search_score(pt)?;
                Ok(RolloutOutcome {
                    score,
                    terminal: MatsuriTerminal::of(&sim),
                })
            })?;
        let picked = out.best_action_idx;
        let mean = out.action_results[picked].weighted_mean(out.radical_factor);
        self.set_last(Self::summary_of(
            &out,
            |position| position,
            |index| format!("选项{}", index + 1),
            picked,
        ));
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
        // 本次若走 fallback（单候选 / 关闭作用面），在线档不应看到上一次搜索的旧摘要
        self.clear_last();
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
        self.clear_last();
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

    /// 最近一次搜索摘要：候选分 / 样本数 / 中选下标与返回值一致；fallback 时清空
    #[test]
    fn search_summary_matches_selected_candidate() {
        let mut cfg = MatsuriConfig::default();
        cfg.search.search_n = 1;
        let trainer = MatsuriSearchTrainer::new(cfg, SearchConfig::default().with_search_n(1))
            .expect("搜索训练员");
        let mut game = crate::testing::sample_game();
        game.stage = Stage::Train;
        game.plan = game.derive_plan();
        game.base.distribution = vec![Vec::new(); 5];

        let actions = vec![Action::Train(0), Action::Rest];
        let mut rng = StdRng::seed_from_u64(42);
        let chosen = trainer
            .select_action(&game, &actions, &mut rng)
            .expect("搜索决策");
        let summary = trainer.last_search().expect("搜索路径应留下摘要");
        assert_eq!(summary.chosen, chosen, "摘要中选下标应与返回值一致");
        assert_eq!(
            summary.candidates.len(),
            actions.len(),
            "Train 搜索候选应覆盖全部动作"
        );
        for (position, candidate) in summary.candidates.iter().enumerate() {
            assert_eq!(candidate.index, position, "Train 摘要保留下标顺序");
            assert!(candidate.score.is_finite(), "候选分应为有限值");
            assert!(candidate.n > 0, "搜索过就应有样本");
            assert!(candidate.stdev >= 0.0, "候选分标准差非负");
            assert!(candidate.five.iter().all(|value| value.is_finite()));
            assert!(candidate.pt.is_finite());
            assert!(summary.baseline.is_finite());
            assert_eq!(candidate.description, actions[position].decision_label());
        }

        // 单候选走 fallback → 必须清空上一次摘要（在线档不能贴旧分）
        let single = vec![Action::Race];
        trainer
            .select_action(&game, &single, &mut rng)
            .expect("单候选决策");
        assert!(trainer.last_search().is_none(), "fallback 不应留下摘要");
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
