//! 手动训练员（无颜色纯文本样例）
//!
//! 用途：机制核对时由人逐决策选择，直接观察局面与候选；不引终端重依赖，只用标准输入输出。
//! 后续实现在这里补自己的资源面板与动作说明（颜色 / 表格归终端前端，不进本文件）。

use std::io::{self, Write};

use anyhow::{Context, Result, anyhow};
use matsuri_core::game::{Action, MatsuriGame};
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};

/// 手动训练员：打印局面与候选，从标准输入读取编号
pub struct ManualTrainer;

impl ManualTrainer {
    /// 读取一个落在 `0..len` 的下标；空行重试，`q` 退出
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

    /// 打印局面摘要与动作候选（纯文本，无颜色）
    fn show_board(game: &MatsuriGame, actions: &[Action]) -> Result<String> {
        let consts = &Data::get()?.consts;
        let uma = game.uma();
        let mut out = String::new();
        out.push_str(&format!(
            "回合 {}/{}  阶段 {}\n",
            game.turn(),
            game.max_turn(),
            game.stage().get()
        ));
        out.push_str(&format!(
            "体力 {}/{}  干劲 {}  技能点 {}  五维 速{} 耐{} 力{} 根{} 智{}\n",
            uma.vital,
            uma.max_vital,
            uma.motivation,
            uma.skill_pt,
            uma.five_status[0],
            uma.five_status[1],
            uma.five_status[2],
            uma.five_status[3],
            uma.five_status[4]
        ));
        out.push_str("人头:\n");
        for person in game.persons() {
            out.push_str(&format!(
                "  [{}] {} 得意={} 羁绊={} 红点={}\n",
                person.person_index,
                person.short_name(),
                person.train_type,
                person.friendship,
                if person.is_hint { "是" } else { "否" }
            ));
        }
        out.push_str("训练位人数:");
        for train in 0..5 {
            out.push_str(&format!(
                " {}{}",
                consts.train_name(train),
                game.count_training_persons(train)
            ));
        }
        out.push('\n');
        out.push_str("可选动作:\n");
        for (idx, action) in actions.iter().enumerate() {
            out.push_str(&format!("  [{idx}] {action}\n"));
        }
        Ok(out)
    }

    /// 打印事件选项组（每组 = 一个可选分支，组内是该分支的可能结果）
    fn show_choices(choices: &[Vec<EventChoice>]) -> String {
        let mut out = String::from("事件选项:\n");
        for (idx, group) in choices.iter().enumerate() {
            out.push_str(&format!("  [{idx}]"));
            if group.is_empty() {
                out.push_str(" （空组）");
            }
            for choice in group {
                out.push_str(&format!("  概率{}% {}", choice.prob, choice.value));
            }
            out.push('\n');
        }
        out
    }
}

impl Trainer<MatsuriGame> for ManualTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        println!("{}", Self::show_board(game, actions)?);
        Self::read_index("> 选择动作编号: ", actions.len())
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        println!("{}", Self::show_choices(choices));
        Self::read_index("> 选择事件选项编号: ", choices.len())
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        println!("事件 #{} {}", event.id, event.name);
        self.select_choice(game, choices, rng)
    }
}
