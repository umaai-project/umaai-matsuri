//! 诊断：Hint 逐技能分桶（每技能封顶 5 级）与现有总等级口径的差异。
//!
//! 手动跑（release，仓根；默认 20 局手写档、每局重放 50 次）：
//! `PROBE_GAMES=20 PROBE_REPEAT=50 cargo test --release --offline \
//!   -p matsuri-core --test tmp_probe_hint_bucket -- --ignored --nocapture`
//! 可选 `PROBE_UMA` / `PROBE_CARDS`（逗号分隔 idrank）换真机局卡组。
//!
//! 口径：
//! - 事件流：diag「【灵感发放】T{turn} person={i} lv={lv} cnt={n} skill={s}」；`s` 只累计技能级数
//!   （属性事件不计），按 `n_events = s / lv` 次、每次 +lv 进入分桶。
//! - 分桶：每卡技能池 = `data/cardDB.json` 的 `skillCount`（master.mdb single_mode_hint_gain type=0）；
//!   每次事件只随机选「未满 5 级」的技能 +lv（截断 5）；池为 0 的卡全部视为溢出。
//! - 重放随机：每局固定 PROBE_REPEAT 次不同种子取均值；非灵感来源（初始 21 / 事件 hint / 进化）不变。

use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Mutex, Once};

use anyhow::{Context, Result};
use matsuri_core::testing::data_dir;
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriTrainer, build_game, deck_comp};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use umaai_core::Data;
use umasim::game::Game;

/// 捕获 diagnostic 目标的日志行
static DIAG_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DIAG_LOGGER: Once = Once::new();

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, meta: &log::Metadata<'_>) -> bool {
        meta.target() == "diagnostic"
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.target() == "diagnostic"
            && let Ok(mut lines) = DIAG_LINES.lock()
        {
            lines.push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}

fn install_capture() {
    DIAG_LOGGER.call_once(|| {
        let _ = log::set_boxed_logger(Box::new(CaptureLogger));
        log::set_max_level(log::LevelFilter::Info);
    });
}

fn take_lines() -> Vec<String> {
    DIAG_LINES
        .lock()
        .map(|mut lines| std::mem::take(&mut *lines))
        .unwrap_or_default()
}

/// 一次灵感发放（该次调用可能含 `count` 次技能事件，每次 `level` 级）
#[derive(Clone, Copy)]
struct HintEvent {
    turn: i32,
    person: usize,
    level: i32,
    count: u32,
}

/// 解析「【灵感发放】」日志行；skill=0（属性事件）跳过
fn parse_events(lines: &[String]) -> Vec<HintEvent> {
    let mut out = Vec::new();
    for line in lines {
        let Some(rest) = line.strip_prefix("【灵感发放】") else {
            continue;
        };
        let (mut turn, mut person, mut level, mut skill) = (0, 0usize, 0, 0);
        for part in rest.split_whitespace() {
            if let Some(v) = part.strip_prefix('T') {
                turn = v.parse().unwrap_or(0);
            } else if let Some(v) = part.strip_prefix("person=") {
                person = v.parse().unwrap_or(0);
            } else if let Some(v) = part.strip_prefix("lv=") {
                level = v.parse().unwrap_or(0);
            } else if let Some(v) = part.strip_prefix("skill=") {
                skill = v.parse().unwrap_or(0);
            }
        }
        if level <= 0 || skill <= 0 {
            continue;
        }
        let count = skill / level;
        if count > 0 {
            out.push(HintEvent {
                turn,
                person,
                level,
                count: count as u32,
            });
        }
    }
    out
}

/// 分桶重放一次：返回（有效总级数，按回合末累计）；`pools` = 每卡技能数
fn replay(events: &[HintEvent], pools: &[usize], seed: u64) -> (i32, BTreeMap<i32, i32>) {
    let mut levels: Vec<Vec<u8>> = pools.iter().map(|count| vec![0u8; *count]).collect();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut total = 0;
    let mut by_turn = BTreeMap::new();
    for event in events {
        if let Some(pool) = levels.get_mut(event.person) {
            for _ in 0..event.count {
                let open: Vec<usize> = pool
                    .iter()
                    .enumerate()
                    .filter(|(_, lv)| **lv < 5)
                    .map(|(index, _)| index)
                    .collect();
                if open.is_empty() {
                    break;
                }
                let index = open[rng.random_range(0..open.len())];
                let gain = event.level.min(5 - i32::from(pool[index]));
                if gain > 0 {
                    pool[index] += gain as u8;
                    total += gain;
                }
            }
        }
        by_turn.insert(event.turn, total);
    }
    (total, by_turn)
}

#[test]
#[ignore = "手动诊断：默认 20 局手写档 × 50 次分桶重放（PROBE_GAMES / PROBE_REPEAT / PROBE_UMA / PROBE_CARDS 可覆盖）"]
fn probe_hint_bucket() -> Result<()> {
    install_capture();
    let root = data_dir();
    Data::init(&root).context("引擎静态数据初始化失败")?;
    let mut engine =
        umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置失败")?;
    if let Ok(uma) = std::env::var("PROBE_UMA") {
        engine.sim.uma = uma.parse().context("PROBE_UMA 非法")?;
    }
    if let Ok(cards) = std::env::var("PROBE_CARDS") {
        engine.sim.cards = cards
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("PROBE_CARDS 非法")?;
    }
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据失败")?);
    let mut base = MatsuriConfig::load_with_overlay_file(&root, ".")
        .context("载入剧本配置失败")?
        .for_deck(deck_comp(&engine.sim.cards).context("卡组构成解析失败")?);
    base.search.search_n = 0;
    // 每卡技能池：cardDB 的 skillCount（0/缺省表示无池）
    let cards: BTreeMap<String, serde_json::Value> = serde_json::from_str(
        &fs::read_to_string(root.join("cardDB.json")).context("读 cardDB 失败")?,
    )
    .context("解析 cardDB 失败")?;
    let skill_counts: BTreeMap<String, u32> = cards
        .into_iter()
        .filter_map(|(id, card)| {
            let count = card.get("skillCount")?.as_u64()?;
            (count > 0).then_some((id, count as u32))
        })
        .collect();

    let games: u64 = env_num("PROBE_GAMES", 20);
    let seed0: u64 = env_num("PROBE_SEED0", 73_200);
    let repeats: u64 = env_num("PROBE_REPEAT", 50);
    let max_turn: i32 = 30;

    println!(
        "# uma={} cards={:?} games={games} seed0={seed0} repeats={repeats}",
        engine.sim.uma, engine.sim.cards
    );
    println!("seed,raw_insp,capped_insp,ratio,total_raw,total_capped");

    let mut raw_curve: Vec<f64> = vec![0.0; max_turn as usize];
    let mut cap_curve: Vec<f64> = vec![0.0; max_turn as usize];
    let mut ratios = Vec::new();
    for i in 0..games {
        let seed = seed0 + i;
        let mut game = build_game(&engine, Arc::clone(&data))?.with_config(base.clone());
        game.set_rule_master(seed);
        let mut rng = StdRng::seed_from_u64(seed);
        let trainer = MatsuriTrainer::new(base.clone());

        take_lines();
        game.run_stage(&trainer, &mut rng)?;
        while game.next() {
            game.run_stage(&trainer, &mut rng)?;
        }
        game.on_simulation_end(&trainer, &mut rng)?;
        let events = parse_events(&take_lines());

        let pools: Vec<usize> = game
            .deck()
            .iter()
            .map(|card| {
                skill_counts
                    .get(&card.card_id.to_string())
                    .copied()
                    .unwrap_or(0) as usize
            })
            .collect();
        let raw_insp: i32 = events.iter().map(|e| e.level * e.count as i32).sum();
        let total_raw = game.uma().total_hints;

        let mut capped_sum = 0i64;
        let mut capped_by_turn: Vec<f64> = vec![0.0; max_turn as usize];
        for r in 0..repeats {
            let (capped, by_turn) = replay(&events, &pools, seed * 1000 + r);
            capped_sum += i64::from(capped);
            let mut running = 0;
            for turn in 0..max_turn {
                if let Some(v) = by_turn.get(&turn) {
                    running = *v;
                }
                capped_by_turn[turn as usize] += f64::from(running);
            }
        }
        let capped_insp = capped_sum as f64 / repeats as f64;
        let ratio = if raw_insp > 0 {
            capped_insp / f64::from(raw_insp)
        } else {
            1.0
        };
        ratios.push(ratio);

        let mut raw_running = 0;
        for turn in 0..max_turn {
            for event in events.iter().filter(|e| e.turn == turn) {
                raw_running += event.level * event.count as i32;
            }
            raw_curve[turn as usize] += f64::from(raw_running);
            cap_curve[turn as usize] += capped_by_turn[turn as usize] / repeats as f64;
        }

        println!(
            "{seed},{raw_insp},{capped_insp:.1},{ratio:.3},{total_raw},{:.1}",
            f64::from(total_raw) - f64::from(raw_insp) + capped_insp
        );
    }
    println!(
        "# ratio mean={:.3} min={:.3} max={:.3}",
        ratios.iter().sum::<f64>() / ratios.len().max(1) as f64,
        ratios.iter().cloned().fold(f64::INFINITY, f64::min),
        ratios.iter().cloned().fold(0.0, f64::max)
    );
    println!("# turn,raw_mean,capped_mean");
    for turn in 0..max_turn {
        println!(
            "{turn},{:.1},{:.1}",
            raw_curve[turn as usize] / games as f64,
            cap_curve[turn as usize] / games as f64
        );
    }
    Ok(())
}

fn env_num<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
