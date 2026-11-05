//! 开局预设扫参：blue_count × extra_count 的枚举与评估
//!
//! 用法（仓根执行，先 cargo build --release --offline -p matsuri-tools）：
//! - sweep enum --uma 105902 --games 5 --keep 100000 --out top.csv
//!   枚举全部开局预设（蓝因子 210 × 剧本因子 33160），每配置跑 --games 个种子，保留均值前 --keep。
//! - sweep eval --uma 105902 --games 300 --in list.csv --out result.csv [--scores scores.csv]
//!   评估给定配置列表；--scores 逐种子落盘，便于配对比较。
//! - sweep uma --games 3000 --seed 67200 [--out ranking.csv]
//!   固定当前开局预设，逐马娘跑手写档，按评分均值排序（特殊机制马娘单独标注）。
//!
//! 配置行格式：b0,b1,b2,b3,b4,x0,x1,x2,x3,x4,x5（与 data/ai_config.toml 的
//! sim.blue_count / sim.extra_count 同口径）。种子固定为 --seed 起的连续块，
//! 所有配置共用同一块种子（CRN），配置间可直接配对比较。

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriTrainer, build_game};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umasim::game::Game;

/// 仓内数据根（相对仓根）
const DATA_ROOT: &str = "data";
/// 蓝因子总格数（blue_count 以 3 为单位，总数 18）
const BLUE_UNITS: i32 = 6;
/// 蓝因子单位步长
const BLUE_STEP: i32 = 3;
/// 剧本因子五维总格数（以 10 为单位，总数 280）
const EXTRA_UNITS: i32 = 28;
/// 剧本因子单位步长
const EXTRA_STEP: i32 = 10;
/// 单属性剧本因子上限（14 轮 ×10）
const EXTRA_MAX: i32 = 140;
/// 技能点因子（不参与五维分配，沿用默认 100）
const EXTRA_PT: i32 = 100;

/// 一次游戏的终局观测
struct Outcome {
    score: i32,
    pt: i32,
}

/// 装载数据与配置（每进程一次）
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
        let mut config =
            MatsuriConfig::load_with_overlay_file(&data_root, ".").context("载入剧本配置失败")?;
        // 本工具只测手写档：默认层已开搜索，这里就地关掉
        if config.search.search_n != 0 {
            eprintln!(
                "# 手写档工具：忽略 search_n={}，本次按 0 跑",
                config.search.search_n
            );
            config.search.search_n = 0;
        }
        Ok(Self {
            engine,
            data,
            config,
        })
    }
}

/// 枚举蓝因子组合（6 格分到 5 个属性，每格 3）
fn blue_combos() -> Vec<[i32; 5]> {
    let mut out = Vec::new();
    for a in 0..=BLUE_UNITS {
        for b in 0..=(BLUE_UNITS - a) {
            for c in 0..=(BLUE_UNITS - a - b) {
                for d in 0..=(BLUE_UNITS - a - b - c) {
                    let e = BLUE_UNITS - a - b - c - d;
                    out.push([
                        a * BLUE_STEP,
                        b * BLUE_STEP,
                        c * BLUE_STEP,
                        d * BLUE_STEP,
                        e * BLUE_STEP,
                    ]);
                }
            }
        }
    }
    out
}

/// 枚举剧本因子组合（28 格分到 5 个属性，每格 10，单属性至多 140）
fn extra_combos() -> Vec<[i32; 6]> {
    let mut out = Vec::new();
    for a in 0..=EXTRA_UNITS {
        for b in 0..=(EXTRA_UNITS - a) {
            for c in 0..=(EXTRA_UNITS - a - b) {
                for d in 0..=(EXTRA_UNITS - a - b - c) {
                    let e = EXTRA_UNITS - a - b - c - d;
                    let mut arr = [a, b, c, d, e];
                    if arr.iter().any(|v| v * EXTRA_STEP > EXTRA_MAX) {
                        continue;
                    }
                    for v in arr.iter_mut() {
                        *v *= EXTRA_STEP;
                    }
                    out.push([arr[0], arr[1], arr[2], arr[3], arr[4], EXTRA_PT]);
                }
            }
        }
    }
    out
}

/// 单配置在种子块上的评估结果
struct Eval {
    sum: i64,
    pt_sum: i64,
    /// 逐种子评分（--scores 落盘用）
    scores: Vec<i32>,
}

/// 枚举阶段单个线程的产出
struct ThreadResult {
    heap: BinaryHeap<Reverse<(i64, usize)>>,
    count: u64,
    sum: f64,
    sumsq: f64,
    games: u64,
}

/// 解析参数：--name value
fn parse_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

/// 解析整型参数
fn parse_usize(args: &[String], name: &str, default: usize) -> Result<usize> {
    match parse_arg(args, name) {
        Some(raw) => raw.parse().with_context(|| format!("{name} 非法: {raw}")),
        None => Ok(default),
    }
}

/// 解析必填路径参数
fn require_arg(args: &[String], name: &str) -> Result<String> {
    parse_arg(args, name).with_context(|| format!("缺少必填参数 {name}"))
}

/// 线程数：0 表示用满可用核
fn resolve_threads(args: &[String]) -> Result<usize> {
    let threads = parse_usize(args, "--threads", 0)?;
    Ok(if threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        threads
    })
}

/// 枚举全部开局预设，保留均值前 keep 的配置
fn cmd_enum(env: &Env, args: &[String]) -> Result<()> {
    let uma = parse_usize(args, "--uma", 105902)? as u32;
    let games = parse_usize(args, "--games", 5)?;
    let keep = parse_usize(args, "--keep", 100_000)?;
    let seed = parse_usize(args, "--seed", 70_000)? as u64;
    let threads = resolve_threads(args)?;
    let out_path = require_arg(args, "--out")?;
    if games == 0 || keep == 0 {
        bail!("--games 与 --keep 必须 ≥ 1");
    }

    let blues = blue_combos();
    let extras = extra_combos();
    let total = blues.len() * extras.len();
    eprintln!(
        "# enum uma={uma} blue={} extra={} total={total} games={games} keep={keep} seed={seed} threads={threads}",
        blues.len(),
        extras.len()
    );

    let next = AtomicUsize::new(0);
    let started = Instant::now();
    let done = AtomicUsize::new(0);
    let chunk = 1024usize;
    let mut results = Vec::with_capacity(threads);

    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let next = &next;
            let done = &done;
            let blues = &blues;
            let extras = &extras;
            handles.push(scope.spawn(move || -> Result<ThreadResult> {
                let mut engine = env.engine.clone();
                engine.sim.uma = uma;
                let mut trainer = MatsuriTrainer::new(env.config.clone());
                let mut heap: BinaryHeap<Reverse<(i64, usize)>> =
                    BinaryHeap::with_capacity(keep + 1);
                let mut count = 0u64;
                let mut sum = 0.0f64;
                let mut sumsq = 0.0f64;
                let mut evaluated = 0u64;
                loop {
                    let start = next.fetch_add(chunk, Ordering::Relaxed);
                    if start >= total {
                        break;
                    }
                    let end = (start + chunk).min(total);
                    for index in start..end {
                        let (b, e) = (index / extras.len(), index % extras.len());
                        let (blue, extra) = (&blues[b], &extras[e]);
                        let mut conf_sum = 0i64;
                        let mut ok = true;
                        for k in 0..games {
                            let run_seed = seed + k as u64;
                            match play_env(env, &mut engine, blue, extra, run_seed, &mut trainer) {
                                Ok(value) => conf_sum += value.score as i64,
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                            evaluated += 1;
                        }
                        if !ok {
                            continue;
                        }
                        count += 1;
                        let mean = conf_sum as f64 / games as f64;
                        sum += mean;
                        sumsq += mean * mean;
                        if heap.len() < keep {
                            heap.push(Reverse((conf_sum, index)));
                        } else if conf_sum > heap.peek().map(|v| v.0.0).unwrap_or(i64::MIN) {
                            heap.pop();
                            heap.push(Reverse((conf_sum, index)));
                        }
                    }
                    done.fetch_add(end - start, Ordering::Relaxed);
                }
                Ok(ThreadResult {
                    heap,
                    count,
                    sum,
                    sumsq,
                    games: evaluated,
                })
            }));
        }
        for handle in handles {
            results.push(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("工作线程 panic"))??,
            );
        }
        Ok(())
    })?;

    let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::with_capacity(keep + 1);
    let (mut count, mut sum, mut sumsq, mut games_run) = (0u64, 0.0f64, 0.0f64, 0u64);
    for result in results {
        count += result.count;
        sum += result.sum;
        sumsq += result.sumsq;
        games_run += result.games;
        for item in result.heap {
            if heap.len() < keep {
                heap.push(item);
            } else if item.0.0 > heap.peek().map(|v| v.0.0).unwrap_or(i64::MIN) {
                heap.pop();
                heap.push(item);
            }
        }
    }

    let mut rows: Vec<(i64, usize)> = heap.into_iter().map(|Reverse(v)| v).collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let elapsed = started.elapsed().as_secs_f64();
    let mean_all = sum / count.max(1) as f64;
    let var_all = (sumsq / count.max(1) as f64 - mean_all * mean_all).max(0.0);
    eprintln!(
        "# done configs={count} games={games_run} elapsed={elapsed:.1}s rate={:.0} games/s config_mean: mean={mean_all:.1} std={:.1}",
        games_run as f64 / elapsed.max(1e-9),
        var_all.sqrt()
    );

    let file = File::create(&out_path).with_context(|| format!("创建输出失败: {out_path}"))?;
    let mut out = BufWriter::new(file);
    writeln!(out, "rank,b0,b1,b2,b3,b4,x0,x1,x2,x3,x4,x5,mean,sum")?;
    for (rank, (conf_sum, index)) in rows.iter().enumerate() {
        let (b, e) = (index / extras.len(), index % extras.len());
        let blue = &blues[b];
        let extra = &extras[e];
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.3},{}",
            rank + 1,
            blue[0],
            blue[1],
            blue[2],
            blue[3],
            blue[4],
            extra[0],
            extra[1],
            extra[2],
            extra[3],
            extra[4],
            extra[5],
            *conf_sum as f64 / games as f64,
            conf_sum
        )?;
    }
    out.flush()?;
    eprintln!("# wrote {} rows to {out_path}", rows.len());
    Ok(())
}

/// 跑一局（复用已更新的引擎配置，避免每局重新克隆）
fn play_env(
    env: &Env,
    engine: &mut umaai::Config,
    blue: &[i32; 5],
    extra: &[i32; 6],
    seed: u64,
    trainer: &mut MatsuriTrainer,
) -> Result<Outcome> {
    engine.sim.blue_count = *blue;
    engine.sim.extra_count = *extra;
    let mut game = build_game(engine, env.data.clone())?.with_config(env.config.clone());
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(&*trainer, &mut rng)?;
    while game.next() {
        game.run_stage(&*trainer, &mut rng)?;
    }
    game.on_simulation_end(&*trainer, &mut rng)?;
    Ok(Outcome {
        score: game.uma().calc_score()?,
        pt: game.uma().total_pt()?,
    })
}

/// 读取配置列表（b0..b4,x0..x5 每行一条）
fn read_configs(path: &str) -> Result<Vec<([i32; 5], [i32; 6])>> {
    let file = File::open(path).with_context(|| format!("打开配置列表失败: {path}"))?;
    let mut out = Vec::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        let text = line.split('#').next().unwrap_or("").trim();
        if text.is_empty() || text.starts_with("b0") {
            continue;
        }
        let values: Vec<i32> = text
            .split(',')
            .map(|raw| raw.trim().parse::<i32>())
            .collect::<std::result::Result<_, _>>()
            .with_context(|| format!("第 {} 行解析失败: {text}", line_no + 1))?;
        if values.len() != 11 {
            bail!("第 {} 行应有 11 个整数，实际 {}", line_no + 1, values.len());
        }
        out.push((
            [values[0], values[1], values[2], values[3], values[4]],
            [
                values[5], values[6], values[7], values[8], values[9], values[10],
            ],
        ));
    }
    Ok(out)
}

/// 评估配置列表：输出均值 / 标准误 / PT 均值，可选逐种子评分
fn cmd_eval(env: &Env, args: &[String]) -> Result<()> {
    let uma = parse_usize(args, "--uma", 105902)? as u32;
    let games = parse_usize(args, "--games", 300)?;
    let seed = parse_usize(args, "--seed", 70_000)? as u64;
    let threads = resolve_threads(args)?;
    let in_path = require_arg(args, "--in")?;
    let out_path = require_arg(args, "--out")?;
    let scores_path = parse_arg(args, "--scores");
    let configs = read_configs(&in_path)?;
    if games == 0 {
        bail!("--games 必须 ≥ 1");
    }
    eprintln!(
        "# eval uma={uma} configs={} games={games} seed={seed} threads={threads}",
        configs.len()
    );

    let next = AtomicUsize::new(0);
    let started = Instant::now();
    let chunk = 32usize;
    let mut results: Vec<Vec<Option<Eval>>> = Vec::with_capacity(threads);

    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let next = &next;
            let configs = &configs;
            handles.push(scope.spawn(move || -> Result<Vec<Option<Eval>>> {
                let mut out: Vec<Option<Eval>> = (0..configs.len()).map(|_| None).collect();
                let mut engine = env.engine.clone();
                engine.sim.uma = uma;
                let mut trainer = MatsuriTrainer::new(env.config.clone());
                loop {
                    let start = next.fetch_add(chunk, Ordering::Relaxed);
                    if start >= configs.len() {
                        break;
                    }
                    let end = (start + chunk).min(configs.len());
                    for index in start..end {
                        let (blue, extra) = &configs[index];
                        let mut scores = Vec::with_capacity(games);
                        let mut pt_sum = 0i64;
                        let mut ok = true;
                        for k in 0..games {
                            match play_env(
                                env,
                                &mut engine,
                                blue,
                                extra,
                                seed + k as u64,
                                &mut trainer,
                            ) {
                                Ok(value) => {
                                    scores.push(value.score);
                                    pt_sum += value.pt as i64;
                                }
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            out[index] = Some(Eval {
                                sum: scores.iter().map(|v| *v as i64).sum(),
                                pt_sum,
                                scores,
                            });
                        }
                    }
                }
                Ok(out)
            }));
        }
        for handle in handles {
            results.push(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("工作线程 panic"))??,
            );
        }
        Ok(())
    })?;

    let mut merged: Vec<Option<Eval>> = (0..configs.len()).map(|_| None).collect();
    for part in results {
        for (index, item) in part.into_iter().enumerate() {
            if item.is_some() {
                merged[index] = item;
            }
        }
    }

    let file = File::create(&out_path).with_context(|| format!("创建输出失败: {out_path}"))?;
    let mut out = BufWriter::new(file);
    writeln!(
        out,
        "idx,b0,b1,b2,b3,b4,x0,x1,x2,x3,x4,x5,mean,se,pt_mean,sum"
    )?;
    let mut failed = 0usize;
    for (index, ((blue, extra), result)) in configs.iter().zip(merged.iter()).enumerate() {
        let Some(eval) = result else {
            failed += 1;
            continue;
        };
        let mean = eval.sum as f64 / games as f64;
        let se = sample_se(&eval.scores);
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{}",
            index,
            blue[0],
            blue[1],
            blue[2],
            blue[3],
            blue[4],
            extra[0],
            extra[1],
            extra[2],
            extra[3],
            extra[4],
            extra[5],
            mean,
            se,
            eval.pt_sum as f64 / games as f64,
            eval.sum
        )?;
    }
    out.flush()?;
    if let Some(path) = scores_path {
        let file = File::create(&path).with_context(|| format!("创建评分输出失败: {path}"))?;
        let mut out = BufWriter::new(file);
        let mut header = String::from("idx");
        for k in 0..games {
            header.push_str(&format!(",s{k}"));
        }
        writeln!(out, "{header}")?;
        for (index, result) in merged.iter().enumerate() {
            let Some(eval) = result else { continue };
            let mut line = format!("{index}");
            for score in &eval.scores {
                line.push_str(&format!(",{score}"));
            }
            writeln!(out, "{line}")?;
        }
        out.flush()?;
    }
    eprintln!(
        "# eval done configs={} failed={failed} elapsed={:.1}s",
        configs.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// 单个马娘的扫测结果
struct UmaEval {
    id: u32,
    name: String,
    sum: i64,
    pt_sum: i64,
    scores: Vec<i32>,
}

/// 特殊机制马娘：暂不参与普通排名（狄杜斯铁心 / 凯斯奇迹强制休息）
const SPECIAL_UMAS: &[u32] = &[106301, 106302, 109301, 109302];

/// 全马娘扫测：固定当前开局预设与手写训练员，按评分均值排序
fn cmd_uma(env: &Env, args: &[String]) -> Result<()> {
    let games = parse_usize(args, "--games", 3000)?;
    let seed = parse_usize(args, "--seed", 67_200)? as u64;
    let threads = resolve_threads(args)?;
    let out_path = parse_arg(args, "--out");
    let scores_path = parse_arg(args, "--scores");
    if games == 0 {
        bail!("--games 必须 ≥ 1");
    }
    let data = Data::get().context("引擎静态数据未初始化")?;
    let mut entries: Vec<(u32, String)> = data
        .uma
        .iter()
        .filter_map(|(key, uma)| key.parse::<u32>().ok().map(|id| (id, uma.name.clone())))
        .collect();
    entries.sort_unstable_by_key(|(id, _)| *id);
    let blue = env.engine.sim.blue_count;
    let extra = env.engine.sim.extra_count;
    eprintln!(
        "# uma scan umas={} games={games} seed={seed} threads={threads} blue={blue:?} extra={extra:?}",
        entries.len()
    );

    let next = AtomicUsize::new(0);
    let started = Instant::now();
    let mut parts: Vec<Vec<Option<UmaEval>>> = Vec::with_capacity(threads);

    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let next = &next;
            let entries = &entries;
            handles.push(scope.spawn(move || -> Result<Vec<Option<UmaEval>>> {
                let mut out: Vec<Option<UmaEval>> = (0..entries.len()).map(|_| None).collect();
                let mut engine = env.engine.clone();
                let mut trainer = MatsuriTrainer::new(env.config.clone());
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= entries.len() {
                        break;
                    }
                    let (id, name) = &entries[index];
                    engine.sim.uma = *id;
                    let mut scores = Vec::with_capacity(games);
                    let mut pt_sum = 0i64;
                    let mut ok = true;
                    let mut first_err: Option<String> = None;
                    for k in 0..games {
                        match play_env(
                            env,
                            &mut engine,
                            &blue,
                            &extra,
                            seed + k as u64,
                            &mut trainer,
                        ) {
                            Ok(value) => {
                                scores.push(value.score);
                                pt_sum += value.pt as i64;
                            }
                            Err(err) => {
                                if first_err.is_none() {
                                    first_err = Some(err.to_string());
                                }
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        out[index] = Some(UmaEval {
                            id: *id,
                            name: name.clone(),
                            sum: scores.iter().map(|v| *v as i64).sum(),
                            pt_sum,
                            scores,
                        });
                    } else if let Some(err) = first_err {
                        eprintln!("# uma {id}（{name}）跳过：{err}");
                    }
                }
                Ok(out)
            }));
        }
        for handle in handles {
            parts.push(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("工作线程 panic"))??,
            );
        }
        Ok(())
    })?;

    let mut merged: Vec<Option<UmaEval>> = (0..entries.len()).map(|_| None).collect();
    for part in parts {
        for (index, item) in part.into_iter().enumerate() {
            if item.is_some() {
                merged[index] = item;
            }
        }
    }
    let mut rows: Vec<&UmaEval> = merged.iter().flatten().collect();
    rows.sort_by(|a, b| b.sum.cmp(&a.sum).then(a.id.cmp(&b.id)));

    if let Some(path) = out_path {
        let file = File::create(&path).with_context(|| format!("创建输出失败: {path}"))?;
        let mut out = BufWriter::new(file);
        writeln!(out, "rank,uma,name,special,mean_score,mean_pt,se,games")?;
        for (rank, eval) in rows.iter().enumerate() {
            writeln!(
                out,
                "{},{},{},{},{:.2},{:.2},{:.2},{}",
                rank + 1,
                eval.id,
                eval.name.replace(',', "，"),
                u8::from(SPECIAL_UMAS.contains(&eval.id)),
                eval.sum as f64 / games as f64,
                eval.pt_sum as f64 / games as f64,
                sample_se(&eval.scores),
                games
            )?;
        }
        out.flush()?;
        eprintln!("# wrote {} rows to {path}", rows.len());
    }

    if let Some(path) = scores_path {
        let file = File::create(&path).with_context(|| format!("创建评分输出失败: {path}"))?;
        let mut out = BufWriter::new(file);
        let mut header = String::from("uma");
        for k in 0..games {
            header.push_str(&format!(",s{k}"));
        }
        writeln!(out, "{header}")?;
        for eval in &rows {
            let mut line = format!("{}", eval.id);
            for score in &eval.scores {
                line.push_str(&format!(",{score}"));
            }
            writeln!(out, "{line}")?;
        }
        out.flush()?;
    }

    let ordinary: Vec<&UmaEval> = rows
        .iter()
        .copied()
        .filter(|eval| !SPECIAL_UMAS.contains(&eval.id))
        .collect();
    println!(
        "# uma ranking: umas={} games={} seed={seed}",
        rows.len(),
        games
    );
    println!("rank,uma,name,mean_score,mean_pt,se");
    for (rank, eval) in ordinary.iter().take(10).enumerate() {
        println!(
            "{:>4},{},{},{:.1},{:.1},{:.1}",
            rank + 1,
            eval.id,
            eval.name,
            eval.sum as f64 / games as f64,
            eval.pt_sum as f64 / games as f64,
            sample_se(&eval.scores)
        );
    }
    println!("# 特殊机制（不计入 Top10）");
    for eval in rows.iter().filter(|eval| SPECIAL_UMAS.contains(&eval.id)) {
        println!(
            "{:>4},{},{},{:.1},{:.1},{:.1}",
            "-",
            eval.id,
            eval.name,
            eval.sum as f64 / games as f64,
            eval.pt_sum as f64 / games as f64,
            sample_se(&eval.scores)
        );
    }
    eprintln!("# uma scan done in {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}

/// 样本标准误（n<2 返回 0）
fn sample_se(values: &[i32]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / n;
    let var = values
        .iter()
        .map(|v| {
            let d = *v as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    (var / n).sqrt()
}

fn main() -> Result<()> {
    // 诊断开关关闭：diag! 连 format_args 都不执行
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("enum");
    let env = Env::load()?;
    match command {
        "enum" => cmd_enum(&env, &args),
        "eval" => cmd_eval(&env, &args),
        "uma" => cmd_uma(&env, &args),
        other => bail!("未知子命令: {other}（可选 enum / eval / uma）"),
    }
}
