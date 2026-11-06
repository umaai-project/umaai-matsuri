//! `online` 子命令：真机帧 / 回放 → 逐帧决策（终端面板或 JSON 行）
//!
//! 本文件是**唯一**的在线外壳：参数解析、渲染、循环都在这里；决策逻辑全在 `matsuri-ai`。
//!
//! 用法：
//!
//! ```text
//! matsuri online [--root <游戏根目录>] [--replay <帧目录>] [--trainer matsuri|search]
//!                [--json] [--verify-training]
//! ```
//!
//! - `--root` 缺省读 `MATSURI_ROOT`，再缺省用当前目录（要求含 `.portable`）
//! - `--replay` 走离线回放（读 `game{id}_turn{n}.json`），不给则挂真机目录监听
//! - `--json` 时 stdout **严格**每行一条 JSON，横幅 / 面板 / 日志全走 stderr

use std::fmt::Write as _;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use matsuri_ai::output::{FinalTurnOutput, OnlineTurnOutput};
use matsuri_ai::{
    FrameSource, OnlineSession, TrainerKind, TurnKind, connected_payload, error_payload,
    resolve_live_root,
};
use matsuri_core::{MatsuriConfig, MatsuriData};
use umaai::{Config, FrameKind};
use umaai_cli::render::term;
use umaai_core::Data;

use crate::logging;
use crate::panel::BoardPanel;

/// 在线档参数
#[derive(Debug, Default)]
pub struct Args {
    /// 真机游戏根目录（含 `.portable`）
    pub root: Option<PathBuf>,
    /// 回放目录（给了就不挂真机监听）
    pub replay: Option<PathBuf>,
    /// 训练员（`matsuri` 手写 / `search` 搜索）
    pub trainer: Option<String>,
    /// 逐行 JSON 输出
    pub json: bool,
    /// 输出训练验算表
    pub verify: bool,
}

/// 解析 `online` 之后的参数（手写解析，不引 clap）
pub fn parse(argv: &[String]) -> Result<Args> {
    let mut args = Args::default();
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                args.root = Some(PathBuf::from(
                    iter.next().ok_or_else(|| anyhow!("--root 缺少路径"))?,
                ));
            }
            "--replay" => {
                args.replay = Some(PathBuf::from(
                    iter.next().ok_or_else(|| anyhow!("--replay 缺少目录"))?,
                ));
            }
            "--trainer" => {
                args.trainer = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--trainer 缺少取值"))?
                        .clone(),
                );
            }
            "--json" => args.json = true,
            "--verify-training" => args.verify = true,
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => {
                return Err(anyhow!("online 子命令不认识参数：{other}\n\n{}", usage()));
            }
        }
    }
    Ok(args)
}

/// 用法文本
#[must_use]
pub fn usage() -> String {
    "用法: matsuri online [--root <游戏根目录>] [--replay <帧目录>] [--trainer matsuri|search] [--json] [--verify-training]\n\
     \n  --root            真机游戏根目录（含 .portable）；缺省读 MATSURI_ROOT，再缺省当前目录\n  \
     --replay          回放目录（game{id}_turn{n}.json + game{id}_finalScore.json）；给了就不挂真机监听\n  \
     --trainer         训练员：matsuri（手写） / search（搜索）；缺省按配置 [search].search_n 决定（0 = 手写，>0 = 搜索）\n  \
     --json            stdout 每行一条 JSON（日志与面板走 stderr）\n  \
     --verify-training 附训练验算表（帧真值 vs 还原态预览，G13）\n\
     \n配置与环境：默认层 data/ai_config.toml + data/matsuri_config.toml，工作目录同名覆盖层；\n\
     MATSURI_SEARCH_N / MATSURI_SEARCH_RADICAL 可临时覆盖搜索预算与激进度。"
        .to_string()
}

/// 跑在线档
pub fn run(argv: &[String]) -> Result<()> {
    let args = parse(argv)?;
    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与离线档同约定）
    let mut cfg = Config::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入配置失败")?;
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(crate::DATA_ROOT)
        .map(|_| ())
        .context("引擎静态数据初始化失败")?;
    let data = Arc::new(MatsuriData::load(crate::DATA_ROOT).context("载入剧本数据失败")?);
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入剧本配置失败")?;
    // 在线档与离线档读同一套配置：预算取 `matsuri_config [search].search_n`，
    // 环境变量只作为临时覆盖（`MATSURI_SEARCH_N` / `MATSURI_SEARCH_RADICAL`）。
    let search_n =
        crate::env_parse::<usize>("MATSURI_SEARCH_N")?.unwrap_or(matsuri_cfg.search.search_n);
    matsuri_cfg.search.search_n = search_n;
    if let Some(radical) = crate::env_parse::<f64>("MATSURI_SEARCH_RADICAL")? {
        cfg.search.radical_factor_max = radical;
    }
    // 未显式给 `--trainer` 时按配置决定：`search_n > 0` = 搜索档，`0` = 手写档
    // （与离线 `matsuri` 训练员的默认口径一致）
    let trainer = match args.trainer.as_deref() {
        Some(name) => TrainerKind::parse(name)?,
        None if search_n > 0 => TrainerKind::Search,
        None => TrainerKind::Hand,
    };
    if !args.json {
        eprintln!(
            "在线档：训练员 {}（[search].search_n = {search_n}；0 = 手写）",
            if trainer.is_search() { "search" } else { "matsuri" }
        );
    }
    let mut session = OnlineSession::new(&cfg, data, matsuri_cfg, trainer, args.verify)?;

    let mut source = match &args.replay {
        Some(dir) => FrameSource::open_replay(dir)
            .with_context(|| format!("打开回放目录失败：{}", dir.display()))?,
        None => {
            let root = resolve_live_root(args.root.as_deref())?;
            if !args.json {
                eprintln!("监听真机帧目录：{}", umaai::plugin_dir(&root).display());
            }
            FrameSource::open_live(&root)?
        }
    };
    let replay = source.is_replay();

    // 面板：JSON 模式一律关色（stdout 只允许 JSON）
    let mut panel = BoardPanel::new(term::use_color(args.json));
    let mut stdout = std::io::stdout();
    if args.json {
        // 启动握手：消费者据此判断「进程已就绪、开始接收帧」
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&connected_payload("matsuri"))?
        )?;
        stdout.flush()?;
    }
    let mut frames = 0usize;
    let mut decisions = 0usize;
    let mut finals = 0usize;
    let mut errors = 0usize;

    loop {
        let timeout = if replay {
            Duration::ZERO
        } else {
            Duration::from_millis(500)
        };
        let frame = match source.next_frame(timeout) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                if replay {
                    break;
                }
                continue;
            }
            Err(err) => return Err(err).context("帧通道读取失败"),
        };
        for note in source.drain_notes() {
            eprintln!("通道提示：{note:?}");
        }
        match frame.kind {
            FrameKind::ThisTurn => {
                frames += 1;
                match session.handle_frame(&frame.contents) {
                    Ok(output) => {
                        if output.kind == TurnKind::Decision {
                            decisions += 1;
                        }
                        render(&mut stdout, &mut panel, &output, args.json)?;
                    }
                    Err(err) => {
                        // 真机档单帧失败不终止循环（等下一帧）；回放档累计后在结束时报错
                        errors += 1;
                        render_error(&mut stdout, args.json, &err)?;
                    }
                }
            }
            FrameKind::FinalScore => {
                finals += 1;
                match session.handle_final(&frame.contents) {
                    Ok(output) => render_final(&mut stdout, &output, args.json)?,
                    Err(err) => {
                        errors += 1;
                        render_error(&mut stdout, args.json, &err)?;
                    }
                }
            }
        }
    }

    if !args.json {
        eprintln!(
            "结束：处理 {frames} 帧（{decisions} 次决策 / {finals} 张终局帧），失败 {errors} 帧"
        );
    }
    if replay && errors > 0 {
        return Err(anyhow!("回放有 {errors} 帧处理失败"));
    }
    Ok(())
}

/// 渲染一帧的输出
fn render(
    stdout: &mut impl Write,
    panel: &mut BoardPanel,
    output: &OnlineTurnOutput,
    json: bool,
) -> Result<()> {
    if json {
        let payload = output.json_payload();
        writeln!(stdout, "{}", serde_json::to_string(&payload)?)?;
        if let Some(report) = &output.verify {
            // 验算表不能污染 stdout：JSON 模式下走 stderr
            eprint!("{}", render_verify(report));
        }
        stdout.flush()?;
        return Ok(());
    }
    match output.kind {
        TurnKind::Decision => {
            let scores = (!output.candidate_scores.is_empty())
                .then_some(output.candidate_scores.as_slice());
            write!(
                stdout,
                "{}",
                panel.render(&output.game, &output.actions, scores)?
            )?;
            if let Some(record) = &output.record {
                let reason = record
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.get("reason"))
                    .and_then(|reason| reason.as_str())
                    .unwrap_or("");
                writeln!(
                    stdout,
                    "【AI 决策】T{} {} → [{}] {}",
                    output.turn, record.kind, record.action_index, record.action_desc
                )?;
                if !reason.is_empty() {
                    writeln!(stdout, "【决策理由】{reason}")?;
                }
            }
        }
        TurnKind::Skipped => {
            writeln!(
                stdout,
                "T{} 跳过：{}",
                output.turn,
                output.skip_reason.as_deref().unwrap_or("—")
            )?;
        }
    }
    for note in &output.notes {
        writeln!(stdout, "  · 帧提示：{note}")?;
    }
    if let Some(report) = &output.verify {
        write!(stdout, "{}", render_verify(report))?;
    }
    stdout.flush()?;
    Ok(())
}

/// 渲染终局帧（`type = final`）
fn render_final(
    stdout: &mut impl Write,
    output: &FinalTurnOutput,
    json: bool,
) -> Result<()> {
    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&output.json_payload())?
        )?;
    } else if output.accepted {
        writeln!(
            stdout,
            "【终局】T{} 评分 {} 档次 {}",
            output.turn,
            output.score.unwrap_or(0),
            output.rank.as_deref().unwrap_or("—")
        )?;
    } else {
        writeln!(
            stdout,
            "【终局】T{} 未接受：{}",
            output.turn,
            output.reason.as_deref().unwrap_or("—")
        )?;
    }
    stdout.flush()?;
    Ok(())
}

/// 单帧失败：JSON 档一行 `type = error`，人类档一行提示
fn render_error(stdout: &mut impl Write, json: bool, err: &anyhow::Error) -> Result<()> {
    let message = format!("{err:#}");
    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&error_payload(&message))?
        )?;
    } else {
        writeln!(stdout, "帧处理失败：{message}")?;
    }
    stdout.flush()?;
    Ok(())
}

/// 训练验算表（7 项：速 / 耐 / 力 / 根 / 智 / PT / 体力）
fn render_verify(report: &matsuri_ai::TrainingReport) -> String {
    if report.rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "  ── 训练验算（帧真值 vs 还原预览；最大偏差 {}）──",
        report.max_abs_delta()
    );
    for row in &report.rows {
        let _ = writeln!(
            out,
            "   {}位{} cmd{} 帧 {:?} 预览 {:?} 差 {:?}",
            row.train,
            if row.keiko { "稽古" } else { "训练" },
            row.command_id,
            row.frame,
            row.preview,
            row.delta
        );
    }
    out
}
