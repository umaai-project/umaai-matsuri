//! 真机帧回放：把 game6308/6309/6310 的 `game{id}_turn{n}.json` 逐帧喂进在线会话
//!
//! 这一层是**端到端守门**：帧解析 → 还原（编成 / 人头 / 分布门禁）→ 路由 → 决策。
//! 帧目录来自仓外（`../../Matsuri_Ai_Test/.portable/PluginData/SendGameStatusPlugin`），
//! 不存在时整组跳过（不把机器路径变成测试硬依赖）。
//!
//! 断言口径：
//! - 78 帧**全部还原成功**（任何一帧还原失败都说明帧口径与还原层对不上）；
//! - 决策只出现在 `Train` / 事件帧上，`special` 中间态与重复帧必须跳过；
//! - `--verify-training` 的偏差表作为**报告**输出（不当作失败条件：`G13` 只验算）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind, TurnKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{MatsuriConfig, MatsuriData};
use umaai_core::Data;
use umasim::game::Game;

/// 真机帧目录（仓外夹具）
fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_Ai_Test/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

/// 会话装配（数据根 = 仓内 `data/`）
fn session(trainer: TrainerKind, verify: bool) -> Result<OnlineSession> {
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".").context("载入剧本配置")?;
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据")?);
    OnlineSession::new(&cfg, data, matsuri_cfg, trainer, verify)
}

#[test]
fn replay_real_frames() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在（仓外夹具 Matsuri_Ai_Test）");
        return Ok(());
    };
    let mut source = FrameSource::open_replay(&dir)?;
    let mut session = session(TrainerKind::Hand, true)?;

    let mut frames = 0usize;
    let mut decisions = 0usize;
    let mut skipped = 0usize;
    let mut train_kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut max_deviation = 0i32;
    let mut deviation_frames = 0usize;
    // 逐轴统计：偏差行数 / 绝对偏差和（速/耐/力/根/智/PT/体力）
    let mut rows_total = 0usize;
    let mut rows_clean = 0usize;
    let mut dirty_by_axis = [0usize; 7];
    let mut signatures: std::collections::BTreeMap<Vec<i32>, usize> = Default::default();
    let mut worst: (i32, usize, Vec<i32>, Vec<i32>) = (0, 0, vec![], vec![]);
    // 逐行最大偏差分桶：0 / 1 / 2 / 3–5 / 6–10 / >10
    let mut buckets = [0usize; 6];
    // (回合, 训练位, 稽古, 帧体力, 我体力, 体力减免%, 我下层体力)
    let mut vital_samples: Vec<(i32, usize, bool, i32, i32, f32, i32)> = Vec::new();
    // (回合, 训练位, 稽古, |Δ|max, Δ, 闪彩数, 友情%)
    let mut outliers: Vec<(i32, usize, bool, i32, Vec<i32>, usize, f32)> = Vec::new();
    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        frames += 1;
        let output = session
            .handle_frame(&frame.contents)
            .with_context(|| format!("帧 #{frames} 处理失败（{} 字符）", frame.contents.len()))?;
        match output.kind {
            TurnKind::Decision => {
                decisions += 1;
                let kind = output
                    .record
                    .as_ref()
                    .map(|record| record.kind.clone())
                    .unwrap_or_default();
                *train_kinds.entry(kind).or_default() += 1;
            }
            TurnKind::Skipped => {
                skipped += 1;
                assert!(
                    output.record.is_none(),
                    "跳过帧不应带决策记录（T{}）",
                    output.turn
                );
            }
        }
        if let Some(report) = &output.verify {
            max_deviation = max_deviation.max(report.max_abs_delta());
            if report.has_deviation() {
                deviation_frames += 1;
            }
            for row in &report.rows {
                rows_total += 1;
                if row.delta.get(6).copied().unwrap_or(0) != 0 {
                    let buffs = output.game.calc_training_buff(row.train)?;
                    let layers = output.game.preview_training_layers(row.train, row.keiko)?;
                    vital_samples.push((
                        output.turn,
                        row.train,
                        row.keiko,
                        row.frame.get(6).copied().unwrap_or(0),
                        row.preview.get(6).copied().unwrap_or(0),
                        buffs.vital_cost_drop,
                        layers.lower.vital,
                    ));
                }
                let row_max = row.delta.iter().map(|d| d.abs()).max().unwrap_or(0);
                if row_max > worst.3.iter().map(|d| d.abs()).max().unwrap_or(0) {
                    worst = (
                        output.turn,
                        row.train,
                        row.frame.clone(),
                        row.preview.clone(),
                    );
                }
                if row.delta.get(6).copied().unwrap_or(0) != 0 {
                    let buffs = output.game.calc_training_buff(row.train)?;
                    let layers = output.game.preview_training_layers(row.train, row.keiko)?;
                    vital_samples.push((
                        output.turn,
                        row.train,
                        row.keiko,
                        row.frame.get(6).copied().unwrap_or(0),
                        row.preview.get(6).copied().unwrap_or(0),
                        buffs.vital_cost_drop,
                        layers.lower.vital,
                    ));
                }
                let row_max = row.delta.iter().map(|d| d.abs()).max().unwrap_or(0);
                buckets[match row_max {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3..=5 => 3,
                    6..=10 => 4,
                    _ => 5,
                }] += 1;
                if row_max > 2 {
                    let layers = output.game.preview_training_layers(row.train, row.keiko)?;
                    let buffs = output.game.calc_training_buff(row.train)?;
                    outliers.push((
                        output.turn,
                        row.train,
                        row.keiko,
                        row_max,
                        row.delta.clone(),
                        layers.shining,
                        buffs.youqing,
                    ));
                }
                if row.is_match() {
                    rows_clean += 1;
                } else {
                    *signatures.entry(row.delta.clone()).or_default() += 1;
                    for (axis, delta) in row.delta.iter().enumerate() {
                        if *delta != 0 {
                            dirty_by_axis[axis] += 1;
                        }
                    }
                }
            }
            // 排查用：`MATSURI_VERIFY_DUMP=1` 时逐项打印偏差（默认静默）
            if std::env::var("MATSURI_VERIFY_DUMP").is_ok() {
                for row in &report.rows {
                    println!(
                        "局{} T{} 训练位{}（{}，cmd {}）帧={:?} 预览={:?} 差={:?} 失败率 帧{:.1} 预览{:.1}",
                        output.epoch,
                        output.turn,
                        row.train,
                        if row.keiko { "稽古" } else { "普通" },
                        row.command_id,
                        row.frame,
                        row.preview,
                        row.delta,
                        row.frame_failure_rate,
                        row.preview_failure_rate
                    );
                }
            }
        }
    }

    // 报告（不是断言：真机帧的偏差要人看）
    println!(
        "回放 {frames} 帧：决策 {decisions}（{train_kinds:?}） / 跳过 {skipped}；\
         训练验算有偏差的帧 {deviation_frames}，最大偏差 {max_deviation}"
    );
    println!(
        "训练行 {rows_total} 条：逐项全等 {rows_clean} 条（{:.0}%），带偏差 {} 条；\
         按轴统计（速/耐/力/根/智/PT/体力）{dirty_by_axis:?}",
        (rows_clean as f64) * 100.0 / (rows_total.max(1) as f64),
        rows_total - rows_clean
    );
    let mut top: Vec<(Vec<i32>, usize)> = signatures.into_iter().collect();
    top.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    println!(
        "量级分布（每行取最大 |Δ|）：0:{0} 1:{1} 2:{2} 3–5:{3} 6–10:{4} >10:{5}",
        buckets[0], buckets[1], buckets[2], buckets[3], buckets[4], buckets[5]
    );
    let mut sorted = outliers.clone();
    sorted.sort_by_key(|(_, _, _, worst, _, _, _)| std::cmp::Reverse(*worst));
    println!("超 2 的 {} 条（前 12）：", outliers.len());
    for (turn, train, keiko, worst, delta, shining, youqing) in sorted.into_iter().take(12) {
        println!(
            "   T{turn} 训练位{train}{} 闪彩{shining} 友情{youqing:.0}% |Δ|max={worst} Δ{delta:?}",
            if keiko { "·稽古" } else { "" }
        );
    }
    let multi: Vec<&(i32, usize, bool, i32, Vec<i32>, usize, f32)> =
        outliers.iter().filter(|row| row.5 >= 2).collect();
    println!(
        "超 2 的行里：闪彩≥2 的 {} 条 / 闪彩≤1 的 {} 条",
        multi.len(),
        outliers.len() - multi.len()
    );
    // 体力偏差样本（W1 排查）
    let vital_rows = vital_samples.clone();
    println!("体力偏差样本 {} 条（前 8）：", vital_rows.len());
    for (turn, train, keiko, frame_v, mine_v, drop, lower_v) in vital_rows.into_iter().take(8) {
        println!(
            "   T{turn} 训练位{train}{} 帧体力{frame_v} 我体力{mine_v} 体力减免{drop:.0}% 我下层{lower_v}",
            if keiko { "·稽古" } else { "" }
        );
    }
    println!(
        "最大偏差行：T{} 训练位{} 帧{:?} 我{:?}",
        worst.0, worst.1, worst.2, worst.3
    );
    println!("偏差签名 Top8（Δ速/耐/力/根/智/PT/体力 × 次数）：");
    for (delta, count) in top.into_iter().take(8) {
        println!("   {delta:?} × {count}");
    }

    assert!(frames >= 70, "真机帧应有 78 帧左右，实际 {frames}");
    assert!(decisions >= 20, "决策帧过少，实际 {decisions}");
    assert!(
        train_kinds.contains_key("train") || train_kinds.contains_key("race"),
        "应至少有一帧训练 / 比赛决策"
    );
    Ok(())
}

/// 同内容重复帧只决策一次（`G7`）
#[test]
fn replay_is_idempotent_for_same_content() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在（仓外夹具 Matsuri_Ai_Test）");
        return Ok(());
    };
    let path = dir.join("game6310_turn40.json");
    if !path.is_file() {
        eprintln!("跳过：缺 game6310_turn40.json");
        return Ok(());
    }
    let contents = std::fs::read_to_string(&path)?;
    let mut session = session(TrainerKind::Hand, false)?;
    let first = session.handle_frame(&contents)?;
    assert_eq!(first.kind, TurnKind::Decision, "首帧应出决策");
    let second = session.handle_frame(&contents)?;
    assert_eq!(second.kind, TurnKind::Skipped, "重复帧应跳过");
    assert!(
        second
            .skip_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("重复帧")),
        "跳过原因应指明重复帧"
    );
    Ok(())
}
