//! 帧来源：真机目录（`FileChannel`）与离线回放目录（`--replay`）
//!
//! **不自建 watcher**：稳定读、去重、终局帧匹配全部复用引擎 [`FileChannel`]。
//! 回放只是把同一个 [`RawFrame`] 形态按文件名顺序吐出来，因此在线循环对两者完全同构——
//! `--replay` 与真机跑的是同一条代码路径，差别只在帧从哪来。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use umaai::{ChannelNote, FileChannel, FrameKind, RawFrame};

/// 真机根目录的环境变量（`--root` 未给时读它）
pub const ROOT_ENV: &str = "MATSURI_ROOT";

/// 帧来源
pub enum FrameSource {
    /// 真机：`.portable` 目录监听
    Live(Box<FileChannel>),
    /// 离线回放：目录里的 `game{id}_turn{n}.json` 按回合顺序
    Replay(ReplaySource),
}

impl FrameSource {
    /// 打开真机来源（根目录 = 含 `.portable` 的游戏根）
    pub fn open_live(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Live(Box::new(FileChannel::open(root)?)))
    }

    /// 打开回放来源（目录里直接放帧文件，或给含 `.portable` 的游戏根）
    pub fn open_replay(dir: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Replay(ReplaySource::load(dir.as_ref())?))
    }

    /// 取下一帧；`Ok(None)` = 这一拍没有新帧（回放口径下 = 已放完）
    pub fn next_frame(&mut self, timeout: Duration) -> Result<Option<RawFrame>> {
        match self {
            Self::Live(channel) => channel.next_frame(timeout),
            Self::Replay(replay) => Ok(replay.next_frame()),
        }
    }

    /// 取走自愈痕迹（回放没有痕迹）
    pub fn drain_notes(&mut self) -> Vec<ChannelNote> {
        match self {
            Self::Live(channel) => channel.drain_notes(),
            Self::Replay(_) => Vec::new(),
        }
    }

    /// 是否为回放（宿主据此决定超时 / 结束语义）
    #[must_use]
    pub fn is_replay(&self) -> bool {
        matches!(self, Self::Replay(_))
    }
}

/// 回放来源：预先读好的帧序列
///
/// 排序口径 **按局分组、局内按回合**：目录里常同时躺着多局的帧（`game6308_*` / `game6309_*` /
/// `game6310_*`），跨局按回合混排会让在线会话反复切局（跨帧基准每帧清零）。回放的多局
/// 语义就是「一局跑完再跑下一局」。
pub struct ReplaySource {
    /// `(文件名, 原文)`
    frames: VecDeque<(String, String)>,
    /// 已吐出帧数（给宿主报进度）
    emitted: usize,
    /// 目录里出现的局号（升序、去重）
    games: Vec<u64>,
}

impl ReplaySource {
    /// 载入目录里的全部回合帧（按 `turn` 升序、同回合按文件名升序）
    pub fn load(dir: &Path) -> Result<Self> {
        let dir = locate_replay_dir(dir)?;
        // (局号, 回合, 文件名, 路径)
        let mut entries: Vec<(u64, i32, String, PathBuf)> = Vec::new();
        let reader = std::fs::read_dir(&dir)
            .with_context(|| format!("读回放目录失败：{}", dir.display()))?;
        for entry in reader {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((game, turn)) = replay_key_of(&name) else {
                continue;
            };
            entries.push((game, turn, name, entry.path()));
        }
        if entries.is_empty() {
            bail!(
                "回放目录里没有回合帧（{}）：期望 `game{{id}}_turn{{n}}.json`",
                dir.display()
            );
        }
        entries.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let games = {
            let mut games: Vec<u64> = entries.iter().map(|entry| entry.0).collect();
            games.dedup();
            games
        };
        let mut frames = VecDeque::with_capacity(entries.len());
        for (_, _, name, path) in entries {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("读回放帧失败：{}", path.display()))?;
            frames.push_back((name, contents));
        }
        Ok(Self {
            frames,
            emitted: 0,
            games,
        })
    }

    /// 下一帧（吐完返回 `None`）
    pub fn next_frame(&mut self) -> Option<RawFrame> {
        let (name, contents) = self.frames.pop_front()?;
        self.emitted += 1;
        log::debug!("回放帧 #{name}（第 {} 帧）", self.emitted);
        Some(RawFrame {
            kind: FrameKind::ThisTurn,
            contents,
        })
    }

    /// 已吐出帧数
    #[must_use]
    pub fn emitted(&self) -> usize {
        self.emitted
    }

    /// 剩余帧数
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.frames.len()
    }

    /// 目录里出现过的局号（升序；用于宿主提示「本次回放了几局」）
    #[must_use]
    pub fn games(&self) -> &[u64] {
        &self.games
    }
}

/// 文件名 → `(局号, 回合号)`：`game{id}_turn{n}.json` / `game{id}_turn{n}_{k}.json` / `thisTurn.json`
///
/// 终局帧（`game{id}_finalScore.json`）不是回合帧，回放时跳过。
fn replay_key_of(name: &str) -> Option<(u64, i32)> {
    if name == umaai::protocol::channel::FRAME_THIS_TURN {
        // `thisTurn.json` 是「当前这一帧」的滚动副本（内容等于某局的最后一帧）：
        // 回放只跑归档帧，读它会多出一帧上一局尾巴
        return None;
    }
    let rest = name.strip_prefix("game")?.strip_suffix(".json")?;
    let (game, tail) = rest.split_once("_turn")?;
    let game: u64 = game.parse().ok()?;
    let digits: String = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    let turn: i32 = digits.parse().ok()?;
    Some((game, turn))
}

/// 回放目录定位：给的可能是游戏根（含 `.portable`），也可能是插件目录本身
fn locate_replay_dir(target: &Path) -> Result<PathBuf> {
    if !target.exists() {
        return Err(anyhow!("回放目录不存在：{}", target.display()));
    }
    let plugin = umaai::plugin_dir(target);
    if plugin.is_dir() {
        return Ok(plugin);
    }
    if target.is_dir() {
        return Ok(target.to_path_buf());
    }
    Err(anyhow!("回放目录不可用：{}", target.display()))
}

/// 真机根目录解析：`--root` > `MATSURI_ROOT` > 当前目录（要求含 `.portable`）
///
/// 注意根目录是**游戏根**（`FileChannel` 自己会拼 `.portable/PluginData/...`），
/// 不是插件目录。
pub fn resolve_live_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root.to_path_buf());
    }
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        return Ok(PathBuf::from(root));
    }
    let cwd = std::env::current_dir().context("取当前目录失败")?;
    if umaai::plugin_dir(&cwd).is_dir() {
        return Ok(cwd);
    }
    Err(anyhow!(
        "没找到小黑板数据目录：请用 --root 指定游戏根目录，或设 {ROOT_ENV}，或在含 `.portable` 的目录下运行"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_number_from_file_name() {
        assert_eq!(replay_key_of("game6310_turn40.json"), Some((6310, 40)));
        assert_eq!(replay_key_of("game6310_turn40_2.json"), Some((6310, 40)));
        assert_eq!(replay_key_of("game6310_turn0.json"), Some((6310, 0)));
        assert_eq!(replay_key_of("thisTurn.json"), None, "滚动副本不参与回放");
        assert_eq!(replay_key_of("game6310_finalScore.json"), None);
        assert_eq!(replay_key_of("notes.txt"), None);
    }

    /// 回放顺序：**先按局分组**，局内按回合、同回合按文件名；终局帧不参与
    #[test]
    fn replay_orders_by_game_then_turn() {
        let dir = std::env::temp_dir().join(format!("matsuri_replay_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建目录");
        // 故意打乱写入顺序，并混入第二局与终局帧
        for (name, tag) in [
            ("game2_turn0.json", "g2t0"),
            ("game1_turn2.json", "g1t2"),
            ("game1_turn0.json", "g1t0"),
            ("game2_turn1.json", "g2t1"),
            ("game1_turn1_2.json", "g1t1b"),
            ("game1_turn1.json", "g1t1"),
            ("game1_finalScore.json", "final"),
        ] {
            std::fs::write(dir.join(name), tag).expect("写帧");
        }
        let mut replay = ReplaySource::load(&dir).expect("载入回放");
        let mut order = Vec::new();
        while let Some(frame) = replay.next_frame() {
            order.push(frame.contents);
        }
        assert_eq!(
            order,
            vec!["g1t0", "g1t1", "g1t1b", "g1t2", "g2t0", "g2t1"],
            "先跑完 1 局再跑 2 局；同回合 `_k` 在后"
        );
        assert_eq!(replay.games(), &[1, 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
