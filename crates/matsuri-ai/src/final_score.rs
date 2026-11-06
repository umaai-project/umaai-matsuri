//! 终局帧（`game{id}_finalScore.json`）解析与剧本门禁
//!
//! 引擎 `FinalScoreFrame` 只负责 DTO 与评分；本模块补两条剧本侧约束：
//!
//! 1. `scenarioId` 必须是骏大祭（15）——目录里可能躺着别的剧本终局帧；
//! 2. 只认引擎已解析的 `game{digits}_finalScore.json`（旧固定名不进入本函数）。
//!
//! 三道闸（必填解析 → `is_usable` → 局号匹配）由调用方 `OnlineSession` 按顺序执行，
//! 本模块不替调用方决定「局号不符时是否仍显示分数」。

use anyhow::{Context, Result, bail};
use umaai::FinalScoreFrame;

use crate::frame::MATSURI_SCENARIO_ID;

/// 解析终局帧并校验剧本
pub fn parse_final_score(contents: &str) -> Result<FinalScoreFrame> {
    let frame = FinalScoreFrame::parse(contents).context("终局帧解析失败")?;
    if frame.scenario_id != MATSURI_SCENARIO_ID {
        bail!(
            "不是骏大祭的终局帧：scenarioId={}（本剧本只接受 {MATSURI_SCENARIO_ID}）",
            frame.scenario_id
        );
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(scenario: u32) -> String {
        format!(
            r#"{{"scenarioId":{scenario},"single_mode_chara_id":6310,"turn":77,
                "fiveStatus":[100,0,0,0,0],"fiveStatusLimit":[200,0,0,0,0],"skillPt":100}}"#
        )
    }

    /// 层1：本剧本终局帧可解析；别的剧本拒绝
    #[test]
    fn scenario_gate() {
        let parsed = parse_final_score(&frame(15)).expect("骏大祭终局帧应可解析");
        assert_eq!(parsed.epoch_key(), Some(6310));
        assert!(parse_final_score(&frame(14)).is_err(), "别的剧本应拒绝");
    }

    /// 层1：契约必填缺失时在引擎层就炸，不退化成零分
    #[test]
    fn required_fields_gate() {
        let missing = r#"{"scenarioId":15,"single_mode_chara_id":6310,"turn":77,
            "fiveStatusLimit":[200,0,0,0,0],"skillPt":100}"#;
        assert!(parse_final_score(missing).is_err(), "缺 fiveStatus 应报错");
    }
}
