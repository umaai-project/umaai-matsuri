//! 复盘工具（事实层）
//!
//! 本模块只做「读局包 → 还原 → 出事实」：不跑搜索、不重算估值。
//! 需要 T(n) / regret 的回合标 `pending`，由后续 `review eval` 按需补。
//!
//! 输入：局目录（散帧 + finalScore）或局包 zip，两者同一条解析路径。

pub mod brief;
pub mod curve;
pub mod derive;
pub mod discover;
pub mod eval;
pub mod facts;
pub mod infer;
pub mod report;
pub mod show;
pub mod tuning;
pub mod whatif;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use matsuri_ai::session::ensure_data;
use matsuri_core::{MatsuriConfig, MatsuriData};

use discover::GameFiles;
use facts::{Facts, open_session};
use tuning::Tuning;

/// 缺省数据根（`--data` 未给时）：优先 exe 旁的 `data/`，再回退仓内数据。
///
/// 查找顺序（以目录内存在 `manifest.json` 为判据）：
/// 1. `<exe 所在目录>/data`：exe 与 data 平铺；
/// 2. `<exe 所在目录>/../data`：skill 布局（`bin/review.exe` + `data/`）；
/// 3. 仓内 `data/`：仅在开发环境成立（`matsuri_core::testing::data_dir()`，编译期路径）。
///
/// exe 位于名为 `bin` 的目录且以上都不存在时，仍按第 2 条返回，
/// 让缺失数据的报错指向 skill 自己的 `data/`，而不是构建机路径。
pub(crate) fn default_data_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin) = exe.parent()
    {
        let beside = bin.join("data");
        if beside.join("manifest.json").is_file() {
            return beside;
        }
        if let Some(root) = bin.parent() {
            let sibling = root.join("data");
            if sibling.join("manifest.json").is_file() {
                return sibling;
            }
            if bin.file_name().and_then(|name| name.to_str()) == Some("bin") {
                return sibling;
            }
        }
    }
    matsuri_core::testing::data_dir()
}

/// `review open` 参数
pub struct OpenArgs<'a> {
    /// 局目录或局包 zip（含 `game{id}_turn*.json` 与 `game{id}_finalScore.json`）
    pub input: &'a Path,
    /// 多局混放时指定局号；`None` = 取帧数最多的一局
    pub game: Option<u64>,
    /// 输出目录；`None` = `<input>/review`
    pub out: Option<&'a Path>,
    /// 数据根；`None` = 按 exe 位置解析（见 `default_data_root`）
    pub data_root: Option<&'a Path>,
    /// 复盘口径（继承标准等）
    pub tuning: Tuning,
}

/// 跑事实层并落盘 `facts.json` / `brief.md`，返回输出目录
pub fn open(args: OpenArgs<'_>) -> Result<PathBuf> {
    let files = GameFiles::scan(args.input, args.game)?;
    let data_root = args
        .data_root
        .map_or_else(default_data_root, Path::to_path_buf);
    ensure_data(&data_root)?;
    let engine = umaai::Config::load_with_overlay_file(&data_root, ".")?;
    let data = Arc::new(MatsuriData::load(&data_root)?);
    let config = MatsuriConfig::load_with_overlay_file(&data_root, ".")?;
    let mut session = open_session(&engine, Arc::clone(&data), config)?;

    let facts = Facts::build(&files, &mut session, data.as_ref(), &args.tuning)?;
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
    std::fs::write(
        out.join("facts.json"),
        serde_json::to_string_pretty(&facts)?,
    )?;
    std::fs::write(out.join("brief.md"), brief::render(&facts))?;
    Ok(out)
}
