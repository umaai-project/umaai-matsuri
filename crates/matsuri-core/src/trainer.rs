//! 手写训练员（S7）：守门链 + 打分表 + 事件选项 + 教诲分配择优
//!
//! 目标口径见 [`crate::value`]：官方评分（五维查表 + PT×折算率），PT 再乘配置的
//! `pt_favor_rate`（`1.0` = 官方评分）。所有决策都是**确定性**的（不消耗决策随机流），
//! 保证回放 / CRN / 基线可复现；策略旋钮全部来自 `MatsuriConfig.trainer`（可扫参）。
//!
//! 决策面（引擎契约只给 `Trainer` 三个 hook，教诲分配由剧本在 `Begin` / `End` 主动询问）：
//! - `select_action`：普通动作候选 → 守门链 + 打分表；教诲分配候选 → 价值矩阵择优；
//! - `select_event_choice`：事件选项 → 通用期望值打分（追加稽古「上 / 下」等按语义自动比）；
//! - `select_choice`：旧接口兜底（无事件数据时按通用期望值打分）。

use anyhow::{Result, bail};
use rand::rngs::StdRng;
use umaai_core::Ill;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};

use crate::config::MatsuriConfig;
use crate::game::{Action, MatsuriGame};
use crate::value::Model;

/// 手写训练员（无状态：策略只来自随局配置；便于 `FlatSearch` rollout 复用）
pub struct MatsuriTrainer {
    /// 剧本配置（融合预设 + 训练员旋钮）
    config: MatsuriConfig,
}

impl MatsuriTrainer {
    /// 用剧本配置构造训练员
    pub fn new(config: MatsuriConfig) -> Self {
        Self { config }
    }

    /// 取剧本配置（宿主 / 训练员参数可读）
    #[must_use]
    pub fn config(&self) -> &MatsuriConfig {
        &self.config
    }

    /// 是否需要优先治病（变胖只影响训练速度、每回合 20% 自解，不占动作；不擅长 / 具名病才治）
    fn needs_clinic(game: &MatsuriGame) -> bool {
        let flags = &game.uma().flags;
        flags.bad_trainer || flags.ill.bits() & !Ill::FAT.bits() != 0
    }
}

impl Trainer<MatsuriGame> for MatsuriTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.is_empty() {
            bail!("候选动作为空，无法选择");
        }
        if actions.len() == 1 {
            return Ok(0);
        }
        // 教诲分配决策点：候选全是分配方案 → 价值矩阵择优（确定性）
        if actions
            .iter()
            .all(|action| matches!(action, Action::AssignAdvice(_)))
        {
            let model = Model::new(game, &self.config.trainer)?;
            let mut best = 0;
            let mut best_value = f64::NEG_INFINITY;
            for (index, action) in actions.iter().enumerate() {
                let Action::AssignAdvice(assign) = action else {
                    continue;
                };
                let value = model.assignment_value(assign);
                if value > best_value {
                    best_value = value;
                    best = index;
                }
            }
            return Ok(best);
        }

        // 守门 ①：生病 / 不擅长且可选治病时优先治病
        if Self::needs_clinic(game)
            && let Some(index) = actions.iter().position(|action| *action == Action::Clinic)
        {
            return Ok(index);
        }

        // 守门 ②：强制比赛 / 强制休息回合候选只有一个，已在上面早退；
        // 其余全部进入打分表（训练 / 稽古 / 休息 / 出行 / 比赛同权比较）
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, action) in actions.iter().enumerate() {
            let value = model.action_value(action);
            if value.is_nan() {
                continue;
            }
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        Ok(best)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        if choices.len() == 1 {
            return Ok(0);
        }
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, group) in choices.iter().enumerate() {
            let value = model.event_choice_value(0, index, group);
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        Ok(best)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        if choices.len() == 1 {
            return Ok(0);
        }
        let model = Model::new(game, &self.config.trainer)?;
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, group) in choices.iter().enumerate() {
            let value = model.event_choice_value(event.id, index, group);
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn trainer_prefers_heal_when_bad_trainer() {
        let mut game = crate::testing::sample_game();
        game.uma_mut().flags.bad_trainer = true;
        game.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
        let trainer = MatsuriTrainer::new(MatsuriConfig::default());
        let mut rng = StdRng::seed_from_u64(1);
        let actions = vec![Action::Train(0), Action::Clinic, Action::Rest];
        let index = trainer
            .select_action(&game, &actions, &mut rng)
            .expect("选择动作");
        assert_eq!(actions[index], Action::Clinic, "不擅长训练时优先治病");
    }

    #[test]
    fn trainer_reads_trainer_knobs() {
        let text = std::fs::read_to_string(crate::testing::data_dir().join("matsuri_config.toml"))
            .expect("仓内配置默认层应存在");
        let config = MatsuriConfig::parse(&text, None).expect("默认层应可解析");
        let trainer = MatsuriTrainer::new(config);
        assert_eq!(trainer.config().trainer.pt_favor_rate, 2.0);
    }
}
