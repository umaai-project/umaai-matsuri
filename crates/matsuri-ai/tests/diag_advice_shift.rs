//! 诊断：教诲窗口边界（T0 载入帧 / T2 首授 / T47 融合）的期望分台阶来源
//!
//! 两件事：
//! 1. 同一局面改一个状态再估（授予 / 清空教诲、应用融合），看期望分台阶是否由该状态驱动；
//! 2. 从上一帧局面按手写档推进一回合，与下一帧真机局面逐字段对比，看模拟与真机帧的
//!    事件 / 属性改变顺序是否一致。
//!
//! 只读诊断；MATSURI_DIAG_N 控制评估预算（默认 512）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, Stage,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::FrameKind;
use umaai_core::Data;
use umasim::game::{Game, Trainer};
use umasim::search::SearchConfig;

fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_AI/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

/// 从帧原文里取局号（回合帧在 `baseGame`、终局帧在顶层）
fn frame_game(contents: &str) -> Option<u64> {
    let key = "\"single_mode_chara_id\":";
    let at = contents.find(key)? + key.len();
    let digits: String = contents[at..]
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn budget() -> usize {
    std::env::var("MATSURI_DIAG_N")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(512)
}

fn session(n: usize) -> Result<(OnlineSession, MatsuriConfig)> {
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置")?;
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(&root, ".").context("载入剧本配置")?;
    matsuri_cfg.search.search_n = n;
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据")?);
    let session = OnlineSession::new(&cfg, data, matsuri_cfg.clone(), TrainerKind::Search, false)?;
    Ok((session, matsuri_cfg))
}

fn eval(game: &MatsuriGame, cfg: &MatsuriConfig, n: usize) -> Option<(f64, f64, String)> {
    let mut cfg = cfg.clone();
    cfg.search.search_n = n;
    let actions = match game.list_actions() {
        Ok(actions) => actions,
        Err(err) => return Some((f64::NAN, f64::NAN, format!("候选生成失败: {err}"))),
    };
    let trainer = match MatsuriSearchTrainer::new(cfg, SearchConfig::default().with_search_n(n)) {
        Ok(trainer) => trainer,
        Err(err) => return Some((f64::NAN, f64::NAN, format!("训练员装配失败: {err}"))),
    };
    let mut rng = StdRng::seed_from_u64(20261113);
    if let Err(err) = trainer.select_action(game, &actions, &mut rng) {
        return Some((f64::NAN, f64::NAN, format!("搜索失败: {err:#}")));
    }
    trainer.last_search().map(|summary| {
        (
            summary.expected_score,
            summary.baseline_stdev,
            String::new(),
        )
    })
}

fn digest(game: &MatsuriGame) -> String {
    let levels: Vec<usize> = (0..5).map(|t| game.train_level(t)).collect();
    let friendship: i32 = game.base.deck.iter().map(|c| c.friendship).sum();
    format!(
        "turn={} vital={} mot={} five={:?} pt={} total_pt={:?} levels={:?} gauge={:?} advice(acq={} slots={:?} fused={:?}) cp={:?} persons={} friend={}",
        game.turn(),
        game.uma().vital,
        game.uma().motivation,
        game.uma().five_status,
        game.uma().skill_pt,
        game.uma().total_pt().ok(),
        levels,
        game.acts.gauge,
        game.advice.acquired,
        game.advice.slots,
        game.advice.fused,
        game.check_points,
        game.base.deck.len(),
        friendship,
    ) + &format!(
        "\n      events={:?} unresolved={:?} tlc={:?}\n      plan={:?}\n      acts={:?}\n      flags={:?} bonus={:?} absent={}\n      dist={:?}",
        game.base.events.len(),
        game.base
            .unresolved_events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        game.base.train_level_count,
        game.plan,
        game.acts,
        game.uma().flags,
        game.uma().five_status_bonus,
        game.base.absent_rate_drop,
        game.base.distribution,
    ) + &format!(
        "\n      persons={:?}\n      tlb={:?} assist={} kensan={} fatrec={} finals={} strategy={} camp={:?}",
        game.persons
            .iter()
            .map(|p| (p.act, p.origin, p.avail_turn))
            .collect::<Vec<_>>(),
        game.train_level_bonus,
        game.assist_support_left,
        game.kensan_pending,
        game.fatigue_recovery,
        game.finals_done,
        game.strategy.is_some(),
        game.camp,
    )
}

fn line_diffs(a: &str, b: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.split(' ').collect(), b.split(' ').collect());
    for (x, y) in la.iter().zip(lb.iter()) {
        if x != y {
            out.push(format!("      {}  →  {}", x, y));
        }
    }
    out
}

fn advance(game: &MatsuriGame, cfg: &MatsuriConfig, seed: u64) -> Result<MatsuriGame> {
    let start = game.turn();
    let mut game = game.clone();
    let hand = MatsuriTrainer::new(cfg.clone());
    let mut rng = StdRng::seed_from_u64(seed);
    for _ in 0..8 {
        game.run_stage(&hand, &mut rng)?;
        if !game.next() {
            break;
        }
        if game.turn() > start {
            break;
        }
    }
    // 停在新回合的 Train 阶段（Begin 的教诲授予已执行完）
    for _ in 0..6 {
        if game.turn() > start && game.stage == Stage::Train {
            break;
        }
        game.run_stage(&hand, &mut rng)?;
        if !game.next() {
            break;
        }
    }
    Ok(game)
}

#[test]
fn diag_advice_window_shift() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在");
        return Ok(());
    };
    let n = budget();
    let (mut sess, cfg) = session(1)?;
    let mut source = FrameSource::open_replay(&dir)?;
    let mut snaps: Vec<(i32, usize, String, String, MatsuriGame)> = Vec::new();
    // 仓外夹具可能混有通不过编成门禁的局：整局跳过，不影响其余局。
    let mut skipped: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind == FrameKind::FinalScore {
            continue;
        }
        let game = frame_game(&frame.contents);
        if let Some(game) = game
            && skipped.contains(&game)
        {
            continue;
        }
        let out = match sess.handle_frame(&frame.contents) {
            Ok(out) => out,
            Err(err) if format!("{err:#}").contains("编成不合规") => {
                if let Some(game) = game {
                    skipped.insert(game);
                }
                eprintln!("跳过编成不合规的局（game{game:?}）：{err:#}");
                continue;
            }
            Err(err) => return Err(err),
        };
        let kind = out
            .record
            .as_ref()
            .map_or_else(|| "skipped".to_string(), |r| r.kind.clone());
        let skip = out.skip_reason.clone().unwrap_or_default();
        snaps.push((out.turn, snaps.len(), kind, skip, out.game.clone()));
    }
    if snaps.is_empty() {
        eprintln!(
            "跳过：帧目录里没有可回放的合规局（跳过 {} 局）",
            skipped.len()
        );
        return Ok(());
    }
    println!("=== 回放 {} 帧；评估预算 N={n} ===", snaps.len());
    let find =
        |turn: i32, kind: &str, nth: usize| -> Option<&(i32, usize, String, String, MatsuriGame)> {
            snaps
                .iter()
                .filter(|(t, _, k, _, _)| *t == turn && k == kind)
                .nth(nth)
        };
    println!("=== A/B：改一个状态再估 ===");
    for (turn, kind, nth) in [
        (1, "train", 0),
        (2, "train", 0),
        (47, "train", 0),
        (48, "train", 0),
    ] {
        let Some((t, idx, k, _, game)) = find(turn, kind, nth) else {
            continue;
        };
        let Some((base, sd, err)) = eval(game, &cfg, n) else {
            continue;
        };
        if base.is_nan() {
            println!("  #{idx:<3} T{t} {k:<8} {err}");
            continue;
        }
        println!(
            "  #{idx:<3} T{t} {k:<8} E={base:.0} (±{sd:.0})  {}",
            digest(game)
        );
        if *t == 1 {
            let mut granted = game.clone();
            granted.advice.acquired = true;
            granted.advice.slots = [101, 201, 301, 401, 501];
            granted.advice.fused = [0; 5];
            if let Some((e2, s2, err)) = eval(&granted, &cfg, n) {
                println!(
                    "      → 强制授予教诲 E={e2:.0} (±{s2:.0}) Δ={:+.0} {err}",
                    e2 - base
                );
            }
            let mut trimmed = game.clone();
            trimmed.persons.truncate(14);
            if let Some((e3, s3, err)) = eval(&trimmed, &cfg, n) {
                println!(
                    "      → 花名册裁到 14 人 E={e3:.0} (±{s3:.0}) Δ={:+.0} {err}",
                    e3 - base
                );
            }
        }
        if *t == 2 {
            let mut default_slots = game.clone();
            default_slots.advice.slots = [101, 201, 301, 401, 501];
            if let Some((e2, s2, err)) = eval(&default_slots, &cfg, n) {
                println!(
                    "      → 教诲槽改回默认排列 E={e2:.0} (±{s2:.0}) Δ={:+.0} {err}",
                    e2 - base
                );
            }
            let mut no_dist = game.clone();
            no_dist.base.distribution = vec![Vec::new(); 5];
            if let Some((e2, s2, err)) = eval(&no_dist, &cfg, n) {
                println!(
                    "      → 清空人头分布 E={e2:.0} (±{s2:.0}) Δ={:+.0} {err}",
                    e2 - base
                );
            }
        }
        if *t == 47 {
            let pairs2 = cfg.deck.fusion.year2.clone();
            let pairs3 = cfg.deck.fusion.year3.clone();
            for (tag, pairs) in [("year2", &pairs2), ("year3", &pairs3)] {
                let mut fused = game.clone();
                if fused.apply_fusion_pairs(pairs).is_ok()
                    && let Some((e2, s2, err)) = eval(&fused, &cfg, n)
                {
                    println!(
                        "      → 应用 {tag} 融合 E={e2:.0} (±{s2:.0}) Δ={:+.0} {err}",
                        e2 - base
                    );
                }
            }
        }
    }
    println!("=== 模拟推进一回合 vs 下一帧真机局面 ===");
    for (turn, kind, nth, label) in [(1, "train", 0, "T1→T2"), (47, "train", 0, "T47→T48")] {
        let Some((_, idx, _, _, game)) = find(turn, kind, nth) else {
            continue;
        };
        let sim = advance(game, &cfg, 7)?;
        let (next_turn, next_turn_idx) = (turn + 1, turn + 1);
        let Some((_, nidx, nkind, _, frame_game)) = snaps
            .iter()
            .find(|(t, i, k, _, _)| *t == next_turn && *i > *idx && k == "train")
        else {
            println!("  {label}：找不到下一帧，跳过");
            continue;
        };
        let _ = next_turn_idx;
        println!("  {label}：模拟(#{idx}) 对 真机帧(#{nidx} {nkind})");
        println!("    真机: {}", digest(frame_game));
        println!("    模拟: {}", digest(&sim));
        for d in line_diffs(&digest(frame_game), &digest(&sim)) {
            println!("{d}");
        }
        if let Some((es, ss, err)) = eval(&sim, &cfg, n) {
            println!("    模拟局面 E={es:.0} (±{ss:.0}) {err}");
        }
        if turn == 1 {
            println!("FULL-FRAME: {:?}", frame_game);
            println!("FULL-SIM: {:?}", sim);
        }
    }
    Ok(())
}
