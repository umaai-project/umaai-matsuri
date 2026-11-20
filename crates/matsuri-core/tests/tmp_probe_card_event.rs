//! 诊断：支援卡连续事件（技能打折 8001/8002/8003）走向。
//!
//! 手动跑（release，仓根；默认 20 局手写档、seed 73200 起）：
//! `PROBE_GAMES=20 PROBE_SEED0=73200 cargo test --release --offline \
//!   -p matsuri-core --test tmp_probe_card_event -- --ignored --nocapture`
//!
//! 口径：与 `bench` 同序推进（规则主种子 = 决策流种子 = seed0+i），逐回合 diff
//! `base.events` 的 8001/8002/8003 计数；按 `turn/24` 归年（Y1 0-23 / Y2 24-47 /
//! Y3 48-71 / URA 72+），`N >= 15` 记局级打满（全局计数上限 = 3 段 × 5 次）。
//! 文件名沿用建时的临时名（沙箱不允许重命名/删除）。

use std::sync::Arc;

use anyhow::{Context, Result};
use matsuri_core::testing::data_dir;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriTrainer, build_game, deck_comp,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umasim::game::Game;

/// 8001/8002/8003 当前计数快照
fn snap(game: &MatsuriGame) -> [u32; 3] {
    [8001u32, 8002u32, 8003u32].map(|id| game.base.events.get(&id).copied().unwrap_or(0))
}

/// 累计一回合的事件增量到分段计数与年段计数
fn accumulate(
    game: &MatsuriGame,
    prev: [u32; 3],
    turn: i32,
    per: &mut [u32; 3],
    years: &mut [u32; 4],
) {
    let now = snap(game);
    let year = (turn.max(0) / 24).min(3) as usize;
    for (k, (before, after)) in prev.iter().zip(now.iter()).enumerate() {
        let delta = after.saturating_sub(*before);
        per[k] += delta;
        years[year] += delta;
    }
}

#[test]
#[ignore = "手动诊断：默认 20 局手写档（PROBE_GAMES / PROBE_SEED0 可覆盖）"]
fn probe_card_event() -> Result<()> {
    let root = data_dir();
    Data::init(&root).context("引擎静态数据初始化失败")?;
    let engine = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置失败")?;
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据失败")?);
    let mut base = MatsuriConfig::load_with_overlay_file(&root, ".")
        .context("载入剧本配置失败")?
        .for_deck(deck_comp(&engine.sim.cards).context("卡组构成解析失败")?);
    base.search.search_n = 0;

    let games: u64 = std::env::var("PROBE_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let seed0: u64 = std::env::var("PROBE_SEED0")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(73_200);

    println!(
        "# uma={} qie={:?} games={games} seed0={seed0}",
        engine.sim.uma, base.pre.qie
    );
    println!("seed,n8001,n8002,n8003,N,y1,y2,y3,y4,hints,score,pt");

    let mut sum_n = 0u32;
    let mut sum_y1 = 0u32;
    let mut full = 0usize;
    for i in 0..games {
        let seed = seed0 + i;
        let mut game = build_game(&engine, Arc::clone(&data))?.with_config(base.clone());
        game.set_rule_master(seed);
        let mut rng = StdRng::seed_from_u64(seed);
        let trainer = MatsuriTrainer::new(base.clone());

        let mut per = [0u32; 3];
        let mut years = [0u32; 4];

        let mut prev = snap(&game);
        let mut turn = game.turn();
        game.run_stage(&trainer, &mut rng)?;
        accumulate(&game, prev, turn, &mut per, &mut years);
        while game.next() {
            turn = game.turn();
            prev = snap(&game);
            game.run_stage(&trainer, &mut rng)?;
            accumulate(&game, prev, turn, &mut per, &mut years);
        }
        game.on_simulation_end(&trainer, &mut rng)?;

        let n = per.iter().sum::<u32>();
        if n >= 15 {
            full += 1;
        }
        sum_n += n;
        sum_y1 += years[0];
        println!(
            "{seed},{},{},{},{n},{},{},{},{},{},{},{}",
            per[0],
            per[1],
            per[2],
            years[0],
            years[1],
            years[2],
            years[3],
            game.uma().total_hints,
            game.uma().calc_score()?,
            game.uma().total_pt()?
        );
    }
    println!(
        "# meanN={:.2} Y1={:.2} full_rate={:.1}%",
        sum_n as f64 / games as f64,
        sum_y1 as f64 / games as f64,
        full as f64 * 100.0 / games as f64
    );
    Ok(())
}
