//! 手写档决策解释包装（explain 模式，2026-11-06）
//!
//! 用 `--explain` / `MATSURI_EXPLAIN=1` 开启：每回合打印与 manual 档相同的完整面板
//! （[`crate::panel::BoardPanel`]），并在面板末尾追加**候选打分表**（与手写打分同源，
//! 由 `MatsuriTrainer::explain_action` 提供）；教诲分配打印 top-N 方案分；
//! 事件选项打印各选项期望值。默认关闭，正常跑局零输出零位移。

use std::fmt::Write as _;

use anyhow::Result;
use matsuri_core::game::{Action, MatsuriGame};
use matsuri_core::{ActionExplanation, MatsuriTrainer};
use rand::rngs::StdRng;
use umaai_cli::render::term;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::Trainer;

use crate::panel::{
    ANSI_BRIGHT_BLUE, ANSI_BRIGHT_CYAN, BoardPanel, act_short, advice_name, choice_result_name,
    paint,
};

/// 解释包装：内层 = 手写训练员；决策仍由内层给出（本层只打印）
pub struct ExplainTrainer {
    inner: MatsuriTrainer,
    panel: BoardPanel,
}

impl ExplainTrainer {
    /// 包装手写训练员
    pub fn new(inner: MatsuriTrainer) -> Self {
        Self {
            inner,
            panel: BoardPanel::new(term::use_color(false)),
        }
    }

    /// 普通动作：完整面板 + 候选打分表
    fn print_action(&self, game: &MatsuriGame, actions: &[Action]) -> Result<()> {
        match self.inner.explain_action(game, actions) {
            Ok(explain) => {
                print!(
                    "{}",
                    self.panel
                        .render(game, actions, Some(&explain.candidates))?
                );
                println!(
                    "【决策理由】{}（选中 [{}] {}）",
                    explain.reason,
                    explain.chosen,
                    actions
                        .get(explain.chosen)
                        .map(ToString::to_string)
                        .unwrap_or_default()
                );
            }
            Err(err) => {
                // 解释失败不影响跑局：退回不带打分的面板
                print!("{}", self.panel.render(game, actions, None)?);
                eprintln!("解释失败（{err:#}），已退回普通面板");
            }
        }
        Ok(())
    }

    /// 教诲分配：当前分配 + top-N 方案分（120 候选只列前 N）
    fn print_advice(&self, game: &MatsuriGame, actions: &[Action], explain: &ActionExplanation) {
        let view = game.view();
        let res = &view.resources;
        let color = term::use_color(false);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "──────── 教诲分配（T{}，{} 候选）────────",
            view.turn.turn,
            actions.len()
        );
        let mut cells = Vec::new();
        for (act, (base, fused)) in res
            .advice_slots
            .iter()
            .zip(res.advice_fused.iter())
            .enumerate()
        {
            let mut text = if *base == 0 {
                format!("{}←—", act_short(act))
            } else {
                format!("{}←{} {}", act_short(act), base, advice_name(*base))
            };
            if *fused != 0 {
                text.push_str(&format!("→{} {}", fused, advice_name(*fused)));
            }
            cells.push(paint(&text, Some(ANSI_BRIGHT_CYAN), color));
        }
        let _ = writeln!(out, "【当前】{}", cells.join("  "));
        let mut sorted = explain.candidates.clone();
        sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = writeln!(out, "【候选打分】");
        for (rank, (index, value)) in sorted.iter().take(10).enumerate() {
            let text = if value.is_finite() {
                format!("{value:.1}")
            } else {
                "—".to_string()
            };
            let _ = writeln!(
                out,
                "  {rank}. [{index}] 值={text}  {}",
                actions
                    .get(*index)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
        }
        let _ = writeln!(out, "【决策理由】{}", explain.reason);
        print!("{out}");
    }

    /// 事件选项：各选项期望值 + 选项内容（与 manual 的选项标签同口径）
    fn print_choices(
        &self,
        event: Option<&EventData>,
        choices: &[Vec<EventChoice>],
        values: &[f64],
    ) {
        let color = term::use_color(false);
        match event {
            Some(event) => println!(
                "──────── 事件 #{} {}  [{}] ────────",
                event.id,
                paint(&event.name, Some(ANSI_BRIGHT_BLUE), color),
                if event.player_select {
                    "需选择"
                } else {
                    "自动"
                }
            ),
            None => println!("──────── 事件选项 ────────"),
        }
        for (index, group) in choices.iter().enumerate() {
            let value = values.get(index).copied().unwrap_or(f64::NAN);
            println!(
                "  [{index}] 期望={}  {}",
                if value.is_finite() {
                    format!("{value:.1}")
                } else {
                    "—".to_string()
                },
                group_label(group)
            );
        }
    }
}

/// 单组选项标签（组内各结果为一行，按当前口径拼成一行）
fn group_label(group: &[EventChoice]) -> String {
    if group.is_empty() {
        return "（空组）".to_string();
    }
    let mut out = String::new();
    for choice in group {
        let _ = write!(
            out,
            "  {}%{}",
            if choice.prob == 0 { 100 } else { choice.prob },
            if choice.result != 0 {
                format!("[{}]", choice_result_name(choice.result))
            } else {
                String::new()
            }
        );
        let value = choice.value.to_string();
        if !value.is_empty() {
            let _ = write!(out, " {value}");
        }
        if let Some(extra) = &choice.extra_status {
            let _ = write!(out, " 额外[{extra}]");
        }
        if let Some(flags) = &choice.add_flags
            && !flags.to_string().is_empty()
        {
            let _ = write!(out, " 状态+[{}]", flags);
        }
        if let Some(flags) = &choice.remove_flags
            && !flags.to_string().is_empty()
        {
            let _ = write!(out, " 状态-[{}]", flags);
        }
    }
    out.trim_start().to_string()
}

impl Trainer<MatsuriGame> for ExplainTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        // 教诲分配：候选 = 120 方案，只列 top-N（不刷整块面板）
        if matches!(actions.first(), Some(Action::AssignAdvice(_))) {
            let explain = self.inner.explain_action(game, actions)?;
            self.print_advice(game, actions, &explain);
            return self.inner.select_action(game, actions, rng);
        }
        self.print_action(game, actions)?;
        self.inner.select_action(game, actions, rng)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let values = self.inner.explain_choice(game, choices)?;
        self.print_choices(None, choices, &values);
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let values = self.inner.explain_event_choice(game, event, choices)?;
        self.print_choices(Some(event), choices, &values);
        self.inner.select_event_choice(game, event, choices, rng)
    }
}
