//! 终局分布（抽样）：从配置卡组建局 → 换种子跑满整局 → 落分布 + 逐样本明细
//!
//! 用途：**配对门禁**的采样侧。改动前后各出一份样本明细（`MC_DUMP`），
//! 交给 `matsuri-gate` 做 **CRN 同种子逐局配对**（同 `MC_SEED` ⇒ 两臂随机流对齐）。
//!
//! 用法：`matsuri-dist [result.json]`
//!
//! 环境变量：
//! - `MC_N`：抽样数（默认 4000）
//! - `MC_SEED`：种子基（默认 0）。采样种子**按样本号派生** ⇒ 换 `MC_SEED` 不换抽样结构，
//!   只换基点；同一 `MC_SEED` 下两批样本逐号对齐 ⇒ 可配对。
//! - `MC_DUMP=<path>`：逐样本明细（JSONL，每行一个样本）
//!
//! 输出（三分量门禁口径，**不含任何拟合系数**）：
//! - `five_sum`：五维和（直接加）
//! - `stat_score`：五维分（游戏原生表；>1200 的属性按客户端口径折算后查表）
//! - `pt`：技能点（`uma.skill_pt`）

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde_json::{Value, json};

use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriTrainer};
use umaai::Config;
use umaai_core::Data;
use umasim::game::Game;
use umasim::search::FlatSearchGame;

/// 统一数据根（代码级配置，与 `matsuri` bin 同源）
const DATA_ROOT: &str = "data";

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("错误: {e:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 跑一遍抽样
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let result_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "out/_dist_result.json".into());
    let n: usize = std::env::var("MC_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4000);
    let seed_base: u64 = std::env::var("MC_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    Data::init(DATA_ROOT)?;
    let data = std::sync::Arc::new(MatsuriData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;
    let trainer = MatsuriTrainer::new(matsuri_cfg);
    let consts = &Data::get()?.consts;

    let game = matsuri_core::build_game(&cfg, data).context("建局失败")?;
    let turn = game.turn();

    let mut five_sums: Vec<i32> = Vec::with_capacity(n);
    let mut stat_scores: Vec<i32> = Vec::with_capacity(n);
    let mut pts: Vec<i32> = Vec::with_capacity(n);
    let mut dump: Vec<Value> = Vec::new();

    for i in 0..n {
        // 采样种子按样本号派生：同 MC_SEED ⇒ 同号同种子 ⇒ 两臂随机流逐回合对齐
        let seed = seed_base
            .wrapping_mul(0x5DEE_CE66_D1CE_4A1F)
            .wrapping_add((i as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let mut g = game.fork_for_rollout(seed);
        let mut rng = StdRng::seed_from_u64(seed ^ 0xA1B2_C3D4_5E6F_7081);
        g.run_full_game(&trainer, &mut rng)?;

        let uma = g.uma();
        let stat_part: i32 = uma
            .five_status
            .iter()
            .map(|&v| consts.status_final_score(revise(v)))
            .sum();
        let five_sum: i32 = uma.five_status.iter().sum();
        let pt = uma.skill_pt;
        five_sums.push(five_sum);
        stat_scores.push(stat_part);
        pts.push(pt);

        if std::env::var("MC_DUMP").is_ok() {
            dump.push(json!({
                "i": i,
                "five": uma.five_status.to_vec(),
                "sum": five_sum,
                "stat": stat_part,
                "pt": pt,
                "act": g.acts.level.to_vec(),
                "gauge": g.acts.gauge.to_vec(),
            }));
        }
    }

    if let Ok(path) = std::env::var("MC_DUMP") {
        let body: Vec<String> = dump.iter().map(|v| v.to_string()).collect();
        std::fs::write(path, body.join("\n")).context("写样本明细失败")?;
    }

    let result = json!({
        "turn": turn,
        "ok": true,
        "engine": "umaai-matsuri-official",
        "n": n,
        "mc_seed": seed_base,
        // 三分量门禁：各自可独立核对，不含拟合系数
        "gate": {
            "note": "五维和 / 五维分 / PT 三项各自与真实局对照；判读走 matsuri-gate 的 CRN 逐局配对",
            "five_sum": describe(&five_sums),
            "stat_score": describe(&stat_scores),
            "pt": describe(&pts),
        },
    });
    let path = PathBuf::from(result_path);
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&path, serde_json::to_string(&result)?).context("写结果失败")?;
    Ok(())
}

/// 游戏客户端 `ScoreUtils`：>1200 的属性按 `(v+1200)/2` 计分；
/// `revise` 是它的逆（`2v-1200`），把原始属性还原成「等效未衰减值」再去查表。
fn revise(v: i32) -> i32 {
    if v <= 1200 { v } else { 2 * v - 1200 }
}

/// 纯数值描述统计（门禁三分量用）
fn describe(values: &[i32]) -> Value {
    let n = values.len();
    let mean = if n == 0 {
        0.0
    } else {
        values.iter().map(|&s| s as f64).sum::<f64>() / n as f64
    };
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let pct = |p: f64| -> f64 {
        if n == 0 {
            0.0
        } else {
            let idx = ((n as f64 - 1.0) * p).round() as usize;
            sorted[idx.min(n - 1)] as f64
        }
    };
    json!({
        "mean": mean,
        "median": pct(0.5),
        "p10": pct(0.10),
        "p90": pct(0.90),
        "min": sorted.first().copied().unwrap_or(0),
        "max": sorted.last().copied().unwrap_or(0),
    })
}
