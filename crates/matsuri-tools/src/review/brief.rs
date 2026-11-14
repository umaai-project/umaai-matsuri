//! `brief.md`：事实层的单页呈现（主路径一次读完）
//!
//! 只铺事实与检查项，不下叙事结论；叙事留给 skill。

use std::fmt::Write as _;

use super::facts::{Facts, display};

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
            "无（未终局或无终局帧）"
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
                "- 终局评分 **{}（{}）**；技能点剩余 {}；本局花掉 {}；动作 {} 条",
                final_score.score,
                final_score.rank,
                final_score.skill_pt,
                final_score.skill_pt_spent,
                final_score.actions.unwrap_or(0)
            );
            let five: Vec<String> = (0..5)
                .map(|i| display(final_score.five[i]).to_string())
                .collect();
            let _ = writeln!(out, "- 终局五维（显示值）：{}", five.join(" / "));
            if !final_score.inherit_gains.is_empty() {
                let gains: Vec<String> = final_score
                    .inherit_gains
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                let _ = writeln!(out, "- 继承增量（终局帧）：{}", gains.join(" / "));
            }
        }
        None => {
            let _ = writeln!(out, "- 无终局数据");
        }
    }
    if let Some(last) = facts.turns.last() {
        let five: Vec<String> = (0..5).map(|i| display(last.five[i]).to_string()).collect();
        let _ = writeln!(
            out,
            "- 末帧 T{}：体力 {}，干劲 {}，技能点 {}，五维 {}",
            last.turn,
            last.vital,
            last.motivation,
            last.skill_pt,
            five.join(" / ")
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §2 教诲与転成（帧 diff 反推）");
    let _ = writeln!(out);
    if facts.derive.advice.is_empty() {
        let _ = writeln!(out, "- 教诲：无落位记录");
    } else {
        let acts: Vec<String> = facts
            .derive
            .advice
            .iter()
            .map(|row| format!("T{} 演目{}→{}", row.turn, row.act + 1, row.advice_id))
            .collect();
        let _ = writeln!(out, "- 落位 {} 条：{}", acts.len(), acts.join("，"));
    }
    if facts.derive.fusion.is_empty() {
        let _ = writeln!(out, "- 転成：无");
    } else {
        let rows: Vec<String> = facts
            .derive
            .fusion
            .iter()
            .map(|row| {
                format!(
                    "T{} 演目{} {}→{}",
                    row.turn,
                    row.act + 1,
                    row.base,
                    row.target
                )
            })
            .collect();
        let _ = writeln!(out, "- 転成 {} 条：{}", rows.len(), rows.join("，"));
    }
    let skill = &facts.derive.skill;
    if skill.events.is_empty() {
        if facts.final_score.is_some() {
            let _ = writeln!(
                out,
                "- 学技能：无剩余点下降（终局 skillPtSpent={}）",
                skill.spent_total
            );
        } else {
            let _ = writeln!(out, "- 学技能：无剩余点下降（无终局帧，花费总额不可得）");
        }
    } else {
        let rows: Vec<String> = skill
            .events
            .iter()
            .map(|row| format!("T{} {}→{}", row.turn, row.from, row.to))
            .collect();
        let _ = writeln!(out, "- 学技能：{}", rows.join("，"));
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §3 追加稽古（事件 400015106）");
    let _ = writeln!(out);
    let kensan = &facts.derive.kensan;
    if kensan.items.is_empty() {
        let _ = writeln!(out, "- 无触发");
    } else {
        let _ = writeln!(
            out,
            "- 触发 {} 次：选加练 {} / 选休息 {} / 不可判 {}",
            kensan.triggers, kensan.executed, kensan.wasted, kensan.rested
        );
        for item in &kensan.items {
            let act = item
                .act
                .map_or("—".to_string(), |act| format!("演目{}", act + 1));
            let choice = item
                .choice
                .map_or("—".to_string(), |choice| choice.to_string());
            let executed = match item.executed {
                Some(true) => "加练",
                Some(false) => "休息",
                None => "不可判",
            };
            let _ = writeln!(
                out,
                "  - T{} {} 选择 {} → {}（{}）",
                item.turn, act, choice, executed, item.note
            );
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §4 实际训练（actions + 预览匹配）");
    let _ = writeln!(out);
    let training = &facts.derive.training;
    if training.is_empty() {
        let _ = writeln!(out, "- 无训练记录（缺 actions）");
    } else {
        let failed = training.iter().filter(|row| row.failed).count();
        let keiko = training.iter().filter(|row| row.keiko.is_some()).count();
        let matched = training.iter().filter(|row| row.matched.is_some()).count();
        let _ = writeln!(
            out,
            "- 训练 {} 次：稽古 {} / 普通 {}；预览匹配 {}；失败 {}",
            training.len(),
            keiko,
            training.len() - keiko,
            matched,
            failed
        );
        for row in training.iter().filter(|row| row.failed) {
            let _ = writeln!(
                out,
                "  - T{} 命令 {} 失败（预览五维非零、实际零增长）",
                row.turn, row.cmd
            );
        }
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

    let _ = writeln!(out, "## §6 合宿延长");
    let _ = writeln!(out);
    let extended: Vec<i32> = facts
        .turns
        .iter()
        .filter(|row| row.camp_extended)
        .map(|row| row.turn)
        .collect();
    if extended.is_empty() {
        let _ = writeln!(out, "- 无延长期回合");
    } else {
        let list: Vec<String> = extended.iter().map(|turn| format!("T{turn}")).collect();
        let _ = writeln!(
            out,
            "- 延长期回合 {} 个：{}",
            extended.len(),
            list.join("，")
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## §8 剧本专属");
    let _ = writeln!(out);
    let script = &facts.script;
    // (1) 演目进度
    for act in &script.acts {
        let nodes: Vec<String> = act
            .nodes
            .iter()
            .map(|(level, turn)| format!("Lv{level}@T{turn}"))
            .collect();
        let _ = writeln!(
            out,
            "- 演目{}（训练位 {}，{}）：局末 Lv{}，节点 {}，稽古 {} 次",
            act.act + 1,
            act.train,
            if act.card { "有卡位" } else { "无卡位" },
            act.max_level,
            if nodes.is_empty() {
                "—".to_string()
            } else {
                nodes.join(" ")
            },
            act.keiko_actions
        );
    }
    // (2) 出行
    let outing = &script.outing;
    let stages: Vec<String> = outing
        .stages
        .iter()
        .map(|turn| format!("T{turn}"))
        .collect();
    let _ = writeln!(
        out,
        "- 友人出行 {}/5 段（{}）；T23 前 {} 段；走满 {}",
        outing.used,
        if stages.is_empty() {
            "—".to_string()
        } else {
            stages.join(" / ")
        },
        outing.before_t23,
        outing
            .full_turn
            .map_or("未走满".to_string(), |turn| format!("T{turn}"))
    );
    // (3) 阴跌窗口
    if script.swings.is_empty() {
        let _ = writeln!(out, "- 阴跌窗口（5 回合 ΔT < −300）：无");
    } else {
        let rows: Vec<String> = script
            .swings
            .iter()
            .map(|window| format!("T{}–T{}（{}）", window.from, window.to, window.delta))
            .collect();
        let _ = writeln!(out, "- 阴跌窗口（5 回合 ΔT < −300）：{}", rows.join("，"));
    }
    // (4) 继承
    if script.inherit.is_empty() {
        let _ = writeln!(out, "- 继承：无记录");
    } else {
        let rows: Vec<String> = script
            .inherit
            .iter()
            .map(|step| {
                format!(
                    "T{} {}（标准 {:.0}，Δ{:+.0}）",
                    step.turn, step.gain, step.expect, step.delta
                )
            })
            .collect();
        let _ = writeln!(out, "- 继承：{}", rows.join("，"));
    }
    // (6) 状态资源
    let status = &script.status;
    let _ = writeln!(
        out,
        "- 状态资源：切者={} 小切={} 心情盾={} 幸运体质={} 休息心得={} 病={}",
        status.qie,
        status.xiaoqie,
        status.positive_thinking,
        status.lucky,
        status.refresh_mind,
        status.ill
    );
    // E7 自选赛
    if !script.free_races.is_empty() {
        let rows: Vec<String> = script
            .free_races
            .iter()
            .map(|race| {
                format!(
                    "T{}–T{} {}/{} 场{}",
                    race.start,
                    race.end,
                    race.done,
                    race.required,
                    race.filled_turn
                        .map_or(String::new(), |turn| format!("（达标 T{turn}）"))
                )
            })
            .collect();
        let _ = writeln!(out, "- 自选赛：{}", rows.join("，"));
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
