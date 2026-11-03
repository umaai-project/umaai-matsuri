//! 手动训练员（S8：机制核对面板）
//!
//! 用途：机制核对时由人逐决策选择。每次动作决策前打印完整面板（渲染在 [`crate::panel`]，
//! 与手写档 explain 模式共用同一份）：回合结构 / 数值与状态 / 剧本资源与教诲（advice）分配 /
//! 彩色训练明细表 / 合宿·友人链·赛程 / 动作候选预览。事件选择时打印选项组与各分支数值；
//! 动作选择在交互终端用 Inquire 列表，非 TTY 退回编号输入。
//!
//! 本档**不择优**（教诲分配只支持「保持现状」）；策略档见 `matsuri`（手写训练员）。

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, anyhow};
use inquire::Select;
use matsuri_core::game::{Action, MatsuriGame, MatsuriView};
use rand::rngs::StdRng;
use umaai_cli::render::term;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::Trainer;

use crate::panel::{
    ANSI_BRIGHT_BLUE, ANSI_BRIGHT_CYAN, BoardPanel, act_short, advice_name, choice_result_name,
    paint,
};

pub(crate) use crate::panel::newgame_config;

/// 是否交互终端（Inquire 可用）；非 TTY（测试 / 管道）退回编号输入
fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// 手动训练员：面板状态（跨提示保留）
pub struct ManualTrainer {
    /// 共用回合面板（快照 / 事件回显状态在其内部）
    panel: BoardPanel,
}

impl ManualTrainer {
    /// 新建手动训练员
    pub fn new() -> Self {
        Self {
            panel: BoardPanel::new(term::use_color(false)),
        }
    }

    /// 读取一个落在 0..len 的下标；空行重试，q 退出
    fn read_index(prompt: &str, len: usize) -> Result<usize> {
        let stdin = io::stdin();
        loop {
            print!("{prompt}");
            io::stdout().flush().context("刷新标准输出失败")?;
            let mut line = String::new();
            if stdin.read_line(&mut line).context("读取标准输入失败")? == 0 {
                return Err(anyhow!("标准输入已关闭"));
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            if text.eq_ignore_ascii_case("q") {
                return Err(anyhow!("手动训练员退出"));
            }
            match text.parse::<usize>() {
                Ok(idx) if idx < len => return Ok(idx),
                _ => println!(
                    "无效输入：请输入 0..{} 之间的编号（q 退出）",
                    len.saturating_sub(1)
                ),
            }
        }
    }

    /// 渲染一次回合面板（与手写档 explain 共用）
    fn show_board(&self, game: &MatsuriGame, actions: &[Action]) -> Result<String> {
        self.panel.render(game, actions, None)
    }

    /// 教诲分配决策：面板只显示当前槽位分配 + 保持现状（本档不择优；策略档见 `matsuri`）
    fn choose_advice(&self, view: &MatsuriView, actions: &[Action]) -> Result<usize> {
        let color = term::use_color(false);
        let res = &view.resources;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "──────── 教诲分配（T{}，{} 个候选）────────",
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
        let _ = writeln!(out, "  [0] 保持现状（手动档不择优）");
        print!("{out}");
        if interactive() {
            let options = vec!["保持现状".to_string()];
            let answer = Select::new("教诲分配（手动档只支持保持现状）", options)
                .raw_prompt()
                .map_err(|err| anyhow!("教诲分配选择被中止: {err}"))?;
            return Ok(answer.index);
        }
        Ok(0)
    }

    /// 选择动作：交互终端用 Inquire 列表；非 TTY（测试 / 管道）退回编号输入
    fn choose_action(&self, actions: &[Action]) -> Result<usize> {
        if !interactive() {
            return Self::read_index("> 选择动作编号: ", actions.len());
        }
        let options: Vec<String> = actions
            .iter()
            .enumerate()
            .map(|(index, action)| format!("[{index}] {}", self.panel.action_label(action)))
            .collect();
        let answer = Select::new("选择动作（↑/↓ 移动 · Enter 确认 · Esc 退出）", options)
            .raw_prompt()
            .map_err(|err| anyhow!("手动选择被中止: {err}"))?;
        Ok(answer.index)
    }

    /// 事件选项头（事件名用亮蓝色突出）
    fn choice_header(event: Option<&EventData>) -> String {
        let color = term::use_color(false);
        match event {
            Some(event) => format!(
                "──────── 事件 #{} {}  [{}] ────────",
                event.id,
                paint(&event.name, Some(ANSI_BRIGHT_BLUE), color),
                if event.player_select {
                    "需选择"
                } else {
                    "自动"
                }
            ),
            None => "──────── 事件选项 ────────".to_string(),
        }
    }

    /// 单组选项的标签（组内各结果为一行，按当前口径拼成一行）
    fn choice_group_label(group: &[EventChoice]) -> String {
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

    /// 事件选项：交互终端用 Inquire 列表；非 TTY（测试 / 管道）退回编号输入
    fn choose_choice(
        event: Option<&EventData>,
        choices: &[Vec<EventChoice>],
        prompt: &str,
    ) -> Result<usize> {
        if !interactive() {
            println!("{}", Self::show_choices(event, choices));
            return Self::read_index(prompt, choices.len());
        }
        println!("{}", Self::choice_header(event));
        let options: Vec<String> = choices
            .iter()
            .enumerate()
            .map(|(index, group)| format!("[{index}] {}", Self::choice_group_label(group)))
            .collect();
        let answer = Select::new("选择事件选项（↑/↓ 移动 · Enter 确认 · Esc 退出）", options)
            .raw_prompt()
            .map_err(|err| anyhow!("手动选择被中止: {err}"))?;
        Ok(answer.index)
    }

    /// 打印事件选项组（非交互终端 / 测试用）
    fn show_choices(event: Option<&EventData>, choices: &[Vec<EventChoice>]) -> String {
        let mut out = Self::choice_header(event);
        out.push('\n');
        for (idx, group) in choices.iter().enumerate() {
            let _ = writeln!(out, "  [{idx}]  {}", Self::choice_group_label(group));
        }
        out
    }
}

impl Default for ManualTrainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer<MatsuriGame> for ManualTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        // 教诲分配决策点：候选全是分配方案，用独立的一行提示（不刷整块面板 / 120 行列表）
        if matches!(actions.first(), Some(Action::AssignAdvice(_))) {
            let view = game.view();
            return self.choose_advice(&view, actions);
        }
        println!("{}", self.show_board(game, actions)?);
        self.choose_action(actions)
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Self::choose_choice(None, choices, "> 选择事件选项编号: ")
    }

    fn select_event_choice(
        &self,
        _game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Self::choose_choice(Some(event), choices, "> 选择事件选项编号: ")
    }
}
