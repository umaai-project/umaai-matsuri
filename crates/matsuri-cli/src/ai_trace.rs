//! AI 在场标记（只给复盘用）
//!
//! T77 帧到达时写 `game{id}_ai-decision.json`：文件存在 = 本局 AI 陪到局终；
//! 中途退出不写。内容只放「AI 最终状态」（开始回合 / 最后一手估分），不含过程。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use matsuri_ai::{OnlineTurnOutput, TurnKind};
use umasim::search::FlatSearchGame;

/// 一局的 AI 侧状态（跨帧累积，换局重置）
#[derive(Debug, Default)]
pub struct AiTrace {
    epoch: Option<u64>,
    first_turn: Option<i32>,
    last_turn: Option<i32>,
    expected: Option<f64>,
    rank: Option<String>,
    written: bool,
}

impl AiTrace {
    /// 逐帧观察：只在决策帧更新「最后一手」；换局重置
    pub fn observe(&mut self, output: &OnlineTurnOutput) {
        if self.epoch != Some(output.epoch) {
            self.epoch = Some(output.epoch);
            self.first_turn = None;
            self.last_turn = None;
            self.expected = None;
            self.rank = None;
            self.written = false;
        }
        if output.kind != TurnKind::Decision {
            return;
        }
        if self.first_turn.is_none() {
            self.first_turn = Some(output.view.turn.turn);
        }
        self.last_turn = Some(output.view.turn.turn);
        if let Some(summary) = &output.summary {
            self.expected = Some(summary.expected_score);
            self.rank = Some(summary.expected_rank.clone());
        } else if let Ok(score) = output.game.search_score(1.0) {
            self.expected = Some(score);
            self.rank = None;
        }
    }

    /// 局终写一次（同一局重复调用只写第一份）
    pub fn flush(&mut self, output: &OnlineTurnOutput, dir: &Path, mode: &str) -> Result<()> {
        if self.written {
            return Ok(());
        }
        let payload = serde_json::json!({
            "schema": 1,
            "game": output.epoch,
            "turn": output.view.turn.turn,
            "firstTurn": self.first_turn,
            "lastTurn": self.last_turn,
            "expected": self.expected,
            "rank": self.rank,
            "mode": mode,
            "writtenAt": now_ms(),
        });
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("game{}_ai-decision.json", output.epoch));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&payload)?)?;
        std::fs::rename(&tmp, &path)?;
        self.written = true;
        Ok(())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
