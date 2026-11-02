//! 配对门禁统计内核（`matsuri-sim`）
//!
//! 口径：**CRN 同种子逐局配对**。改动前（A）与改动后（B）两批样本按样本号一一对齐，
//! 每局配对差 `d_i = b_i − a_i` 是机制改动的净效应；与真值的贴近幅度
//! `e_i = |a_i − real| − |b_i − real|`（正值 = 改动后更接近真值）。
//!
//! 判读只认 `e` 的 95% 区间：下界 >0 ⇒ 显著更近；上界 <0 ⇒ 显著更远；跨 0 ⇒ 无显著差异
//! （留档，不转正）。`d` 的区间与两个 p 值是佐证，不参与判读。
//!
//! 本模块无 IO、不依赖 serde：只做纯数值统计。读 `MC_DUMP` 的 IO 在 `matsuri-gate`
//! （`matsuri-cli`）。

use std::collections::BTreeMap;
use std::fmt;

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

/// 一个分量的判读结论
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 改动后显著更接近真值
    Closer,
    /// 改动后显著远离真值
    Farther,
    /// 无显著差异（留档，不转正）
    NoDifference,
}

impl Verdict {
    /// 中文标签（表格 / JSON 同一口径）
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Closer => "显著更近",
            Verdict::Farther => "显著更远",
            Verdict::NoDifference => "无显著差异",
        }
    }
}

/// 统计参数：置换次数 / bootstrap 次数 / 随机种子
#[derive(Debug, Clone, Copy)]
pub struct StatsConfig {
    /// 符号翻转置换次数（求 p 值）
    pub perms: usize,
    /// bootstrap 重采样次数（求百分位区间）
    pub boot: usize,
    /// 随机种子（同输入同输出）
    pub seed: u64,
}

impl Default for StatsConfig {
    fn default() -> Self {
        StatsConfig {
            perms: 20_000,
            boot: 20_000,
            seed: 0,
        }
    }
}

/// 统计内核的输入错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchError {
    /// A / B 样本数不等
    LengthMismatch {
        /// A 的长度
        a: usize,
        /// B 的长度
        b: usize,
    },
    /// 样本为空
    Empty,
    /// 样本太少（< 2，给不出区间）
    TooFew {
        /// 实际样本数
        n: usize,
    },
    /// 两臂的样本号集合不一致（无法配对）
    IndexMismatch {
        /// 只在 A 里出现的样本号
        missing_in_b: Vec<u64>,
        /// 只在 B 里出现的样本号
        missing_in_a: Vec<u64>,
    },
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::LengthMismatch { a, b } => {
                write!(f, "A / B 样本数不等：{a} vs {b}")
            }
            BenchError::Empty => write!(f, "样本为空"),
            BenchError::TooFew { n } => write!(f, "样本只有 {n} 条，给不出区间（至少 2 条）"),
            BenchError::IndexMismatch {
                missing_in_b,
                missing_in_a,
            } => {
                write!(
                    f,
                    "样本号集合不一致：缺于 B 的 {} 个（{}）、缺于 A 的 {} 个（{}）",
                    missing_in_b.len(),
                    preview(missing_in_b),
                    missing_in_a.len(),
                    preview(missing_in_a)
                )
            }
        }
    }
}

impl std::error::Error for BenchError {}

/// 样本号列表的短预览（错误信息用，最多 8 个）
fn preview(ids: &[u64]) -> String {
    let head: Vec<String> = ids.iter().take(8).map(|i| i.to_string()).collect();
    head.join(",")
}

/// 一个分量的配对统计结果
#[derive(Debug, Clone)]
pub struct ComponentReport {
    /// 分量名（如「五维和」）
    pub name: String,
    /// 配对样本数
    pub n: usize,
    /// 真值（真实局的分量值）
    pub real: f64,
    /// A 臂（改动前）均值
    pub mean_a: f64,
    /// B 臂（改动后）均值
    pub mean_b: f64,
    /// `d_i = b_i − a_i` 的均值
    pub mean_delta: f64,
    /// `d` 的 95% 置信区间
    pub delta_ci: (f64, f64),
    /// `d` 的置换检验 p 值（双侧）
    pub delta_p: f64,
    /// `e_i = |a_i − real| − |b_i − real|` 的均值（正值 = 改动后更近）
    pub mean_closer: f64,
    /// `e` 的 95% 置信区间
    pub closer_ci: (f64, f64),
    /// `e` 的置换检验 p 值（双侧）
    pub closer_p: f64,
    /// B 严格更接近真值的比例
    pub win_rate: f64,
    /// 判读结论
    pub verdict: Verdict,
}

/// 样本号 → 分量值（一份 dump 的一个分量）
pub type SampleMap = BTreeMap<u64, f64>;

/// 按样本号对齐后的两臂序列
#[derive(Debug, Clone)]
pub struct PairedSeries {
    /// A 臂（改动前）
    pub a: Vec<f64>,
    /// B 臂（改动后）
    pub b: Vec<f64>,
}

/// 按样本号对齐两臂；样本号集合不一致时报错。
pub fn align_by_index(a: &SampleMap, b: &SampleMap) -> Result<PairedSeries, BenchError> {
    let missing_in_b: Vec<u64> = a.keys().filter(|k| !b.contains_key(k)).copied().collect();
    let missing_in_a: Vec<u64> = b.keys().filter(|k| !a.contains_key(k)).copied().collect();
    if !missing_in_b.is_empty() || !missing_in_a.is_empty() {
        return Err(BenchError::IndexMismatch {
            missing_in_b,
            missing_in_a,
        });
    }
    let mut va = Vec::with_capacity(a.len());
    let mut vb = Vec::with_capacity(b.len());
    for (i, x) in a {
        va.push(*x);
        vb.push(b[i]);
    }
    Ok(PairedSeries { a: va, b: vb })
}

/// 一个分量的配对统计：两份等长序列 + 真值 → 结论。
///
/// `a` = 改动前，`b` = 改动后；`real` = 真实局的该分量值。
pub fn paired_stats(
    name: &str,
    a: &[f64],
    b: &[f64],
    real: f64,
    cfg: StatsConfig,
) -> Result<ComponentReport, BenchError> {
    if a.len() != b.len() {
        return Err(BenchError::LengthMismatch {
            a: a.len(),
            b: b.len(),
        });
    }
    if a.is_empty() {
        return Err(BenchError::Empty);
    }
    if a.len() < 2 {
        return Err(BenchError::TooFew { n: a.len() });
    }

    let n = a.len();
    let delta: Vec<f64> = a.iter().zip(b).map(|(&x, &y)| y - x).collect();
    let closer: Vec<f64> = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (x - real).abs() - (y - real).abs())
        .collect();

    let delta_ci = bootstrap_ci(&delta, cfg.boot, cfg.seed, &format!("boot:{name}"));
    let delta_p = permutation_p(&delta, cfg.perms, cfg.seed, &format!("perm:{name}"));
    let closer_ci = bootstrap_ci(&closer, cfg.boot, cfg.seed, &format!("boot:{name}"));
    let closer_p = permutation_p(&closer, cfg.perms, cfg.seed, &format!("perm:{name}"));
    let win_rate = closer.iter().filter(|&&e| e > 0.0).count() as f64 / n as f64;

    let verdict = if closer_ci.0 > 0.0 {
        Verdict::Closer
    } else if closer_ci.1 < 0.0 {
        Verdict::Farther
    } else {
        Verdict::NoDifference
    };

    Ok(ComponentReport {
        name: name.to_string(),
        n,
        real,
        mean_a: mean(a),
        mean_b: mean(b),
        mean_delta: mean(&delta),
        delta_ci,
        delta_p,
        mean_closer: mean(&closer),
        closer_ci,
        closer_p,
        win_rate,
        verdict,
    })
}

/// 均值
fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// 百分位 bootstrap 置信区间（重采样下均值的 2.5% / 97.5% 分位）
fn bootstrap_ci(xs: &[f64], boot: usize, seed: u64, tag: &str) -> (f64, f64) {
    if boot == 0 || xs.is_empty() {
        let m = mean(xs);
        return (m, m);
    }
    let n = xs.len();
    let mut rng = StdRng::seed_from_u64(mix_seed(seed, tag));
    let mut means = Vec::with_capacity(boot);
    for _ in 0..boot {
        let mut s = 0.0;
        for _ in 0..n {
            s += xs[rng.random_range(0..n)];
        }
        means.push(s / n as f64);
    }
    means.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    (percentile(&means, 0.025), percentile(&means, 0.975))
}

/// 符号翻转置换检验（配对随机化）：原假设 = 差值的符号与配对无关。
///
/// p 值取双侧（`|置换统计量| ≥ |观测统计量|` 的比例），带 +1 修正，永不为 0。
fn permutation_p(xs: &[f64], perms: usize, seed: u64, tag: &str) -> f64 {
    let n = xs.len();
    if n == 0 || perms == 0 {
        return 1.0;
    }
    let obs = mean(xs).abs();
    let mut rng = StdRng::seed_from_u64(mix_seed(seed, tag));
    let mut hits = 0usize;
    for _ in 0..perms {
        let mut s = 0.0;
        for &x in xs {
            s += if rng.random_bool(0.5) { x } else { -x };
        }
        if (s / n as f64).abs() >= obs {
            hits += 1;
        }
    }
    (hits as f64 + 1.0) / (perms as f64 + 1.0)
}

/// 已排序序列的分位数（线性位置取最近邻）
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 种子与标签混合（FNV-1a）：同 seed 不同分量拿到不同随机流
fn mix_seed(seed: u64, tag: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in tag.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed ^ h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 小预算：单元测试要快，统计结论不受影响
    fn cfg() -> StatsConfig {
        StatsConfig {
            perms: 2000,
            boot: 2000,
            seed: 7,
        }
    }

    #[test]
    fn paired_stats_identical_series_gives_zero_delta_and_p_one() {
        let a = vec![100.0, 120.0, 90.0, 110.0];
        let r = paired_stats("五维和", &a, &a, 105.0, cfg()).unwrap();
        assert_eq!(r.mean_delta, 0.0, "自比的 Δ 必须是 0");
        assert_eq!(r.mean_closer, 0.0, "自比的贴近幅度必须是 0");
        assert_eq!(r.delta_p, 1.0, "自比的 p 必须是 1");
        assert_eq!(r.closer_p, 1.0, "自比的 p 必须是 1");
        assert_eq!(r.win_rate, 0.0, "自比没有严格更近的样本");
        assert_eq!(r.verdict, Verdict::NoDifference);
    }

    #[test]
    fn paired_stats_detects_uniform_improvement() {
        let a = vec![100.0; 20];
        let b = vec![110.0; 20];
        let r = paired_stats("PT", &a, &b, 120.0, cfg()).unwrap();
        assert_eq!(r.mean_delta, 10.0);
        assert_eq!(r.mean_closer, 10.0, "B 更靠近真值 10 点");
        assert_eq!(r.win_rate, 1.0);
        assert!(
            r.closer_ci.0 > 0.0,
            "贴近幅度区间下界应为正：{:?}",
            r.closer_ci
        );
        assert!(r.closer_p < 0.05, "p 应显著：{}", r.closer_p);
        assert_eq!(r.verdict, Verdict::Closer);
    }

    #[test]
    fn paired_stats_detects_uniform_regression() {
        let a = vec![110.0; 20];
        let b = vec![100.0; 20];
        let r = paired_stats("PT", &a, &b, 120.0, cfg()).unwrap();
        assert_eq!(r.mean_delta, -10.0);
        assert_eq!(r.mean_closer, -10.0);
        assert!(
            r.closer_ci.1 < 0.0,
            "贴近幅度区间上界应为负：{:?}",
            r.closer_ci
        );
        assert_eq!(r.verdict, Verdict::Farther);
    }

    #[test]
    fn paired_stats_win_rate_counts_strict_improvement() {
        let a = vec![100.0, 100.0, 200.0, 200.0];
        let b = vec![105.0, 90.0, 195.0, 210.0];
        let r = paired_stats("五维分", &a, &b, 100.0, cfg()).unwrap();
        // e = [-5, -10, +5, -10] ⇒ 只有第 3 局严格更近
        assert_eq!(r.mean_closer, -5.0);
        assert_eq!(r.win_rate, 0.25);
    }

    #[test]
    fn paired_stats_rejects_length_mismatch() {
        let err = paired_stats("PT", &[1.0, 2.0], &[1.0], 0.0, cfg()).unwrap_err();
        assert_eq!(
            err,
            BenchError::LengthMismatch { a: 2, b: 1 },
            "长度不等必须报错，不能截断对齐"
        );
    }

    #[test]
    fn paired_stats_rejects_empty_and_single() {
        assert_eq!(
            paired_stats("PT", &[], &[], 0.0, cfg()).unwrap_err(),
            BenchError::Empty
        );
        assert_eq!(
            paired_stats("PT", &[1.0], &[1.0], 0.0, cfg()).unwrap_err(),
            BenchError::TooFew { n: 1 }
        );
    }

    #[test]
    fn align_by_index_pairs_by_sample_id() {
        let mut a = SampleMap::new();
        a.insert(0, 10.0);
        a.insert(2, 30.0);
        let mut b = SampleMap::new();
        b.insert(0, 11.0);
        b.insert(2, 31.0);
        let s = align_by_index(&a, &b).unwrap();
        assert_eq!(s.a, vec![10.0, 30.0]);
        assert_eq!(s.b, vec![11.0, 31.0]);
    }

    #[test]
    fn align_by_index_rejects_missing_ids() {
        let mut a = SampleMap::new();
        a.insert(0, 10.0);
        a.insert(1, 20.0);
        let mut b = SampleMap::new();
        b.insert(0, 11.0);
        let err = align_by_index(&a, &b).unwrap_err();
        match err {
            BenchError::IndexMismatch { missing_in_b, .. } => assert_eq!(missing_in_b, vec![1]),
            other => panic!("期望 IndexMismatch，得到 {other:?}"),
        }
    }
}
