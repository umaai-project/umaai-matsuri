//! 卡组参数（`[decks.*]`）：`DeckParams` / `DeckTable` / 教诲落位预设 / 深合并与校验
//!
//! `matsuri_config.toml` 的 `[decks.default]` 是完整默认档（= 原 speed_build 参数），
//! 其他档只写差异并深合并到 default；`variant = true` 的档只用于扫参，必须按名字显式选。
//! 本模块同时承载原 `config.rs` 里的 `FusionConfig` / `TrainerConfig` 及其校验，
//! 使 `MatsuriConfig` 只保留全局运行设置（`[pre]` / `[search]`）。

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use umasim::game::SupportCard;

/// 基础教诲 id（融合的 `base` 只能是这 5 个，且每个只能用一次）
const BASE_ADVICE_IDS: [i32; 5] = [101, 201, 301, 401, 501];

/// 第 2 年（turn 23）可用的融合对象：特化 / 研钻
const YEAR2_TARGETS: [i32; 2] = [601, 701];

/// 第 3 年（turn 47）可用的融合对象：协助 / 不屈 / 兼修
const YEAR3_TARGETS: [i32; 3] = [801, 901, 1001];

/// 默认档名（保留名；完整档 + 未匹配兜底）
pub const DEFAULT_DECK: &str = "default";

// ---------------------------------------------------------------------------
// 卡组参数
// ---------------------------------------------------------------------------

/// 一套卡组的完整参数（融合 / 教诲落位 / 手写训练员）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckParams {
    /// 融合（転成）预设
    pub fusion: FusionConfig,
    /// 六窗口教诲落位预设（基础 id：101–501）
    pub advice: AdvicePreset,
    /// 手写训练员策略旋钮
    pub trainer: TrainerConfig,
}

impl DeckParams {
    /// 校验：融合 2+3 / 教诲每窗口排列 / 训练员区间
    pub fn validate(&self) -> Result<()> {
        validate_fusion(&self.fusion)?;
        validate_advice(&self.advice)?;
        validate_trainer(&self.trainer)
    }
}

impl Default for DeckParams {
    /// 内置默认档（= 原 speed_build 参数；无文件 / 测试兜底）
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
                        base: 501,
                        target: 801,
                    },
                    FusionPair {
                        base: 301,
                        target: 901,
                    },
                    FusionPair {
                        base: 401,
                        target: 1001,
                    },
                ],
            },
            advice: AdvicePreset::default(),
            trainer: TrainerConfig::default(),
        }
    }
}

/// 六窗口教诲落位预设：每窗口按「速 / 耐 / 力 / 根 / 智」写基础教诲 id
///
/// 写基础 id（101–501）而不是転成后的 601–1001：転成只改槽内容、不改槽身份，
/// 换融合方案或在线实际融合与配置不一致时仍能正确落位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvicePreset {
    /// turn 2（Begin 首次传授）
    pub t2: [i32; 5],
    /// turn 11（合宿前改派）
    pub t11: [i32; 5],
    /// turn 23（第 2 年结束；先転成再分配）
    pub t23: [i32; 5],
    /// turn 35（合宿前改派）
    pub t35: [i32; 5],
    /// turn 47（第 3 年结束；先転成再分配）
    pub t47: [i32; 5],
    /// turn 59（合宿前改派）
    pub t59: [i32; 5],
}

impl AdvicePreset {
    /// 按 `data.advice_windows` 的小标取该窗口的落位表
    pub fn window(&self, index: usize) -> Option<[i32; 5]> {
        match index {
            0 => Some(self.t2),
            1 => Some(self.t11),
            2 => Some(self.t23),
            3 => Some(self.t35),
            4 => Some(self.t47),
            5 => Some(self.t59),
            _ => None,
        }
    }

    /// 逐窗口遍历（校验 / 诊断用）
    pub fn windows(&self) -> [(&'static str, &[i32; 5]); 6] {
        [
            ("t2", &self.t2),
            ("t11", &self.t11),
            ("t23", &self.t23),
            ("t35", &self.t35),
            ("t47", &self.t47),
            ("t59", &self.t59),
        ]
    }
}

impl Default for AdvicePreset {
    /// 内置默认档 = GA 冠军 `g0269`（与 `[decks.default.advice]` 由测试锁死）
    fn default() -> Self {
        Self {
            t2: [101, 201, 501, 301, 401],
            t11: [101, 501, 201, 301, 401],
            t23: [101, 401, 501, 301, 201],
            t35: [101, 501, 301, 401, 201],
            t47: [101, 501, 401, 301, 201],
            t59: [501, 401, 101, 301, 201],
        }
    }
}

// ---------------------------------------------------------------------------
// 原 config.rs 的融合 / 训练员类型（现归卡组参数）
// ---------------------------------------------------------------------------

/// 融合预设：第 2 年 2 组、第 3 年剩余 3 组
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionConfig {
    /// 第 2 年（turn 23 结束）：恰好 2 组
    pub year2: Vec<FusionPair>,
    /// 第 3 年（turn 47 结束）：剩余 3 组
    pub year3: Vec<FusionPair>,
}

/// 一组融合配对（基础教诲 → 融合对象）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionPair {
    /// 被融合的基础教诲 id（101–501）
    pub base: i32,
    /// 融合对象 id（第 2 年 601 / 701，第 3 年 801 / 901 / 1001）
    pub target: i32,
}

/// 手写训练员策略旋钮（全部可扫参；默认档 = 攻略 / memo 方向）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 自选比赛（freeRaces 义务区间）补足紧迫度的软加分权重
    ///
    /// 区间内少打 1 场即育成失败（引擎通用规则）；`0` = 关闭软加分，
    /// 但 `need ≥ left`（再不打赢不了）的硬保底不受此开关影响。
    #[serde(default = "default_free_race_urgency")]
    pub free_race_urgency: f64,
    /// 五维策略权重（官方评分边际之外的偏好；默认全 1）
    pub status_weights: [f64; 5],
    /// 属性 / 资源 / 事件的折算权重
    pub weights: ValueWeights,
    /// 教诲分配价值权重
    pub advice: AdviceWeights,
    /// 动态权重（随回合 / 饱和度 / 进度调制上面三套静态权重）
    ///
    /// `#[serde(default)]`：默认层文件不写这一段也能解析，缺省即全 0 = 静态行为。
    #[serde(default)]
    pub dynamic: DynamicConfig,
    /// 溢出处理（近顶轴让位、副属性 / PT 保留；默认层见 `data/matsuri_config.toml`）
    #[serde(default)]
    pub overflow: OverflowConfig,
    /// 第三年合宿结束～T71 的稽古兑现（缺省段 = 关闭；URA 不参与）
    #[serde(default)]
    pub endgame: EndgameConfig,
}

/// 动态权重旋钮（逐回合调制；全部默认 `0.0` = 关闭，完全退化成静态打分）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DynamicConfig {
    /// 五维追赶强度：`w[i] *= 1 + 2k(head[i] − mean_head)`，`head[i]` = 剩余空间占比
    pub status_catchup: f64,
    /// 演目 Lv 追赶强度：`level_value *= 1 + k·max(0, mean_lv − lv[act])`
    pub act_catchup: f64,
    /// 体力影子价格随回合衰减：`weights.vital *= 1 − k·(turn / max_turn)`
    pub vital_decay: f64,
    /// 满体力闲置惩罚放大：`idle_penalty *= 1 + k·(vital − 70) / 30`
    pub vital_idle: f64,
    /// 稽古节奏（卡回合 ↔ 加速升级）：`level_value *= 1 + k·(达标线 − 短板 Lv) / 3`
    pub keiko_tempo: f64,
}

impl Default for DynamicConfig {
    /// 全 0 = 关闭全部动态调制（与上游静态行为逐位相同）
    fn default() -> Self {
        Self {
            status_catchup: 0.0,
            act_catchup: 0.0,
            vital_decay: 0.0,
            vital_idle: 0.0,
            keiko_tempo: 0.0,
        }
    }
}

/// 溢出处理旋钮（属性接近**生效上限**时，训练分配向其余轴倾斜）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OverflowConfig {
    /// 距生效上限多少比例内算「近顶」；`0` = 只把真正顶满的轴算近顶
    pub margin: f64,
    /// 非近顶轴的权重放大强度：`w[j] *= 1 + tilt * near_n / 5`
    pub tilt: f64,
    /// 主属性近顶时，副属性 + PT 的保留加分（防止完全放弃该训练）
    pub keep: f64,
}

impl Default for OverflowConfig {
    fn default() -> Self {
        Self {
            margin: 0.0,
            tilt: 0.0,
            keep: 0.0,
        }
    }
}

/// 第三年合宿结束～T71 的稽古兑现旋钮（URA 全稽古回合不参与）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EndgameConfig {
    /// 每个满槽稽古候选的基础加分
    pub keiko: f64,
    /// 满槽数 > 剩余普通回合数时的放大强度
    pub scarcity: f64,
    /// 每级未满 `act Lv` 的加分（最多按 2 级计）
    pub level: f64,
    /// 同一窗口普通训练的扣分（只扣 `Action::Train`，不扣休息 / 治病）
    pub normal_penalty: f64,
}

impl Default for EndgameConfig {
    fn default() -> Self {
        Self {
            keiko: 0.0,
            scarcity: 0.0,
            level: 0.0,
            normal_penalty: 0.0,
        }
    }
}

/// `free_race_urgency` 的缺省值（未写该键时按 1.0，保持补足行为开启）
fn default_free_race_urgency() -> f64 {
    1.0
}

/// 属性 / 资源 / 事件的折算权重（单位 = 评分点）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 特化上限（601 特別稽古的 `status_limit`）权重
    #[serde(default)]
    pub status_limit: f64,
    /// 助力の支え（401 的 `support_assist` 开关量）权重
    #[serde(default)]
    pub assist: f64,
    /// 落位先验权重：命中一位预设落位加这么多分（0 = 关闭）
    #[serde(default)]
    pub route: f64,
}

impl Default for TrainerConfig {
    /// 内置默认档 = GA 冠军 `g0269`（与 `[decks.default.trainer]` 由测试锁死）
    fn default() -> Self {
        Self {
            pt_favor_rate: 1.931954,
            rest_vital: 66,
            checkpoint_margin: 4,
            act_level_target: 2,
            self_race: false,
            friend_outing_early: true,
            free_race_urgency: 1.764718,
            status_weights: [0.994888, 0.542242, 0.407084, 0.156003, 0.860951],
            weights: ValueWeights {
                vital: 5.730157,
                motivation: 10.93116,
                friendship: 3.339714,
                hint: 4.313881,
                gauge: 53.094769,
                act_level: 58.536224,
                checkpoint: 2223.151961,
                fat: 193.041812,
            },
            advice: AdviceWeights {
                card: 1.684001,
                npc: 0.3559,
                train: 0.153215,
                keiko: 1.158553,
                level: 0.772682,
                milestone: 0.028781,
                status_limit: 0.0,
                assist: 0.0,
                route: 392.251041,
            },
            dynamic: DynamicConfig {
                status_catchup: 3.086141,
                act_catchup: 0.012934,
                vital_decay: 0.005776,
                vital_idle: 1.625975,
                keiko_tempo: 2.239315,
            },
            overflow: OverflowConfig {
                margin: 0.076106,
                tilt: 3.54432,
                keep: 0.018218,
            },
            endgame: EndgameConfig {
                keiko: 763.274261,
                scarcity: 0.45993,
                level: 162.659083,
                normal_penalty: 462.583525,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// 卡组表（[decks.*]）
// ---------------------------------------------------------------------------

/// 解析后的卡组表：default 完整档 + 非 variant comp 索引 + 全量名字索引
#[derive(Debug, Clone, PartialEq)]
pub struct DeckTable {
    /// 完整默认档（继承基座 + 未匹配兜底）
    default: DeckParams,
    /// 非 variant 档：comp → 档名（自动匹配）
    by_comp: BTreeMap<[usize; 5], String>,
    /// 全部档（含 default / variant）：名字 → 参数（显式选择）
    by_name: BTreeMap<String, DeckParams>,
    /// 档名 → note（展示用）
    notes: BTreeMap<String, String>,
    /// variant 档名（只允许显式选择）
    variants: BTreeSet<String>,
}

impl DeckTable {
    /// 默认档参数
    #[must_use]
    pub fn default_params(&self) -> &DeckParams {
        &self.default
    }

    /// 自动匹配：按 comp 查非 variant 档；找不到返回 default
    #[must_use]
    pub fn resolve_comp(&self, comp: [usize; 5]) -> &DeckParams {
        self.by_name
            .get(self.profile_name(comp))
            .unwrap_or(&self.default)
    }

    /// 自动匹配的档名：按 comp 查非 variant 档；找不到返回 `default`
    #[must_use]
    pub fn profile_name(&self, comp: [usize; 5]) -> &str {
        self.by_comp.get(&comp).map_or(DEFAULT_DECK, String::as_str)
    }

    /// 显式选择：按名字查（含 default / variant）
    #[must_use]
    pub fn resolve_name(&self, name: &str) -> Option<&DeckParams> {
        self.by_name.get(name)
    }

    /// 档名列表（含 default；展示 / 诊断用）
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }

    /// 某档的 note（无则空串）
    #[must_use]
    pub fn note(&self, name: &str) -> &str {
        self.notes.get(name).map_or("", String::as_str)
    }

    /// 是否扫参变体
    #[must_use]
    pub fn is_variant(&self, name: &str) -> bool {
        self.variants.contains(name)
    }

    /// 全表校验（载入时已做；`Default` 兜底再查一次）
    pub fn validate(&self) -> Result<()> {
        self.default.validate().context("[decks.default]")?;
        for (name, params) in &self.by_name {
            params
                .validate()
                .with_context(|| format!("[decks.{name}]"))?;
        }
        Ok(())
    }

    /// 由原始条目构建（default 必须完整；其他档深合并到 default）
    fn from_entries(mut raw: BTreeMap<String, DeckEntry>) -> Result<Self> {
        let default_entry = raw
            .remove(DEFAULT_DECK)
            .context("[decks.default] 必须存在")?;
        ensure!(
            !default_entry.variant,
            "[decks.default] 不能标记 variant = true"
        );
        validate_comp(&default_entry.comp)?;
        let default = default_entry.resolve_complete()?;
        default.validate().context("[decks.default]")?;

        let mut by_name = BTreeMap::new();
        let mut by_comp = BTreeMap::new();
        let mut notes = BTreeMap::new();
        let mut variants = BTreeSet::new();
        by_name.insert(DEFAULT_DECK.to_string(), default.clone());
        by_comp.insert(default_entry.comp, DEFAULT_DECK.to_string());
        notes.insert(DEFAULT_DECK.to_string(), default_entry.note.clone());

        for (name, entry) in raw {
            ensure!(name != DEFAULT_DECK, "[decks.default] 重复");
            validate_comp(&entry.comp)?;
            let params = entry
                .resolve_patch(&default)
                .with_context(|| format!("[decks.{name}] 合并失败"))?;
            params
                .validate()
                .with_context(|| format!("[decks.{name}] 校验失败"))?;
            if entry.variant {
                variants.insert(name.clone());
            } else if by_comp.insert(entry.comp, name.clone()).is_some() {
                bail!(
                    "[decks.{name}] comp {:?} 与非 variant 档重复（用 variant = true 区分扫参变体）",
                    entry.comp
                );
            }
            notes.insert(name.clone(), entry.note);
            by_name.insert(name, params);
        }

        Ok(Self {
            default,
            by_comp,
            by_name,
            notes,
            variants,
        })
    }
}

impl Default for DeckTable {
    /// 内置兜底：只有 default 档；`resolve_comp` 全部回落到它
    fn default() -> Self {
        let default = DeckParams::default();
        let mut by_name = BTreeMap::new();
        let mut notes = BTreeMap::new();
        by_name.insert(DEFAULT_DECK.to_string(), default.clone());
        notes.insert(DEFAULT_DECK.to_string(), String::new());
        Self {
            default,
            by_comp: BTreeMap::new(),
            by_name,
            notes,
            variants: BTreeSet::new(),
        }
    }
}

impl<'de> Deserialize<'de> for DeckTable {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = BTreeMap::<String, DeckEntry>::deserialize(deserializer)?;
        Self::from_entries(raw).map_err(serde::de::Error::custom)
    }
}

/// `[decks.<name>]` 原始条目（参数块可选；default 档要求完整）
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckEntry {
    /// 速 / 耐 / 力 / 根 / 智 训练卡张数（合计 5）
    comp: [usize; 5],
    /// 扫参变体：只允许显式选择，不参与自动匹配与兜底
    #[serde(default)]
    variant: bool,
    /// 人读备注（不参与决策）
    #[serde(default)]
    note: String,
    /// 融合覆盖（year2 / year3 分段）
    #[serde(default)]
    fusion: Option<FusionPatch>,
    /// 教诲落位覆盖（按窗口）
    #[serde(default)]
    advice: Option<AdvicePatch>,
    /// 训练员覆盖（逐字段深合并；先转 toml::Value）
    #[serde(default)]
    trainer: Option<toml::Value>,
}

/// `[decks.x.fusion]`：两段分别整段替换
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FusionPatch {
    #[serde(default)]
    year2: Option<Vec<FusionPair>>,
    #[serde(default)]
    year3: Option<Vec<FusionPair>>,
}

/// `[decks.x.advice]`：按窗口覆盖
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvicePatch {
    #[serde(default)]
    t2: Option<[i32; 5]>,
    #[serde(default)]
    t11: Option<[i32; 5]>,
    #[serde(default)]
    t23: Option<[i32; 5]>,
    #[serde(default)]
    t35: Option<[i32; 5]>,
    #[serde(default)]
    t47: Option<[i32; 5]>,
    #[serde(default)]
    t59: Option<[i32; 5]>,
}

impl DeckEntry {
    /// default 档：三个参数块必须给全
    fn resolve_complete(&self) -> Result<DeckParams> {
        let fusion = self
            .fusion
            .as_ref()
            .context("缺少 [decks.default.fusion]")?;
        let advice = self
            .advice
            .as_ref()
            .context("缺少 [decks.default.advice]")?;
        let trainer = self
            .trainer
            .as_ref()
            .context("缺少 [decks.default.trainer]")?;
        Ok(DeckParams {
            fusion: FusionConfig {
                year2: fusion.year2.clone().context("缺少 fusion.year2")?,
                year3: fusion.year3.clone().context("缺少 fusion.year3")?,
            },
            advice: advice.to_preset()?,
            trainer: trainer
                .clone()
                .try_into::<TrainerConfig>()
                .map_err(|err| anyhow::anyhow!("trainer 解析失败: {err}"))?,
        })
    }

    /// 其他档：以 default 为基座深合并
    fn resolve_patch(&self, base: &DeckParams) -> Result<DeckParams> {
        let mut params = base.clone();
        if let Some(fusion) = &self.fusion {
            if let Some(year2) = &fusion.year2 {
                params.fusion.year2 = year2.clone();
            }
            if let Some(year3) = &fusion.year3 {
                params.fusion.year3 = year3.clone();
            }
        }
        if let Some(advice) = &self.advice {
            advice.apply_to(&mut params.advice);
        }
        if let Some(patch) = &self.trainer {
            let mut value = toml::Value::try_from(&params.trainer)
                .map_err(|err| anyhow::anyhow!("序列化 trainer 失败: {err}"))?;
            deep_merge(&mut value, patch);
            params.trainer = value
                .try_into::<TrainerConfig>()
                .map_err(|err| anyhow::anyhow!("合并后 trainer 解析失败: {err}"))?;
        }
        Ok(params)
    }
}

impl AdvicePatch {
    /// 只写部分窗口时，其余继承 default
    fn to_preset(&self) -> Result<AdvicePreset> {
        Ok(AdvicePreset {
            t2: self.t2.context("缺少 advice.t2")?,
            t11: self.t11.context("缺少 advice.t11")?,
            t23: self.t23.context("缺少 advice.t23")?,
            t35: self.t35.context("缺少 advice.t35")?,
            t47: self.t47.context("缺少 advice.t47")?,
            t59: self.t59.context("缺少 advice.t59")?,
        })
    }

    fn apply_to(&self, preset: &mut AdvicePreset) {
        if let Some(value) = self.t2 {
            preset.t2 = value;
        }
        if let Some(value) = self.t11 {
            preset.t11 = value;
        }
        if let Some(value) = self.t23 {
            preset.t23 = value;
        }
        if let Some(value) = self.t35 {
            preset.t35 = value;
        }
        if let Some(value) = self.t47 {
            preset.t47 = value;
        }
        if let Some(value) = self.t59 {
            preset.t59 = value;
        }
    }
}

/// TOML 表递归合并：表递归、数组 / 标量整体替换
fn deep_merge(base: &mut toml::Value, patch: &toml::Value) {
    match (base, patch) {
        (toml::Value::Table(base), toml::Value::Table(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(slot) => deep_merge(slot, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

// ---------------------------------------------------------------------------
// 校验
// ---------------------------------------------------------------------------

/// 训练卡构成：每类 0–5，合计 5
fn validate_comp(comp: &[usize; 5]) -> Result<()> {
    ensure!(
        comp.iter().all(|count| *count <= 5),
        "comp 单类型张数不能超过 5（实际 {comp:?}）"
    );
    let total: usize = comp.iter().sum();
    ensure!(
        total == 5,
        "comp 训练卡合计须为 5（实际 {total}: {comp:?}）"
    );
    Ok(())
}

/// 教诲落位：每窗口必须是 101/201/301/401/501 的一个排列
fn validate_advice(advice: &AdvicePreset) -> Result<()> {
    for (name, window) in advice.windows() {
        let mut sorted = *window;
        sorted.sort_unstable();
        ensure!(
            sorted == BASE_ADVICE_IDS,
            "advice.{name} 必须是 101/201/301/401/501 的排列（实际 {window:?}）"
        );
    }
    Ok(())
}

/// 融合校验：year2 恰 2 / year3 恰 3；base 只能用 101–501 且覆盖全部；
/// 年份内 target 各自恰好用一次
fn validate_fusion(fusion: &FusionConfig) -> Result<()> {
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
    ensure!(
        trainer.free_race_urgency.is_finite() && (0.0..=10.0).contains(&trainer.free_race_urgency),
        "trainer.free_race_urgency 应在 0–10"
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
    let entries: [(&str, f64); 9] = [
        ("advice.card", advice.card),
        ("advice.npc", advice.npc),
        ("advice.train", advice.train),
        ("advice.keiko", advice.keiko),
        ("advice.level", advice.level),
        ("advice.milestone", advice.milestone),
        ("advice.status_limit", advice.status_limit),
        ("advice.assist", advice.assist),
        ("advice.route", advice.route),
    ];
    for (name, value) in entries {
        ensure!(
            value.is_finite() && value >= 0.0,
            "trainer.{name} 应为非负有限数"
        );
    }
    let dynamic = &trainer.dynamic;
    let entries: [(&str, f64, f64); 5] = [
        ("dynamic.status_catchup", dynamic.status_catchup, 4.0),
        ("dynamic.act_catchup", dynamic.act_catchup, 2.0),
        ("dynamic.vital_decay", dynamic.vital_decay, 1.0),
        ("dynamic.vital_idle", dynamic.vital_idle, 4.0),
        ("dynamic.keiko_tempo", dynamic.keiko_tempo, 4.0),
    ];
    for (name, value, upper) in entries {
        ensure!(
            value.is_finite() && (0.0..=upper).contains(&value),
            "trainer.{name} 应在 0–{upper}（实际 {value}）"
        );
    }
    let overflow = &trainer.overflow;
    let entries: [(&str, f64, f64); 3] = [
        ("overflow.margin", overflow.margin, 0.5),
        ("overflow.tilt", overflow.tilt, 4.0),
        ("overflow.keep", overflow.keep, 2.0),
    ];
    for (name, value, upper) in entries {
        ensure!(
            value.is_finite() && (0.0..=upper).contains(&value),
            "trainer.{name} 应在 0–{upper}（实际 {value}）"
        );
    }
    let endgame = &trainer.endgame;
    let entries: [(&str, f64, f64); 4] = [
        ("endgame.keiko", endgame.keiko, 1000.0),
        ("endgame.scarcity", endgame.scarcity, 4.0),
        ("endgame.level", endgame.level, 500.0),
        ("endgame.normal_penalty", endgame.normal_penalty, 1000.0),
    ];
    for (name, value, upper) in entries {
        ensure!(
            value.is_finite() && (0.0..=upper).contains(&value),
            "trainer.{name} 应在 0–{upper}（实际 {value}）"
        );
    }
    Ok(())
}

/// 由卡组 id（`card_id * 10 + rank`，含 1 张友人）算 5 类训练卡构成
pub fn deck_comp(deck: &[u32]) -> Result<[usize; 5]> {
    let mut comp = [0usize; 5];
    let mut training = 0usize;
    for &idrank in deck {
        let card = SupportCard::new(idrank)?;
        match card.card_type {
            0..=4 => {
                comp[card.card_type as usize] += 1;
                training += 1;
            }
            5 => {}
            other => bail!("卡类型不合法：{other}（card {idrank}）"),
        }
    }
    ensure!(training == 5, "训练卡应恰好 5 张（实际 {training}）");
    Ok(comp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp_value(comp: [usize; 5]) -> toml::Value {
        toml::Value::Array(
            comp.iter()
                .map(|value| toml::Value::Integer(*value as i64))
                .collect(),
        )
    }

    fn base_table() -> toml::Table {
        let mut table = toml::Table::new();
        let mut value = toml::Value::try_from(DeckParams::default()).expect("默认档可序列化");
        value
            .as_table_mut()
            .expect("默认档为表")
            .insert("comp".to_string(), comp_value([2, 1, 1, 0, 1]));
        table.insert(DEFAULT_DECK.to_string(), value);
        table
    }

    fn parse(table: &toml::Table) -> Result<DeckTable> {
        let text = toml::to_string(table)?;
        Ok(toml::from_str(&text)?)
    }

    #[test]
    fn default_matches_builtin() {
        let table = parse(&base_table()).expect("默认档应可解析");
        assert_eq!(table.resolve_comp([2, 1, 1, 0, 1]), &DeckParams::default());
    }

    #[test]
    fn patch_deep_merges_trainer_and_advice() {
        let mut table = base_table();
        let mut entry = toml::Table::new();
        entry.insert("comp".to_string(), comp_value([1, 2, 1, 0, 1]));
        entry.insert("note".to_string(), toml::Value::String("耐主".into()));
        let mut trainer = toml::Table::new();
        trainer.insert(
            "status_weights".to_string(),
            toml::Value::Array(
                [0.6, 1.3, 0.8, 0.3, 2.0]
                    .into_iter()
                    .map(toml::Value::Float)
                    .collect(),
            ),
        );
        let mut weights = toml::Table::new();
        weights.insert("vital".to_string(), toml::Value::Float(9.0));
        trainer.insert("weights".to_string(), toml::Value::Table(weights));
        entry.insert("trainer".to_string(), toml::Value::Table(trainer));
        let mut advice = toml::Table::new();
        advice.insert("t47".to_string(), comp_value([101, 501, 401, 301, 201]));
        entry.insert("advice".to_string(), toml::Value::Table(advice));
        table.insert("stamina_build".to_string(), toml::Value::Table(entry));

        let parsed = parse(&table).expect("深合并应可解析");
        let params = parsed.resolve_comp([1, 2, 1, 0, 1]);
        assert_eq!(params.trainer.status_weights, [0.6, 1.3, 0.8, 0.3, 2.0]);
        assert_eq!(params.trainer.weights.vital, 9.0);
        assert_eq!(
            params.trainer.weights.hint,
            DeckParams::default().trainer.weights.hint,
            "未写的字段继承 default"
        );
        assert_eq!(params.advice.t47, [101, 501, 401, 301, 201]);
        assert_eq!(
            params.advice.t2,
            DeckParams::default().advice.t2,
            "未写的窗口继承 default"
        );
        assert_eq!(parsed.note("stamina_build"), "耐主");
        assert!(!parsed.is_variant("stamina_build"));
    }

    #[test]
    fn variant_is_explicit_only() {
        let mut table = base_table();
        let mut entry = toml::Table::new();
        entry.insert("comp".to_string(), comp_value([2, 1, 1, 0, 1]));
        entry.insert("variant".to_string(), toml::Value::Boolean(true));
        let mut advice = toml::Table::new();
        advice.insert("t47".to_string(), comp_value([101, 501, 401, 301, 201]));
        entry.insert("advice".to_string(), toml::Value::Table(advice));
        table.insert("default__advB".to_string(), toml::Value::Table(entry));

        let parsed = parse(&table).expect("变体应可解析");
        assert_eq!(
            parsed.resolve_comp([2, 1, 1, 0, 1]).advice,
            DeckParams::default().advice,
            "自动匹配只认非 variant 档"
        );
        let variant = parsed.resolve_name("default__advB").expect("显式选择");
        assert_eq!(variant.advice.t47, [101, 501, 401, 301, 201]);
        assert!(parsed.is_variant("default__advB"));
    }

    #[test]
    fn duplicate_non_variant_comp_rejected() {
        let mut table = base_table();
        let mut entry = toml::Table::new();
        entry.insert("comp".to_string(), comp_value([2, 1, 1, 0, 1]));
        table.insert("speed_dup".to_string(), toml::Value::Table(entry));
        assert!(parse(&table).is_err(), "非 variant 同 comp 应报错");
    }

    #[test]
    fn missing_default_rejected() {
        let mut table = base_table();
        table.remove(DEFAULT_DECK);
        assert!(parse(&table).is_err(), "缺 default 应报错");
    }

    #[test]
    fn bad_advice_permutation_rejected() {
        let mut table = base_table();
        let default = table
            .get_mut(DEFAULT_DECK)
            .and_then(toml::Value::as_table_mut)
            .expect("default");
        let advice = default
            .get_mut("advice")
            .and_then(toml::Value::as_table_mut)
            .expect("advice");
        advice.insert("t2".to_string(), comp_value([101, 101, 301, 401, 501]));
        assert!(parse(&table).is_err(), "重复槽位应报错");
    }

    #[test]
    fn unknown_trainer_key_rejected() {
        let mut table = base_table();
        let mut entry = toml::Table::new();
        entry.insert("comp".to_string(), comp_value([1, 2, 1, 0, 1]));
        let mut trainer = toml::Table::new();
        trainer.insert("typo".to_string(), toml::Value::Float(1.0));
        entry.insert("trainer".to_string(), toml::Value::Table(trainer));
        table.insert("stamina_build".to_string(), toml::Value::Table(entry));
        assert!(parse(&table).is_err(), "未知训练员字段应报错");
    }
}
