//! 复盘口径参数（可被 CLI 覆盖；不进 `matsuri_config`，避免污染游戏配置与内置默认锁）
//!
//! 只放"可能调整的判据标准"：当前只有继承标准（默认 400 点/次，来自模拟口径）。

/// 复盘口径参数
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// 单次继承的期望属性总量（模拟口径约 400/次）
    pub inherit_expect: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            inherit_expect: 400.0,
        }
    }
}
