//! 复盘工具入口：`review open|eval ...`
//!
//! `open` 出事实（零重算）；`eval` 按需单点重算（候选表 + regret + 配对 CI + 缓存）。

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map_or("", String::as_str);
    match command {
        "open" => open(&args),
        "eval" => eval(&args),
        "infer" => infer(&args),
        "curve" => curve(&args),
        "report" => report(&args),
        _ => {
            println!("用法：");
            println!(
                "  review open <局目录|局包.zip> [--game <id>] [--out <目录>] [--data <数据根>]"
            );
            println!(
                "  review eval <局目录|局包.zip> --turn <N> [--seq <k>] [--n 1024] [--game <id>] [--out <目录>] [--data <数据根>] [--force]"
            );
            println!(
                "  review infer <局目录|局包.zip> [--what advice,fusion,skill,kensan|all] [--turn <N>] [--game <id>] [--out <目录>] [--json]"
            );
            println!(
                "  review curve <局目录|局包.zip> [--from <A>] [--to <B>] [--n 1024] [--game <id>] [--out <目录>] [--force]"
            );
            println!(
                "  review report <局目录|局包.zip> [--narrative <md>] [--game <id>] [--out <目录>] [--data <数据根>]"
            );
            Ok(())
        }
    }
}

/// `review open`
fn open(args: &[String]) -> anyhow::Result<()> {
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review open <局目录>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut game: Option<u64> = None;
    let mut tuning = matsuri_tools::review::tuning::Tuning::default();
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--game" => {
                game = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--inherit-expect" => {
                tuning.inherit_expect = args
                    .get(index + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(tuning.inherit_expect);
                index += 2;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    let dir = matsuri_tools::review::open(matsuri_tools::review::OpenArgs {
        input: &input,
        game,
        out: out.as_deref(),
        data_root: data_root.as_deref(),
        tuning,
    })?;
    println!("{}", dir.display());
    Ok(())
}

/// `review curve`
fn curve(args: &[String]) -> anyhow::Result<()> {
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review curve <局目录>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut game: Option<u64> = None;
    let mut from: Option<i32> = None;
    let mut to: Option<i32> = None;
    let mut n: u32 = 1024;
    let mut force = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--game" => {
                game = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--from" => {
                from = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--to" => {
                to = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--n" => {
                n = args
                    .get(index + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1024);
                index += 2;
            }
            "--force" => {
                force = true;
                index += 1;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    let (path, report) =
        matsuri_tools::review::curve::curve(matsuri_tools::review::curve::CurveArgs {
            input: &input,
            game,
            from,
            to,
            n,
            out: out.as_deref(),
            data_root: data_root.as_deref(),
            force,
        })?;
    println!("{}", path.display());
    println!(
        "曲线 {} 点（T{}–T{}）：首 T(n) {} → 末 T(n) {} · 形态 {} · 终局 {} · 终局−末点 {:?}",
        report.points.len(),
        report
            .summary
            .first_turn
            .map_or_else(|| "—".to_string(), |t| t.to_string()),
        report
            .summary
            .last_turn
            .map_or_else(|| "—".to_string(), |t| t.to_string()),
        report
            .summary
            .first_t_n
            .map_or_else(|| "—".to_string(), |v| format!("{v:.0}")),
        report
            .summary
            .last_t_n
            .map_or_else(|| "—".to_string(), |v| format!("{v:.0}")),
        report.summary.shape,
        report
            .summary
            .final_score
            .map_or_else(|| "—".to_string(), |v| v.to_string()),
        report.summary.final_gap.map(|v| format!("{v:+.0}")),
    );
    Ok(())
}

/// `review report`
fn report(args: &[String]) -> anyhow::Result<()> {
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review report <局目录>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut game: Option<u64> = None;
    let mut narrative: Option<PathBuf> = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--game" => {
                game = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--narrative" => {
                narrative = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    let (html, digest) =
        matsuri_tools::review::report::report(matsuri_tools::review::report::ReportArgs {
            input: &input,
            game,
            out: out.as_deref(),
            data_root: data_root.as_deref(),
            narrative: narrative.as_deref(),
        })?;
    println!("{}", html.display());
    println!("{}", digest.display());
    Ok(())
}

/// `review infer`
fn infer(args: &[String]) -> anyhow::Result<()> {
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review infer <局目录>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut game: Option<u64> = None;
    let mut turn: Option<i32> = None;
    let mut what: Vec<String> = Vec::new();
    let mut json_mode = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--game" => {
                game = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--turn" => {
                turn = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--what" => {
                if let Some(value) = args.get(index + 1) {
                    what.extend(value.split(',').map(|item| item.trim().to_string()));
                }
                index += 2;
            }
            "--json" => {
                json_mode = true;
                index += 1;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    let (path, report) =
        matsuri_tools::review::infer::infer(matsuri_tools::review::infer::InferArgs {
            input: &input,
            game,
            what,
            turn,
            out: out.as_deref(),
            data_root: data_root.as_deref(),
        })?;
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("{}", path.display());
    let count = |key: &str| {
        report
            .get(key)
            .and_then(|value| value.as_array())
            .map_or(0, Vec::len)
    };
    println!(
        "反推：教诲 {} 条 / 転成 {} 条 / 学技能 {} 次 / 追加稽古 {} 次",
        count("advice"),
        count("fusion"),
        count("skill"),
        count("kensan")
    );
    Ok(())
}

/// `review eval`
fn eval(args: &[String]) -> anyhow::Result<()> {
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review eval <局目录> --turn <N>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut game: Option<u64> = None;
    let mut turn: Option<i32> = None;
    let mut seq: Option<u32> = None;
    let mut n: u32 = 1024;
    let mut force = false;
    let mut all = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--game" => {
                game = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--turn" => {
                turn = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--seq" => {
                seq = args.get(index + 1).and_then(|v| v.parse().ok());
                index += 2;
            }
            "--n" => {
                n = args
                    .get(index + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1024);
                index += 2;
            }
            "--force" => {
                force = true;
                index += 1;
            }
            "--all" => {
                all = true;
                index += 1;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    if all {
        let results =
            matsuri_tools::review::eval::eval_all(matsuri_tools::review::eval::EvalArgs {
                input: &input,
                game,
                turn: 0,
                seq: None,
                n,
                out: out.as_deref(),
                data_root: data_root.as_deref(),
                force,
                all: true,
            })?;
        let mut significant = 0;
        let mut neutral = 0;
        let mut unknown = 0;
        for (_, report) in &results {
            print_report(report);
            match (report.regret, report.ci95) {
                (Some(regret), Some(ci)) if regret.abs() > ci => significant += 1,
                (Some(_), Some(_)) => neutral += 1,
                _ => unknown += 1,
            }
        }
        println!(
            "汇总：{} 点（显著 {} / 跨零 {} / 不可判 {}）",
            results.len(),
            significant,
            neutral,
            unknown
        );
        return Ok(());
    }
    let Some(turn) = turn else {
        anyhow::bail!("缺少 --turn：review eval <局目录> --turn <N>（或 --all 批量）");
    };
    let (path, report) =
        matsuri_tools::review::eval::eval(matsuri_tools::review::eval::EvalArgs {
            input: &input,
            game,
            turn,
            seq,
            n,
            out: out.as_deref(),
            data_root: data_root.as_deref(),
            force,
            all: false,
        })?;
    println!("{}", path.display());
    print_report(&report);
    Ok(())
}

/// 打印单点摘要 + rollout 告警（stderr）
fn print_report(report: &matsuri_tools::review::eval::EvalReport) {
    let fmt_score =
        |value: Option<f64>| value.map_or_else(|| "—".to_string(), |value| format!("{value:.0}"));
    println!(
        "T{} {} 候选 {} 个：best {} / chosen {} · regret {} ± {}（配对 n={}）· T(n) {} · {} ms",
        report.turn,
        report.kind,
        report.candidates.len(),
        report.best.as_ref().map_or_else(
            || "—".to_string(),
            |best| format!("#{} {}", best.index, best.description)
        ),
        report.chosen.as_ref().map_or_else(
            || "—".to_string(),
            |chosen| format!("#{} {}", chosen.index, chosen.description)
        ),
        fmt_score(report.regret),
        fmt_score(report.ci95),
        report.paired_n,
        fmt_score(report.t_n),
        report.elapsed_ms,
    );
    if let Some(note) = &report.note {
        println!("备注：{note}");
    }
    if let Some(rollout) = &report.rollout {
        if rollout.failed > 0 {
            eprintln!(
                "警告：T{} 搜索有 {} 次 rollout 报错（计划 {} 次），样本不足",
                report.turn, rollout.failed, rollout.planned
            );
        }
        if rollout.max_career_failed_rate > f64::EPSILON {
            eprintln!(
                "警告：T{} 样本含育成失败（自选赛未达标）最高 {:.1}%",
                report.turn,
                rollout.max_career_failed_rate * 100.0
            );
        }
    }
}
