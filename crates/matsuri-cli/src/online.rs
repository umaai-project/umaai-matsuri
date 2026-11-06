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
use matsuri_ai::output::OnlineTurnOutput;
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind, TurnKind, resolve_live_root};
use matsuri_core::{MatsuriConfig, MatsuriData};
use umaai::Config;
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
     --replay          回放目录（game{id}_turn{n}.json）；给了就不挂真机监听\n  \
     --trainer         训练员：matsuri（手写，默认） / search（搜索）\n  \
     --json            stdout 每行一条 JSON（日志与面板走 stderr）\n  \
     --verify-training 附训练验算表（帧真值 vs 还原态预览，G13）"
        .to_string()
}

/// 跑在线档
pub fn run(argv: &[String]) -> Result<()> {
    let args = parse(argv)?;
    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与离线档同约定）
    let cfg = Config::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入配置失败")?;
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(crate::DATA_ROOT)
        .map(|_| ())
        .context("引擎静态数据初始化失败")?;
    let data = Arc::new(MatsuriData::load(crate::DATA_ROOT).context("载入剧本数据失败")?);
    let matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入剧本配置失败")?;
    let trainer = TrainerKind::parse(args.trainer.as_deref().unwrap_or("matsuri"))?;
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
    let mut frames = 0usize;
    let mut decisions = 0usize;

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
        frames += 1;
        let output = session
            .handle_frame(&frame.contents)
            .with_context(|| format!("第 {frames} 帧处理失败（拒绝计算）"))?;
        if output.kind == TurnKind::Decision {
            decisions += 1;
        }
        render(&mut stdout, &mut panel, &output, args.json)?;
    }

    if !args.json {
        eprintln!("结束：处理 {frames} 帧，其中 {decisions} 次决策");
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
            write!(
                stdout,
                "{}",
                panel.render(&output.game, &output.actions, None)?
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
