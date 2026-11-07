//! 一次性诊断工具（不入库）：定位「某候选全部 rollout 失败」的真实错误
//!
//! 用法：`./target/release/probe_fail --seed 61446 --n 512`

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use matsuri_core::game::Action;
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, build_game};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};
use umasim::search::FlatSearch;

const DATA_ROOT: &str = "data";

/// 最小日志器：只为让引擎 search 的 `debug!`（rollout 失败原因）可见
struct PrintLog;

impl log::Log for PrintLog {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= log::Level::Debug
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.target(), record.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: PrintLog = PrintLog;

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
        let comp = matsuri_core::deck_comp(&engine.sim.cards).context("卡组构成解析失败")?;
        let config = config.for_deck(comp);
        Ok(Self {
            engine,
            data,
            config,
        })
    }
}

/// 包装：一旦内层决策报错，就地复现单次 rollout，打印真实错误
struct Probe {
    inner: MatsuriSearchTrainer,
    search: FlatSearch<MatsuriGame>,
    failpts: std::sync::Mutex<usize>,
    times: std::sync::Mutex<Vec<(i32, &'static str, f64, usize)>>,
}

impl Trainer<MatsuriGame> for Probe {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let started = std::time::Instant::now();
        let out = self.inner.select_action(game, actions, rng);
        if let Ok(mut t) = self.times.lock() {
            t.push((
                game.turn(),
                "train",
                started.elapsed().as_secs_f64() * 1e3,
                actions.len(),
            ));
        }
        match out {
            Ok(i) => Ok(i),
            Err(e) => {
                eprintln!(
                    "=== 决策失败 T{} stage={:?} err={e}",
                    game.turn(),
                    game.stage
                );
                eprintln!("候选 {} 个: {actions:?}", actions.len());
                for (k, a) in actions.iter().enumerate() {
                    let r = self
                        .search
                        .simulate_common(game, a, 12345)
                        .map(|v| format!("ok {v:.1}"))
                        .unwrap_or_else(|e| format!("ERR {e:#?}"));
                    eprintln!("  cand {k} {a}: {r}");
                }
                Err(e)
            }
        }
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
        let _ = (&self.failpts, &self.search);
        eprintln!(
            "[事件搜索] T{} event#{} choices={} advice.acquired={} slots={:?}",
            game.turn(),
            event.id,
            choices.len(),
            game.advice.acquired,
            game.advice.slots
        );
        let started = std::time::Instant::now();
        let out = self.inner.select_event_choice(game, event, choices, rng);
        if let Ok(mut t) = self.times.lock() {
            t.push((
                game.turn(),
                "event",
                started.elapsed().as_secs_f64() * 1e3,
                choices.len(),
            ));
        }
        match out {
            Ok(i) => Ok(i),
            Err(e) => {
                eprintln!(
                    "=== 事件决策失败 T{} event#{} err={e}",
                    game.turn(),
                    event.id
                );
                Err(e)
            }
        }
    }
}

fn main() -> Result<()> {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str, default: usize| -> usize {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let seed = get("--seed", 61446) as u64;
    let n = get("--n", 512);
    let env = Env::load()?;
    let mut config = env.config.clone();
    config.search.search_n = n;
    let trainer =
        MatsuriSearchTrainer::new(config.clone(), env.engine.search.clone().with_search_n(n))?;
    let mut cfg = env.engine.search.clone().with_search_n(n);
    cfg.pt_favor_rate = config.deck.trainer.pt_favor_rate;
    let search = FlatSearch::<MatsuriGame>::new(cfg)
        .with_rollout_trainer(matsuri_core::MatsuriTrainer::new(config.clone()))
        .with_parallel(false);
    let probe = Probe {
        inner: trainer,
        search,
        failpts: std::sync::Mutex::new(0),
        times: std::sync::Mutex::new(Vec::new()),
    };
    let mut game = build_game(&env.engine, env.data.clone())?.with_config(config.clone());
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut result = game.run_stage(&probe, &mut rng);
    while result.is_ok() && game.next() {
        result = game.run_stage(&probe, &mut rng);
    }
    match result {
        Ok(()) => println!(
            "完成：seed={seed} n={n} score={} turn={}",
            game.uma().calc_score()?,
            game.turn()
        ),
        Err(e) => eprintln!("中止：{e}"),
    }
    if let Ok(times) = probe.times.lock() {
        let mut sorted = times.clone();
        sorted.sort_by(|a, b| b.2.total_cmp(&a.2));
        let total: f64 = times.iter().map(|t| t.2).sum();
        println!(
            "# 决策 {} 次 合计 {:.1}s 均值 {:.0}ms 最大 {:.0}ms",
            times.len(),
            total / 1e3,
            total / times.len().max(1) as f64,
            sorted.first().map(|t| t.2).unwrap_or(0.0)
        );
        println!("T,kind,ms,choices");
        for (turn, kind, ms, c) in sorted.iter().take(12) {
            println!("{turn},{kind},{ms:.0},{c}");
        }
    }
    Ok(())
}
