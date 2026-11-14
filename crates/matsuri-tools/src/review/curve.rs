//! `review curve`：区间 / 全量 T(n) 曲线（复用 eval 单点与缓存）
//!
//! 只对「有决策面的回合」算点；区间内的点各自带 N 与缓存，同参数重跑直接命中。

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::discover::GameFiles;
use super::eval::{EvalArgs, eval, parse_frame_name};

/// `review curve` 参数
pub struct CurveArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 起点回合（含）
    pub from: Option<i32>,
    /// 终点回合（含）
    pub to: Option<i32>,
    /// 每候选 rollout 次数
    pub n: u32,
    /// 输出目录
    pub out: Option<&'a Path>,
    /// 数据根
    pub data_root: Option<&'a Path>,
    /// 忽略缓存
    pub force: bool,
}

/// 曲线产物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveReport {
    /// 局号
    pub game: u64,
    /// 每候选 rollout 次数
    pub n: u32,
    /// 逐点
    pub points: Vec<CurvePoint>,
    /// 汇总
    pub summary: CurveSummary,
}

/// 一个决策点的曲线行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurvePoint {
    /// 回合
    pub turn: i32,
    /// 决策类型
    pub kind: String,
    /// 该点期望分（无搜索分时为 `None`）
    pub t_n: Option<f64>,
    /// 最优候选分
    pub best: Option<f64>,
    /// 实际选择分
    pub chosen: Option<f64>,
    /// `best − chosen`
    pub regret: Option<f64>,
    /// 配对 95% 区间
    pub ci95: Option<f64>,
}

/// 曲线汇总（只陈述形态，不判失误）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveSummary {
    /// 首点回合
    pub first_turn: Option<i32>,
    /// 末点回合
    pub last_turn: Option<i32>,
    /// 首点 T(n)
    pub first_t_n: Option<f64>,
    /// 末点 T(n)
    pub last_t_n: Option<f64>,
    /// 相邻点最大单步跌幅（回合，跌幅）
    pub max_drop: Option<(i32, f64)>,
    /// 形态：`step` / `ramp` / `sawtooth` / `flat`
    pub shape: String,
    /// 终局实际分（终局帧）
    pub final_score: Option<i32>,
    /// 终局实际 − 末点 T(n)
    pub final_gap: Option<f64>,
}

/// 跑曲线
pub fn curve(args: CurveArgs<'_>) -> Result<(PathBuf, CurveReport)> {
    let files = GameFiles::scan(args.input, args.game)?;
    let mut turns: Vec<i32> = files
        .frames
        .iter()
        .filter_map(|frame| parse_frame_name(&frame.name).map(|(turn, _)| turn))
        .collect();
    turns.sort_unstable();
    turns.dedup();
    let from = args.from.unwrap_or(i32::MIN);
    let to = args.to.unwrap_or(i32::MAX);
    let mut points = Vec::new();
    for turn in turns
        .into_iter()
        .filter(|turn| *turn >= from && *turn <= to)
    {
        let point = EvalArgs {
            input: args.input,
            game: args.game,
            turn,
            seq: None,
            n: args.n,
            out: args.out,
            data_root: args.data_root,
            force: args.force,
            all: false,
        };
        match eval(point) {
            Ok((_, report)) => points.push(CurvePoint {
                turn,
                kind: report.kind,
                t_n: report.t_n,
                best: report.best.as_ref().map(|best| best.score),
                chosen: report.chosen.as_ref().map(|chosen| chosen.score),
                regret: report.regret,
                ci95: report.ci95,
            }),
            Err(error) => eprintln!("跳过 T{turn}：{error:#}"),
        }
    }
    let final_score = files
        .final_frame
        .as_ref()
        .and_then(|frame| matsuri_ai::parse_final_score(&frame.contents).ok())
        .and_then(|frame| frame.calc_score().ok());
    let summary = summarize(&points, final_score);
    let report = CurveReport {
        game: files.game,
        n: args.n,
        points,
        summary,
    };
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
    let path = out.join("curve.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    Ok((path, report))
}

/// 汇总：首末 T(n)、最大单步跌幅、形态
fn summarize(points: &[CurvePoint], final_score: Option<i32>) -> CurveSummary {
    let scored: Vec<(i32, f64)> = points
        .iter()
        .filter_map(|point| point.t_n.map(|t_n| (point.turn, t_n)))
        .collect();
    let first = scored.first().copied();
    let last = scored.last().copied();
    let mut max_drop: Option<(i32, f64)> = None;
    for pair in scored.windows(2) {
        let drop = pair[1].1 - pair[0].1;
        if max_drop.is_none_or(|(_, worst)| drop < worst) {
            max_drop = Some((pair[1].0, drop));
        }
    }
    let shape = shape_of(&scored);
    CurveSummary {
        first_turn: first.map(|(turn, _)| turn),
        last_turn: last.map(|(turn, _)| turn),
        first_t_n: first.map(|(_, t_n)| t_n),
        last_t_n: last.map(|(_, t_n)| t_n),
        max_drop,
        shape,
        final_score,
        final_gap: match (final_score, last) {
            (Some(final_score), Some((_, t_n))) => Some(f64::from(final_score) - t_n),
            _ => None,
        },
    }
}

/// 形态判定（只用于陈述；sawtooth 不标失误）
fn shape_of(scored: &[(i32, f64)]) -> String {
    if scored.len() < 3 {
        return "flat".to_string();
    }
    let deltas: Vec<f64> = scored
        .windows(2)
        .map(|pair| pair[1].1 - pair[0].1)
        .collect();
    let drops = deltas.iter().filter(|delta| **delta < -300.0).count();
    let rises = deltas.iter().filter(|delta| **delta > 300.0).count();
    let big = drops + rises;
    if big == 0 {
        return "flat".to_string();
    }
    if drops == 1 && deltas.iter().skip_while(|delta| **delta >= -300.0).count() > 0 {
        // 单点跌幅后回到平台：阶梯
        let after: Vec<f64> = deltas
            .iter()
            .copied()
            .skip_while(|delta| *delta >= -300.0)
            .skip(1)
            .collect();
        if after.iter().all(|delta| delta.abs() < 100.0) {
            return "step".to_string();
        }
    }
    if drops >= 3 && rises == 0 {
        return "ramp".to_string();
    }
    "sawtooth".to_string()
}
