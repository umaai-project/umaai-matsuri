//! 局包发现：目录或 zip → 有序帧文本
//!
//! 文件名契约（与插件一致）：`game{id}_turn{N}[_{k}].json` + `game{id}_finalScore.json`。
//! 只接受新协议帧（含 `charaEffects`）；旧 `illEffects` 帧直接报错，不做字段映射。

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// 一帧的原文
#[derive(Debug, Clone)]
pub struct FrameText {
    /// 文件名（不含目录）
    pub name: String,
    /// 帧原文
    pub contents: String,
}

/// 扫描期按局号聚合的条目：回合帧（回合 / 序号 / 原文）+ 终局帧
type GameEntries = std::collections::BTreeMap<u64, (Vec<(i32, u32, FrameText)>, Option<FrameText>)>;

/// 一局的帧集合
#[derive(Debug, Clone)]
pub struct GameFiles {
    /// 局号（`single_mode_chara_id`）
    pub game: u64,
    /// 回合帧，按 (回合, 同回合序号) 升序
    pub frames: Vec<FrameText>,
    /// 终局帧
    pub final_frame: Option<FrameText>,
    /// 输入形态：`dir` / `zip`
    pub source: String,
}

impl GameFiles {
    /// 扫描目录或 zip；多局混放时取帧数最多的一局（可用 `game` 指定）
    pub fn scan(input: &Path, game: Option<u64>) -> Result<Self> {
        let (entries, source) = if input.is_dir() {
            (read_dir(input)?, "dir")
        } else {
            (read_zip(input)?, "zip")
        };
        Self::collect(entries, source, game)
    }

    fn collect(entries: Vec<(String, String)>, source: &str, only: Option<u64>) -> Result<Self> {
        let mut by_game: GameEntries = std::collections::BTreeMap::new();
        for (name, contents) in entries {
            let base = basename(&name).to_string();
            let Some((game, rest)) = base.trim_end_matches(".json").split_once('_') else {
                continue;
            };
            let Ok(game) = game.trim_start_matches("game").parse::<u64>() else {
                continue;
            };
            if let Some(only) = only
                && game != only
            {
                continue;
            }
            let slot = by_game.entry(game).or_default();
            if rest == "finalScore" {
                slot.1 = Some(FrameText {
                    name: base,
                    contents,
                });
            } else if let Some(after) = rest.strip_prefix("turn") {
                let mut parts = after.split('_');
                let turn = parts.next().and_then(|v| v.parse::<i32>().ok());
                let seq = parts
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                if let Some(turn) = turn {
                    ensure_new_protocol(&base, &contents)?;
                    slot.0.push((
                        turn,
                        seq,
                        FrameText {
                            name: base,
                            contents,
                        },
                    ));
                }
            }
        }
        let picked = match only {
            Some(game) => by_game.into_iter().find(|(id, _)| *id == game),
            None => by_game.into_iter().max_by_key(|(_, (f, _))| f.len()),
        };
        let Some((game, (mut frames, final_frame))) = picked else {
            let hint = only.map_or_else(String::new, |game| format!("（未找到 game{game}）"));
            bail!("输入里没有可识别的局帧{hint}：{}", source);
        };
        frames.sort_by_key(|(turn, seq, _)| (*turn, *seq));
        Ok(Self {
            game,
            frames: frames.into_iter().map(|(_, _, frame)| frame).collect(),
            final_frame,
            source: source.to_string(),
        })
    }
}

/// 旧协议守卫：有 `illEffects` 且无 `charaEffects` 的帧不支持
fn ensure_new_protocol(name: &str, contents: &str) -> Result<()> {
    if contents.contains("\"illEffects\"") && !contents.contains("\"charaEffects\"") {
        bail!(
            "帧 {name} 是旧协议（illEffects）：本工具只支持 charaEffects 之后的新帧，请用新版插件重录"
        );
    }
    Ok(())
}

fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn read_dir(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
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
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("读取帧失败：{}", path.display()))?;
        out.push((name.to_string(), contents));
    }
    Ok(out)
}

fn read_zip(path: &Path) -> Result<Vec<(String, String)>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("打开局包失败：{}", path.display()))?;
    let mut zip =
        zip::ZipArchive::new(file).with_context(|| format!("解析局包失败：{}", path.display()))?;
    let mut out = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .with_context(|| format!("读取包内条目 {index} 失败"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let base = basename(&name).to_string();
        if base == "thisTurn.json" || !base.ends_with(".json") {
            continue;
        }
        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .with_context(|| format!("读取包内帧失败：{name}"))?;
        out.push((base, contents));
    }
    Ok(out)
}
