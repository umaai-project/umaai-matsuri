//! 局包发现：散帧目录 → 有序帧序列
//!
//! 文件名契约（与插件一致）：`game{id}_turn{N}[_{k}].json` + `game{id}_finalScore.json`。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// 一局的帧文件集合
#[derive(Debug, Clone)]
pub struct GameFiles {
    /// 局号（`single_mode_chara_id`）
    pub game: u64,
    /// 回合帧，按 (回合, 同回合序号) 升序
    pub frames: Vec<PathBuf>,
    /// 终局帧
    pub final_frame: Option<PathBuf>,
}

impl GameFiles {
    /// 扫描目录；多局混放时取帧数最多的那一局
    pub fn scan(dir: &Path) -> Result<Self> {
        let mut by_game: std::collections::BTreeMap<
            u64,
            (Vec<(i32, u32, PathBuf)>, Option<PathBuf>),
        > = std::collections::BTreeMap::new();
        for entry in
            std::fs::read_dir(dir).with_context(|| format!("读取局目录失败：{}", dir.display()))?
        {
            let path = entry?.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name == "thisTurn.json" || !name.ends_with(".json") {
                continue;
            }
            let Some((game, rest)) = name.trim_end_matches(".json").split_once('_') else {
                continue;
            };
            let Ok(game) = game.trim_start_matches("game").parse::<u64>() else {
                continue;
            };
            let slot = by_game.entry(game).or_default();
            if rest == "finalScore" {
                slot.1 = Some(path);
            } else if let Some(after) = rest.strip_prefix("turn") {
                let mut parts = after.split('_');
                let turn = parts.next().and_then(|v| v.parse::<i32>().ok());
                let seq = parts
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                if let Some(turn) = turn {
                    slot.0.push((turn, seq, path));
                }
            }
        }
        let Some((game, (mut frames, final_frame))) =
            by_game.into_iter().max_by_key(|(_, (f, _))| f.len())
        else {
            bail!("目录里没有可识别的局帧：{}", dir.display());
        };
        frames.sort_by_key(|(turn, seq, _)| (*turn, *seq));
        Ok(Self {
            game,
            frames: frames.into_iter().map(|(_, _, path)| path).collect(),
            final_frame,
        })
    }
}
