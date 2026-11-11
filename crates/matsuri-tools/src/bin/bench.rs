//! 性能测量入口：手写档整局 / 决策链分段 / 搜索固定根 / rollout 阶段拆分
//!
//! 用法（仓根执行，先 cargo build --release --offline -p matsuri-tools）：
//! - bench game --games 20 --seed 61444 --search-n 0  整局耗时 + 终局 CSV（手写档；缺省走配置 search_n=8192）
//! - bench luck --games 20 --seed 73200 --search-n 2048  MCTS 自博弈逐决策 expected_score，
//!   输出每局首末回合估分差与 `luck_turn_bonus` 补偿斜率
//! - bench micro --turn 32 --iters 2000            决策链分段 ns/op
//! - bench root --turn 32 --search-n 512 --repeat 3 固定根搜索耗时 + 探针计数
//! - bench stages --turn 32                        单次 rollout 按阶段机拆分
//! - bench parts --turn 32 --iters 2000            Distribute / 事件管线单点 ns/op
//! - bench flat --turn 32 --rollouts 2048 --threads 24  平铺线程池 rollout（并行上限对照）
//! - bench prof <子命令> [原参数] --freq 997 --out SVG  pprof 火焰图 + 自耗时 TopN（需 `--features profile`）
//! - bench ... --matsuri-config <完整配置>  测卡 / 扫参专用：显式载入完整
//!   `matsuri_config.toml`（含 `[decks.*]` 变体），不读仓内默认层；玩家 CLI 不暴露该入口
//!
//! 测量纪律：双方同一 Release 构建、同一数据 / 配置；耗时只声明本机当轮。
//! 日志不装 logger，避免把诊断格式化的成本算进测量。

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use matsuri_core::game::Action;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, Qie, build_game,
};
use matsuri_sim::baseline::judge;
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearch, FlatSearchGame, NoTerminal};

/// 仓内数据根（相对仓根）
const DATA_ROOT: &str = "data";

/// 分配计数开关（只在 `bench allocs` 打开；关闭时每次分配多一次 Relaxed 读）
static COUNT_ALLOCS: AtomicBool = AtomicBool::new(false);
/// 累计分配次数
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
/// 累计分配字节
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// 计数分配器：只包一层原子计数，基准测量时关闭
struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCS.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: 转发给系统分配器，layout 由调用方保证有效
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: 指针与 layout 来自匹配的 System.alloc
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNT_ALLOCS.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        // SAFETY: 转发给系统分配器，参数由调用方保证有效
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: CountingAlloc = CountingAlloc;

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
    /// 载入数据与配置。
    ///
    /// `--matsuri-config` 是**测卡 / 扫参专用**入口：给了就读显式完整配置，
    /// 不走仓内默认层。玩家终端 / online 不暴露这个入口，`[decks.*]` 的玩家覆盖
    /// 限制仍由 `MatsuriConfigOverlay` 保持。
    fn load(args: &[String]) -> Result<Self> {
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
        let config = match parse_arg(args, "--matsuri-config") {
            Some(path) => MatsuriConfig::load(&path, None)
                .with_context(|| format!("载入剧本配置失败: {path}"))?,
            None => MatsuriConfig::load_with_overlay_file(&data_root, ".")
                .context("载入剧本配置失败")?,
        };
        let comp = matsuri_core::deck_comp(&engine.sim.cards).context("卡组构成解析失败")?;
        let config = config.for_deck(comp);
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
    play_with_engine(env, &env.engine, config, seed, trainer)
}

/// 同 [`play`]，但允许换引擎配置（基线复跑要按档切马娘）
fn play_with_engine(
    env: &Env,
    engine: &umaai::Config,
    config: &MatsuriConfig,
    seed: u64,
    trainer: &impl Trainer<MatsuriGame>,
) -> Result<Outcome> {
    let mut game = build_game(engine, env.data.clone())?.with_config(config.clone());
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
    let mut env = Env::load(&args)?;
    // 显式卡组档（扫参变体）：缺省按 comp 自动匹配
    if let Some(name) = parse_arg(&args, "--deck-profile") {
        env.config = env.config.for_profile(&name)?;
    }
    // 理想测试固定条件：缺省取 data 默认层，命令行可覆盖（马娘 / 切状态 / 心情盾层数）
    apply_fixture(&mut env, &args)?;
    match command {
        "game" => cmd_game(&env, &args),
        "luck" => cmd_luck(&env, &args),
        "micro" => cmd_micro(&env, &args),
        "root" => cmd_root(&env, &args),
        "trace" => cmd_trace(&env, &args),
        "stages" => cmd_stages(&env, &args),
        "flat" => cmd_flat(&env, &args),
        "parts" => cmd_parts(&env, &args),
        "allocs" => cmd_allocs(&env, &args),
        "baseline" => cmd_baseline(&env, &args),
        #[cfg(feature = "profile")]
        "prof" => prof::cmd_prof(&env, &args),
        other => bail!(
            "未知子命令: {other}（可选 game / luck / micro / root / trace / stages / parts / flat / allocs）"
        ),
    }
}

/// 切状态的显示名（CSV 头用）
fn qie_name(qie: Qie) -> &'static str {
    match qie {
        Qie::None => "none",
        Qie::Xiaoqie => "xiaoqie",
        Qie::Qiezhe => "qiezhe",
    }
}

/// 理想测试固定条件的命令行覆盖：`--uma` / `--qie` / `--shield`
///
/// 缺省取 `data/` 默认层（马娘 105902、小切、心情盾 2 层）。只改本进程内存中的配置，
/// 不写回文件；所有子命令共用同一入口，保证同一次测量里条件一致。
fn apply_fixture(env: &mut Env, args: &[String]) -> Result<()> {
    if let Some(raw) = parse_arg(args, "--uma") {
        env.engine.sim.uma = raw.parse().with_context(|| format!("--uma 非法: {raw}"))?;
    }
    if let Some(raw) = parse_arg(args, "--qie") {
        env.config.pre.qie = match raw.as_str() {
            "none" => Qie::None,
            "xiaoqie" => Qie::Xiaoqie,
            "qiezhe" => Qie::Qiezhe,
            other => bail!("--qie 只能取 none / xiaoqie / qiezhe（收到 {other}）"),
        };
    }
    if let Some(raw) = parse_arg(args, "--shield") {
        env.config.pre.positive_thinking = raw
            .parse()
            .with_context(|| format!("--shield 非法: {raw}"))?;
    }
    Ok(())
}

/// 基线复跑：`bench baseline [--name a-normal] [--trainer hand|search|both] [--games N]`
///
/// 读仓根 `benchmark/baselines/*.json`，按档内的马娘 / 切状态 / 心情盾 / 搜索参数复跑，再按
/// 合并标准误与相对容差判定。任一档漂移即以非零退出码返回，便于 CI 当门禁用。
fn cmd_baseline(env: &Env, args: &[String]) -> Result<()> {
    let only = parse_arg(args, "--name");
    let games = parse_usize(args, "--games", 0)?;
    let sigma = parse_arg(args, "--sigma")
        .map(|v| v.parse::<f64>())
        .transpose()
        .with_context(|| "--sigma 非法")?
        .unwrap_or(3.0);
    let rel = parse_arg(args, "--rel")
        .map(|v| v.parse::<f64>())
        .transpose()
        .with_context(|| "--rel 非法")?
        .unwrap_or(0.01);
    let want = parse_arg(args, "--trainer").unwrap_or_else(|| "both".to_string());

    let dir = env.root.join("benchmark").join("baselines");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("读取基线目录失败: {}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("json"))
        .filter(|path| path.file_name().and_then(|s| s.to_str()) != Some("README.json"))
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("{} 下没有基线文件", dir.display());
    }

    let mut failed = 0;
    for path in files {
        let case = matsuri_sim::baseline::Case::load(&path)?;
        if only.as_deref().is_some_and(|name| name != case.name) {
            continue;
        }
        println!(
            "# 基线 {} uma={} qie={} shield={}（{}）",
            case.name, case.uma, case.qie, case.shield, case.note
        );
        println!(
            "trainer,search_n,games,score_mean,score_sd,score_delta,score_allow,pt_mean,pt_sd,pt_delta,pt_allow,verdict"
        );
        for record in &case.records {
            let run_search = match want.as_str() {
                "both" => true,
                "search" => record.search_n > 0,
                "hand" | "handwritten" => record.search_n == 0,
                other => bail!("--trainer 只能取 hand / search / both（收到 {other}）"),
            };
            if !run_search {
                continue;
            }
            let n = if games > 0 { games } else { record.games };
            let mut config = env.config.clone();
            config.search.search_n = record.search_n;
            config.pre.qie = match case.qie.as_str() {
                "none" => Qie::None,
                "xiaoqie" => Qie::Xiaoqie,
                "qiezhe" => Qie::Qiezhe,
                other => bail!("基线 {} 的 qie 非法: {other}", case.name),
            };
            config.pre.positive_thinking = case.shield;
            let mut engine = env.engine.clone();
            engine.sim.uma = case.uma;
            let started = Instant::now();
            let samples: Vec<(i32, i32)> = if record.search_n > 0 {
                let trainer = MatsuriSearchTrainer::new(
                    config.clone(),
                    engine.search.clone().with_search_n(record.search_n),
                )?;
                (0..n)
                    .map(|i| {
                        let outcome = play_with_engine(
                            env,
                            &engine,
                            &config,
                            record.seed0 + i as u64,
                            &trainer,
                        )?;
                        Ok((outcome.score, outcome.pt))
                    })
                    .collect::<Result<Vec<_>>>()?
            } else {
                let trainer = MatsuriTrainer::new(config.clone());
                (0..n)
                    .map(|i| {
                        let outcome = play_with_engine(
                            env,
                            &engine,
                            &config,
                            record.seed0 + i as u64,
                            &trainer,
                        )?;
                        Ok((outcome.score, outcome.pt))
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let verdict = judge(record, &samples, sigma, rel);
            let score: Vec<f64> = samples.iter().map(|(s, _)| f64::from(*s)).collect();
            let pts: Vec<f64> = samples.iter().map(|(_, p)| f64::from(*p)).collect();
            println!(
                "{},{},{},{:.1},{:.1},{:+.1},{:.1},{:.1},{:.1},{:+.1},{:.1},{}（{:.1}s）",
                record.trainer,
                record.search_n,
                n,
                umaai_core::stats::mean(&score),
                umaai_core::stats::stdev(&score),
                verdict.score_delta,
                verdict.score_allow,
                umaai_core::stats::mean(&pts),
                umaai_core::stats::stdev(&pts),
                verdict.pt_delta,
                verdict.pt_allow,
                if verdict.ok { "OK" } else { "DRIFT" },
                started.elapsed().as_secs_f64()
            );
            if !verdict.ok {
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} 条基线漂移");
    }
    Ok(())
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
        "# search_n={search_n} games={games} seed={seed} uma={} qie={} shield={} root={}",
        env.engine.sim.uma,
        qie_name(config.pre.qie),
        config.pre.positive_thinking,
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

/// 单次搜索决策的估分快照（官方口径 `expected_score`）
#[derive(Clone)]
struct LuckRow {
    turn: i32,
    expected: f64,
}

/// 采集 MCTS 自博弈估分轨迹的训练员包装（只记主行动决策，不含教诲 / 事件）
struct LuckTrainer {
    inner: MatsuriSearchTrainer,
    rows: Mutex<Vec<LuckRow>>,
}

impl LuckTrainer {
    fn new(inner: MatsuriSearchTrainer) -> Self {
        Self {
            inner,
            rows: Mutex::new(Vec::new()),
        }
    }

    fn rows(&self) -> Vec<LuckRow> {
        self.rows
            .lock()
            .map(|rows| rows.clone())
            .unwrap_or_default()
    }
}

impl Trainer<MatsuriGame> for LuckTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let advice = actions
            .iter()
            .all(|action| matches!(action, Action::AssignAdvice(_)));
        let chosen = self.inner.select_action(game, actions, rng)?;
        if !advice
            && let Some(summary) = self.inner.last_search()
            && let Ok(mut rows) = self.rows.lock()
        {
            rows.push(LuckRow {
                turn: game.turn(),
                expected: summary.expected_score,
            });
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
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

/// 最小二乘斜率（x = turn，y = expected）
fn ols_slope(rows: &[LuckRow]) -> Option<f64> {
    if rows.len() < 2 {
        return None;
    }
    let n = rows.len() as f64;
    let mean_x = rows.iter().map(|row| f64::from(row.turn)).sum::<f64>() / n;
    let mean_y = rows.iter().map(|row| row.expected).sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for row in rows {
        let dx = f64::from(row.turn) - mean_x;
        sxy += dx * (row.expected - mean_y);
        sxx += dx * dx;
    }
    (sxx > 0.0).then_some(sxy / sxx)
}

/// 估分随回合漂移：`bench luck --games 20 --seed 73200 --search-n 2048`
///
/// 整局用搜索训练员推进，记录每次主行动决策的官方口径 `expected_score`；
/// 每局取首 / 末搜索决策的差与跨度得到 `luck_turn_bonus` 补偿斜率
/// `b = ΔE / Δturn`（显示分 = `expected + (MAX_TURN − turn) × b`）。
fn cmd_luck(env: &Env, args: &[String]) -> Result<()> {
    let games = parse_usize(args, "--games", 20)?;
    let seed0 = parse_usize(args, "--seed", 73200)? as u64;
    let search_n = parse_usize(args, "--search-n", env.config.search.search_n)?;
    anyhow::ensure!(
        search_n > 0,
        "--search-n 必须大于 0（0 = 手写档，无搜索估分）"
    );
    let mut config = env.config.clone();
    config.search.search_n = search_n;
    println!(
        "# games={games} seed0={seed0} search_n={search_n} max_turn={} uma={}",
        matsuri_core::MAX_TURN,
        env.engine.sim.uma
    );
    println!("seed,first_turn,first_E,last_turn,last_E,delta,b,reg_slope,decisions,score,pt");
    let mut firsts = Vec::with_capacity(games);
    let mut lasts = Vec::with_capacity(games);
    let mut deltas = Vec::with_capacity(games);
    let mut slopes = Vec::with_capacity(games);
    let mut regs = Vec::with_capacity(games);
    for index in 0..games {
        let seed = seed0 + index as u64;
        let trainer = LuckTrainer::new(MatsuriSearchTrainer::new(
            config.clone(),
            env.engine.search.clone().with_search_n(search_n),
        )?);
        let mut game = env.build(&config)?;
        game.set_rule_master(seed);
        let mut rng = StdRng::seed_from_u64(seed);
        game.run_stage(&trainer, &mut rng)?;
        while game.next() {
            game.run_stage(&trainer, &mut rng)?;
        }
        game.on_simulation_end(&trainer, &mut rng)?;
        let rows = trainer.rows();
        let Some(first) = rows.first() else {
            bail!("seed {seed} 未记录到任何搜索决策");
        };
        let Some(last) = rows.last() else {
            bail!("seed {seed} 未记录到任何搜索决策");
        };
        let span = last.turn - first.turn;
        anyhow::ensure!(span > 0, "seed {seed} 首末回合相同（T{}）", first.turn);
        let delta = last.expected - first.expected;
        let b = delta / f64::from(span);
        let reg = ols_slope(&rows).unwrap_or(0.0);
        let score = game.uma().calc_score()?;
        let pt = game.uma().total_pt()?;
        println!(
            "{seed},{},{:.1},{},{:.1},{:+.1},{:.3},{:.3},{},{score},{pt}",
            first.turn,
            first.expected,
            last.turn,
            last.expected,
            delta,
            b,
            reg,
            rows.len()
        );
        firsts.push(first.expected);
        lasts.push(last.expected);
        deltas.push(delta);
        slopes.push(b);
        regs.push(reg);
    }
    let report = |name: &str, values: &[f64]| {
        let mean = umaai_core::stats::mean(values);
        let sd = umaai_core::stats::stdev(values);
        let se = sd / (values.len() as f64).sqrt();
        let ci = 1.96 * se;
        let t = if se > 0.0 { mean / se } else { 0.0 };
        println!(
            "# {name}: mean={mean:.3} sd={sd:.3} se={se:.3} ci95=±{ci:.3} t={t:.2} n={}",
            values.len()
        );
    };
    report("first_E", &firsts);
    report("last_E", &lasts);
    report("delta", &deltas);
    report("b_gap", &slopes);
    report("b_regression", &regs);
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
    let pt_favor = game.config.deck.trainer.pt_favor_rate;
    let score_ns = bench(iters, || {
        black_box(game.search_score(pt_favor).ok());
    });
    println!("search_score           {score_ns:10.1}");
    let list_ns = bench(iters.max(200), || {
        black_box(black_box(&game).list_actions().ok());
    });
    println!("list_actions           {list_ns:10.1}");
    Ok(())
}

/// 只累计决策耗时的包装训练员（原子累加，不逐条记录，热路径开销最小）
struct SumTrainer {
    inner: MatsuriTrainer,
    select_ns: AtomicU64,
    select_n: AtomicU64,
    event_ns: AtomicU64,
    event_n: AtomicU64,
}

impl SumTrainer {
    fn new(inner: MatsuriTrainer) -> Self {
        Self {
            inner,
            select_ns: AtomicU64::new(0),
            select_n: AtomicU64::new(0),
            event_ns: AtomicU64::new(0),
            event_n: AtomicU64::new(0),
        }
    }

    fn add(counter: &AtomicU64, n: &AtomicU64, started: Instant) {
        counter.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        n.fetch_add(1, Ordering::Relaxed);
    }
}

impl Trainer<MatsuriGame> for SumTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let started = Instant::now();
        let chosen = self.inner.select_action(game, actions, rng)?;
        Self::add(&self.select_ns, &self.select_n, started);
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
        Self::add(&self.event_ns, &self.event_n, started);
        Ok(chosen)
    }
}

/// 平铺并行（std::thread）rollout：bench flat --turn 32 --rollouts 2048 --threads 24
///
/// 与 `root` 的引擎嵌套 rayon 路径对照：同样从固定根跑 total 次 rollout，只把并行
/// 组织换成「一层线程池 × 每线程串行 rollout」，用来判断瓶颈是搜索调度还是工作负载本身。
fn cmd_flat(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let total = parse_usize(args, "--rollouts", 2048)?;
    let threads = parse_usize(args, "--threads", 8)?;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    let game = Arc::new(game);
    let actions = Arc::new(actions);
    let pt = config.deck.trainer.pt_favor_rate;
    let trainer = Arc::new(MatsuriTrainer::new(config));
    let per = total.div_ceil(threads);
    let started = Instant::now();
    let mut handles = Vec::with_capacity(threads);
    for thread in 0..threads {
        let game = game.clone();
        let actions = actions.clone();
        let trainer = trainer.clone();
        handles.push(std::thread::spawn(move || -> f64 {
            let mut sum = 0.0;
            for k in 0..per {
                let index = (thread * per + k) % actions.len().max(1);
                let rollout_seed = seed.wrapping_add((thread * per + k) as u64);
                if let Ok(score) = rollout_with(&game, &actions[index], rollout_seed, &*trainer, pt)
                {
                    sum += score;
                }
            }
            sum
        }));
    }
    let mut sum = 0.0;
    for handle in handles {
        sum += handle
            .join()
            .map_err(|_| anyhow::anyhow!("rollout 线程 panic"))?;
    }
    let elapsed = started.elapsed();
    println!(
        "# turn={turn} threads={threads} rollouts={} elapsed_ms={:.1} ns_per_rollout={:.0} checksum={sum:.1}",
        per * threads,
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_nanos() as f64 / (per * threads) as f64
    );
    Ok(())
}

/// 统计训练员内部分配的包装训练员（`bench allocs` 用）
struct AllocCountTrainer {
    inner: MatsuriTrainer,
    action: AtomicU64,
    action_n: AtomicU64,
    event: AtomicU64,
    event_n: AtomicU64,
}

impl Trainer<MatsuriGame> for AllocCountTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let before = ALLOC_COUNT.load(Ordering::Relaxed);
        let chosen = self.inner.select_action(game, actions, rng)?;
        self.action.fetch_add(
            ALLOC_COUNT.load(Ordering::Relaxed) - before,
            Ordering::Relaxed,
        );
        self.action_n.fetch_add(1, Ordering::Relaxed);
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
        let before = ALLOC_COUNT.load(Ordering::Relaxed);
        let chosen = self.inner.select_event_choice(game, event, choices, rng)?;
        self.event.fetch_add(
            ALLOC_COUNT.load(Ordering::Relaxed) - before,
            Ordering::Relaxed,
        );
        self.event_n.fetch_add(1, Ordering::Relaxed);
        Ok(chosen)
    }
}

/// 阶段机编号 → 名称（`StageId` 显式赋号见 stage.rs）
/// 单次 rollout 的分配计数：bench allocs --turn 32
fn cmd_allocs(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let action_index = parse_usize(args, "--action", 0)?;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    let action = actions.get(action_index).context("动作下标越界")?;
    let trainer = AllocCountTrainer {
        inner: MatsuriTrainer::new(config),
        action: AtomicU64::new(0),
        action_n: AtomicU64::new(0),
        event: AtomicU64::new(0),
        event_n: AtomicU64::new(0),
    };
    let mut rng = StdRng::seed_from_u64(seed);
    let mut sim = game.fork_for_rollout(seed);
    sim.apply_root_action(action, &mut rng)?;
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    COUNT_ALLOCS.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let mut turns = 0usize;
    let mut stage_allocs = [0u64; 5];
    let mut stage_counts = [0u64; 5];
    while sim.next() {
        let index = usize::try_from(sim.stage().get()).unwrap_or(4).min(4);
        let before = ALLOC_COUNT.load(Ordering::Relaxed);
        sim.run_stage(&trainer, &mut rng)?;
        stage_allocs[index] += ALLOC_COUNT.load(Ordering::Relaxed) - before;
        stage_counts[index] += 1;
        turns += 1;
    }
    let end_before = ALLOC_COUNT.load(Ordering::Relaxed);
    sim.on_simulation_end(&trainer, &mut rng)?;
    let end_allocs = ALLOC_COUNT.load(Ordering::Relaxed) - end_before;
    let elapsed = started.elapsed();
    COUNT_ALLOCS.store(false, Ordering::Relaxed);
    println!(
        "# per_stage_allocs Begin={} Train={} End={} Distribute={} AfterTrain={} on_end={}",
        stage_allocs[0],
        stage_allocs[1],
        stage_allocs[2],
        stage_allocs[3],
        stage_allocs[4],
        end_allocs
    );
    println!(
        "# per_stage_calls Begin={} Train={} End={} Distribute={} AfterTrain={}",
        stage_counts[0], stage_counts[1], stage_counts[2], stage_counts[3], stage_counts[4]
    );
    let counts = ALLOC_COUNT.load(Ordering::Relaxed);
    let bytes = ALLOC_BYTES.load(Ordering::Relaxed);
    println!(
        "# turn={turn} stages={turns} elapsed_us={:.1} allocs={counts} bytes={bytes} per_stage_allocs={:.1}",
        elapsed.as_secs_f64() * 1e6,
        counts as f64 / turns.max(1) as f64
    );
    println!(
        "# avg_alloc_bytes={:.1} total_kib={:.1}",
        bytes as f64 / counts.max(1) as f64,
        bytes as f64 / 1024.0
    );
    println!(
        "# trainer_allocs action={} n={} event={} n={}",
        trainer.action.load(Ordering::Relaxed),
        trainer.action_n.load(Ordering::Relaxed),
        trainer.event.load(Ordering::Relaxed),
        trainer.event_n.load(Ordering::Relaxed)
    );
    Ok(())
}

/// rollout 内按阶段拆分：bench stages --turn 32
fn cmd_stages(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let action_index = parse_usize(args, "--action", 0)?;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    let action = actions.get(action_index).context("动作下标越界")?;
    let trainer = SumTrainer::new(MatsuriTrainer::new(config.clone()));

    let mut rng = StdRng::seed_from_u64(seed);
    let fork_started = Instant::now();
    let mut sim = game.fork_for_rollout(seed);
    let fork_ns = fork_started.elapsed().as_nanos() as u64;
    let root_started = Instant::now();
    sim.apply_root_action(action, &mut rng)?;
    let root_ns = root_started.elapsed().as_nanos() as u64;

    let mut sums = [0u64; 5];
    let mut counts = [0u64; 5];
    let mut wait_ns = 0u64;
    let mut wait_n = 0u64;
    while {
        let started = Instant::now();
        let more = sim.next();
        wait_ns += started.elapsed().as_nanos() as u64;
        if more {
            wait_n += 1;
        }
        more
    } {
        let id = sim.stage().get();
        let index = usize::try_from(id).unwrap_or(4).min(4);
        let started = Instant::now();
        sim.run_stage(&trainer, &mut rng)?;
        let elapsed = started.elapsed().as_nanos() as u64;
        sums[index] += elapsed;
        counts[index] += 1;
    }
    let end_started = Instant::now();
    sim.on_simulation_end(&trainer, &mut rng)?;
    let end_ns = end_started.elapsed().as_nanos() as u64;

    let total: u64 = sums.iter().sum::<u64>() + fork_ns + root_ns + end_ns;
    let select_ns = trainer.select_ns.load(Ordering::Relaxed);
    let select_n = trainer.select_n.load(Ordering::Relaxed);
    let event_ns = trainer.event_ns.load(Ordering::Relaxed);
    let event_n = trainer.event_n.load(Ordering::Relaxed);
    println!(
        "# turn={turn} seed={seed} action={action} stage_calls={} total_us={:.1}",
        counts.iter().sum::<u64>(),
        total as f64 / 1e3
    );
    println!("stage,Begin,Train,End,Distribute,AfterTrain,next,root,fork,on_end,total");
    let row = |values: &[f64]| {
        values
            .iter()
            .map(|v| format!("{v:.1}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let us = |ns: u64| ns as f64 / 1e3;
    println!(
        "us,{}",
        row(&[
            us(sums[0]),
            us(sums[1]),
            us(sums[2]),
            us(sums[3]),
            us(sums[4]),
            us(wait_ns),
            us(root_ns),
            us(fork_ns),
            us(end_ns),
            us(total),
        ])
    );
    println!(
        "share%,{}",
        row(&[
            shares(sums[0], total),
            shares(sums[1], total),
            shares(sums[2], total),
            shares(sums[3], total),
            shares(sums[4], total),
            shares(wait_ns, total),
            shares(root_ns, total),
            shares(fork_ns, total),
            shares(end_ns, total),
            100.0,
        ])
    );
    println!(
        "calls,{},{},{},{},{},{},{},{},{},{}",
        counts[0], counts[1], counts[2], counts[3], counts[4], wait_n, 1, 1, 1, 1
    );
    println!(
        "# trainer.action_us={:.1} n={select_n} trainer.event_us={:.1} n={event_n}",
        select_ns as f64 / 1e3,
        event_ns as f64 / 1e3
    );
    Ok(())
}

/// 百分比（分母 0 时返回 0）
fn shares(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// 阶段内单点 ns/op：bench parts --turn 32 --iters 2000
fn cmd_parts(env: &Env, args: &[String]) -> Result<()> {
    let turn = parse_i32(args, "--turn", 32)?;
    let iters = parse_usize(args, "--iters", 2000)?;
    let seed = parse_usize(args, "--seed", 61444)? as u64;
    let config = env.config.clone();
    let (game, actions) = capture_root(env, &config, seed, turn)?;
    println!("# turn={turn} actions={} iters={iters}", actions.len());

    let mut rng = StdRng::seed_from_u64(seed);
    let events_ns = bench(iters, || {
        black_box(black_box(&game).generate_events(&mut rng));
    });
    println!("generate_events        {events_ns:10.1}");

    let mut mutable = game.clone();
    let distribute_ns = bench(iters, || {
        black_box(black_box(&mut mutable).distribute_all(&mut rng).ok());
    });
    println!("distribute_all         {distribute_ns:10.1}");
    let hint_ns = bench(iters, || {
        black_box(black_box(&mut mutable).distribute_hint(&mut rng).ok());
    });
    println!("distribute_hint        {hint_ns:10.1}");
    let list_ns = bench(iters.max(200), || {
        black_box(black_box(&game).list_actions().ok());
    });
    println!("list_actions           {list_ns:10.1}");
    let dist_clone_ns = bench(iters, || {
        let mut copy = black_box(&game).clone();
        black_box(black_box(&mut copy).distribute_all(&mut rng).ok());
    });
    println!(
        "distribute+clone       {dist_clone_ns:10.1}  (clone 参考 {} )",
        bench(iters, || {
            black_box(black_box(&game).clone());
        })
    );
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
    search_config.pt_favor_rate = config.deck.trainer.pt_favor_rate;
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
    let pt = config.deck.trainer.pt_favor_rate;
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

/// `bench prof`：pprof（SIGPROF 采样）包一层现有测量命令，产出 SVG 火焰图 + 自耗时 TopN。
///
/// - 不依赖 `perf_event_open` / root：pprof 走 `setitimer(ITIMER_PROF)` + `SIGPROF`；
///   只采用户态 CPU，内核态时间用 `perf stat` 另看。
/// - 用法：`bench prof flat --turn 32 --rollouts 120000 --threads 16 --freq 997 --out benchmark/_local/perf/flame-flat.svg`。
#[cfg(feature = "profile")]
mod prof {
    use super::*;
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::fs::File;

    /// prof 子命令：剥掉 `prof` 与自身参数后交给原测量命令，前后包一层 pprof
    pub(super) fn cmd_prof(env: &Env, args: &[String]) -> Result<()> {
        let inner = args.get(2).map(String::as_str).context(
            "用法: bench prof <game|micro|root|trace|stages|flat|parts|allocs> [原参数] [--freq HZ] [--out PATH]",
        )?;
        let freq = parse_usize(args, "--freq", 997)?;
        anyhow::ensure!((1..=2000).contains(&freq), "--freq 超出范围 1..=2000");
        let out = parse_arg(args, "--out")
            .unwrap_or_else(|| format!("benchmark/_local/perf/flame-{inner}.svg"));

        // 重建原命令参数：去掉 "prof" 与 --freq / --out，其余 flags 原样保留
        let mut sub: Vec<String> = Vec::with_capacity(args.len());
        sub.push(args[0].clone());
        let mut skip_value = false;
        for arg in args.iter().skip(2) {
            if skip_value {
                skip_value = false;
                continue;
            }
            if arg == "--freq" || arg == "--out" {
                skip_value = true;
                continue;
            }
            sub.push(arg.clone());
        }

        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(freq as i32)
            .blocklist(&["libgcc", "pthread", "vdso"])
            .build()
            .map_err(|err| anyhow::anyhow!("pprof 启动失败: {err}"))?;

        let outcome = match inner {
            "game" => cmd_game(env, &sub),
            "micro" => cmd_micro(env, &sub),
            "root" => cmd_root(env, &sub),
            "trace" => cmd_trace(env, &sub),
            "stages" => cmd_stages(env, &sub),
            "flat" => cmd_flat(env, &sub),
            "parts" => cmd_parts(env, &sub),
            "allocs" => cmd_allocs(env, &sub),
            other => bail!("bench prof: 未知子命令 {other}"),
        };

        let report = guard.report().build();
        drop(guard);
        outcome?;
        let report = report.map_err(|err| anyhow::anyhow!("pprof 报告生成失败: {err}"))?;

        if let Some(parent) = Path::new(&out).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&out)?;
        report
            .flamegraph(file)
            .map_err(|err| anyhow::anyhow!("火焰图写出失败: {err}"))?;

        // 自耗时 TopN：叶帧聚合（火焰图看调用树，这里看单函数占比）
        let mut self_count: HashMap<String, isize> = HashMap::new();
        let mut total = 0isize;
        for (frames, count) in &report.data {
            total += *count;
            let leaf = frames
                .frames
                .first()
                .and_then(|frame| frame.first())
                .map(|symbol| symbol.name())
                .unwrap_or_else(|| "<unknown>".to_string());
            *self_count.entry(leaf).or_default() += *count;
        }
        let mut rows: Vec<(String, isize)> = self_count.into_iter().collect();
        rows.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));

        let mut text = String::new();
        writeln!(
            text,
            "# pprof freq={freq}Hz samples={total} command={inner} svg={out}"
        )?;
        for (name, count) in rows.iter().take(40) {
            writeln!(
                text,
                "{:6.2}% {:6}  {name}",
                *count as f64 * 100.0 / total.max(1) as f64,
                count
            )?;
        }
        let text_path = Path::new(&out).with_extension("txt");
        std::fs::write(&text_path, text)?;
        println!(
            "# pprof freq={freq}Hz samples={total} svg={out} top={}",
            text_path.display()
        );
        if total == 0 {
            eprintln!("# 警告：0 个样本，加大 --rollouts / --games 或提高 --freq");
        }
        Ok(())
    }
}
