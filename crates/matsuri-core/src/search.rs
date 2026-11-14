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

use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer, UmaFlagsExt};
use umasim::search::{
    FlatSearch, FlatSearchGame, OrderedRollouts, RolloutOutcome, SearchConfig, SearchOutput,
    SearchProbe,
};

use crate::config::{MatsuriConfig, SearchScopeConfig};
use crate::diag;
use crate::game::{Action, MatsuriGame};
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
        career_failed => { key: "career_failed", label: "育成失败", unit: "flag" },
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
            // 育成失败（自选赛未达标）是合法终局，不是 rollout 错误；单独记一维供在线告警
            career_failed: if game.career_failed { 1.0 } else { 0.0 },
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

/// **单局终局分的标准差**（候选样本方差按搜索次数加权的合并）
///
/// `sqrt(Σ nᵢσᵢ² / N)`：语义是「照着搜索选出的走法打一局，终局得分相对期望的典型波动」，
/// 不随搜索预算收敛（「期望分本身估得准不准」是另一回事，不在本字段里）。在线运气行的
/// 「期望 ±σ」用它——玩家关心的是终局落在哪个区间，而不是期望分本身估得有多准。
fn pooled_stdev(candidates: &[SearchCandidate]) -> f64 {
    let total_n: f64 = candidates.iter().map(|c| f64::from(c.n)).sum();
    if candidates.is_empty() || total_n <= 0.0 {
        return 0.0;
    }
    let variance: f64 = candidates
        .iter()
        .map(|c| f64::from(c.n) * c.stdev * c.stdev)
        .sum();
    (variance / total_n).sqrt()
}

/// 终局 PT 数（原始值，非分数）的样本数加权均值——官方口径换算用
fn weighted_pt(candidates: &[SearchCandidate]) -> f64 {
    let total_n: f64 = candidates.iter().map(|c| f64::from(c.n)).sum();
    if candidates.is_empty() || total_n <= 0.0 {
        return 0.0;
    }
    candidates
        .iter()
        .map(|c| c.pt * f64::from(c.n))
        .sum::<f64>()
        / total_n
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
    /// 单局终局分标准差（候选样本方差按样本数加权的合并；0 = 无有效样本）
    pub baseline_stdev: f64,
    /// 期望分（**官方口径**：搜索分扣掉 PT 项放大差额，与终局得分同量纲）
    pub expected_score: f64,
    /// 期望分对应的评价档次名（`constants.rank_names`；数据未初始化时为空串）
    pub expected_rank: String,
}

/// 一次搜索的 rollout 健康度（在线告警 / eval 样本可信度）
///
/// 两条"失败"分开记：`failed` 是 rollout 真报错（引擎 `CandidateAccum.failed`），
/// `career_failed_rate` 是育成失败（自选赛未达标）比例——后者不是错误，但会让该候选
/// 的期望分系统性偏低，必须能被调用方看见。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RolloutHealth {
    /// 计划 rollout 总数
    pub planned: usize,
    /// 成功 rollout 总数
    pub succeeded: usize,
    /// 失败（`Err`）rollout 总数
    pub failed: usize,
    /// 每候选育成失败率（下标与 `SearchSummary.candidates` 对齐）
    pub career_failed_rate: Vec<f64>,
}

impl RolloutHealth {
    /// 是否完全干净（无 `Err`、无育成失败样本）
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.max_career_failed_rate() <= f64::EPSILON
    }

    /// 候选里最高的育成失败率
    #[must_use]
    pub fn max_career_failed_rate(&self) -> f64 {
        self.career_failed_rate
            .iter()
            .copied()
            .fold(0.0f64, f64::max)
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
    /// 最近一次搜索摘要（在线档取候选分；`None` = 本次走了手写 fallback）
    last: Mutex<Option<SearchSummary>>,
    /// 引擎探针输出（每次搜索 push 一条；取最后一条算 rollout 健康度）
    probe: Arc<Mutex<Vec<SearchProbe>>>,
    /// 最近一次搜索的按序号原始分（配对差 / eval 的 CI 用）
    last_ordered: Mutex<Option<OrderedRollouts>>,
    /// 最近一次搜索的健康度
    last_health: Mutex<Option<RolloutHealth>>,
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
        let probe = Arc::new(Mutex::new(Vec::new()));
        let searcher = FlatSearch::<MatsuriGame>::new(search)
            .with_rollout_trainer(MatsuriTrainer::new(config))
            .with_parallel(true)
            .with_probe(Arc::clone(&probe));
        Ok(Self {
            search: searcher,
            fallback,
            scope,
            pt_favor_rate,
            last: Mutex::new(None),
            probe,
            last_ordered: Mutex::new(None),
            last_health: Mutex::new(None),
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

    /// 最近一次搜索的 rollout 健康度（`None` = 走了手写 fallback 或尚未搜索）
    #[must_use]
    pub fn last_health(&self) -> Option<RolloutHealth> {
        self.last_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 最近一次搜索的按序号原始分（配对差用；需要 `record_ordered_rollouts`）
    #[must_use]
    pub fn last_ordered(&self) -> Option<OrderedRollouts> {
        self.last_ordered
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
        *self
            .last_ordered
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .last_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    /// 搜索结束后收集有序样本与健康度（探针取最后一条）
    fn record_rollout(&self, out: &SearchOutput<Action, MatsuriTerminalStats>) {
        *self
            .last_ordered
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = out.ordered_rollouts.clone();
        let probe = self.probe.lock().ok().and_then(|mut list| list.pop());
        let career_failed_rate = out
            .terminal_results
            .iter()
            .map(|stats| stats.career_failed.mean())
            .collect();
        let health = match probe {
            Some(probe) => RolloutHealth {
                planned: probe.total_planned(),
                succeeded: probe.total_succeeded(),
                failed: probe.total_failed(),
                career_failed_rate,
            },
            None => RolloutHealth {
                career_failed_rate,
                ..RolloutHealth::default()
            },
        };
        *self
            .last_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(health);
    }

    /// 搜索结果 → 摘要（`index_map` 把搜索输入下标映射回完整候选空间）
    ///
    /// `baseline` 留在搜索口径（PT 项被 `pt_favor_rate` 放大），因为运气分 / 候选分差都在这个空间里；
    /// 另给 `expected_score` = 官方口径（`official = search − Σpt×(favor−1)`），供呈现与档次判定。
    fn summary_of(
        &self,
        game: &MatsuriGame,
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
        let baseline_stdev = pooled_stdev(&candidates);
        // 官方口径期望分：搜索分把 PT 项乘了 `pt_favor_rate`（`parts.pt` 已含 ×2.0 与切者 / 小切因子），
        // 扣掉放大差额即回到与终局得分同量纲的分数
        let pt_mean = weighted_pt(&candidates);
        let data = Data::get().ok();
        let pt_rate = data
            .as_ref()
            .map_or(2.0, |data| f64::from(data.consts.pt_score_rate));
        let pt_factor = f64::from(game.uma().flags.pt_score_rate_factor());
        let expected_score = baseline - pt_mean * pt_rate * pt_factor * (self.pt_favor_rate - 1.0);
        // 期望分 → 评价档次（呈现用；`rank_names` 空表时 `get_rank_name` 会给兜底名）
        let expected_rank = data
            .map(|data| {
                data.consts
                    .get_rank_name(expected_score.round() as i32)
                    .to_string()
            })
            .unwrap_or_default();
        SearchSummary {
            candidates,
            chosen,
            baseline,
            baseline_stdev,
            expected_score,
            expected_rank,
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
        // Train 全候选（ramen 口径）；唯一例外是自选比赛最后期限：其余动作必然育成失败，
        // 只搜比赛——必败候选不占预算、不进期望基线（否则该回合期望 / 运气被拉低、下回合弹回）
        let keep: Vec<usize> = match game.forced_race(actions) {
            Some(index) => vec![index],
            None => (0..actions.len()).collect(),
        };
        let subset: Vec<Action> = keep.iter().map(|&index| actions[index].clone()).collect();
        let started = Instant::now();
        // 与 ramen 一致：搜索从调用方决策流取种子；局面规则流由 `fork_for_rollout`
        // 独立派生，搜索不读 / 不改真实局面的规则流。
        let out =
            self.search
                .search::<MatsuriTerminal, _>(game, &subset, rng, MatsuriTerminal::of)?;
        let picked = out.best_action_idx;
        let chosen = keep[picked];
        let mean = out.action_results[picked].weighted_mean(out.radical_factor);
        self.set_last(self.summary_of(
            game,
            &out,
            |position| keep[position],
            |index| actions[index].decision_label(),
            chosen,
        ));
        self.record_rollout(&out);
        diag!(
            "【搜索】T{} Train 候选 {} × {} → #{} {} 均值={:.0} 耗时={}ms",
            game.turn(),
            subset.len(),
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
        self.set_last(self.summary_of(
            game,
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
        self.record_rollout(&out);
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
                // 事件可能在 Begin / End 内部结算：由剧本按根动作补完当前阶段
                // （Begin 补 begin_tail，End 补教诲窗口与尾部），再推进到下一阶段。
                sim.finish_root_stage(token, fallback, &mut sim_rng)?;
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
        self.set_last(self.summary_of(
            game,
            &out,
            |position| position,
            |index| format!("选项{}", index + 1),
            picked,
        ));
        self.record_rollout(&out);
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
        match self.search_event(game, event, choices, rng) {
            Ok(index) => Ok(index),
            Err(err) => {
                // 在线单帧决策不能因一次搜索失败而中断：回落手写并留下诊断
                diag!(
                    "【搜索】T{} 事件#{} 搜索失败，回退手写：{err:#}",
                    game.turn(),
                    event.id
                );
                self.clear_last();
                self.fallback.select_event_choice(game, event, choices, rng)
            }
        }
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
    use crate::game::Stage;

    /// 期望分与单局终局 σ：候选按样本数加权合并
    #[test]
    fn baseline_and_stdev_weight_by_samples() {
        let candidate = |score: f64, stdev: f64, n: u32| SearchCandidate {
            index: 0,
            description: String::new(),
            score,
            stdev,
            n,
            five: [0.0; 5],
            pt: score,
        };
        let candidates = [candidate(100.0, 10.0, 3), candidate(200.0, 20.0, 1)];
        assert!(
            (weighted_baseline(&candidates) - 125.0).abs() < 1e-9,
            "加权期望"
        );
        // σ² = (3×100 + 1×400) / 4 = 175
        assert!(
            (pooled_stdev(&candidates) - 175f64.sqrt()).abs() < 1e-9,
            "合并 σ"
        );
        // 无有效样本 → 0（在线运气行按 0 视为「不显示 σ」）
        assert_eq!(pooled_stdev(&[]), 0.0);
        assert_eq!(pooled_stdev(&[candidate(1.0, 5.0, 0)]), 0.0);
        assert_eq!(weighted_baseline(&[]), 0.0);
        // 官方口径换算用：PT 数按样本数加权（(100×3 + 200×1) / 4 = 125）
        assert!((weighted_pt(&candidates) - 125.0).abs() < 1e-9, "加权 PT");
        assert_eq!(weighted_pt(&[]), 0.0);
    }

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

    /// 自选比赛最后期限：只搜比赛（必败候选不进期望基线）；中选与决策流消费不变
    #[test]
    fn deadline_turn_searches_race_only() {
        let mut cfg = MatsuriConfig::default();
        cfg.search.search_n = 1;
        let trainer = MatsuriSearchTrainer::new(cfg, SearchConfig::default().with_search_n(1))
            .expect("搜索训练员");
        let mut game = crate::testing::sample_game_with_uma(102102);
        game.turn = (29..=48)
            .rev()
            .find(|turn| game.base.uma.free_races[1].mask.contains_turn(*turn))
            .expect("第二区间应有可赛回合");
        game.stage = Stage::Train;
        game.plan = game.derive_plan();
        game.base.distribution = vec![Vec::new(); 5];
        let actions = game.list_actions().expect("候选动作");
        let race = game
            .forced_race(&actions)
            .expect("期限回合只剩比赛可避免育成失败");
        assert!(actions.len() > 1, "期限回合仍列出其它动作：{actions:?}");

        let mut rng = StdRng::seed_from_u64(123);
        let mut probe = StdRng::seed_from_u64(123);
        let chosen = trainer
            .select_action(&game, &actions, &mut rng)
            .expect("搜索决策");
        assert_eq!(chosen, race, "中选仍是比赛");
        let summary = trainer.last_search().expect("仍走搜索、留下摘要");
        assert_eq!(summary.candidates.len(), 1, "只搜比赛");
        assert_eq!(summary.candidates[0].index, race);
        assert_eq!(summary.chosen, race);
        assert!(
            (summary.baseline - summary.candidates[0].score).abs() < 1e-9,
            "期望基线 = 比赛期望分"
        );
        probe.next_u64();
        assert_eq!(rng.next_u64(), probe.next_u64(), "决策流仍只消费一个随机字");
    }
}
