//! 教诲选择边界：运气分 / 评分波动 + 选择前后回合状态核对
//!
//! 用仓外真机帧逐帧跑搜索档会话，对每个教诲窗口取「教诲帧前后的训练决策帧」，
//! 打印运气分 / 评分 / 状态对比表，并守门以下契约：
//! - 教诲帧不挂运气分（`LuckScorer` 只在 Train / Race 决策推进）；
//! - 同局同回合的多张决策帧复用同一份运气快照；
//! - 教诲帧搜索摘要齐全、回合落在窗口表，且搜索根阶段不污染输出局面；
//! - 教诲帧后一帧的教诲账本按该帧真值还原，不泄漏会话内补授予 / 融合；
//! - 边界前后期望分 E 的波动不超过 3×σ（σ = 单局终局标准差，噪声尺度）。
//!
//! 帧目录不存在时整组跳过（与 `tests/replay.rs` 同口径）。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use matsuri_ai::{FrameSource, OnlineSession, OnlineTurnOutput, TrainerKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{MatsuriConfig, MatsuriData};
use umaai::FrameKind;
use umaai_core::Data;

/// 教诲窗口回合（0 基，与 `data/scenario_matsuri.json::advice_windows` 同表）
const WINDOWS: [i32; 6] = [2, 11, 23, 35, 47, 59];

/// 发布包真机帧目录（仓外夹具）
fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_AI/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

/// 本次回放的搜索预算（默认 32；环境变量用于噪声对照）
fn budget() -> usize {
    std::env::var("MATSURI_BOUNDARY_N")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(32)
}

/// 搜索档会话（小预算：诊断默认 N=32；`MATSURI_BOUNDARY_N` 可调，用于噪声对照）
fn session() -> Result<OnlineSession> {
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置")?;
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(&root, ".").context("载入剧本配置")?;
    matsuri_cfg.search.search_n = budget();
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据")?);
    OnlineSession::new(&cfg, data, matsuri_cfg, TrainerKind::Search, false)
}

/// 一帧的诊断快照：在线输出侧 + 帧真值
#[derive(Debug, Clone)]
struct Snap {
    idx: usize,
    epoch: u64,
    turn: i32,
    kind: String,
    score: Option<f64>,
    expected: Option<f64>,
    stdev: Option<f64>,
    luck_total: Option<f64>,
    luck_delta: Option<f64>,
    stage: String,
    window: Option<usize>,
    slots: String,
    fused: String,
    acquired: bool,
    check_points: String,
    gauge: String,
    frame_turn: i32,
    frame_slots: String,
    frame_fused: String,
}

/// 帧真值里的教诲账本（`restore_advice` 同口径：fusionId != 0 时 slots 记原基础 id）
fn frame_advice(book: &serde_json::Value) -> (String, String) {
    let mut slots = [0i32; 5];
    let mut fused = [0i32; 5];
    if let Some(acts) = book["festa"]["acts"].as_array() {
        for (index, act) in acts.iter().take(5).enumerate() {
            let aid = act["adviceId"].as_i64().unwrap_or(0) as i32;
            let fid = act["fusionId"].as_i64().unwrap_or(0) as i32;
            if fid == 0 {
                slots[index] = aid;
            } else {
                slots[index] = fid;
                fused[index] = aid;
            }
        }
    }
    (format!("{slots:?}"), format!("{fused:?}"))
}

fn snap(idx: usize, out: &OnlineTurnOutput, contents: &str) -> Result<Snap> {
    let book: serde_json::Value = serde_json::from_str(contents)?;
    let (frame_slots, frame_fused) = frame_advice(&book);
    let record = out.record.as_ref();
    let luck = record.and_then(|record| record.luck.as_ref());
    let summary = out.summary.as_ref();
    Ok(Snap {
        idx,
        epoch: out.epoch,
        turn: out.turn,
        kind: record.map_or_else(|| "skipped".to_string(), |record| record.kind.clone()),
        score: record
            .and_then(|record| record.candidates.get(record.action_index))
            .and_then(|candidate| candidate.score),
        expected: summary.map(|summary| summary.expected_score),
        stdev: summary.map(|summary| summary.baseline_stdev),
        luck_total: luck.map(|luck| luck.snapshot.total_luck_score),
        luck_delta: luck.and_then(|luck| luck.snapshot.last_turn_delta),
        stage: format!("{:?}", out.game.stage),
        window: out.game.plan.advice_window,
        slots: format!("{:?}", out.game.advice.slots),
        fused: format!("{:?}", out.game.advice.fused),
        acquired: out.game.advice.acquired,
        check_points: format!("{:?}", out.game.check_points),
        gauge: format!("{:?}", out.game.acts.gauge),
        frame_turn: book["baseGame"]["turn"].as_i64().unwrap_or(-1) as i32,
        frame_slots,
        frame_fused,
    })
}

/// 一行诊断输出
fn line(tag: &str, s: &Snap) -> String {
    let f = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{v:.0}"));
    format!(
        "  {tag:<7} #{:<3} T{:<3} {:<8} score={:<7} E={:<7} σ={:<6} 运气 本局={:<8} 本轮={:<8} stage={:<10} win={:?} cp={} gauge={} slots={} fused={} acquired={}",
        s.idx,
        s.turn,
        s.kind,
        f(s.score),
        f(s.expected),
        f(s.stdev),
        f(s.luck_total),
        f(s.luck_delta),
        s.stage,
        s.window,
        s.check_points,
        s.gauge,
        s.slots,
        s.fused,
        s.acquired,
    )
}

/// 教诲选择边界：波动与状态守门
#[test]
fn advice_boundary_keeps_luck_and_state_stable() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：发布包真机帧目录不存在（Matsuri_AI）");
        return Ok(());
    };
    let mut source = FrameSource::open_replay(&dir)?;
    let mut sess = session()?;
    let mut snaps: Vec<Snap> = Vec::new();
    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind == FrameKind::FinalScore {
            sess.handle_final(&frame.contents)?;
            continue;
        }
        let out = sess.handle_frame(&frame.contents)?;
        snaps.push(snap(snaps.len(), &out, &frame.contents)?);
    }
    assert!(!snaps.is_empty(), "帧目录应有可回放帧");

    // ① 每帧还原回合与帧真值一致
    for s in &snaps {
        assert_eq!(s.turn, s.frame_turn, "帧 #{} 回合还原错位", s.idx);
    }

    let advice: Vec<usize> = snaps
        .iter()
        .filter(|s| s.kind == "advice")
        .map(|s| s.idx)
        .collect();
    assert!(!advice.is_empty(), "帧序列里应有教诲决策帧");

    println!(
        "=== 教诲选择边界诊断（搜索档 N={}，共 {} 帧 / {} 个教诲窗口）===",
        budget(),
        snaps.len(),
        advice.len()
    );
    let mut boundary_deltas = Vec::new();
    for &i in &advice {
        let a = &snaps[i];
        // ② 教诲帧契约：不挂运气分、摘要齐全、窗口表对齐、搜索根不污染输出局面
        assert!(a.luck_total.is_none(), "教诲帧 #{} 不应挂运气分", i);
        assert!(
            a.expected.is_some() && a.stdev.is_some(),
            "教诲帧 #{} 应带搜索摘要",
            i
        );
        assert!(
            WINDOWS.contains(&a.turn),
            "教诲帧 #{} 回合 {} 不在窗口表",
            i,
            a.turn
        );
        assert!(a.window.is_some(), "教诲帧 #{} 缺窗口下标", i);
        assert!(
            matches!(a.stage.as_str(), "Train" | "AfterTrain"),
            "教诲帧 #{} 输出局面阶段被搜索根污染：{}",
            i,
            a.stage
        );
        // ③ 教诲帧后一帧按该帧真值还原，不泄漏会话内补授予 / 融合
        if let Some(next) = snaps.get(i + 1) {
            assert_eq!(
                next.slots, next.frame_slots,
                "教诲帧 #{} 后一帧 #{} 的教诲槽未按帧真值还原",
                i, next.idx
            );
            assert_eq!(
                next.fused, next.frame_fused,
                "教诲帧 #{} 后一帧 #{} 的融合槽未按帧真值还原",
                i, next.idx
            );
            assert!(
                next.acquired,
                "教诲窗口之后应已获得教诲（帧 #{}）",
                next.idx
            );
        }

        let before = snaps[..i].iter().rev().find(|s| {
            s.epoch == a.epoch
                && matches!(s.kind.as_str(), "train" | "race")
                && s.expected.is_some()
        });
        let after = snaps[i + 1..].iter().find(|s| {
            s.epoch == a.epoch
                && matches!(s.kind.as_str(), "train" | "race")
                && s.expected.is_some()
        });
        println!("-- 教诲窗口 T{}（epoch {}）--", a.turn, a.epoch);
        if let Some(b) = before {
            println!("{}", line("before", b));
        }
        println!("{}", line("advice", a));
        if let Some(n) = snaps.get(i + 1) {
            println!("{}", line("next", n));
        }
        if let Some(f) = after {
            println!("{}", line("after", f));
        }
        if let (Some(b), Some(f)) = (before, after) {
            let delta = (f.expected.unwrap() - b.expected.unwrap()).abs();
            let sigma = b.stdev.unwrap().max(f.stdev.unwrap()).max(1.0);
            let luck_delta = match (b.luck_total, f.luck_total) {
                (Some(x), Some(y)) => (y - x).abs(),
                _ => f64::NAN,
            };
            boundary_deltas.push(delta);
            println!(
                "  → 选择前后 |ΔE|={delta:.0}（3σ={:.0}），|Δ本局运气|={:.0}，教诲帧自身 E={:.0}",
                3.0 * sigma,
                luck_delta,
                a.expected.unwrap()
            );
            assert!(
                delta <= 3.0 * sigma,
                "教诲边界 T{} 的期望分波动 {delta:.0} 超过 3σ={:.0}",
                a.turn,
                3.0 * sigma
            );
        }
    }

    // ④ 同局同回合的运气分快照必须逐位复用
    let mut groups: BTreeMap<(u64, i32), Vec<f64>> = BTreeMap::new();
    for s in &snaps {
        if let Some(total) = s.luck_total {
            groups.entry((s.epoch, s.turn)).or_default().push(total);
        }
    }
    for ((epoch, turn), values) in &groups {
        let first = values[0];
        assert!(
            values.iter().all(|v| (v - first).abs() < 1e-9),
            "局 {} T{} 同回合运气快照未复用：{values:?}",
            epoch,
            turn
        );
    }

    // ⑤ 全局相邻训练帧 |ΔE| 分布，用于判断教诲边界是否属于尾部异常
    let train: Vec<&Snap> = snaps
        .iter()
        .filter(|s| matches!(s.kind.as_str(), "train" | "race") && s.expected.is_some())
        .collect();
    let mut diffs: Vec<f64> = train
        .windows(2)
        .filter(|w| w[0].epoch == w[1].epoch)
        .map(|w| (w[1].expected.unwrap() - w[0].expected.unwrap()).abs())
        .collect();
    diffs.sort_by(|x, y| x.total_cmp(y));
    let q95 = diffs
        .get(
            (diffs.len() * 95 / 100)
                .saturating_sub(1)
                .min(diffs.len().saturating_sub(1)),
        )
        .copied()
        .unwrap_or(0.0);
    println!(
        "全局相邻训练帧 |ΔE|：n={} 中位={:.0} 95分位={:.0} 最大={:.0}",
        diffs.len(),
        diffs.get(diffs.len() / 2).copied().unwrap_or(0.0),
        q95,
        diffs.last().copied().unwrap_or(0.0)
    );
    println!("教诲边界 |ΔE|：{boundary_deltas:?}");

    // ⑥ 教诲窗口回合 → 下一回合（用户点名的 T2/3、T35/36、T47/48）
    println!("教诲跨回合边界（窗口回合 → +1 回合，相邻训练决策）:");
    for w in train.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a.epoch != b.epoch || b.turn != a.turn + 1 || !WINDOWS.contains(&a.turn) {
            continue;
        }
        let delta = (b.expected.unwrap() - a.expected.unwrap()).abs();
        let sigma = a.stdev.unwrap().max(b.stdev.unwrap()).max(1.0);
        let luck_delta = match (a.luck_total, b.luck_total) {
            (Some(x), Some(y)) => (y - x).abs(),
            _ => f64::NAN,
        };
        println!(
            "  局 {} T{}→T{}：|ΔE|={:.0}（3σ={:.0}，全局95分位={:.0}），|Δ本局运气|={:.0}",
            a.epoch,
            a.turn,
            b.turn,
            delta,
            3.0 * sigma,
            q95,
            luck_delta
        );
        assert!(
            delta <= 3.0 * sigma,
            "教诲跨回合边界 T{}→T{} 的 |ΔE|={delta:.0} 超过 3σ={:.0}",
            a.turn,
            b.turn,
            3.0 * sigma
        );
    }
    Ok(())
}
