//! `review report`：合并 facts + 已缓存 eval / curve → `digest.json` + `report.html`
//!
//! HTML 只做「事实可视化」：4 张内嵌 SVG（分数轨迹 / 五维 / eval 点 regret / 决策与检查构成）
//! 加可注入的 `narrative.md`。叙事由 skill 写，本模块不生成结论。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use matsuri_ai::session::ensure_data;
use matsuri_core::{MatsuriConfig, MatsuriData};
use serde_json::json;

use super::curve::CurveReport;
use super::discover::GameFiles;
use super::eval::EvalReport;
use super::facts::{Facts, display, open_session};
use super::tuning::Tuning;

/// `review report` 参数
pub struct ReportArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 输出目录
    pub out: Option<&'a Path>,
    /// 数据根
    pub data_root: Option<&'a Path>,
    /// 叙事注入文件（markdown）
    pub narrative: Option<&'a Path>,
}

/// 生成报告，返回（html 路径，digest 路径）
pub fn report(args: ReportArgs<'_>) -> Result<(PathBuf, PathBuf)> {
    let files = GameFiles::scan(args.input, args.game)?;
    let data_root = args
        .data_root
        .map_or_else(matsuri_core::testing::data_dir, Path::to_path_buf);
    ensure_data(&data_root)?;
    let engine = umaai::Config::load_with_overlay_file(&data_root, ".")?;
    let data = Arc::new(MatsuriData::load(&data_root)?);
    let config = MatsuriConfig::load_with_overlay_file(&data_root, ".")?;
    let mut session = open_session(&engine, Arc::clone(&data), config)?;
    let facts = Facts::build(&files, &mut session, data.as_ref(), &Tuning::default())?;

    let out = args.out.map_or_else(
        || {
            if args.input.is_dir() {
                args.input.join("review")
            } else {
                args.input.with_extension("review")
            }
        },
        Path::to_path_buf,
    );
    std::fs::create_dir_all(&out)?;
    let evals = read_evals(&out)?;
    let curve = read_curve(&out);
    let narrative = match args.narrative {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("读取叙事文件失败：{}", path.display()))?,
        ),
        None => None,
    };

    let digest = json!({
        "game": facts.game,
        "final": facts.final_score,
        "coverage": facts.coverage,
        "checks": facts.checks,
        "derive": {
            "advice": facts.derive.advice,
            "fusion": facts.derive.fusion,
            "skill": facts.derive.skill,
            "kensan": facts.derive.kensan,
            "inherit": facts.derive.inherit,
        },
        "script": facts.script,
        "evals": evals.iter().map(|report| json!({
            "turn": report.turn,
            "seq": report.seq,
            "kind": report.kind,
            "regret": report.regret,
            "ci95": report.ci95,
            "t_n": report.t_n,
            "best": report.best,
            "chosen": report.chosen,
            "rollout": report.rollout,
            "note": report.note,
        })).collect::<Vec<_>>(),
        "curve": curve.as_ref().map(|curve| json!({
            "summary": curve.summary,
            "points": curve.points,
        })),
    });
    let digest_path = out.join("digest.json");
    std::fs::write(&digest_path, serde_json::to_string_pretty(&digest)?)?;

    let html = render(&facts, &evals, curve.as_ref(), narrative.as_deref());
    let html_path = out.join("report.html");
    std::fs::write(&html_path, html)?;
    Ok((html_path, digest_path))
}

/// 读已缓存的 eval 点（按回合升序）
fn read_evals(out: &Path) -> Result<Vec<EvalReport>> {
    let dir = out.join("recompute");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut rows: Vec<EvalReport> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    rows.sort_by_key(|row| (row.turn, row.seq));
    Ok(rows)
}

/// 读曲线缓存（没有就是 `None`）
fn read_curve(out: &Path) -> Option<CurveReport> {
    let text = std::fs::read_to_string(out.join("curve.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// 渲染 HTML（内嵌 SVG + 可选叙事）
fn render(
    facts: &Facts,
    evals: &[EvalReport],
    curve: Option<&CurveReport>,
    narrative: Option<&str>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<!doctype html><html lang=\"zh\"><head><meta charset=\"utf-8\"><title>赛后回看 · game{}</title>",
        facts.game
    );
    let _ = writeln!(
        out,
        "<style>body{{font-family:system-ui,sans-serif;max-width:980px;margin:24px auto;padding:0 16px;color:#222}}\
         h1{{font-size:22px}}h2{{font-size:17px;margin-top:28px;border-left:4px solid #4676d7;padding-left:8px}}\
         .grid{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}\
         .card{{border:1px solid #ddd;border-radius:8px;padding:8px;background:#fff}}\
         .muted{{color:#666;font-size:13px}}table{{border-collapse:collapse;width:100%;font-size:13px}}\
         td,th{{border-bottom:1px solid #eee;padding:4px 6px;text-align:left}}\
         .gate{{color:#b00020}}.warn{{color:#a06000}}.info{{color:#456}}</style></head><body>"
    );
    let _ = writeln!(out, "<h1>赛后回看 · game{}</h1>", facts.game);
    if let Some(final_score) = &facts.final_score {
        let _ = writeln!(
            out,
            "<p class=\"muted\">终局 {}（{}）· 技能点剩余 {} · 动作 {} 条 · {} 帧</p>",
            final_score.score,
            final_score.rank,
            final_score.skill_pt,
            final_score.actions.unwrap_or(0),
            facts.source.frames
        );
    } else {
        let _ = writeln!(out, "<p class=\"muted\">无终局帧</p>");
    }
    if let Some(text) = narrative {
        let _ = writeln!(
            out,
            "<section class=\"card\"><h2>叙事</h2><p>{}</p></section>",
            escape(text)
        );
    }
    let _ = writeln!(out, "<h2>分数轨迹</h2>");
    let _ = writeln!(out, "{}", svg_score(facts));
    let _ = writeln!(out, "<h2>五维（显示值）</h2>");
    let _ = writeln!(out, "{}", svg_five(facts));
    let _ = writeln!(out, "<h2>eval 点：best / chosen</h2>");
    let _ = writeln!(out, "{}", svg_regret(evals));
    let _ = writeln!(out, "<h2>决策与检查构成</h2>");
    let _ = writeln!(out, "<div class=\"grid\">");
    let _ = writeln!(out, "<div class=\"card\">{}</div>", svg_decisions(facts));
    let _ = writeln!(out, "<div class=\"card\">{}</div>", table_checks(facts));
    let _ = writeln!(out, "</div>");
    if let Some(curve) = curve {
        let _ = writeln!(out, "<h2>曲线汇总</h2>");
        let _ = writeln!(
            out,
            "<p class=\"muted\">{} 点 · 形态 {} · 最大单步跌幅 {:?} · 终局 − 末点 T(n) {:?}</p>",
            curve.points.len(),
            curve.summary.shape,
            curve.summary.max_drop,
            curve.summary.final_gap
        );
    }
    let _ = writeln!(out, "</body></html>");
    out
}

/// 分数轨迹 SVG
fn svg_score(facts: &Facts) -> String {
    let points: Vec<(f64, f64)> = facts
        .turns
        .iter()
        .filter(|row| row.score.is_finite())
        .map(|row| (f64::from(row.turn), row.score))
        .collect();
    line_chart(&points, 760.0, 220.0, 46.0, 24.0)
}

/// 五维 SVG（5 条线）
fn svg_five(facts: &Facts) -> String {
    let mut out = String::from("<svg width=\"760\" height=\"240\" role=\"img\">");
    const COLORS: [&str; 5] = ["#d94a4a", "#e0a020", "#3f9b4f", "#3868c8", "#8a4ac8"];
    for (axis, color) in COLORS.iter().enumerate() {
        let points: Vec<(f64, f64)> = facts
            .turns
            .iter()
            .map(|row| (f64::from(row.turn), f64::from(display(row.five[axis]))))
            .collect();
        out.push_str(&polyline(&points, 760.0, 220.0, 46.0, 24.0, color, 1.5));
    }
    out.push_str(
        "<text x=\"760\" y=\"14\" text-anchor=\"end\" font-size=\"11\" fill=\"#999\">速/耐/力/根/智</text></svg>",
    );
    out
}

/// eval 点 regret 图（best / chosen 双色条）
fn svg_regret(evals: &[EvalReport]) -> String {
    let mut out = String::from("<svg width=\"760\" height=\"240\" role=\"img\">");
    let rows: Vec<&EvalReport> = evals
        .iter()
        .filter(|report| report.best.is_some() || report.chosen.is_some())
        .collect();
    if rows.is_empty() {
        out.push_str("<text x=\"10\" y=\"20\" font-size=\"12\" fill=\"#999\">没有 eval 缓存（先跑 review eval --all）</text></svg>");
        return out;
    }
    let max = rows
        .iter()
        .flat_map(|report| [report.best.as_ref(), report.chosen.as_ref()])
        .flatten()
        .map(|row| row.score)
        .fold(f64::MIN, f64::max);
    let width = 700.0 / rows.len() as f64;
    for (index, report) in rows.iter().enumerate() {
        let x = 30.0 + index as f64 * width;
        for (score, color, offset) in [
            (report.best.as_ref().map(|row| row.score), "#4676d7", 0.0),
            (report.chosen.as_ref().map(|row| row.score), "#d94a4a", 6.0),
        ] {
            let Some(score) = score else { continue };
            let height = (score / max) * 190.0;
            let _ = write!(
                out,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{color}\" opacity=\"0.75\"/>",
                x + offset,
                200.0 - height,
                (width - 10.0).max(2.0),
                height
            );
        }
    }
    out.push_str("<text x=\"760\" y=\"18\" text-anchor=\"end\" font-size=\"11\" fill=\"#999\">蓝=best 红=chosen</text></svg>");
    out
}

/// 折线图（通用）
fn line_chart(points: &[(f64, f64)], width: f64, height: f64, pad_x: f64, pad_y: f64) -> String {
    let mut out = String::from("<svg width=\"760\" height=\"240\" role=\"img\">");
    if points.len() < 2 {
        out.push_str(
            "<text x=\"10\" y=\"20\" font-size=\"12\" fill=\"#999\">数据不足</text></svg>",
        );
        return out;
    }
    let min_x = points.iter().map(|(x, _)| *x).fold(f64::MAX, f64::min);
    let max_x = points.iter().map(|(x, _)| *x).fold(f64::MIN, f64::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f64::MAX, f64::min);
    let max_y = points.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);
    let sx = |x: f64| pad_x + (x - min_x) / (max_x - min_x).max(1.0) * (width - 2.0 * pad_x);
    let sy =
        |y: f64| height - pad_y - (y - min_y) / (max_y - min_y).max(1.0) * (height - 2.0 * pad_y);
    let path: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("{:.1},{:.1}", sx(*x), sy(*y)))
        .collect();
    let _ = write!(
        out,
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#4676d3\" stroke-width=\"2\"/>",
        path.join(" ")
    );
    let _ = write!(
        out,
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#999\">{:.0}</text>",
        width - pad_x,
        height - 6.0,
        max_y
    );
    out.push_str("</svg>");
    out
}

/// 折线片段（多线图用）
fn polyline(
    points: &[(f64, f64)],
    width: f64,
    height: f64,
    pad_x: f64,
    pad_y: f64,
    color: &str,
    width_px: f64,
) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let min_x = points.iter().map(|(x, _)| *x).fold(f64::MAX, f64::min);
    let max_x = points.iter().map(|(x, _)| *x).fold(f64::MIN, f64::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f64::MAX, f64::min);
    let max_y = points.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);
    let sx = |x: f64| pad_x + (x - min_x) / (max_x - min_x).max(1.0) * (width - 2.0 * pad_x);
    let sy =
        |y: f64| height - pad_y - (y - min_y) / (max_y - min_y).max(1.0) * (height - 2.0 * pad_y);
    let path: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("{:.1},{:.1}", sx(*x), sy(*y)))
        .collect();
    format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{width_px}\"/>",
        path.join(" ")
    )
}

/// 决策构成 SVG（横向条）
fn svg_decisions(facts: &Facts) -> String {
    let mut counts = [0usize; 5];
    for row in &facts.derive.decisions {
        let index = match row.kind.as_str() {
            "train" => 0,
            "rest" => 1,
            "race" => 2,
            "event" => 3,
            _ => 4,
        };
        counts[index] += 1;
    }
    let labels = ["训练", "休息", "比赛", "事件", "其它"];
    let max = counts.iter().copied().max().unwrap_or(1).max(1);
    let mut out = String::from("<svg width=\"360\" height=\"170\" role=\"img\">");
    for (index, (label, count)) in labels.iter().zip(counts.iter()).enumerate() {
        let y = 12.0 + index as f64 * 30.0;
        let width = (*count as f64 / max as f64) * 250.0;
        let _ = write!(
            out,
            "<text x=\"6\" y=\"{:.1}\" font-size=\"12\" fill=\"#555\">{label}</text>",
            y + 12.0
        );
        let _ = write!(
            out,
            "<rect x=\"52\" y=\"{:.1}\" width=\"{:.1}\" height=\"16\" fill=\"#4676d7\" opacity=\"0.8\"/>",
            y,
            width.max(1.0)
        );
        let _ = write!(
            out,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#666\">{count}</text>",
            58.0 + width,
            y + 12.0
        );
    }
    out.push_str("</svg>");
    out
}

/// 检查项表
fn table_checks(facts: &Facts) -> String {
    if facts.checks.is_empty() {
        return "<p class=\"muted\">无检查项</p>".to_string();
    }
    let mut out = String::from("<table><tr><th>项</th><th>说明</th></tr>");
    for check in &facts.checks {
        let class = match check.level.as_str() {
            "gate" => "gate",
            "warn" => "warn",
            _ => "info",
        };
        let turn = check
            .turn
            .map_or_else(String::new, |turn| format!("T{turn} "));
        let _ = writeln!(
            out,
            "<tr><td class=\"{class}\">[{}]</td><td>{turn}{}</td></tr>",
            check.level,
            escape(&check.text)
        );
    }
    out.push_str("</table>");
    out
}

/// 最小 HTML 转义
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "<br>")
}
