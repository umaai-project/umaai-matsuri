//! 复盘统计口径：配对差、分位与命中判定
//!
//! 原语全部转调 `umaai_core::stats`，本模块只固定复盘侧的三条口径：
//! 配对差取 `变动臂 − 对照臂`、区间取 95%（正态近似）、命中即区间跨零。

use serde::{Deserialize, Serialize};
use umaai_core::stats::{self, Paired};

/// 选择偏差的命中判定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hit {
    /// 区间跨零：差异在随机噪声内，不展开结论
    Within,
    /// 区间不跨零：差异显著，需进一步归因
    Beyond,
    /// 样本不足或实际选择不可判
    Undecidable,
}

impl Hit {
    /// 中文标签（终端与报告共用）
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Within => "命中",
            Self::Beyond => "未命中",
            Self::Undecidable => "不可判",
        }
    }
}

/// 命中判定：`abs(delta) <= ci95` 即区间跨零
#[must_use]
pub fn hit(delta: Option<f64>, ci95: Option<f64>) -> Hit {
    match (delta, ci95) {
        (Some(delta), Some(ci95)) if delta.abs() <= ci95 => Hit::Within,
        (Some(_), Some(_)) => Hit::Beyond,
        _ => Hit::Undecidable,
    }
}

/// 按 rollout 序号配对的差值 `b − a`
///
/// 两臂按位置逐步对齐，任一侧缺样本（`None`）即跳过；不足 2 对返回 `None`。
#[must_use]
pub fn paired_diff(a: &[Option<f64>], b: &[Option<f64>]) -> Option<Paired> {
    let pairs: Vec<(f64, f64)> = a
        .iter()
        .zip(b.iter())
        .filter_map(|(left, right)| (*left).zip(*right))
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let (left, right): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
    stats::paired(&left, &right).ok()
}

/// 升序样本的第 `p` 分位（`p ∈ [0, 1]`，线性插值）；空样本返回 `None`
#[must_use]
pub fn quantile(values: &mut [f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(stats::percentile(values, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 命中判定覆盖三态与边界
    #[test]
    fn test_hit_threshold() {
        assert_eq!(hit(Some(10.0), Some(20.0)), Hit::Within);
        assert_eq!(hit(Some(-10.0), Some(20.0)), Hit::Within);
        assert_eq!(hit(Some(20.0), Some(20.0)), Hit::Within);
        assert_eq!(hit(Some(-30.0), Some(20.0)), Hit::Beyond);
        assert_eq!(hit(None, Some(20.0)), Hit::Undecidable);
        assert_eq!(hit(Some(10.0), None), Hit::Undecidable);
    }

    /// 配对差跳过缺样本并按 `b − a` 计
    #[test]
    fn test_paired_diff_skips_missing() {
        let a = [Some(1.0), None, Some(3.0), Some(4.0)];
        let b = [Some(2.0), Some(9.0), Some(5.0), None];
        let paired = paired_diff(&a, &b).expect("两对可算");
        assert_eq!(paired.n, 2);
        assert!((paired.mean - 1.5).abs() < 1e-9, "mean={}", paired.mean);
        assert_eq!(paired.wins, 2);
    }

    /// 不足两对返回 None（样本不够不下结论）
    #[test]
    fn test_paired_diff_too_few() {
        assert!(paired_diff(&[Some(1.0)], &[Some(2.0)]).is_none());
        assert!(paired_diff(&[None, None], &[Some(1.0), Some(2.0)]).is_none());
    }

    /// 分位线性插值
    #[test]
    fn test_quantile_linear() {
        let mut values = [4.0, 1.0, 3.0, 2.0];
        let median = quantile(&mut values, 0.5).expect("非空");
        assert!((median - 2.5).abs() < 1e-9, "median={median}");
        assert!(quantile(&mut [], 0.5).is_none());
    }
}
