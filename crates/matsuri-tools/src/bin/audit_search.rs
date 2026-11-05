//! 一次性测量工具（不入库）：固定根下的搜索分辨率 / 预算分配 / 目标口径
//!
//! 用法（仓根执行）：
//! `RAYON_NUM_THREADS=24 ./target/release/audit_search --turns 10,32,50,65 --seeds 20`
//!
//! 输出 CSV 风格的行，由外层脚本汇总。口径：
//! - `ref,turn,obj,act,i,mean,se,sd`：均匀分配 + 大预算、多根种子平均出的候选真值（两种目标口径）；
//! - `run,turn,N,s,chosen,best,regret_off,regret_prod,gap,indep_se,na,nb,total,cand`：一次生产搜索；
//! - `cand,turn,N,s,i,n,mean,sd,se`：逐候选样本数 / 均分 / 标准差 / 均分标准误；
//! - `pair,turn,N,s,a,b,m,md,paired_se,indep_se`：前两名的配对差（公共序号前缀）；
//! - `alloc,turn,N,s,hist`：UCB 预算分配直方图；
//! - `cobj,turn,s,tag,chosen,official_of_chosen,official_best`：目标口径对照（官方 1.0 vs 生产）。

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use matsuri_core::game::Action;
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriGame, MatsuriTrainer, build_game};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::Arc;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, NoTerminal, SearchConfig};

const DATA_ROOT: &str = "data";

struct Env {
    engine: umaai::Config,
    data: Arc<MatsuriData>,
    config: MatsuriConfig,
}

impl Env {
    fn load() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("crate 目录上溯两级即仓根")?
            .to_path_buf();
        let data_root = root.join(DATA_ROOT);
        Data::init(&data_root).context("引擎静态数据初始化失败")?;
        let engine =
            umaai::Config::load_with_overlay_file(&data_root, ".").context("载入引擎配置失败")?;
        let data = Arc::new(MatsuriData::load(&data_root).context("载入剧本数据失败")?);
        let config =
            MatsuriConfig::load_with_overlay_file(&data_root, ".").context("载入剧本配置失败")?;
        Ok(Self {
            engine,
            data,
            config,
        })
    }

    fn build(&self, config: &MatsuriConfig) -> Result<MatsuriGame> {
        Ok(build_game(&self.engine, self.data.clone())?.with_config(config.clone()))
    }
}

type ActionList = Vec<Action>;

/// 捕获若干目标回合的决策点
struct CaptureTrainer {
    inner: MatsuriTrainer,
    targets: Vec<i32>,
    slots: Mutex<Vec<(i32, MatsuriGame, ActionList)>>,
}

impl CaptureTrainer {
    fn new(inner: MatsuriTrainer, targets: Vec<i32>) -> Self {
        Self {
            inner,
            targets,
            slots: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<(i32, MatsuriGame, ActionList)> {
        match self.slots.lock() {
            Ok(mut slots) => std::mem::take(&mut *slots),
            Err(_) => Vec::new(),
        }
    }
}

impl Trainer<MatsuriGame> for CaptureTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let turn = game.turn();
        if self.targets.contains(&turn)
            && actions.iter().any(|a| matches!(a, Action::Train(_)))
            && let Ok(mut slots) = self.slots.lock()
            && !slots.iter().any(|(t, _, _)| *t == turn)
        {
            slots.push((turn, game.clone(), actions.to_vec()));
        }
        self.inner.select_action(game, actions, rng)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

fn capture_roots(
    env: &Env,
    config: &MatsuriConfig,
    seed: u64,
    targets: Vec<i32>,
) -> Result<Vec<(i32, MatsuriGame, ActionList)>> {
    let trainer = CaptureTrainer::new(MatsuriTrainer::new(config.clone()), targets.clone());
    let mut game = env.build(config)?;
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(&trainer, &mut rng)?;
    while game.next() {
        game.run_stage(&trainer, &mut rng)?;
    }
    let got = trainer.take();
    if got.is_empty() {
        bail!("未捕获任何目标决策点");
    }
    Ok(got)
}

#[allow(clippy::too_many_arguments)]
fn search_once(
    game: &MatsuriGame,
    actions: &[Action],
    config: &MatsuriConfig,
    engine: &SearchConfig,
    n: usize,
    ucb: bool,
    record: bool,
    pt: f64,
    search_seed: u64,
) -> Result<umasim::search::SearchOutput<Action>> {
    let mut cfg = engine.clone().with_search_n(n);
    cfg.use_ucb = ucb;
    cfg.record_ordered_rollouts = record;
    cfg.pt_favor_rate = pt;
    let mut rng = StdRng::seed_from_u64(search_seed);
    let search = FlatSearch::<MatsuriGame>::new(cfg)
        .with_rollout_trainer(MatsuriTrainer::new(config.clone()))
        .with_parallel(true);
    search.search::<NoTerminal, _>(game, actions, &mut rng, |_| NoTerminal)
}

fn parse_usize(args: &[String], name: &str, default: usize) -> Result<usize> {
    match args.iter().position(|a| a == name) {
        Some(i) => args
            .get(i + 1)
            .with_context(|| format!("{name} 缺少值"))?
            .parse()
            .with_context(|| format!("{name} 非法")),
        None => Ok(default),
    }
}

fn parse_list(args: &[String], name: &str, default: &str) -> Vec<usize> {
    let raw = args
        .iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.to_string());
    raw.split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect()
}

fn main() -> Result<()> {
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let seed = parse_usize(&args, "--seed", 61444)? as u64;
    let seeds = parse_usize(&args, "--seeds", 20)?;
    let ref_n = parse_usize(&args, "--ref-n", 6144)?;
    let ref_roots = parse_usize(&args, "--ref-roots", 4)?;
    let ns = parse_list(&args, "--ns", "512,1024,2048,4096,8192,12288");
    let turns: Vec<i32> = parse_list(&args, "--turns", "32")
        .into_iter()
        .map(|t| t as i32)
        .collect();
    let env = Env::load()?;
    let config = env.config.clone();
    let engine = env.engine.search.clone();
    let prod_pt = config.trainer.pt_favor_rate;

    println!(
        "# meta seed={seed} nt={seeds} ref_n={ref_n} ref_roots={ref_roots} prod_pt={prod_pt} ucb={} group={} cpuct={} exp_sd={}",
        engine.use_ucb, engine.search_group_size, engine.search_cpuct, engine.expected_search_stdev
    );

    let roots = capture_roots(&env, &config, seed, turns.clone())?;
    for (turn, game, actions) in roots {
        let cand = actions.len().min(32);
        println!("# root turn={turn} candidates={cand}");
        for (i, a) in actions.iter().enumerate().take(cand) {
            println!("act,{turn},{i},\"{a}\"");
        }
        // 两种口径的真值：均匀分配 + 多根种子
        let mut truth = [[0.0f64; 2]; 32];
        let mut truth_sd = [[0.0f64; 2]; 32];
        for (oi, pt) in [1.0f64, prod_pt].into_iter().enumerate() {
            for r in 0..ref_roots {
                let out = search_once(
                    &game,
                    &actions,
                    &config,
                    &engine,
                    ref_n,
                    false,
                    false,
                    pt,
                    900_000 + r as u64,
                )?;
                for i in 0..cand {
                    truth[i][oi] += out.action_results[i].mean() / ref_roots as f64;
                    truth_sd[i][oi] += out.action_results[i].stdev().powi(2) / ref_roots as f64;
                }
            }
            for i in 0..cand {
                let mean = truth[i][oi];
                let sd = truth_sd[i][oi].sqrt();
                println!(
                    "ref,{turn},{},\"{}\",{i},{mean:.4},{:.4},{sd:.4}",
                    if oi == 0 { "off" } else { "prod" },
                    actions[i],
                    sd / ((ref_n * ref_roots) as f64).sqrt()
                );
            }
        }
        let best_off = (0..cand)
            .max_by(|&a, &b| truth[a][0].total_cmp(&truth[b][0]))
            .unwrap_or(0);
        let mut order_off: Vec<usize> = (0..cand).collect();
        order_off.sort_by(|&a, &b| truth[b][0].total_cmp(&truth[a][0]));
        println!(
            "refbest,{turn},{best_off},{},{:.4},{:.4}",
            order_off[1],
            truth[order_off[0]][0] - truth[order_off[1]][0],
            truth[order_off[0]][0] - truth[order_off[2]][0]
        );

        for &n in &ns {
            for s in 0..seeds {
                let record = n * cand <= 120_000;
                let out = search_once(
                    &game,
                    &actions[..cand],
                    &config,
                    &engine,
                    n,
                    true,
                    record,
                    prod_pt,
                    s as u64,
                )?;
                let chosen = out.best_action_idx;
                let mut counts = vec![0u32; cand];
                let mut means = vec![0.0f64; cand];
                let mut sds = vec![0.0f64; cand];
                for (i, res) in out.action_results.iter().enumerate().take(cand) {
                    counts[i] = res.count();
                    means[i] = res.mean();
                    sds[i] = res.stdev();
                }
                let mut order: Vec<usize> = (0..cand).collect();
                order.sort_by(|&a, &b| means[b].total_cmp(&means[a]));
                let (a, b) = (order[0], order[1]);
                let se_a = sds[a] / (counts[a].max(1) as f64).sqrt();
                let se_b = sds[b] / (counts[b].max(1) as f64).sqrt();
                let indep = (se_a * se_a + se_b * se_b).sqrt();
                let regret_off = truth[best_off][0] - truth[chosen][0];
                let regret_prod = truth[best_off][1] - truth[chosen][1];
                println!(
                    "run,{turn},{n},{s},{chosen},{best_off},{regret_off:.4},{regret_prod:.4},{:.4},{:.4},{},{},{},{}",
                    means[a] - means[b],
                    indep,
                    counts[a],
                    counts[b],
                    counts.iter().sum::<u32>(),
                    cand
                );
                for i in 0..cand {
                    println!(
                        "cand,{turn},{n},{s},{i},{},{:.4},{:.4},{:.4}",
                        counts[i],
                        means[i],
                        sds[i],
                        sds[i] / (counts[i].max(1) as f64).sqrt()
                    );
                }
                // 分配直方图
                let mut hist: Vec<(u32, usize)> = Vec::new();
                for &c in &counts {
                    match hist.iter_mut().find(|(v, _)| *v == c) {
                        Some((_, k)) => *k += 1,
                        None => hist.push((c, 1)),
                    }
                }
                hist.sort_unstable();
                let body = hist
                    .iter()
                    .map(|(v, k)| format!("{v}x{k}"))
                    .collect::<Vec<_>>()
                    .join("|");
                println!("alloc,{turn},{n},{s},{body}");
                if let Some(ordered) = out.ordered_rollouts.as_ref() {
                    let m = (counts[a].min(counts[b]) as usize)
                        .min(ordered.per_candidate[a].len())
                        .min(ordered.per_candidate[b].len());
                    let mut diffs = Vec::with_capacity(m);
                    for k in 0..m {
                        if let (Some(x), Some(y)) = (
                            ordered.per_candidate[a].get(k).copied().flatten(),
                            ordered.per_candidate[b].get(k).copied().flatten(),
                        ) {
                            diffs.push(x - y);
                        }
                    }
                    if diffs.len() > 1 {
                        let md = diffs.iter().sum::<f64>() / diffs.len() as f64;
                        let var = diffs.iter().map(|d| (d - md) * (d - md)).sum::<f64>()
                            / (diffs.len() as f64 - 1.0);
                        let pse = (var / diffs.len() as f64).sqrt();
                        println!(
                            "pair,{turn},{n},{s},{a},{b},{},{md:.4},{pse:.4},{indep:.4}",
                            diffs.len()
                        );
                    }
                }
            }
        }

        // 等预算对照：均匀分配（每候选 n=总预算/候选数）vs UCB，回答「预算不够还是分配错了」
        for &n in &ns {
            // UCB 的计划总量 ≈ n + (cand-1) × 首组大小；均匀分配用同一总量除以候选数
            let total = n + (cand - 1) * engine.search_group_size;
            let un = (total / cand).max(1);
            for s in 0..seeds.min(12) {
                let out = search_once(
                    &game,
                    &actions[..cand],
                    &config,
                    &engine,
                    un,
                    false,
                    false,
                    prod_pt,
                    s as u64,
                )?;
                let chosen = out.best_action_idx;
                let mut means = vec![0.0f64; cand];
                let mut sd0 = 0.0f64;
                for (i, res) in out.action_results.iter().enumerate().take(cand) {
                    means[i] = res.mean();
                    sd0 = res.stdev();
                }
                let regret_off = truth[best_off][0] - truth[chosen][0];
                println!("unif,{turn},{n},{s},{un},{chosen},{best_off},{regret_off:.4},{sd0:.4}");
            }
        }

        // 目标口径对照：均匀分配，同根种子，官方 vs 生产
        for s in 0..seeds.min(10) {
            for (tag, pt) in [("off", 1.0f64), ("prod", prod_pt)] {
                let out = search_once(
                    &game,
                    &actions[..cand],
                    &config,
                    &engine,
                    4096,
                    false,
                    false,
                    pt,
                    s as u64,
                )?;
                let chosen = out.best_action_idx;
                println!(
                    "cobj,{turn},{s},{tag},{chosen},{:.4},{:.4}",
                    truth[chosen][0], truth[best_off][0]
                );
            }
        }
    }
    Ok(())
}
