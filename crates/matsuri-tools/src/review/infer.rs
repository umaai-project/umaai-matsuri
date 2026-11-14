//! `review infer`：纯帧反推的参数化入口（教诲 / 転成 / 学技能 / 追加稽古）
//!
//! 与 `open` 共用事实层（`Facts::build`），只是产出被裁剪过的 `infer.json` 与摘要：
//! 不写 `facts.json` / `brief.md`，不需要算力（没有搜索）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use matsuri_ai::session::ensure_data;
use matsuri_core::{MatsuriConfig, MatsuriData};
use serde_json::{Value, json};

use super::discover::GameFiles;
use super::facts::{Facts, open_session};
use super::tuning::Tuning;

/// `review infer` 参数
pub struct InferArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 只取这些项（`advice` / `fusion` / `skill` / `kensan` / `all`）
    pub what: Vec<String>,
    /// 只取该回合
    pub turn: Option<i32>,
    /// 输出目录；`None` = 与 `open` 同规则
    pub out: Option<&'a Path>,
    /// 数据根
    pub data_root: Option<&'a Path>,
}

/// 跑一次反推，返回（输出路径，结果）
pub fn infer(args: InferArgs<'_>) -> Result<(PathBuf, Value)> {
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

    let want = |name: &str| -> bool {
        args.what.is_empty() || args.what.iter().any(|item| item == "all" || item == name)
    };
    let hit = |turn: i32| args.turn.is_none_or(|only| only == turn);
    let mut report = json!({
        "game": facts.game,
        "turn": args.turn,
        "advice": if want("advice") {
            facts.derive.advice.iter().filter(|row| hit(row.turn)).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
        "fusion": if want("fusion") {
            facts.derive.fusion.iter().filter(|row| hit(row.turn)).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
        "skill": if want("skill") {
            Value::Array(
                facts
                    .derive
                    .skill
                    .events
                    .iter()
                    .filter(|row| hit(row.turn))
                    .map(|row| json!({ "turn": row.turn, "from": row.from, "to": row.to }))
                    .collect(),
            )
        } else {
            Value::Array(Vec::new())
        },
        "kensan": if want("kensan") {
            Value::Array(
                facts
                    .derive
                    .kensan
                    .items
                    .iter()
                    .filter(|row| hit(row.turn))
                    .map(|row| {
                        json!({
                            "turn": row.turn,
                            "act": row.act,
                            "choice": row.choice,
                            "executed": row.executed,
                            "note": row.note,
                        })
                    })
                    .collect(),
            )
        } else {
            Value::Array(Vec::new())
        },
    });

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
    let path = out.join("infer.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    if let Some(object) = report.as_object_mut() {
        object.remove("game");
    }
    Ok((path, report))
}
