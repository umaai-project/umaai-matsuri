//! 剧本配置段（`matsuri_config.toml`）：schema、载入与覆盖层合并
//!
//! 复用引擎「默认层 + 覆盖层」的泛型解析（`OverlayConfig` / `parse_with_overlay`）：
//! 默认层字段齐全、未知键严格报错；覆盖层同构、字段全 `Option`，段缺失整段沿用。
//! 根目录由宿主注入，本模块不写死路径。
//!
//! 当前只放**融合（転成）预设**：默认方案固定、在线可覆盖一次（融合不进搜索树），
//! 训练员 / 搜索参数留 S7 / M2，不预置。

use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use umaai::config::{OverlayConfig, parse_with_overlay};

/// 剧本配置文件名（放在宿主注入的剧本配置根下）
pub const MATSURI_CONFIG_FILE: &str = "matsuri_config.toml";

/// 基础教诲 id（融合的 `base` 只能是这 5 个，且每个只能用一次）
const BASE_ADVICE_IDS: [i32; 5] = [101, 201, 301, 401, 501];

/// 第 2 年（turn 23）可用的融合对象：特化 / 研钻
const YEAR2_TARGETS: [i32; 2] = [601, 701];

/// 第 3 年（turn 47）可用的融合对象：协助 / 不屈 / 兼修
const YEAR3_TARGETS: [i32; 3] = [801, 901, 1001];

/// 剧本专属配置段
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriConfig {
    /// 融合（転成）预设
    pub fusion: FusionConfig,
}

/// 融合预设：第 2 年 2 组、第 3 年剩余 3 组
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionConfig {
    /// 第 2 年（turn 23 结束）：恰好 2 组
    pub year2: Vec<FusionPair>,
    /// 第 3 年（turn 47 结束）：剩余 3 组
    pub year3: Vec<FusionPair>,
}

/// 一组融合配对（基础教诲 → 融合对象）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionPair {
    /// 被融合的基础教诲 id（101–501）
    pub base: i32,
    /// 融合对象 id（第 2 年 601 / 701，第 3 年 801 / 901 / 1001）
    pub target: i32,
}

/// 覆盖层：与默认层同构、字段全 `Option`（`None` = 沿用默认层）
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriConfigOverlay {
    fusion: Option<FusionOverlay>,
}

/// `fusion` 段的覆盖层（整段替换；配对列表不按元素合并）
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionOverlay {
    year2: Option<Vec<FusionPair>>,
    year3: Option<Vec<FusionPair>>,
}

impl OverlayConfig for MatsuriConfig {
    type Overlay = MatsuriConfigOverlay;

    fn apply_overlay(&mut self, overlay: Self::Overlay) {
        if let Some(fusion) = overlay.fusion {
            if let Some(year2) = fusion.year2 {
                self.fusion.year2 = year2;
            }
            if let Some(year3) = fusion.year3 {
                self.fusion.year3 = year3;
            }
        }
    }

    fn validate(&self) -> Result<()> {
        MatsuriConfig::validate(self)
    }
}

impl MatsuriConfig {
    /// 解析 TOML 文本（默认层 + 可选覆盖层；测试 / 内嵌场景复用）
    pub fn parse(base_toml: &str, overlay_toml: Option<&str>) -> Result<Self> {
        parse_with_overlay(base_toml, overlay_toml)
    }

    /// 载入：默认层文件 + 显式指定的覆盖层文件（给了就必须存在）
    pub fn load(base: impl AsRef<Path>, overlay: Option<&Path>) -> Result<Self> {
        let base = base.as_ref();
        let base_text = std::fs::read_to_string(base)
            .with_context(|| format!("读取剧本配置失败: {}", base.display()))?;
        let overlay_text = match overlay {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .with_context(|| format!("读取剧本覆盖配置失败: {}", path.display()))?,
            ),
            None => None,
        };
        Self::parse(&base_text, overlay_text.as_deref())
            .with_context(|| format!("剧本配置来源: {}", base.display()))
    }

    /// 载入：默认层 + **约定覆盖层**（`<dir>/matsuri_config.toml`，存在才合并）
    ///
    /// 与引擎 `Config::load_with_overlay_file` 同语义：覆盖层缺席不报错。
    pub fn load_with_overlay_file(
        base_dir: impl AsRef<Path>,
        dir: impl AsRef<Path>,
    ) -> Result<Self> {
        let overlay = dir.as_ref().join(MATSURI_CONFIG_FILE);
        let overlay = overlay.exists().then_some(overlay);
        Self::load(
            base_dir.as_ref().join(MATSURI_CONFIG_FILE),
            overlay.as_deref(),
        )
    }

    /// 载入期校验：第 2 年恰 2 组、第 3 年恰 3 组；base 只用 101–501 且不重复；
    /// target 年份内各自恰好用一次
    fn validate(&self) -> Result<()> {
        let fusion = &self.fusion;
        ensure!(
            fusion.year2.len() == YEAR2_TARGETS.len(),
            "fusion.year2 应恰好 {} 组，实际 {}",
            YEAR2_TARGETS.len(),
            fusion.year2.len()
        );
        ensure!(
            fusion.year3.len() == YEAR3_TARGETS.len(),
            "fusion.year3 应恰好 {} 组，实际 {}",
            YEAR3_TARGETS.len(),
            fusion.year3.len()
        );
        let mut bases = Vec::new();
        for (i, pair) in fusion.year2.iter().enumerate() {
            ensure!(
                BASE_ADVICE_IDS.contains(&pair.base),
                "fusion.year2[{i}].base 越界: {}（应为 101–501）",
                pair.base
            );
            ensure!(
                YEAR2_TARGETS.contains(&pair.target),
                "fusion.year2[{i}].target 越界: {}（应为 601 / 701）",
                pair.target
            );
            ensure!(
                !bases.contains(&pair.base),
                "fusion.year2[{i}].base 重复: {}",
                pair.base
            );
            bases.push(pair.base);
        }
        for (i, pair) in fusion.year3.iter().enumerate() {
            ensure!(
                BASE_ADVICE_IDS.contains(&pair.base),
                "fusion.year3[{i}].base 越界: {}（应为 101–501）",
                pair.base
            );
            ensure!(
                YEAR3_TARGETS.contains(&pair.target),
                "fusion.year3[{i}].target 越界: {}（应为 801 / 901 / 1001）",
                pair.target
            );
            ensure!(
                !bases.contains(&pair.base),
                "fusion.year3[{i}].base 重复: {}（第 3 年只用剩余基础教诲）",
                pair.base
            );
            bases.push(pair.base);
        }
        ensure!(
            bases.len() == BASE_ADVICE_IDS.len(),
            "fusion 应覆盖全部 {} 个基础教诲，实际 {} 个",
            BASE_ADVICE_IDS.len(),
            bases.len()
        );
        let mut year2_targets: Vec<i32> = fusion.year2.iter().map(|pair| pair.target).collect();
        year2_targets.sort_unstable();
        ensure!(
            year2_targets == YEAR2_TARGETS,
            "fusion.year2 的 target 应各自恰好使用一次"
        );
        let mut year3_targets: Vec<i32> = fusion.year3.iter().map(|pair| pair.target).collect();
        year3_targets.sort_unstable();
        ensure!(
            year3_targets == YEAR3_TARGETS,
            "fusion.year3 的 target 应各自恰好使用一次"
        );
        Ok(())
    }
}

impl Default for MatsuriConfig {
    /// 与仓内默认层 `data/matsuri_config.toml` 同值（未注入配置的宿主 / 测试兜底）
    fn default() -> Self {
        Self {
            fusion: FusionConfig {
                year2: vec![
                    FusionPair {
                        base: 101,
                        target: 601,
                    },
                    FusionPair {
                        base: 201,
                        target: 701,
                    },
                ],
                year3: vec![
                    FusionPair {
                        base: 401,
                        target: 801,
                    },
                    FusionPair {
                        base: 301,
                        target: 901,
                    },
                    FusionPair {
                        base: 501,
                        target: 1001,
                    },
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仓内默认层文本
    fn repo_text() -> String {
        std::fs::read_to_string(crate::testing::data_dir().join(MATSURI_CONFIG_FILE))
            .expect("仓内剧本配置默认层应存在")
    }

    #[test]
    fn parse_repo_default() {
        let cfg = MatsuriConfig::parse(&repo_text(), None).expect("默认层应可解析");
        assert_eq!(cfg, MatsuriConfig::default());
    }

    #[test]
    fn unknown_key_rejected() {
        let bad = repo_text().replace("year2", "year_two");
        let err = MatsuriConfig::parse(&bad, None).unwrap_err();
        assert!(err.to_string().contains("解析默认配置失败"));
    }

    #[test]
    fn overlay_replaces_year2_only() {
        let overlay =
            "[fusion]\nyear2 = [{ base = 201, target = 601 }, { base = 101, target = 701 }]\n";
        let cfg = MatsuriConfig::parse(&repo_text(), Some(overlay)).expect("覆盖层应可合并");
        assert_eq!(
            cfg.fusion.year2,
            vec![
                FusionPair {
                    base: 201,
                    target: 601
                },
                FusionPair {
                    base: 101,
                    target: 701
                },
            ]
        );
        assert_eq!(cfg.fusion.year3, MatsuriConfig::default().fusion.year3);
    }

    #[test]
    fn duplicate_base_rejected() {
        let bad = "[fusion]\n\
                   year2 = [{ base = 101, target = 601 }, { base = 101, target = 701 }]\n\
                   year3 = [{ base = 201, target = 801 }, { base = 301, target = 901 }, { base = 401, target = 1001 }]\n";
        assert!(
            MatsuriConfig::parse(bad, None).is_err(),
            "基础教诲重复应报错"
        );
    }

    #[test]
    fn wrong_target_rejected() {
        let bad = "[fusion]\n\
                   year2 = [{ base = 101, target = 801 }, { base = 201, target = 701 }]\n\
                   year3 = [{ base = 301, target = 801 }, { base = 401, target = 901 }, { base = 501, target = 1001 }]\n";
        assert!(
            MatsuriConfig::parse(bad, None).is_err(),
            "第 2 年 target 越界应报错"
        );
    }

    #[test]
    fn wrong_count_rejected() {
        let bad = "[fusion]\n\
                   year2 = [{ base = 101, target = 601 }]\n\
                   year3 = [{ base = 201, target = 801 }, { base = 301, target = 901 }, { base = 401, target = 1001 }]\n";
        assert!(
            MatsuriConfig::parse(bad, None).is_err(),
            "第 2 年组数不对应报错"
        );
    }
}
