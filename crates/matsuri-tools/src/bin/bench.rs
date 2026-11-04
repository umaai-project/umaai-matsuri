//! 性能测量入口：手写档整局 / 决策链分段 / 搜索固定根
//!
//! 用法（仓根执行，先 cargo build --release --offline -p matsuri-tools）：
//! - bench game --games 20 --seed 61444            整局耗时 + 终局 CSV（跨版本逐位对照）
//! - bench micro --turn 32 --iters 2000            决策链分段 ns/op
//! - bench root --turn 32 --search-n 512 --repeat 3 固定根搜索耗时 + 探针计数
//!
//! 测量纪律：双方同一 Release 构建、同一数据 / 配置；耗时只声明本机当轮。
//! 日志不装 logger，避免把诊断格式化的成本算进测量。

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use matsuri_core::game::Action;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, build_game,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, FlatSearchGame, NoTerminal};

/// 仓内数据根（相对仓根）
const DATA_ROOT: &str = "data";

/// 一局终局观测
struct Outcome {
    score: i32,
    pt: i32,
    turn: i32,
}

/// 装载数据与配置（每个测量进程一次）
struct Env {
    root: PathBuf,
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
            root,
            engine,
            data,
            config,
        })
    }

    fn build(&self, config: &MatsuriConfig) -> Result<MatsuriGame> {
        Ok(build_game(&self.engine, self.data.clone())?.with_config(config.clone()))
    }
}

/// 按 Runner 同序推进一局（规则主种子 = seed，决策流 = seed）
fn play(
    env: &Env,
    config: &MatsuriConfig,
    seed: u64,
    trainer: &impl Trainer<MatsuriGame>,
) -> Result<Outcome> {
    let mut game = env.build(config)?;
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(trainer, &mut rng)?;
    while game.next() {
        game.run_stage(trainer, &mut rng)?;
    }
    game.on_simulation_end(trainer, &mut rng)?;
    Ok(Outcome {
        score: game.uma().calc_score()?,
        pt: game.uma().total_pt()?,
        turn: game.turn(),
    })
}

/// 捕获固定决策点（目标回合、至少含一个训练候选）的训练员包装
struct CaptureTrainer {
    inner: MatsuriTrainer,
    target: i32,
    slot: Mutex<Option<(MatsuriGame, Vec<Action>)>>,
}

impl CaptureTrainer {
    fn new(inner: MatsuriTrainer, target: i32) -> Self {
        Self {
            inner,
            target,
            slot: Mutex::new(None),
        }
    }

    fn take(&self) -> Option<(MatsuriGame, Vec<Action>)> {
        self.slot.lock().ok().and_then(|mut slot| slot.take())
    }
}

impl Trainer<MatsuriGame> for CaptureTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        if game.turn() == self.target
            && actions
                .iter()
                .any(|action| matches!(action, Action::Train(_)))
            && let Ok(mut slot) = self.slot.lock()
            && slot.is_none()
        {
            *slot = Some((game.clone(), actions.to_vec()));
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

/// 空决策训练员：永远选 0，用来把「引擎机制成本」与「训练员决策成本」拆开
struct NoOpTrainer;

impl Trainer<MatsuriGame> for NoOpTrainer {
    fn select_action(
        &self,
        _game: &MatsuriGame,
        _actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Ok(0)
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        _choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Ok(0)
    }

    fn select_event_choice(
        &self,
        _game: &MatsuriGame,
        _event: &EventData,
        _choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Ok(0)
    }
}

/// 单次 rollout（与 FlatSearch::simulate_common 同序，可注入任意训练员）
fn rollout_with<T: Trainer<MatsuriGame>>(
    game: &MatsuriGame,
    action: &Action,
    seed: u64,
    trainer: &T,
    pt_favor_rate: f64,
) -> Result<f64> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut sim = game.fork_for_rollout(seed);
    sim.apply_root_action(action, &mut rng)?;
    while sim.next() {
        sim.run_stage(trainer, &mut rng)?;
    }
    sim.on_simulation_end(trainer, &mut rng)?;
    sim.search_score(pt_favor_rate)
}

/// 记录每次动作决策耗时的包装训练员（诊断 rollout 内成本分布用）
struct TimingTrainer {
    inner: MatsuriTrainer,
    log: Mutex<Vec<(i32, usize, u64, bool)>>,
}

impl TimingTrainer {
    fn new(inner: MatsuriTrainer) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
        }
    }

    fn rows(&self) -> Vec<(i32, usize, u64, bool)> {
        self.log.lock().map(|v| v.clone()).unwrap_or_default()
    }
}

impl Trainer<MatsuriGame> for TimingTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let is_advice = actions
            .iter()
            .all(|action| matches!(action, Action::AssignAdvice(_)));
        let started = Instant::now();
        let chosen = self.inner.select_action(game, actions, rng)?;
        if let Ok(mut log) = self.log.lock() {
            log.push((
                game.turn(),
                actions.len(),
                started.elapsed().as_nanos() as u64,
                is_advice,
            ));
        }
        Ok(chosen)
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
        let started = Instant::now();
        let chosen = self.inner.select_event_choice(game, event, choices, rng)?;
        if let Ok(mut log) = self.log.lock() {
            log.push((
                game.turn(),
                choices.len(),
                started.elapsed().as_nanos() as u64,
                false,
            ));
        }
        Ok(chosen)
    }
}

/// 手写推进到目标回合，返回该决策点的局面与候选
fn capture_root(
    env: &Env,
    config: &MatsuriConfig,
    seed: u64,
    turn: i32,
) -> Result<(MatsuriGame, Vec<Action>)> {
    let trainer = CaptureTrainer::new(MatsuriTrainer::new(config.clone()), turn);
    let mut game = env.build(config)?;
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(&trainer, &mut rng)?;
    while game.next() {
        if let Some(found) = trainer.take() {
            return Ok(found);
        }
        game.run_stage(&trainer, &mut rng)?;
    }
    bail!("手写推进未在 T{turn} 捕获到含训练候选的决策点")
}

/// 每轮迭代的耗时统计（纳秒 / 次）
fn bench<F: FnMut()>(iters: usize, mut f: F) -> f64 {
    let started = Instant::now();
    for _ in 0..iters {
        f();
    }
    started.elapsed().as_nanos() as f64 / iters as f64
}

fn parse_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn parse_usize(args: &[String], name: &str, default: usize) -> Result<usize> {
    match parse_arg(args, name) {
        Some(raw) => raw.parse().with_context(|| format!("{name} 非法: {raw}")),
        None => Ok(default),
    }
}

fn parse_i32(args: &[String], name: &str, default: i32) -> Result<i32> {
    match parse_arg(args, name) {
        Some(raw) => raw.parse().with_context(|| format!("{name} 非法: {raw}")),
        None => Ok(default),
    }
}

fn main() -> Result<()> {
    // 诊断开关关闭：diag! 连 format_args 都不执行
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("game");
    let env = Env::load()?;
    match command {
        "game" => cmd_game(&env, &args),
        "micro" => cmd_micro(&env, &args),
        "root" => cmd_root(&env, &args),
        "trace" => cmd_trace(&env, &args),
        other => bail!("未知子命令: {other}（可选 game / micro / root）"),
    }
}

/// 整局：bench game --games 20 --seed 61444 [--search-n 0] [--warmup 1]
fn cmd_game(env: &Env, args: &[String]) -> Result<()> {
    let games = parse_usize(args, "--games", 10)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let warmup = parse_usize(args, "--warmup", 1)?;
    let search_n = parse_usize(args, "--search-n", env.config.search.search_n)?;

    let mut config = env.config.clone();
    config.search.search_n = search_n;
    if search_n > 0 {
        let trainer = MatsuriSearchTrainer::new(
            config.clone(),
            env.engine.search.clone().with_search_n(search_n),
        )?;
        run_batch(env, &config, &trainer, games, seed, warmup, search_n)
    } else {
        let trainer = MatsuriTrainer::new(config.clone());
        run_batch(env, &config, &trainer, games, seed, warmup, search_n)
    }
}

/// 批量跑局并打印 CSV + 汇总
fn run_batch<T: Trainer<MatsuriGame>>(
    env: &Env,
    config: &MatsuriConfig,
    trainer: &T,
    games: usize,
    seed: u64,
    warmup: usize,
    search_n: usize,
) -> Result<()> {
    for index in 0..warmup {
        let _ = play(env, config, seed + index as u64, trainer)?;
    }
    let mut total = std::time::Duration::ZERO;
    let mut scores = Vec::with_capacity(games);
    let mut pts = Vec::with_capacity(games);
    let mut rows = Vec::with_capacity(games);
    for index in 0..games {
        let run_seed = seed + index as u64;
        let started = Instant::now();
        let outcome = play(env, config, run_seed, trainer)?;
        let elapsed = started.elapsed();
        total += elapsed;
        scores.push(outcome.score);
        pts.push(outcome.pt);
        rows.push(format!(
            "{run_seed},{},{},{},{}",
            outcome.score,
            outcome.pt,
            outcome.turn,
            elapsed.as_micros()
        ));
    }
    let mean_ms = total.as_secs_f64() * 1000.0 / games as f64;
    println!(
        "# search_n={search_n} games={games} seed={seed} root={}",
        env.root.display()
    );
    println!("seed,score,pt,turn,elapsed_us");
    for row in &rows {
        println!("{row}");
    }
    println!(
        "# mean_ms={mean_ms:.4} total_s={:.4} score_mean={:.1} pt_mean={:.1}",
        total.as_secs_f64(),
        scores.iter().sum::<i32>() as f64 / games as f64,
        pts.iter().sum::<i32>() as f64 / games as f64
    );
    Ok(())
}

/// 决策链分段：bench micro --turn 32 --iters 2000
fn cmd_micro(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let iters = parse_usize(args, "--iters", 2000)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    let trainer = MatsuriTrainer::new(config.clone());
    let train_actions: Vec<usize> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Train(train) => Some(usize::from(*train)),
            _ => None,
        })
        .collect();

    println!(
        "# turn={turn} actions={} train_actions={train_actions:?} iters={iters}",
        actions.len()
    );
    let clone_ns = bench(iters, || {
        black_box(black_box(&game).clone());
    });
    println!("clone_whole_game       {clone_ns:10.1}");
    let mut no_log = game.clone();
    no_log.event_log.clear();
    no_log.event_queue.clear();
    let clone_nolog = bench(iters, || {
        black_box(black_box(&no_log).clone());
    });
    println!("clone_no_evlog         {clone_nolog:10.1}");
    let mut no_cfg = game.clone();
    no_cfg.config = MatsuriConfig::default();
    let clone_nocfg = bench(iters, || {
        black_box(black_box(&no_cfg).clone());
    });
    println!("clone_cfg_default      {clone_nocfg:10.1}");
    let mut slim = game.clone();
    slim.event_log.clear();
    slim.event_queue.clear();
    slim.config = MatsuriConfig::default();
    let clone_slim = bench(iters, || {
        black_box(black_box(&slim).clone());
    });
    println!("clone_slim             {clone_slim:10.1}");
    let preview_ns = bench(iters.max(200), || {
        for &train in &train_actions {
            black_box(black_box(&game).preview_training(train, false).ok());
        }
    });
    println!(
        "preview_train_x{}      {preview_ns:10.1}  per_call={:.1}",
        train_actions.len(),
        preview_ns / train_actions.len() as f64
    );
    let keiko_ns = bench(iters.max(200), || {
        for &train in &train_actions {
            black_box(black_box(&game).preview_training(train, true).ok());
        }
    });
    println!(
        "preview_keiko_x{}      {keiko_ns:10.1}  per_call={:.1}",
        train_actions.len(),
        keiko_ns / train_actions.len() as f64
    );
    let resource_ns = bench(iters.max(200), || {
        for action in &actions {
            black_box(black_box(&game).preview_action_resources(action));
        }
    });
    println!(
        "resource_preview_x{}   {resource_ns:10.1}  per_call={:.1}",
        actions.len(),
        resource_ns / actions.len() as f64
    );
    let mut rng = StdRng::seed_from_u64(seed);
    let select_ns = bench(iters.max(200), || {
        black_box(
            trainer
                .select_action(black_box(&game), black_box(&actions), &mut rng)
                .ok(),
        );
    });
    println!("select_action          {select_ns:10.1}");
    let explain_ns = bench(iters.max(200), || {
        black_box(
            trainer
                .explain_action(black_box(&game), black_box(&actions))
                .ok(),
        );
    });
    println!("explain_action         {explain_ns:10.1}  (choose + 全量打分)");
    let score_ns = bench(iters, || {
        black_box(game.search_score(1.7271).ok());
    });
    println!("search_score           {score_ns:10.1}");
    let list_ns = bench(iters.max(200), || {
        black_box(black_box(&game).list_actions().ok());
    });
    println!("list_actions           {list_ns:10.1}");
    Ok(())
}

/// rollout 内决策成本分布：bench trace --turn 32
fn cmd_trace(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    let trainer = TimingTrainer::new(MatsuriTrainer::new(config.clone()));
    let mut rng = StdRng::seed_from_u64(seed);
    let mut sim = game.fork_for_rollout(seed);
    sim.apply_root_action(&actions[0], &mut rng)?;
    while sim.next() {
        sim.run_stage(&trainer, &mut rng)?;
    }
    sim.on_simulation_end(&trainer, &mut rng)?;
    let rows = trainer.rows();
    let total: u64 = rows.iter().map(|row| row.2).sum();
    println!(
        "# turn={turn} decision_points={} total_decision_ms={:.3}",
        rows.len(),
        total as f64 / 1e6
    );
    println!("turn,actions,us,advice");
    for (turn, count, ns, advice) in &rows {
        println!("{turn},{count},{:.1},{advice}", *ns as f64 / 1e3);
    }
    Ok(())
}

/// 固定根搜索：bench root --turn 32 --search-n 512 --repeat 3
fn cmd_root(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let search_n = parse_usize(args, "--search-n", 512)?;
    let repeat = parse_usize(args, "--repeat", 3)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let search_seed = parse_usize(args, "--search-seed", 61444)? as u64;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    // 与 MatsuriSearchTrainer::new 同口径：内核参数取引擎配置，预算与 PT 偏好取剧本配置
    let mut search_config = env.engine.search.clone().with_search_n(search_n);
    search_config.pt_favor_rate = config.trainer.pt_favor_rate;
    println!(
        "# turn={turn} candidates={} search_n={search_n} ucb={} parallel={} threads={:?}",
        actions.len(),
        search_config.use_ucb,
        umasim::search::parallel_supported(),
        std::env::var("RAYON_NUM_THREADS").ok()
    );

    let mut best = Vec::new();
    let mut times = Vec::new();
    for round in 0..repeat {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let search = FlatSearch::<MatsuriGame>::new(search_config.clone())
            .with_rollout_trainer(MatsuriTrainer::new(config.clone()))
            .with_parallel(true)
            .with_probe(sink.clone());
        let mut rng = StdRng::seed_from_u64(search_seed);
        let started = Instant::now();
        let out = search.search::<NoTerminal, _>(&game, &actions, &mut rng, |_| NoTerminal)?;
        let elapsed = started.elapsed();
        let probes = sink.lock().map(|v| v.clone()).unwrap_or_default();
        let probe = probes.first().context("探针未记录")?;
        let total = probe.total_planned();
        let failed = probe.total_failed();
        println!(
            "round={round} elapsed_ms={:.3} best={} planned={total} failed={failed} mean={:.1} ns_per_rollout={:.0}",
            elapsed.as_secs_f64() * 1000.0,
            out.best_action_idx,
            out.action_results[out.best_action_idx].weighted_mean(out.radical_factor),
            elapsed.as_nanos() as f64 / total.max(1) as f64
        );
        for (i, result) in out.action_results.iter().enumerate() {
            println!(
                "  cand {i} n={} mean={:.1} wmean={:.1}",
                result.count(),
                result.mean(),
                result.weighted_mean(out.radical_factor)
            );
        }
        best.push(out.best_action_idx);
        times.push(elapsed.as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "# median_ms={:.3} best_indices={best:?}",
        times[times.len() / 2]
    );

    // 单次 rollout 拆分：根克隆 / 完整一次 rollout
    let rollout_seed = search_seed;
    let fork_ns = bench(200, || {
        black_box(black_box(&game).fork_for_rollout(rollout_seed));
    });
    println!("fork_for_rollout       {fork_ns:10.1}");
    let pt = config.trainer.pt_favor_rate;
    let search = FlatSearch::<MatsuriGame>::new(search_config)
        .with_rollout_trainer(MatsuriTrainer::new(config));
    let rollout_ns = bench(50, || {
        black_box(
            search
                .simulate_common(black_box(&game), black_box(&actions[0]), rollout_seed)
                .ok(),
        );
    });
    println!("simulate_common(seq)   {rollout_ns:10.1}  (单次 rollout，含 fork)");
    let noop_ns = bench(50, || {
        black_box(
            rollout_with(
                black_box(&game),
                black_box(&actions[0]),
                rollout_seed,
                &NoOpTrainer,
                pt,
            )
            .ok(),
        );
    });
    println!("rollout_noop(seq)      {noop_ns:10.1}  (机制成本；差值 = 训练员决策)");
    Ok(())
}
