//! 样例训练员（`MatsuriTrainer`）
//!
//! 只演示「训练员落在 core」的接线：按固定顺序轮转选训练，事件选项取首项。
//! 剧本配置在 S7 训练员参数落地前只做载入 / 校验（随局持有），不参与决策。
//! 后续实现在这里替换为自己的手写策略 / 搜索训练员；动作与决策的随机纪律由引擎契约约束。

use anyhow::{Result, anyhow};
use rand::rngs::StdRng;
use umaai_core::gamedata::EventChoice;
use umasim::game::{Game, Trainer};

use crate::MatsuriConfig;
use crate::game::{Action, MatsuriGame};

/// 样例训练员的训练轮转顺序（0-4：速 / 耐 / 力 / 根 / 智；S7 换成策略参数）
const SAMPLE_ORDER: [usize; 5] = [0, 1, 2, 3, 4];

/// 样例训练员的休息阈值（体力低于此值且可选休息时先休息；S7 换成策略参数）
const SAMPLE_REST_VITAL: i32 = 40;

/// 样例训练员：按固定顺序轮转训练
pub struct MatsuriTrainer {
    /// 剧本配置（S7 训练员参数落地前只随局持有，不参与决策）
    config: MatsuriConfig,
}

impl MatsuriTrainer {
    /// 用剧本配置构造训练员
    pub fn new(config: MatsuriConfig) -> Self {
        Self { config }
    }

    /// 取剧本配置（宿主 / 训练员参数可读；当前决策未使用）
    #[must_use]
    pub fn config(&self) -> &MatsuriConfig {
        &self.config
    }

    /// 取当前回合偏好的训练下标（0-4）
    fn preferred_training(&self, turn: i32) -> usize {
        SAMPLE_ORDER[turn.max(0) as usize % SAMPLE_ORDER.len()]
    }
}

impl Trainer<MatsuriGame> for MatsuriTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        let train = self.preferred_training(game.turn());
        // 样例策略（S4 占位，S7 整体替换）：生病先治病、体力见底先休息，
        // 然后偏好训练位的普通训练 → 同位的演目稽古 → 任意普通训练 → 任意稽古
        // → 比赛（目标赛 / URA 强制回合只有比赛）→ 兜底取第一个。
        if !game.uma().flags.ill.is_empty()
            && let Some(index) = actions.iter().position(|action| *action == Action::Clinic)
        {
            return Ok(index);
        }
        if game.uma().vital < SAMPLE_REST_VITAL
            && let Some(index) = actions.iter().position(|action| *action == Action::Rest)
        {
            return Ok(index);
        }
        if let Some(index) = actions
            .iter()
            .position(|action| matches!(action, Action::Train(idx) if usize::from(*idx) == train))
        {
            return Ok(index);
        }
        if let Some(index) = actions.iter().position(
            |action| matches!(action, Action::ActTrain(idx) if usize::from(*idx) == train),
        ) {
            return Ok(index);
        }
        if let Some(index) = actions
            .iter()
            .position(|action| matches!(action, Action::Train(_)))
        {
            return Ok(index);
        }
        if let Some(index) = actions
            .iter()
            .position(|action| matches!(action, Action::ActTrain(_)))
        {
            return Ok(index);
        }
        if let Some(index) = actions.iter().position(|action| *action == Action::Race) {
            return Ok(index);
        }
        actions
            .first()
            .map(|_| 0)
            .ok_or_else(|| anyhow!("候选动作为空，无法选择"))
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        _choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        // S1 占位：后续实现在这里按事件内容选择（可用 `select_event_choice` 拿完整事件）
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_training_wraps() {
        let trainer = MatsuriTrainer::new(MatsuriConfig::default());
        assert_eq!(trainer.preferred_training(0), 0);
        assert_eq!(trainer.preferred_training(2), 2);
        assert_eq!(trainer.preferred_training(4), 4);
        assert_eq!(trainer.preferred_training(5), 0);
        assert_eq!(trainer.preferred_training(-3), 0);
    }
}
