//! 配对门禁判官 bin `matsuri-gate`
//!
//! 把「改动前 / 改动后」两份 `matsuri-dist` 样本明细做 **CRN 同种子逐局配对**，
//! 对 **五维和 / 五维分 / PT** 三个分量各自判读「改动后是否显著更接近真值」。
//!
//! 用法：
//!   MC_N=4000 MC_SEED=0 MC_DUMP=a.jsonl matsuri-dist _state_t30.json _gate_a.json
//!   MC_N=4000 MC_SEED=0 MC_DUMP=b.jsonl matsuri-dist _state_t30.json _gate_b.json
//!   matsuri-gate --a a.jsonl --b b.jsonl \
//!       --five 1998,1470,1656,1344,1573 --pt 8339 --stat-score 42201
//!
//! **配对前提**：两份 dump 必须是同一 `MC_N` / 同一 `MC_SEED` 生成的——采样种子按样本号派生，
//! 同号同种子 ⇒ 两臂随机流对齐 ⇒ 配对差只来自机制改动。
//!
//! 判读只认贴近幅度 `e_i = |a_i − 真值| − |b_i − 真值|` 的 95% 区间：
//! 下界 >0 = 显著更近；上界 <0 = 显著更远；跨 0 = 无显著差异（留档，不转正）。
//! 真值是**单局**真实值，所以结论只到「这一局的方向」，不是「普遍成立」的证明。
//!
//! 退出码：0 正常出报告（含「无显著差异」）；2 输入不合法。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use matsuri_sim::bench::{ComponentReport, SampleMap, StatsConfig, align_by_index, paired_stats};
use serde_json::{Value, json};

/// 输入不合法时的退出码
const EXIT_BAD_INPUT: u8 = 2;

/// 命令行参数
struct Args {
    /// 改动前的 dump（`MC_DUMP` JSONL）
    a: PathBuf,
    /// 改动后的 dump
    b: PathBuf,
    /// 真值五维（速,耐,力,根,智）→ 推出真值五维和
    five: Option<Vec<f64>>,
    /// 真值 PT
    pt: Option<f64>,
    /// 真值五维分（游戏原生表）
    stat_score: Option<f64>,
    /// 置换检验次数
    perms: usize,
    /// bootstrap 次数
    boot: usize,
    /// 随机种子
    seed: u64,
    /// 输出 JSON 而非表格
    json: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::from(EXIT_BAD_INPUT)
        }
    }
}

/// 跑一遍判官
fn run() -> Result<()> {
    let args = parse_args()?;
    let cfg = StatsConfig {
        perms: args.perms,
        boot: args.boot,
        seed: args.seed,
    };

    // 要判的分量：(名字, dump 字段, 真值)
    let mut wanted: Vec<(&str, &str, f64)> = Vec::new();
    if let Some(five) = &args.five {
        if five.len() != 5 {
            bail!("--five 需要 5 个值（速,耐,力,根,智），收到 {}", five.len());
        }
        wanted.push(("五维和", "sum", five.iter().sum()));
    }
    if let Some(v) = args.stat_score {
        wanted.push(("五维分", "stat", v));
    }
    if let Some(v) = args.pt {
        wanted.push(("PT", "pt", v));
    }
    if wanted.is_empty() {
        bail!("至少要给一个真值：--five / --stat-score / --pt");
    }

    let mut reports: Vec<ComponentReport> = Vec::new();
    for (name, field, real) in wanted {
        let a = read_component(&args.a, field)?;
        let b = read_component(&args.b, field)?;
        let paired = align_by_index(&a, &b)
            .with_context(|| format!("{name}：两臂样本号集合不一致，配对失败"))?;
        let r = paired_stats(name, &paired.a, &paired.b, real, cfg)
            .with_context(|| format!("{name}：配对统计失败"))?;
        reports.push(r);
    }

    if args.json {
        println!("{}", to_json(&args, &reports));
    } else {
        print_table(&args, &reports);
    }
    Ok(())
}

/// 解析命令行
fn parse_args() -> Result<Args> {
    let mut a: Option<PathBuf> = None;
    let mut b: Option<PathBuf> = None;
    let mut five: Option<Vec<f64>> = None;
    let mut pt: Option<f64> = None;
    let mut stat_score: Option<f64> = None;
    let mut perms = 20_000usize;
    let mut boot = 20_000usize;
    let mut seed = 0u64;
    let mut json = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--a" => a = Some(PathBuf::from(next_value(&mut it, "--a")?)),
            "--b" => b = Some(PathBuf::from(next_value(&mut it, "--b")?)),
            "--five" => {
                let raw = next_value(&mut it, "--five")?;
                let mut vals = Vec::new();
                for piece in raw.replace('，', ",").split(',') {
                    let piece = piece.trim();
                    if piece.is_empty() {
                        continue;
                    }
                    vals.push(
                        piece
                            .parse::<f64>()
                            .with_context(|| format!("--five 的 {piece} 不是数字"))?,
                    );
                }
                five = Some(vals);
            }
            "--pt" => pt = Some(parse_f64(&next_value(&mut it, "--pt")?, "--pt")?),
            "--stat-score" => {
                stat_score = Some(parse_f64(
                    &next_value(&mut it, "--stat-score")?,
                    "--stat-score",
                )?);
            }
            "--perms" => perms = parse_usize(&next_value(&mut it, "--perms")?, "--perms")?,
            "--boot" => boot = parse_usize(&next_value(&mut it, "--boot")?, "--boot")?,
            "--seed" => {
                let raw = next_value(&mut it, "--seed")?;
                seed = raw
                    .parse::<u64>()
                    .with_context(|| format!("--seed 的 {raw} 不是无符号整数"))?;
            }
            "--json" => json = true,
            other => bail!("未知参数 {other}"),
        }
    }

    Ok(Args {
        a: a.context("缺少 --a（改动前的 dump 路径）")?,
        b: b.context("缺少 --b（改动后的 dump 路径）")?,
        five,
        pt,
        stat_score,
        perms,
        boot,
        seed,
        json,
    })
}

/// 取下一个参数值
fn next_value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next().with_context(|| format!("{flag} 后面缺值"))
}

/// 解析浮点参数
fn parse_f64(raw: &str, flag: &str) -> Result<f64> {
    raw.parse::<f64>()
        .with_context(|| format!("{flag} 的 {raw} 不是数字"))
}

/// 解析无符号整数参数
fn parse_usize(raw: &str, flag: &str) -> Result<usize> {
    raw.parse::<usize>()
        .with_context(|| format!("{flag} 的 {raw} 不是非负整数"))
}

/// 读一份 dump 的某个分量：样本号 → 值
fn read_component(path: &Path, field: &str) -> Result<SampleMap> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("读 dump 失败：{}", path.display()))?;
    let mut map: SampleMap = BTreeMap::new();
    for (lineno, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .with_context(|| format!("{}:{} 解析失败", path.display(), lineno + 1))?;
        let i = v["i"]
            .as_u64()
            .with_context(|| format!("{}:{} 缺字段 i", path.display(), lineno + 1))?;
        let x = v[field]
            .as_f64()
            .with_context(|| format!("{}:{} 缺字段 {field}", path.display(), lineno + 1))?;
        map.insert(i, x);
    }
    if map.is_empty() {
        bail!("{} 是空的", path.display());
    }
    Ok(map)
}

/// 表格输出
fn print_table(args: &Args, reports: &[ComponentReport]) {
    let n = reports.first().map(|r| r.n).unwrap_or(0);
    println!("=== 配对门禁（CRN 同种子逐局配对）===");
    println!("A = {}", args.a.display());
    println!("B = {}", args.b.display());
    println!(
        "n = {n}   置换 {} 次   bootstrap {} 次   seed {}",
        args.perms, args.boot, args.seed
    );
    println!("配对前提：两份 dump 由同一 MC_N / MC_SEED 生成（采样种子按样本号派生）");
    println!();
    println!(
        "| 分量 | 真值 | A 均值 | B 均值 | Δ=B−A | Δ 95%CI | p | 更近幅度 | 95%CI | p | 胜率 | 判定 |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for r in reports {
        println!(
            "| {} | {} | {} | {} | {} | [{}, {}] | {:.4} | {} | [{}, {}] | {:.4} | {:.2}% | {} |",
            r.name,
            num(r.real, 0),
            num(r.mean_a, 1),
            num(r.mean_b, 1),
            signed(r.mean_delta),
            signed(r.delta_ci.0),
            signed(r.delta_ci.1),
            r.delta_p,
            signed(r.mean_closer),
            signed(r.closer_ci.0),
            signed(r.closer_ci.1),
            r.closer_p,
            r.win_rate * 100.0,
            r.verdict.label(),
        );
    }
    println!();
    println!(
        "判读：贴近幅度 95% 区间下界 >0 = 显著更近；上界 <0 = 显著更远；跨 0 = 无显著差异（留档，不转正）。"
    );
    println!("口径：真值是单局真实值，结论只到「这一局的方向」；Δ 与 p 是佐证，不参与判读。");
}

/// JSON 输出
fn to_json(args: &Args, reports: &[ComponentReport]) -> String {
    let components: Vec<Value> = reports
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "n": r.n,
                "real": r.real,
                "mean_a": r.mean_a,
                "mean_b": r.mean_b,
                "mean_delta": r.mean_delta,
                "delta_ci": [r.delta_ci.0, r.delta_ci.1],
                "delta_p": r.delta_p,
                "mean_closer": r.mean_closer,
                "closer_ci": [r.closer_ci.0, r.closer_ci.1],
                "closer_p": r.closer_p,
                "win_rate": r.win_rate,
                "verdict": r.verdict.label(),
            })
        })
        .collect();
    let out = json!({
        "a": args.a.display().to_string(),
        "b": args.b.display().to_string(),
        "n": reports.first().map(|r| r.n).unwrap_or(0),
        "perms": args.perms,
        "boot": args.boot,
        "seed": args.seed,
        "components": components,
    });
    out.to_string()
}

/// 千分位数字（dec 位小数）
fn num(v: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, v.abs());
    let (int_part, frac) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s.as_str(), ""),
    };
    let mut out = String::with_capacity(int_part.len() + int_part.len() / 3 + 2);
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let sign = if v < 0.0 { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{out}")
    } else {
        format!("{sign}{out}.{frac}")
    }
}

/// 带符号的小数（Δ / 贴近幅度用）
fn signed(v: f64) -> String {
    format!("{v:+.1}")
}
