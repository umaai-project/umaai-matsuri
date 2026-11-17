//! 复盘口径参数（可被 CLI 覆盖；不进 `matsuri_config`，避免污染游戏配置与内置默认锁）
//!
//! 只放"可能调整的判据标准"：当前只有继承标准（默认 306 点/次，内部判据，不进叙事）。

/// 复盘口径参数
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// 单次继承的期望属性总量（内部判据，输出只说低于/高于预期，不暴露数值）
    pub inherit_expect: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            inherit_expect: 306.0,
        }
    }
}
