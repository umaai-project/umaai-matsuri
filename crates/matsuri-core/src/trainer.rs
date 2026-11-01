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
        actions
            .iter()
            .position(|action| matches!(action, Action::Train(idx) if usize::from(*idx) == train))
            .ok_or_else(|| anyhow!("候选动作缺少训练 {train}"))
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
