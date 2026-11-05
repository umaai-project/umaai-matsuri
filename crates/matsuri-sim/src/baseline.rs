//! 基线数据与判定
//!
//! 一档一 JSON（仓根 `baselines/`），记录固定局面条件与固定种子块下的均分。判定只比
//! **分数与 PT 的均值**：单局耗时会随机器负载漂移，不进基线。
//!
//! 容差同时受两个东西约束：基线与本轮的合并标准误（统计噪声）和相对容差（口径允许的
//! 小幅位移）。任何一项被突破才算漂移，避免小样本把噪声报成回归。

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// 一档基线
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// 档名（= 文件名主干）
    pub name: String,
    /// 说明
    pub note: String,
    /// 马娘 ID
    pub uma: u32,
    /// 局外切状态：`none` / `xiaoqie` / `qiezhe`
    pub qie: String,
    /// 心情盾层数
    pub shield: i32,
    /// 录制时的搜索参数
    pub config: Config,
    /// 按训练员与种子块分开的若干条记录
    pub records: Vec<Record>,
}

/// 录制时的搜索参数（只作记录与一致性核对，不参与判定）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 目标函数的 PT 偏好
    pub pt_favor_rate: f64,
    /// 每候选 rollout 次数
    pub search_n: usize,
    /// UCB 分组大小
    pub search_group_size: usize,
    /// 探索项标准差
    pub expected_search_stdev: f64,
}

/// 一条基线记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// 训练员：`handwritten` / `search`
    pub trainer: String,
    /// 该条的 `search_n`（手写档为 0）
    pub search_n: usize,
    /// 起始种子
    pub seed0: u64,
    /// 局数
    pub games: usize,
    /// 分数均值
    pub score_mean: f64,
    /// 分数样本标准差
    pub score_sd: f64,
    /// PT 均值
    pub pt_mean: f64,
    /// PT 样本标准差
    pub pt_sd: f64,
}

impl Record {
    /// 均值的标准误
    fn se(sd: f64, n: usize) -> f64 {
        if n == 0 { 0.0 } else { sd / (n as f64).sqrt() }
    }
}

/// 判定结果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// 是否在容差内
    pub ok: bool,
    /// 本轮分数均值 − 基线分数均值
    pub score_delta: f64,
    /// 本轮 PT 均值 − 基线 PT 均值
    pub pt_delta: f64,
    /// 本轮允许的分数偏差
    pub score_allow: f64,
    /// 本轮允许的 PT 偏差
    pub pt_allow: f64,
}

/// 判定一次复跑
///
/// `samples` 是本轮逐局的 `(score, pt)`。`sigma` 是标准误的倍数（默认 3），`rel` 是相对
/// 容差（默认 0.01），两者取大：样本少时由标准误兜底，样本多时由相对容差兜底。
pub fn judge(record: &Record, samples: &[(i32, i32)], sigma: f64, rel: f64) -> Verdict {
    let n = samples.len();
    if n == 0 {
        return Verdict {
            ok: false,
            score_delta: 0.0,
            pt_delta: 0.0,
            score_allow: 0.0,
            pt_allow: 0.0,
        };
    }
    let score_mean = samples.iter().map(|(s, _)| f64::from(*s)).sum::<f64>() / n as f64;
    let pt_mean = samples.iter().map(|(_, p)| f64::from(*p)).sum::<f64>() / n as f64;
    let score_var =
        Record::se(record.score_sd, record.games).powi(2) + Record::se(record.score_sd, n).powi(2);
    let pt_var =
        Record::se(record.pt_sd, record.games).powi(2) + Record::se(record.pt_sd, n).powi(2);
    let score_allow = (sigma * score_var.sqrt()).max(rel * record.score_mean.abs());
    let pt_allow = (sigma * pt_var.sqrt()).max(rel * record.pt_mean.abs());
    let score_delta = score_mean - record.score_mean;
    let pt_delta = pt_mean - record.pt_mean;
    Verdict {
        ok: score_delta.abs() <= score_allow && pt_delta.abs() <= pt_allow,
        score_delta,
        pt_delta,
        score_allow,
        pt_allow,
    }
}

impl Case {
    /// 从 JSON 文件读取
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取基线失败: {}", path.display()))?;
        let case: Self = serde_json::from_str(&text)
            .with_context(|| format!("解析基线失败: {}", path.display()))?;
        case.validate()?;
        Ok(case)
    }

    /// 字段自检：档名非空、切状态合法、记录非空且局数 > 0
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            bail!("基线档名为空");
        }
        if !matches!(self.qie.as_str(), "none" | "xiaoqie" | "qiezhe") {
            bail!("基线 {} 的 qie 非法: {}", self.name, self.qie);
        }
        if self.shield < 0 {
            bail!("基线 {} 的 shield 为负", self.name);
        }
        if self.records.is_empty() {
            bail!("基线 {} 没有记录", self.name);
        }
        for record in &self.records {
            if record.games == 0 {
                bail!("基线 {} 有记录局数为 0", self.name);
            }
            if !matches!(record.trainer.as_str(), "handwritten" | "search") {
                bail!("基线 {} 的训练员标记非法: {}", self.name, record.trainer);
            }
        }
        Ok(())
    }

    /// 取某训练员与 `search_n` 的记录
    pub fn record(&self, trainer: &str, search_n: usize) -> Option<&Record> {
        self.records
            .iter()
            .find(|r| r.trainer == trainer && r.search_n == search_n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        Record {
            trainer: "search".to_string(),
            search_n: 8192,
            seed0: 73200,
            games: 10,
            score_mean: 84_639.2,
            score_sd: 1526.3,
            pt_mean: 12_247.4,
            pt_sd: 276.2,
        }
    }

    /// 层1：同分布复跑判定通过
    #[test]
    fn judge_matches_same_distribution() {
        let samples = vec![(84_600, 12_240), (84_700, 12_260), (84_620, 12_250)];
        let verdict = judge(&record(), &samples, 3.0, 0.01);
        assert!(verdict.ok, "{verdict:?}");
        assert!(verdict.score_allow >= 846.0, "相对容差应兜底到 1%");
    }

    /// 层1：超出合并标准误与相对容差的位移判为漂移
    #[test]
    fn judge_flags_drift() {
        let samples = vec![(80_000, 11_500), (80_100, 11_520), (80_050, 11_510)];
        let verdict = judge(&record(), &samples, 3.0, 0.01);
        assert!(!verdict.ok, "{verdict:?}");
        assert!(verdict.score_delta < -4000.0);
    }

    /// 层1：空样本判为失败，不 panic
    #[test]
    fn judge_rejects_empty() {
        let verdict = judge(&record(), &[], 3.0, 0.01);
        assert!(!verdict.ok);
    }

    /// 层2：仓根基线文件全部可解析且字段自检通过
    #[test]
    fn repo_baselines_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("上溯两级到仓根")
            .join("baselines");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut seen = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|s| s.to_str()) == Some("README.json") {
                continue;
            }
            let case = Case::load(&path).expect("基线应可解析");
            assert!(
                case.record("handwritten", 0).is_some(),
                "{} 缺少手写档记录",
                case.name
            );
            seen += 1;
        }
        assert!(seen > 0, "仓根 baselines/ 下应有基线文件");
    }
}
