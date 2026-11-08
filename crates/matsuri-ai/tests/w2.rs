//! W2 反解：把下层每一行的「乘子积」与真机帧反解出的乘子积对照，定位偏差落在哪个乘子
//!
//! `cargo test -p matsuri-ai --test w2 -- --nocapture`
//!
//! 口径（§9.8 ②最终版）：帧 `training` 永远是**表示値**（`ReviseOver1200`），本仓模型持有
//! 未减半真值。对账前先用 [`matsuri_core::restore_displayed_gain`] 把帧值还原成真值。
//! T0 载入快照（预览非零 + 分布全空）不参与统计。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{MatsuriConfig, MatsuriData, TrainingBreakdown};
use umaai_core::Data;

/// 三局全量回放（6308 / 6309 / 6310；友情圈最多、偏差最集中的合宿期都在里面）
const GAMES: [u64; 3] = [6308, 6309, 6310];
/// 只看这些回合
const TURNS: [i32; 0] = [];

fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_Ai_Test/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

/// 引擎下层的乘子积（六轴共用，除 `status_bonus` 逐轴）
fn factor_of(b: &TrainingBreakdown, axis: usize) -> f64 {
    let motivation_gain = f64::from((b.motivation - 3) * 10);
    (1.0 + f64::from(b.youqing) / 100.0)
        * (1.0 + 0.01 * motivation_gain * (1.0 + f64::from(b.ganjing) / 100.0))
        * (1.0 + f64::from(b.xunlian) / 100.0)
        * (1.0 + 0.05 * f64::from(b.person_count))
        * (1.0 + f64::from(b.status_bonus[axis]) / 100.0)
}

#[test]
fn reverse_engineer_lower_multiplier() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在");
        return Ok(());
    };
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".")?;
    let data = Arc::new(MatsuriData::load(&root)?);
    let mut session = OnlineSession::new(
        &cfg,
        Arc::clone(&data),
        matsuri_cfg,
        TrainerKind::Hand,
        false,
    )?;
    let mut source = FrameSource::open_replay(&dir)?;

    let mut stats = Stats::default();
    println!(
        "\n{:<6} {:>4} {:>3} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>8}",
        "回合", "cmd", "位", "轴", "帧", "我", "当前", "上限", "余量", "隐含真值"
    );
    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind != umaai::FrameKind::ThisTurn {
            continue;
        }
        let parsed = matsuri_ai::ParsedFrame::parse(&frame.contents)?;
        if !GAMES.contains(&parsed.epoch_key()) {
            continue;
        }
        let output = session.handle_frame(&frame.contents)?;
        let turn = output.turn;
        // 幂等早退 / 跨局时 `output.game` 可能是别的帧的局面：必须与帧自洽才拿来比
        if output.epoch != parsed.epoch_key() || turn != parsed.status.turn {
            continue;
        }
        // 分布未捕获（T0 载入快照）：还原态人数恒 0，比了也是假偏差
        if !matsuri_ai::online::distribution_captured(&output.game) {
            continue;
        }
        if (!TURNS.is_empty() && !TURNS.contains(&turn)) || parsed.training.is_empty() {
            continue;
        }
        let mut keys: Vec<&String> = parsed.training.keys().collect();
        keys.sort();
        // 合宿期帧同时带普通训练与稽古两套预览：普通训练那套是「不可用」的占位值，
        // 统计时只取稽古行，否则会把「游戏压小的占位值」当成公式误差
        let camp = keys.iter().any(|key| {
            key.parse::<i32>()
                .is_ok_and(|id| matsuri_ai::online::command_to_train(id).is_some_and(|(_, k)| k))
        });
        for key in keys {
            let Ok(command_id) = key.parse::<i32>() else {
                continue;
            };
            let Some((train, keiko)) = matsuri_ai::online::command_to_train(command_id) else {
                continue;
            };
            if camp != keiko {
                continue;
            }
            let row = &parsed.training[key];
            let b = output.game.training_breakdown(train, keiko)?;
            // 逐轴比：只取该轴帧值 >0 且该轴当前值 <1200（否则帧值是折半表示値）
            // 整行合计对比（含裁半还原），只统计不打印
            {
                let layers = output.game.preview_training_layers(train, keiko)?;
                let mut frame_total = row.total();
                for axis in 0..5 {
                    frame_total[axis] = matsuri_core::restore_displayed_gain(
                        output.game.uma.five_status[axis],
                        frame_total[axis],
                    );
                }
                if frame_total.iter().any(|v| *v != 0) {
                    let delta: Vec<i32> = (0..6)
                        .map(|axis| layers.total.status_pt[axis] - frame_total[axis])
                        .collect();
                    let worst = delta.iter().map(|d| d.abs()).max().unwrap_or(0);
                    stats.rows += 1;
                    if worst == 0 {
                        stats.clean += 1;
                    } else {
                        stats.dirty += 1;
                        if worst > 2 {
                            stats
                                .bad
                                .push((turn, command_id, delta.clone(), frame_total.clone()));
                        }
                    }
                }
            }
            for axis in 0..5 {
                let frame = row.lower[axis];
                let mine = output
                    .game
                    .preview_training_layers(train, keiko)?
                    .lower
                    .status_pt[axis];
                if frame <= 0 || mine <= 0 {
                    continue;
                }

                let base = f64::from(b.basic_row[axis] + b.bonus[axis]);
                if base <= 0.0 {
                    continue;
                }
                if command_id == 101 || command_id == 5101 {
                    let full = output.game.preview_training_layers(train, keiko)?;
                    println!(
                        "  [层] T{turn} cmd{command_id} 帧L{:?} U{:?} 我L{:?} U{:?} 帧总{:?} 我总{:?} 裁{:?}",
                        &row.lower[..5],
                        &row.upper[..5],
                        &full.lower.status_pt[..5],
                        &full.upper.status_pt[..5],
                        &row.total()[..6],
                        &full.total.status_pt[..6],
                        full.clip
                    );
                    println!("  [快照] {b:?}");
                }
                let cur = output.game.uma.five_status[axis];
                let lim = output.game.uma.five_status_limit[axis];
                // 帧值：属性 ≥1200 时为表示値（折半），还原成真值
                let real_frame = matsuri_core::restore_displayed_gain(cur, frame);
                let _ = factor_of(&b, axis);
                println!(
                    "{:<6} {:>4} {:>3} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>8}",
                    turn,
                    command_id,
                    train,
                    ["速", "耐", "力", "根", "智"][axis],
                    frame,
                    mine,
                    cur,
                    lim,
                    (lim - cur).max(0),
                    real_frame,
                );
            }
        }
    }
    println!(
        "\n合计 {} 行：逐项全等 {} / 有偏差 {} / Δmax>2 {} 条",
        stats.rows,
        stats.clean,
        stats.dirty,
        stats.bad.len()
    );
    let mut sorted = stats.bad.clone();
    sorted.sort_by_key(|(_, _, d, _)| {
        std::cmp::Reverse(d.iter().map(|v| v.abs()).max().unwrap_or(0))
    });
    for (turn, cmd, delta, frame) in sorted.into_iter().take(12) {
        let ratio = delta
            .iter()
            .zip(&frame)
            .map(|(d, f)| if *f > 0 { *d as f64 / *f as f64 } else { 0.0 })
            .fold(0.0f64, |acc, r| if r.abs() > acc.abs() { r } else { acc });
        println!(
            "   超差 T{turn} cmd{cmd} Δ{delta:?} 帧{frame:?} 比例{:.0}%",
            ratio * 100.0
        );
    }
    Ok(())
}

#[derive(Default)]
struct Stats {
    rows: usize,
    clean: usize,
    dirty: usize,
    bad: Vec<(i32, i32, Vec<i32>, Vec<i32>)>,
}

/// 训练基础值求解器：对指定训练位的指定轴，试 `basic ∈ 14..=26`，
/// 看哪个基准值能让整局的帧 `lower` 全部对上
///
/// 预测口径：`lower = floor((basic + bonus) × 乘子积)`，帧值是**表示値**
/// （属性 ≥1200 时按 `floor(实际 / 2)` 给出），所以比较前先折半。
#[test]
fn solve_basic_value() -> Result<()> {
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在");
        return Ok(());
    };
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".")?;
    let data = Arc::new(MatsuriData::load(&root)?);
    let mut session = OnlineSession::new(
        &cfg,
        Arc::clone(&data),
        matsuri_cfg,
        TrainerKind::Hand,
        false,
    )?;
    let source_dir = dir;
    for game in [6310u64, 6309u64, 6308u64] {
        let mut source = FrameSource::open_replay(&source_dir)?;
        // (训练位, 轴) → [(帧原始 lower, bonus, 乘子积, 是否折半)]
        let mut samples: Vec<Sample> = Vec::new();
        while let Some(frame) = source.next_frame(Duration::ZERO)? {
            if frame.kind != umaai::FrameKind::ThisTurn {
                continue;
            }
            let parsed = matsuri_ai::ParsedFrame::parse(&frame.contents)?;
            if parsed.epoch_key() != game {
                continue;
            }
            let output = session.handle_frame(&frame.contents)?;
            if output.epoch != parsed.epoch_key() || output.turn != parsed.status.turn {
                continue;
            }
            // 分布未捕获（T0 载入快照）：乘子积失真，样本作废
            if !matsuri_ai::online::distribution_captured(&output.game) {
                continue;
            }
            if parsed.training.is_empty() {
                continue;
            }
            let mut keys: Vec<&String> = parsed.training.keys().collect();
            keys.sort();
            let camp = keys.iter().any(|key| {
                key.parse::<i32>().is_ok_and(|id| {
                    matsuri_ai::online::command_to_train(id).is_some_and(|(_, k)| k)
                })
            });
            for key in keys {
                let Ok(command_id) = key.parse::<i32>() else {
                    continue;
                };
                let Some((train, keiko)) = matsuri_ai::online::command_to_train(command_id) else {
                    continue;
                };
                if camp != keiko {
                    continue;
                }
                let row = &parsed.training[key];
                let Ok(b) = output.game.training_breakdown(train, keiko) else {
                    continue;
                };
                for axis in 0..5 {
                    let raw = row.lower[axis];
                    if raw <= 0 {
                        continue;
                    }
                    let cur = output.game.uma.five_status[axis];
                    samples.push((
                        output.turn,
                        train,
                        axis,
                        raw,
                        b.bonus[axis],
                        factor_of(&b, axis),
                        cur >= matsuri_core::DISPLAY_HALF_FROM_STATUS,
                        keiko,
                        b.basic_row[axis],
                    ));
                }
            }
        }
        if samples.is_empty() {
            continue;
        }
        // 只看「速训练位 + 速轴」（已知唯一对不上的组合），也顺带看别的位做对照
        // 按「训练位 / 轴 / 稽古 / 当前基础值」分组：junior 与 classic 是两套表，混在一起没有意义
        let mut groups: std::collections::BTreeMap<GroupKey, Vec<&Sample>> = Default::default();
        for row in &samples {
            groups
                .entry((row.1, row.2, row.7, row.8))
                .or_default()
                .push(row);
        }
        for ((train, axis, keiko, current), rows) in groups {
            let label = format!(
                "位{train}/轴{axis}/{}/现值{current}",
                if keiko { "稽古" } else { "普通" }
            );
            // 已知唯一对不上的组合：逐行看「反解需要的基础值」
            if train == 0 && axis == 0 && keiko && current == 21 {
                println!("  [逐行反解] 局{game} 位0/轴0/稽古（classic）");
                for (turn, _, _, raw, bonus, factor, halved, _, _) in &rows {
                    let real_low = if *halved { raw * 2 } else { *raw };
                    let needed = (f64::from(real_low) / factor) - f64::from(*bonus);
                    println!(
                        "     T{turn} 帧{raw}{} 乘子积{factor:.3} bonus{bonus} → 需要 basic≈{needed:.1}",
                        if *halved { "（折半）" } else { "" }
                    );
                }
            }
            if rows.len() < 3 {
                continue;
            }
            let mut best: Vec<(usize, i32)> = Vec::new();
            for basic in 10i32..=30 {
                let hits = rows
                    .iter()
                    .filter(|(_, _, _, raw, bonus, factor, halved, _, _)| {
                        let real = ((f64::from(basic + bonus) * factor) as i32).max(0);
                        let shown = if *halved { real / 2 } else { real };
                        (shown - raw).abs() <= 1
                    })
                    .count();
                best.push((hits, basic));
            }
            best.sort_by_key(|(hits, _)| std::cmp::Reverse(*hits));
            let top: Vec<String> = best
                .iter()
                .take(3)
                .map(|(hits, basic)| format!("basic={basic} 命中{hits}/{}", rows.len()))
                .collect();
            println!("局{game} {label}：{}", top.join("，"));
        }
    }
    Ok(())
}

/// 一条训练行样本：`(回合, 训练位, 轴, 帧原值, bonus, 乘子积, 是否折半, 稽古, 当前基础值)`
type Sample = (i32, usize, usize, i32, i32, f64, bool, bool, i32);

/// 分组键：`(训练位, 轴, 是否稽古, 当前基础值)`
type GroupKey = (usize, usize, bool, i32);

/// 剩余超差逐条拆解：帧两层 / 本仓两层 / 未裁下层 / 六位乘子 / 参与人头
///
/// `cargo test -p matsuri-ai --test w2 -- --nocapture dump_outliers`
#[test]
fn dump_outliers() -> Result<()> {
    /// 已知的超差点：`(回合, 指令)`
    const CASES: [(i32, i32); 8] = [
        (51, 5103),
        (30, 101),
        (33, 5104),
        (74, 5102),
        (54, 5102),
        (54, 5104),
        (72, 5101),
        (0, 102),
    ];
    let Some(dir) = frames_dir() else {
        eprintln!("跳过：真机帧目录不存在");
        return Ok(());
    };
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".")?;
    let data = Arc::new(MatsuriData::load(&root)?);
    let mut session = OnlineSession::new(
        &cfg,
        Arc::clone(&data),
        matsuri_cfg,
        TrainerKind::Hand,
        false,
    )?;
    let mut source = FrameSource::open_replay(&dir)?;

    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind != umaai::FrameKind::ThisTurn {
            continue;
        }
        let parsed = matsuri_ai::ParsedFrame::parse(&frame.contents)?;
        if !(6308..=6310).contains(&parsed.epoch_key()) {
            continue;
        }
        let output = session.handle_frame(&frame.contents)?;
        if output.epoch != parsed.epoch_key() || output.turn != parsed.status.turn {
            continue;
        }
        // 分布未捕获（T0 载入快照）：还原态人数恒 0，拆解无意义
        if !matsuri_ai::online::distribution_captured(&output.game) {
            continue;
        }
        if parsed.training.is_empty() {
            continue;
        }
        let mut keys: Vec<&String> = parsed.training.keys().collect();
        keys.sort();
        let camp = keys.iter().any(|key| {
            key.parse::<i32>()
                .is_ok_and(|id| matsuri_ai::online::command_to_train(id).is_some_and(|(_, k)| k))
        });
        for key in keys {
            let Ok(command_id) = key.parse::<i32>() else {
                continue;
            };
            let Some((train, keiko)) = matsuri_ai::online::command_to_train(command_id) else {
                continue;
            };
            if camp != keiko {
                continue;
            }
            if !CASES.contains(&(output.turn, command_id)) {
                continue;
            }
            let row = &parsed.training[key];
            let layers = output.game.preview_training_layers(train, keiko)?;
            let b = output.game.training_breakdown(train, keiko)?;
            let restore = |values: &[i32]| -> Vec<i32> {
                values
                    .iter()
                    .enumerate()
                    .map(|(axis, value)| {
                        if axis < 5 {
                            matsuri_core::restore_displayed_gain(
                                output.game.uma.five_status[axis],
                                *value,
                            )
                        } else {
                            *value
                        }
                    })
                    .collect()
            };
            println!(
                "\n===== 局{} T{} cmd{} 训练位{}·{}",
                output.epoch,
                output.turn,
                command_id,
                train,
                if keiko { "稽古" } else { "普通" }
            );
            println!(
                "  属  当前{:?} 上限{:?}",
                output.game.uma.five_status, output.game.uma.five_status_limit
            );
            println!(
                "  帧  lower{:?} upper{:?} 合计{:?}",
                &row.lower[..6],
                &row.upper[..6],
                &row.total()[..6]
            );
            println!(
                "  帧  还原后 lower{:?} upper{:?} 合计{:?}",
                &restore(&row.lower)[..6],
                &restore(&row.upper)[..6],
                &restore(&row.total())[..6]
            );
            println!(
                "  我  下层{:?} 上层{:?} 合计{:?}",
                &layers.lower.status_pt[..6],
                &layers.upper.status_pt[..6],
                &layers.total.status_pt[..6]
            );
            println!(
                "  裁  下层未裁{:?} → 裁后{:?}（上限 {}）；属性余量裁掉{:?}",
                b.lower_raw,
                b.lower_clipped,
                output.game.data.rules.training_upper_cap,
                layers.clip
            );
            println!(
                "  因  基础{:?} 副属性+{:?} 成长{:?} 人数{} 友情{:.0}% 训练{} 干劲{}%（档{}）减免{:.0}%",
                &b.basic_row[..6],
                &b.bonus[..6],
                &b.status_bonus[..6],
                b.person_count,
                b.youqing,
                b.xunlian,
                b.ganjing,
                b.motivation,
                b.vital_cost_drop
            );
            println!(
                "  层  発表会{} 協力{} 演目{} 稽古{}=>{} 效果{} 主获得{} 副{} cap{} flat{}",
                b.friendship_presentation,
                b.friendship_cooperate,
                b.friendship_act,
                b.friendship_keiko,
                b.friendship_presentation
                    + b.friendship_cooperate
                    + b.friendship_act
                    + b.friendship_keiko,
                b.training_effect,
                b.main_gain,
                b.sub_gain,
                b.status_cap,
                b.keiko_flat
            );
            println!(
                "  PT  下层{} 原始增益{} 上限{}（五维上限 {}）",
                b.lower_clipped[5], b.sp_gain_raw, b.sp_cap, b.status_cap
            );
            let people: Vec<String> = output.view.board.distribution[train]
                .iter()
                .map(|member| {
                    let origin = output.game.persons[member.person].origin;
                    let friendship = output.game.persons[member.person].friendship();
                    format!(
                        "{origin:?}/羁绊{friendship}{}",
                        if member.shining { "/★" } else { "" }
                    )
                })
                .collect();
            println!("  人  {}", people.join("  "));
        }
    }
    Ok(())
}
