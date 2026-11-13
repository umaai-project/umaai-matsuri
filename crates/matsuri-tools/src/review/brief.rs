//! `brief.md`：事实层的单页呈现（主路径一次读完）
//!
//! 只铺事实与检查项，不下叙事结论；叙事留给 skill。

use std::fmt::Write as _;

use super::facts::Facts;

/// 渲染 brief
pub fn render(facts: &Facts) -> String {
    let mut out = String::new();
    let game = facts.game;
    let _ = writeln!(out, "# 赛后回看事实 · game{game}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "> 由 review open 生成；只含事实与检查项。T(n) / regret 需要按需重算（见末尾 pending）。"
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## §0 数据来源与覆盖");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- 来源：{}（{} 帧）",
        facts.source.kind, facts.source.frames
    );
    let _ = writeln!(
        out,
        "- 回合：{}-{}",
        facts
            .coverage
            .first_turn
            .map_or("—".to_string(), |t| format!("T{t}")),
        facts
            .coverage
            .last_turn
            .map_or("—".to_string(), |t| format!("T{t}")),
    );
    let _ = writeln!(
        out,
        "- review 段：{}；解析失败帧：{}",
        if facts.coverage.has_review {
            "有"
        } else {
            "无（老包）"
        },
        facts.coverage.failed_frames.len()
    );
    if !facts.coverage.missing_turns.is_empty() {
        let _ = writeln!(out, "- 缺帧：{} 个", facts.coverage.missing_turns.len());
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §1 总体");
    let _ = writeln!(out);
    match &facts.final_score {
        Some(final_score) => {
            let _ = writeln!(
                out,
                "- 终局评分 **{}（{}）**；技能点剩余 {}；动作 {} 条",
                final_score.score,
                final_score.rank,
                final_score.skill_pt,
                final_score.actions.map_or(0, |n| n)
            );
            let five: Vec<String> = (0..5)
                .map(|i| super::facts::display(final_score.five[i]).to_string())
                .collect();
            let _ = writeln!(out, "- 终局五维（显示值）：{}", five.join(" / "));
        }
        None => {
            let _ = writeln!(out, "- 无终局数据");
        }
    }
    if let Some(last) = facts.turns.last() {
        let five: Vec<String> = (0..5)
            .map(|i| super::facts::display(last.five[i]).to_string())
            .collect();
        let _ = writeln!(
            out,
            "- 末帧 T{}：体力 {}/{}，干劲 {}，技能点 {}，五维 {}",
            last.turn,
            last.vital,
            facts
                .final_score
                .as_ref()
                .map_or(last.vital, |_| last.vital),
            last.motivation,
            last.skill_pt,
            five.join(" / ")
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §5 状态波动");
    let _ = writeln!(out);
    let low: Vec<&super::facts::TurnFact> =
        facts.turns.iter().filter(|row| row.vital < 35).collect();
    if low.is_empty() {
        let _ = writeln!(out, "- 体力低点（<35）：无");
    } else {
        let min = low.iter().map(|row| row.vital).min().unwrap_or(0);
        let _ = writeln!(
            out,
            "- 体力低点（<35）：{} 次，最低 {}（{}）",
            low.len(),
            min,
            low.iter()
                .find(|r| r.vital == min)
                .map_or_else(|| "—".to_string(), |r| format!("T{}", r.turn))
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §7 检查项");
    let _ = writeln!(out);
    if facts.checks.is_empty() {
        let _ = writeln!(out, "- 无");
    }
    for check in &facts.checks {
        let turn = check.turn.map_or_else(String::new, |t| format!("T{t} "));
        let _ = writeln!(out, "- [{}] {}{}", check.level, turn, check.text);
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## 待重算（pending）");
    let _ = writeln!(out);
    if facts.pending.is_empty() {
        let _ = writeln!(out, "- 无");
    }
    for item in &facts.pending {
        let _ = writeln!(
            out,
            "- T{} {}（估算 {} ms，N=1024）",
            item.turn, item.kind, item.est_ms
        );
    }
    out
}
