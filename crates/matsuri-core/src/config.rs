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
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriConfig {
    /// 融合（転成）预设
    pub fusion: FusionConfig,
    /// 手写训练员策略旋钮（S7；后续扫参）
    pub trainer: TrainerConfig,
}

/// 手写训练员策略旋钮（全部可扫参；默认档 = 攻略 / memo 方向）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerConfig {
    /// 目标函数的 PT 偏好：`1.0` = 官方评分；引擎既有定档 `2.0`
    pub pt_favor_rate: f64,
    /// 体力低于此值进入「该休息了」区间（仍由打分表对比收益）
    pub rest_vital: i32,
    /// 発表会 / 本番达标保护的提前回合（0 = 关闭）
    pub checkpoint_margin: i32,
    /// Senior 合宿前把「有编成卡的演目」推到的最小 Lv（0 = 关闭）
    pub act_level_target: i32,
    /// 是否打自选比赛（false = 只在目标赛出手）
    pub self_race: bool,
    /// 友人外出是否尽早用完（攻略策略；false = 按收益打分）
    pub friend_outing_early: bool,
    /// 五维策略权重（官方评分边际之外的偏好；默认全 1）
    pub status_weights: [f64; 5],
    /// 属性 / 资源 / 事件的折算权重
    pub weights: ValueWeights,
    /// 教诲分配价值权重
    pub advice: AdviceWeights,
}

/// 属性 / 资源 / 事件的折算权重（单位 = 评分点）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueWeights {
    /// 每点体力基础价值（再乘分段系数）
    pub vital: f64,
    /// 每级干劲
    pub motivation: f64,
    /// 每点羁绊
    pub friendship: f64,
    /// 每级灵感
    pub hint: f64,
    /// 每点演目槽
    pub gauge: f64,
    /// 每级演目 Lv（普通价值；不含达标急迫）
    pub act_level: f64,
    /// 発表会每缺 1 个演目达标 / 每缺 1 级的惩罚
    pub checkpoint: f64,
    /// 【变胖】状态的期望惩罚（一次性）
    pub fat: f64,
}

/// 教诲分配价值权重（特征系数）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdviceWeights {
    /// 每张支援卡担当权重
    pub card: f64,
    /// 每名编成外神驹担当权重
    pub npc: f64,
    /// 预计普通训练次数权重
    pub train: f64,
    /// 预计演目稽古次数权重
    pub keiko: f64,
    /// 绑定演目 Lv 权重
    pub level: f64,
    /// 発表会达标急迫度权重
    pub milestone: f64,
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
    /// 训练员段：给了就**整段替换**（不按字段合并，避免半套旋钮交叉污染）
    trainer: Option<TrainerConfig>,
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
        if let Some(trainer) = overlay.trainer {
            self.trainer = trainer;
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
        validate_trainer(&self.trainer)
    }
}

/// 训练员旋钮校验：范围 + 非负 / 有限（扫参硬边界；非法值直接拒绝，不静默回退）
fn validate_trainer(trainer: &TrainerConfig) -> Result<()> {
    ensure!(
        trainer.pt_favor_rate.is_finite() && trainer.pt_favor_rate > 0.0,
        "trainer.pt_favor_rate 应为正数"
    );
    ensure!(
        (0..=100).contains(&trainer.rest_vital),
        "trainer.rest_vital 应在 0–100"
    );
    ensure!(
        (0..=20).contains(&trainer.checkpoint_margin),
        "trainer.checkpoint_margin 应在 0–20"
    );
    ensure!(
        (0..=10).contains(&trainer.act_level_target),
        "trainer.act_level_target 应在 0–10"
    );
    for (index, weight) in trainer.status_weights.iter().enumerate() {
        ensure!(
            weight.is_finite() && *weight >= 0.0,
            "trainer.status_weights[{index}] 应为非负有限数"
        );
    }
    let weights = &trainer.weights;
    let entries: [(&str, f64); 8] = [
        ("weights.vital", weights.vital),
        ("weights.motivation", weights.motivation),
        ("weights.friendship", weights.friendship),
        ("weights.hint", weights.hint),
        ("weights.gauge", weights.gauge),
        ("weights.act_level", weights.act_level),
        ("weights.checkpoint", weights.checkpoint),
        ("weights.fat", weights.fat),
    ];
    for (name, value) in entries {
        ensure!(
            value.is_finite() && value >= 0.0,
            "trainer.{name} 应为非负有限数"
        );
    }
    let advice = &trainer.advice;
    let entries: [(&str, f64); 6] = [
        ("advice.card", advice.card),
        ("advice.npc", advice.npc),
        ("advice.train", advice.train),
        ("advice.keiko", advice.keiko),
        ("advice.level", advice.level),
        ("advice.milestone", advice.milestone),
    ];
    for (name, value) in entries {
        ensure!(
            value.is_finite() && value >= 0.0,
            "trainer.{name} 应为非负有限数"
        );
    }
    Ok(())
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
            trainer: TrainerConfig::default(),
        }
    }
}

impl Default for TrainerConfig {
    /// 与仓内默认层 `[trainer]` 段同值（攻略 / memo 方向；后续扫参）
    fn default() -> Self {
        Self {
            pt_favor_rate: 2.0,
            rest_vital: 45,
            checkpoint_margin: 3,
            act_level_target: 8,
            self_race: false,
            friend_outing_early: true,
            status_weights: [1.0; 5],
            weights: ValueWeights {
                vital: 1.2,
                motivation: 30.0,
                friendship: 0.6,
                hint: 1.0,
                gauge: 25.0,
                act_level: 120.0,
                checkpoint: 800.0,
                fat: 120.0,
            },
            advice: AdviceWeights {
                card: 1.0,
                npc: 0.6,
                train: 1.0,
                keiko: 1.0,
                level: 1.0,
                milestone: 1.0,
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
        let bad = repo_text().replace("base = 201\ntarget = 701", "base = 101\ntarget = 701");
        assert!(
            MatsuriConfig::parse(&bad, None).is_err(),
            "基础教诲重复应报错"
        );
    }

    #[test]
    fn wrong_target_rejected() {
        let bad = repo_text().replace("target = 601", "target = 801");
        assert!(
            MatsuriConfig::parse(&bad, None).is_err(),
            "第 2 年 target 越界应报错"
        );
    }

    #[test]
    fn wrong_count_rejected() {
        let bad = repo_text().replace("[[fusion.year2]]\nbase = 201\ntarget = 701\n\n", "");
        assert!(
            MatsuriConfig::parse(&bad, None).is_err(),
            "第 2 年组数不对应报错"
        );
    }

    #[test]
    fn trainer_overlay_requires_full_section() {
        let overlay = "[trainer]\npt_favor_rate = 1.0\n";
        assert!(
            MatsuriConfig::parse(&repo_text(), Some(overlay)).is_err(),
            "trainer 段覆盖必须整段给出（不按字段合并）"
        );
    }

    #[test]
    fn trainer_overlay_replaces_whole_section() {
        let overlay = "[trainer]\n\
                       pt_favor_rate = 1.0\n\
                       rest_vital = 40\n\
                       checkpoint_margin = 2\n\
                       act_level_target = 0\n\
                       self_race = true\n\
                       friend_outing_early = false\n\
                       status_weights = [2.0, 1.0, 1.0, 1.0, 1.0]\n\
                       [trainer.weights]\n\
                       vital = 1.0\n\
                       motivation = 20.0\n\
                       friendship = 0.5\n\
                       hint = 1.0\n\
                       gauge = 10.0\n\
                       act_level = 50.0\n\
                       checkpoint = 100.0\n\
                       fat = 10.0\n\
                       [trainer.advice]\n\
                       card = 1.0\n\
                       npc = 1.0\n\
                       train = 1.0\n\
                       keiko = 1.0\n\
                       level = 1.0\n\
                       milestone = 1.0\n";
        let cfg = MatsuriConfig::parse(&repo_text(), Some(overlay)).expect("整段覆盖应可合并");
        assert_eq!(cfg.trainer.pt_favor_rate, 1.0);
        assert_eq!(cfg.trainer.rest_vital, 40);
        assert!(cfg.trainer.self_race);
        assert_eq!(
            cfg.fusion,
            MatsuriConfig::default().fusion,
            "fusion 段不受影响"
        );
    }

    #[test]
    fn trainer_bad_values_rejected() {
        let bad = repo_text().replace("rest_vital = 45", "rest_vital = -1");
        assert!(
            MatsuriConfig::parse(&bad, None).is_err(),
            "体力阈值越界应报错"
        );
        let bad = repo_text().replace("pt_favor_rate = 2.0", "pt_favor_rate = 0.0");
        assert!(
            MatsuriConfig::parse(&bad, None).is_err(),
            "PT 偏好非正数应报错"
        );
        let bad = repo_text().replace("vital = 1.2", "vital = -0.1");
        assert!(MatsuriConfig::parse(&bad, None).is_err(), "权重为负应报错");
    }
}
