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
use matsuri_core::{MAX_TURN, MatsuriConfig, MatsuriData, SearchCandidate};
use umaai::{Config, DecisionRecord, FrameKind, LuckAttach};
use umaai_cli::render::term;
use umaai_core::Data;

use crate::logging;
use crate::panel::{BoardPanel, choice_effect_label, paint};

/// 在线档参数
#[derive(Debug, Default, clap::Args)]
pub struct Args {
    /// 真机游戏根目录（含 `.portable`）；缺省读 MATSURI_ROOT，再缺省当前目录
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,
    /// 回放目录（逐帧 JSON）；给了就不挂真机监听
    #[arg(long, value_name = "DIR")]
    pub replay: Option<PathBuf>,
    /// 训练员：`matsuri` 手写 / `search` 搜索；缺省按 `[search].search_n` 决定
    #[arg(long, value_name = "TRAINER")]
    pub trainer: Option<String>,
    /// stdout 每行一条 JSON（日志与面板走 stderr）
    #[arg(long)]
    pub json: bool,
    /// 附训练验算表（帧真值 vs 还原态预览）
    #[arg(long = "verify-training")]
    pub verify: bool,
}

/// 跑在线档
pub fn run(args: Args) -> Result<()> {
    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与离线档同约定）
    let mut cfg = Config::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入配置失败")?;
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(crate::DATA_ROOT)
        .map(|_| ())
        .context("引擎静态数据初始化失败")?;
    let data = Arc::new(MatsuriData::load(crate::DATA_ROOT).context("载入剧本数据失败")?);
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(crate::DATA_ROOT, ".").context("载入剧本配置失败")?;
    let comp = matsuri_core::deck_comp(&cfg.sim.cards).context("卡组构成解析失败")?;
    matsuri_cfg = matsuri_cfg.for_deck(comp);
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
            if trainer.is_search() {
                "search"
            } else {
                "matsuri"
            }
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

    // 面板：JSON 模式一律关色（stdout 只允许 JSON）；在线档用精简档（不显示演目分配表）
    let color = term::use_color(args.json);
    let panel = BoardPanel::new(color).compact();
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
                let handled = session.handle_frame_with(&frame.contents, |game, actions| {
                    // 搜索前先显示还原后的局面（JSON 模式不渲染面板）
                    if !args.json {
                        write!(stdout, "{}", panel.render(game, actions, None)?)?;
                        write!(stdout, "{}", search_banner(color))?;
                        stdout.flush()?;
                    }
                    Ok(())
                });
                match handled {
                    Ok(output) => {
                        if output.kind == TurnKind::Decision {
                            decisions += 1;
                        }
                        render(
                            &mut stdout,
                            &output,
                            args.json,
                            color,
                            cfg.record.luck_turn_bonus,
                        )?;
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

/// 搜索开始前的分隔线与提示（面板之后、决策之前）
fn search_banner(color: bool) -> String {
    format!(
        "{}\n{}\n",
        paint(&"─".repeat(36), Some("90"), color),
        paint("AI计算中…", Some("96"), color)
    )
}

/// 渲染一帧的输出
fn render(
    stdout: &mut impl Write,
    output: &OnlineTurnOutput,
    json: bool,
    color: bool,
    luck_bonus: i32,
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
        TurnKind::Decision => render_decision(stdout, output, color, luck_bonus)?,
        TurnKind::Skipped => {
            writeln!(
                stdout,
                "{}",
                paint(
                    &format!(
                        "T{} 跳过：{}",
                        output.turn,
                        output.skip_reason.as_deref().unwrap_or("—")
                    ),
                    Some("33"),
                    color
                )
            )?;
        }
    }
    // 跳过帧已有黄色跳过行说明原因，帧提示只在决策帧输出
    render_notes(stdout, output.kind, &output.notes)?;
    if let Some(report) = &output.verify {
        write!(stdout, "{}", render_verify(report))?;
    }
    stdout.flush()?;
    Ok(())
}

/// 帧提示：跳过帧不打印（黄色跳过行已经给出原因），决策帧照常输出
fn render_notes(stdout: &mut impl Write, kind: TurnKind, notes: &[String]) -> Result<()> {
    if kind != TurnKind::Decision {
        return Ok(());
    }
    for note in notes {
        writeln!(stdout, "  · 帧提示：{note}")?;
    }
    Ok(())
}

/// 决策候选展示上限（首选 + 其余；后续接配置 `[output] decision_top_n`）
const DECISION_TOP_N: usize = 5;

/// 极简决策输出：运气行 + 首选 / Top N（涕分差 + 终局五维 / PT 差）
fn render_decision(
    stdout: &mut impl Write,
    output: &OnlineTurnOutput,
    color: bool,
    luck_bonus: i32,
) -> Result<()> {
    let Some(record) = &output.record else {
        return Ok(());
    };
    if let Some(luck) = &record.luck {
        // 搜索档额外带「期望 ± 单局 σ + 档次」；手写档 / 无摘要时为 `None`
        let expected = output.summary.as_ref().map(|summary| {
            (
                summary.expected_score,
                summary.baseline_stdev,
                summary.expected_rank.as_str(),
            )
        });
        render_luck(stdout, output.turn, luck, expected, luck_bonus, color)?;
    }
    if let Some(fusion) = record
        .extra
        .as_ref()
        .and_then(|extra| extra.get("fusion"))
        .and_then(|fusion| fusion.as_str())
    {
        writeln!(stdout, "[回合 {} 融合]{}", output.turn, fusion)?;
    }
    // 事件决策前先回显「还原出来的事件」与各选项实际效果（帧可能没带 `story`，属性以本地数据为准）
    if record.kind == "event" {
        render_event_detail(stdout, output, color)?;
    }
    let kind = decision_kind_label(&record.kind);
    let top = "41;93";
    if let Some(summary) = &output.summary
        && !summary.candidates.is_empty()
    {
        let Some(chosen) = summary
            .candidates
            .iter()
            .find(|candidate| candidate.index == summary.chosen)
            .or_else(|| summary.candidates.first())
        else {
            return Ok(());
        };
        let top_delta = chosen.score - summary.baseline;
        writeln!(
            stdout,
            "[回合 {} {}]{}",
            output.turn,
            kind,
            paint(
                &format!("首选：{} {top_delta:+.0}", chosen.description),
                Some(top),
                color
            )
        )?;
        let mut rest: Vec<&SearchCandidate> = summary
            .candidates
            .iter()
            .filter(|candidate| candidate.index != chosen.index)
            .collect();
        rest.sort_by(|a, b| b.score.total_cmp(&a.score));
        // 同一有效布局（801 硬约束后）只展示一次，避免同分方案占满 Top N
        let mut seen = std::collections::HashSet::new();
        seen.insert(chosen.description.clone());
        let mut rank = 2;
        for candidate in rest {
            if rank > DECISION_TOP_N || !seen.insert(candidate.description.clone()) {
                continue;
            }
            let diff = candidate.score - summary.baseline;
            let code =
                candidate_color(chosen.score, candidate.score, resolution(chosen, candidate));
            let line = format!(
                "#{}: {} {diff:+.0} ({})",
                rank,
                candidate.description,
                terminal_delta(candidate, chosen)
            );
            writeln!(stdout, "{}", paint(&line, Some(code), color))?;
            rank += 1;
        }
        render_preset_diff(stdout, output.turn, record, color)?;
        return Ok(());
    }
    // 手写档 / 无搜索分：只报首选，不伪造候选分
    writeln!(
        stdout,
        "[回合 {} {}]{}",
        output.turn,
        kind,
        paint(&format!("首选：{}", record.action_desc), Some(top), color)
    )?;
    render_preset_diff(stdout, output.turn, record, color)?;
    Ok(())
}

/// 预设差异行：与 `AdvicePreset` 对比；一致时明确标出可自动选择
fn render_preset_diff(
    stdout: &mut impl Write,
    turn: i32,
    record: &DecisionRecord,
    color: bool,
) -> Result<()> {
    let Some(diff) = record
        .extra
        .as_ref()
        .and_then(|extra| extra.get("preset_diff"))
    else {
        return Ok(());
    };
    let Some(text) = diff.get("text").and_then(|text| text.as_str()) else {
        return Ok(());
    };
    writeln!(
        stdout,
        "[回合 {turn} 预设]{}",
        paint(text, Some("90"), color)
    )?;
    Ok(())
}

/// 决策类别中文名
fn decision_kind_label(kind: &str) -> &'static str {
    match kind {
        "train" => "训练",
        "race" => "比赛",
        "event" => "事件",
        "advice" => "教诲",
        _ => "决策",
    }
}

/// 事件回显：事件名 / id + 每个选项的实际效果（帧 `story` 有文本时附在「选项N」后）
///
/// 属性一律取**还原出的 `EventData`**（本仓数据），不读帧里的效果文本：插件只透传 id 时
/// 帧文本本来就没有，而两边的数值口径也可能不同。没有选项文本时退化成「选项1 / 选项2」。
fn render_event_detail(
    stdout: &mut impl Write,
    output: &OnlineTurnOutput,
    color: bool,
) -> Result<()> {
    let Some(event) = matsuri_ai::online::pending_event(&output.game) else {
        return Ok(());
    };
    if event.name.is_empty() {
        writeln!(stdout, "[回合 {} 事件]#{}", output.turn, event.id)?;
    } else {
        writeln!(
            stdout,
            "[回合 {} 事件]{} #{}",
            output.turn,
            paint(&event.name, Some("94"), color),
            event.id
        )?;
    }
    for (index, group) in event.choices.iter().enumerate() {
        let text = output
            .story_options
            .get(index)
            .map(String::as_str)
            .unwrap_or_default();
        let text = if text.is_empty() {
            String::new()
        } else {
            format!("（{text}）")
        };
        writeln!(
            stdout,
            "  选项{}{}：{}",
            index + 1,
            text,
            choice_effect_label(group)
        )?;
    }
    Ok(())
}

/// 运气行：本局分 <0 红 / <500 黄 / <1500 绿 / ≥1500 亮绿；本轮亮青
///
/// `expected` = `(官方口径期望分 E₀, 单局终局 σ, 档次名)`：E₀ 取 `SearchSummary::expected_score`
/// （搜索分因 PT 项被放大，不能直接当期望分用），显示时按 `E₀ + (MAX_TURN − turn) × luck_bonus`
/// 投影到显示分空间，与「本局/本轮」同量纲；档次仍按 E₀ 查 `constants.rank_names`（空串则不写），
/// σ 取候选样本方差的合并值。分数与档次亮青，σ 不着色。
/// σ 为 0 / 无摘要时整段不显示（手写档、搜索 fallback 都走这里）。
fn render_luck(
    stdout: &mut impl Write,
    turn: i32,
    luck: &LuckAttach,
    expected: Option<(f64, f64, &str)>,
    luck_bonus: i32,
    color: bool,
) -> Result<()> {
    let total = luck.snapshot.total_luck_score.round() as i64;
    let total_code = if total < 0 {
        "31"
    } else if total < 500 {
        "33"
    } else if total < 1500 {
        "32"
    } else {
        "92"
    };
    let delta = luck
        .snapshot
        .last_turn_delta
        .map_or_else(|| "--".to_string(), |d| format!("{:+.0}", d));
    let expected = expected
        .filter(|(_, stdev, _)| *stdev > 0.0)
        .map(|(score, stdev, rank)| {
            // 显示层把官方口径期望分投影到显示分空间（与「本局/本轮」同量纲）
            let display = score + (MAX_TURN - turn) as f64 * f64::from(luck_bonus);
            let score = paint(&format!("{display:.0}"), Some("96"), color);
            let tail = if rank.is_empty() {
                format!("（±{stdev:.0}）")
            } else {
                format!("（±{stdev:.0}，{}）", paint(rank, Some("96"), color))
            };
            format!(", 期望 {score} {tail}")
        })
        .unwrap_or_default();
    writeln!(
        stdout,
        "[回合 {} 运气]本局：{}, 本轮: {}{expected}",
        turn,
        paint(&format!("{total:+}"), Some(total_code), color),
        paint(&delta, Some("96"), color)
    )?;
    Ok(())
}

/// 两候选差的 95% 分辨率（单候选标准误合成）
fn resolution(chosen: &SearchCandidate, other: &SearchCandidate) -> f64 {
    let se = |candidate: &SearchCandidate| {
        if candidate.n <= 1 {
            0.0
        } else {
            candidate.stdev / f64::from(candidate.n).sqrt()
        }
    };
    let se_chosen = se(chosen);
    let se_other = se(other);
    1.96 * (se_chosen * se_chosen + se_other * se_other).sqrt()
}

/// 候选颜色：绿（分辨率内）/ 白（落后 <500）/ 灰（落后 ≥500）
fn candidate_color(chosen_score: f64, candidate_score: f64, resolution: f64) -> &'static str {
    let behind = chosen_score - candidate_score;
    if behind.abs() <= resolution {
        "32"
    } else if behind < 500.0 {
        "37"
    } else {
        "90"
    }
}

/// 相对首选的终局五维 / PT 差
fn terminal_delta(candidate: &SearchCandidate, chosen: &SearchCandidate) -> String {
    const NAMES: [&str; 5] = ["速", "耐", "力", "根", "智"];
    let mut parts: Vec<String> = (0..5)
        .map(|axis| {
            format!(
                "{}{:+}",
                NAMES[axis],
                (candidate.five[axis] - chosen.five[axis]).round() as i64
            )
        })
        .collect();
    parts.push(format!("PT{:+}", (candidate.pt - chosen.pt).round() as i64));
    parts.join(" ")
}

/// 渲染终局帧（`type = final`）
fn render_final(stdout: &mut impl Write, output: &FinalTurnOutput, json: bool) -> Result<()> {
    if json {
        writeln!(stdout, "{}", serde_json::to_string(&output.json_payload())?)?;
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

#[cfg(test)]
mod tests {
    use umaai::{LuckAttach, LuckScoreSnapshot};

    use super::{TurnKind, candidate_color, render_luck, render_notes};

    /// 单一快照（终局显示分口径）：`delta` 为本轮运气
    fn luck(total: f64, delta: Option<f64>) -> LuckAttach {
        LuckAttach {
            snapshot: LuckScoreSnapshot {
                initial_terminal_baseline: 50_000.0,
                current_terminal_baseline: 50_000.0 + total,
                total_luck_score: total,
                last_turn_delta: delta,
            },
            action_luck: Vec::new(),
        }
    }

    /// 运气行：搜索档追加「期望 分数 （±σ，档次）」；无摘要 / σ=0 时不追加
    #[test]
    fn render_luck_appends_expected_score() {
        let mut out = Vec::new();
        render_luck(
            &mut out,
            32,
            &luck(150.0, Some(150.0)),
            Some((82140.0, 3012.0, "LF")),
            0,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "[回合 32 运气]本局：+150, 本轮: +150, 期望 82140 （±3012，LF）\n"
        );

        let mut out = Vec::new();
        render_luck(&mut out, 32, &luck(-40.0, None), None, 0, false).expect("渲染运气行");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "[回合 32 运气]本局：-40, 本轮: --\n"
        );

        let mut out = Vec::new();
        render_luck(
            &mut out,
            32,
            &luck(0.0, None),
            Some((80000.0, 0.0, "UF")),
            0,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "[回合 32 运气]本局：+0, 本轮: --\n",
            "σ 为 0 时不显示期望段"
        );

        // 数据未初始化 / rank_names 缺失 → 档次留空，只写 σ
        let mut out = Vec::new();
        render_luck(
            &mut out,
            32,
            &luck(0.0, None),
            Some((80000.0, 900.0, "")),
            0,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "[回合 32 运气]本局：+0, 本轮: --, 期望 80000 （±900）\n"
        );

        // 上色：期望分与档次亮青（96），σ 不着色
        let mut out = Vec::new();
        render_luck(
            &mut out,
            32,
            &luck(150.0, Some(150.0)),
            Some((82140.0, 3012.0, "LF")),
            0,
            true,
        )
        .expect("渲染运气行");
        let text = String::from_utf8(out).expect("utf8");
        assert!(
            text.contains("\u{1b}[96m82140\u{1b}[0m"),
            "期望分应亮青：{text}"
        );
        assert!(text.contains("\u{1b}[96mLF\u{1b}[0m"), "档次应亮青：{text}");
        assert!(!text.contains("\u{1b}[96m±"), "σ 不着色：{text}");
    }

    /// 期望分按回合投影到显示分空间：`E = E₀ + (77 − turn) × luck_bonus`
    #[test]
    fn render_luck_projects_expected_by_turn() {
        let mut early = Vec::new();
        render_luck(
            &mut early,
            0,
            &luck(0.0, None),
            Some((82140.0, 3000.0, "LF")),
            100,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(early).expect("utf8"),
            "[回合 0 运气]本局：+0, 本轮: --, 期望 89840 （±3000，LF）\n"
        );

        let mut late = Vec::new();
        render_luck(
            &mut late,
            76,
            &luck(0.0, None),
            Some((82140.0, 3000.0, "LF")),
            100,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(late).expect("utf8"),
            "[回合 76 运气]本局：+0, 本轮: --, 期望 82240 （±3000，LF）\n"
        );

        // bonus = 0 → 投影退化为官方口径
        let mut raw = Vec::new();
        render_luck(
            &mut raw,
            0,
            &luck(0.0, None),
            Some((82140.0, 3000.0, "LF")),
            0,
            false,
        )
        .expect("渲染运气行");
        assert_eq!(
            String::from_utf8(raw).expect("utf8"),
            "[回合 0 运气]本局：+0, 本轮: --, 期望 82140 （±3000，LF）\n"
        );
    }

    /// 跳过帧已有黄色跳过行：帧提示只在决策帧输出
    #[test]
    fn render_notes_only_on_decision() {
        let notes = vec![
            "EventSkipped { id: 501099715 }".to_string(),
            "storyId 未收录".to_string(),
        ];
        let mut skipped = Vec::new();
        render_notes(&mut skipped, TurnKind::Skipped, &notes).expect("渲染帧提示");
        assert!(
            String::from_utf8(skipped).expect("utf8").is_empty(),
            "跳过帧不应再打印帧提示"
        );

        let mut decision = Vec::new();
        render_notes(&mut decision, TurnKind::Decision, &notes).expect("渲染帧提示");
        assert_eq!(
            String::from_utf8(decision).expect("utf8"),
            "  · 帧提示：EventSkipped { id: 501099715 }\n  · 帧提示：storyId 未收录\n"
        );
    }

    #[test]
    fn candidate_color_has_three_tiers() {
        assert_eq!(candidate_color(1000.0, 950.0, 60.0), "32", "分辨率内绿色");
        assert_eq!(candidate_color(1000.0, 700.0, 50.0), "37", "落后 <500 白色");
        assert_eq!(candidate_color(1000.0, 400.0, 50.0), "90", "落后 ≥500 灰色");
        assert_eq!(candidate_color(1000.0, 500.0, 50.0), "90", "边界 500 归灰");
    }
}
