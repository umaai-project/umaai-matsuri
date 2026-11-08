//! 剧本全局配置段（`matsuri_config.toml`）：schema、载入与覆盖层合并
//!
//! 复用引擎「默认层 + 覆盖层」的泛型解析（`OverlayConfig` / `parse_with_overlay`）：
//! 默认层字段齐全、未知键严格报错；覆盖层同构、字段全 `Option`，段缺失整段沿用。
//! 根目录由宿主注入，本模块不写死路径。
//!
//! 文件只保留**全局运行设置**：`[pre]`（局外切 / 心情盾）与 `[search]`（搜索预算 / 作用面）。
//! 按卡组的融合 / 教诲落位 / 训练员旋钮在 [`crate::deck_config`] 的 `[decks.*]` 段；
//! `MatsuriConfig.deck` 是本次跑局解析后的生效参数（`#[serde(skip)]`，文件里不出现）。

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use umaai::config::{OverlayConfig, parse_with_overlay};

use crate::deck_config::{DeckParams, DeckTable};

/// 剧本配置文件名（放在宿主注入的剧本配置根下）
pub const MATSURI_CONFIG_FILE: &str = "matsuri_config.toml";

/// 剧本专属配置段
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriConfig {
    /// 育成前（局外）状态
    #[serde(default)]
    pub pre: PreConfig,
    /// 搜索档作用面（`search_n > 0` 时生效）
    #[serde(default)]
    pub search: SearchScopeConfig,
    /// 按卡组的参数表（`[decks.*]`；载入时解析完成）
    ///
    /// `Arc`：`MatsuriGame` 每次 rollout 克隆只拷指针，不深拷贝整张表。
    #[serde(default)]
    pub decks: Arc<DeckTable>,
    /// 本次跑局的生效参数（`for_deck` / `for_profile` 填充；文件里不出现）
    #[serde(skip)]
    pub deck: Arc<DeckParams>,
}

/// 搜索档作用面：预算 + 并行度 + 每一类决策走搜索还是手写
///
/// `search_n` 归剧本配置（每候选 rollout 次数，`0` = 手写档，默认 8192）；`threads`
/// 是搜索 rayon 全局池线程数（`0` = 自动，由入口在首次搜索前应用）；其余内核参数
/// （`max_depth` / UCB / 激进度）仍取引擎 `[search]`。候选剪枝宽度不进配置，
/// 由搜索训练员的常量决定（Train 全量、教诲 16）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchScopeConfig {
    /// 每个候选的 rollout 次数（0 = 手写档；测试 1/1024；默认 8192）
    pub search_n: usize,
    /// 搜索 rayon 全局池线程数（0 = 自动；显式 `RAYON_NUM_THREADS` 优先）
    pub threads: usize,
    /// Train 主动作是否走搜索
    pub train: bool,
    /// 教诲分配是否走搜索（120 候选先按手写分剪枝）
    pub advice: bool,
    /// 事件选项是否走搜索（应用当前选项后从同 stage 的下一事件继续）
    pub event: bool,
}

impl Default for SearchScopeConfig {
    /// 与仓内默认层 `[search]` 段同值
    fn default() -> Self {
        Self {
            search_n: 8192,
            threads: 0,
            train: true,
            advice: true,
            event: true,
        }
    }
}

/// 局外「切」状态：小切 / 切者二选一，建局时注入
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Qie {
    /// 不注入（默认：按账号实际情况用覆盖层选）
    #[default]
    None,
    /// 小切（勉強家）：评分 PT 项 ×1.04；局内获得切者后失效
    Xiaoqie,
    /// 切者：评分 PT 项 ×1.1；与小切互斥，局内获得切者时同样落到本状态
    Qiezhe,
}

/// 育成前（局外）状态
///
/// 与引擎 `[sim]` 的种马因子同属**建局前**输入；剧本自持，不进引擎配置。
/// 默认不注入任何局外状态；理想测试档用 `bench --qie xiaoqie --shield 2` 或覆盖层开启。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreConfig {
    /// 局外切状态（默认不注入）
    pub qie: Qie,
    /// 心情盾（ポジティブ思考）次数：抵消 N 次干劲下降（一次扣减只消耗 1 层）
    pub positive_thinking: i32,
}

impl Default for PreConfig {
    fn default() -> Self {
        Self {
            qie: Qie::None,
            positive_thinking: 0,
        }
    }
}

/// 覆盖层：与默认层同构、字段全 `Option`（`None` = 沿用默认层）
///
/// 卡组参数（`[decks.*]`）不提供玩家覆盖；覆盖层只调全局运行设置。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriConfigOverlay {
    /// 育成前状态：给了就**整段替换**
    pre: Option<PreConfig>,
    /// 搜索作用面：给了就**整段替换**
    search: Option<SearchScopeConfig>,
}

impl OverlayConfig for MatsuriConfig {
    type Overlay = MatsuriConfigOverlay;

    fn apply_overlay(&mut self, overlay: Self::Overlay) {
        if let Some(pre) = overlay.pre {
            self.pre = pre;
        }
        if let Some(search) = overlay.search {
            self.search = search;
        }
    }

    fn validate(&self) -> Result<()> {
        MatsuriConfig::validate(self)
    }
}

impl MatsuriConfig {
    /// 解析 TOML 文本（默认层 + 可选覆盖层；测试 / 内嵌场景复用）
    ///
    /// 解析后把 `deck` 预置为 `[decks.default]`；入口再按实际卡组 `for_deck` 覆盖。
    pub fn parse(base_toml: &str, overlay_toml: Option<&str>) -> Result<Self> {
        let mut config: Self = parse_with_overlay(base_toml, overlay_toml)?;
        config.deck = Arc::new(config.decks.default_params().clone());
        Ok(config)
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

    /// 按卡组构成解析出生效参数（找不到 → default 档）
    #[must_use]
    pub fn for_deck(&self, comp: [usize; 5]) -> Self {
        let mut config = self.clone();
        config.deck = Arc::new(self.decks.resolve_comp(comp).clone());
        config
    }

    /// 按档名解析出生效参数（`default` / 显式档 / 扫参 variant）
    pub fn for_profile(&self, name: &str) -> Result<Self> {
        let params = self
            .decks
            .resolve_name(name)
            .with_context(|| format!("未知卡组档: {name}"))?;
        let mut config = self.clone();
        config.deck = Arc::new(params.clone());
        Ok(config)
    }

    /// 载入期校验：默认档 / 各卡组档（含深合并结果）已在 `DeckTable` 解析时完成；
    /// 这里再查一遍，保证 `Default` 兜底与覆盖层路径也不会带进非法参数。
    fn validate(&self) -> Result<()> {
        self.decks.validate()?;
        self.deck.validate()
    }
}

impl Default for MatsuriConfig {
    /// 与仓内默认层同值（未注入配置的宿主 / 测试兜底）；默认档 = 内置 `DeckParams`
    fn default() -> Self {
        Self {
            pre: PreConfig::default(),
            search: SearchScopeConfig::default(),
            decks: Arc::new(DeckTable::default()),
            deck: Arc::new(DeckParams::default()),
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
        let default = MatsuriConfig::default();
        assert_eq!(cfg.pre, default.pre);
        assert_eq!(cfg.search, default.search);
        assert_eq!(cfg.deck, default.deck, "生效参数默认 = 内置 default 档");
        assert_eq!(
            cfg.decks.default_params(),
            &DeckParams::default(),
            "[decks.default] 必须与内置兜底一致"
        );
        assert!(
            cfg.decks.resolve_name("wisdom_build").is_some(),
            "默认层应带 wisdom_build 档（stamina / guts 已于 2026-11-07 退役）"
        );
    }

    #[test]
    fn unknown_key_rejected() {
        let bad = repo_text().replace("year2", "year_two");
        let err = MatsuriConfig::parse(&bad, None).unwrap_err();
        assert!(err.to_string().contains("解析默认配置失败"));
    }

    #[test]
    fn overlay_replaces_pre_and_search() {
        let overlay = "[pre]\nqie = \"xiaoqie\"\npositive_thinking = 2\n\
                       [search]\nsearch_n = 1024\nthreads = 8\n\
                       train = false\nadvice = false\nevent = false\n";
        let cfg = MatsuriConfig::parse(&repo_text(), Some(overlay)).expect("覆盖层应可合并");
        assert_eq!(cfg.pre.qie, Qie::Xiaoqie);
        assert_eq!(cfg.pre.positive_thinking, 2);
        assert_eq!(cfg.search.search_n, 1024);
        assert_eq!(cfg.search.threads, 8, "覆盖层可指定搜索线程数");
        assert!(!cfg.search.train);
        assert_eq!(*cfg.deck, DeckParams::default(), "卡组档不受覆盖层影响");
    }

    /// 卡组参数不给玩家覆盖：覆盖层写 `[trainer]` / `[decks.*]` 应严格报错
    #[test]
    fn deck_overlay_rejected() {
        let bad = "[trainer]\npt_favor_rate = 1.0\n";
        assert!(
            MatsuriConfig::parse(&repo_text(), Some(bad)).is_err(),
            "覆盖层不应接受 [trainer]"
        );
        let bad = "[decks.foo]\ncomp = [2, 1, 1, 0, 1]\n";
        assert!(
            MatsuriConfig::parse(&repo_text(), Some(bad)).is_err(),
            "覆盖层不应接受 [decks.*]"
        );
    }

    #[test]
    fn search_scope_overlay_and_unknown_key() {
        let overlay = "[search]\n\
                       search_n = 1024\n\
                       train = false\n\
                       advice = false\n\
                       event = false\n";
        let cfg = MatsuriConfig::parse(&repo_text(), Some(overlay)).expect("整段覆盖应可合并");
        assert_eq!(cfg.search.search_n, 1024);
        assert_eq!(
            cfg.search.threads, 0,
            "[search] 整段覆盖时未写 threads 回落为自动"
        );
        assert!(!cfg.search.train);

        let bad_key = repo_text().replace("[search]", "[search]\nunknown = 1");
        assert!(MatsuriConfig::parse(&bad_key, None).is_err());
    }

    #[test]
    fn for_deck_falls_back_to_default() {
        let cfg = MatsuriConfig::default();
        let resolved = cfg.for_deck([2, 1, 1, 0, 1]);
        assert_eq!(*resolved.deck, DeckParams::default());
        let unknown = cfg.for_deck([1, 2, 1, 0, 1]);
        assert_eq!(*unknown.deck, DeckParams::default());
    }

    #[test]
    fn for_profile_selects_default_and_rejects_unknown() {
        let cfg = MatsuriConfig::default();
        let resolved = cfg.for_profile("default").expect("default 档应存在");
        assert_eq!(*resolved.deck, DeckParams::default());
        assert!(cfg.for_profile("missing").is_err(), "未知档应报错");
    }
}
