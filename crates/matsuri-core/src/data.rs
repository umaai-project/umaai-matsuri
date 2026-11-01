//! 剧本资产（`scenario_matsuri.json`）：schema、载入与校验
//!
//! 剧本数据**不进引擎仓**：根目录由宿主注入（终端 / Web / Android 各按自家布局），
//! 本模块只负责「从给定位置读 + 校验」，不碰任何写死路径。
//!
//! 数据来源：`master.mdb` 的 `single_mode_15_*` 11 张表 + `single_mode_training_effect`
//! （scenario 15，普通训练 / Junior 稽古）+ 官方 / `memo` 口径（普通训练 Lv2–5、Classic 稽古、
//! 基础上限 2200 一组、一次性奖励成功档等）；资产 ↔ 表映射见 [`docs/plan/intake.md`] §4.1。
//! 所有回合字段已转 **0 基**（`mdb` 为 1 基）。
//!
//! [`docs/plan/intake.md`]: ../../docs/plan/intake.md

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use umaai_core::Array5;

/// 剧本数据文件名（放在宿主注入的剧本数据根下）
pub const MATSURI_DATA_FILE: &str = "scenario_matsuri.json";

/// 剧本 ID（骏大祭为 15）
const SCENARIO_ID: i32 = 15;

/// 演目 / 教诲 / 合宿 / 進捗発表会 / 教诲窗口的固定个数
const ACT_COUNT: usize = 5;
const ADVICE_COUNT: usize = 10;
const CAMP_COUNT: usize = 3;
const CHECK_POINT_COUNT: usize = 3;
const ADVICE_WINDOW_COUNT: usize = 6;

/// 演目 Lv 表长度（Lv1–10；上限 6 / SSR 吾妻 10，表本身给满 10 级）
const ACT_LEVEL_MAX: usize = 10;

/// 教诲 id 顺序（与 `single_mode_15_advice` 一致：基础 5 种 + 追加 5 种）
const ADVICE_IDS: [i32; ADVICE_COUNT] = [101, 201, 301, 401, 501, 601, 701, 801, 901, 1001];

// ---------------------------------------------------------------------------
// schema
// ---------------------------------------------------------------------------

/// 剧本全部静态数据
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatsuriData {
    /// 剧本 ID（骏大祭为 15）
    pub scenario_id: i32,
    /// 属性上限与编成位数（`single_mode_scenario` + 官方口径）
    pub limits: Limits,
    /// 5 条演目：`act_type` → 命令 / 训练位 / 编成外神驹
    pub acts: Vec<Act>,
    /// 训练基础值（普通训练 / 演目稽古）
    pub training: TrainingData,
    /// 演目 Lv 1–10 效果（5 条演目共用）
    pub act_effects: Vec<ActLevelEffect>,
    /// 10 种教诲 × Lv1–10（`single_mode_15_advice` + `single_mode_15_advice_effect`）
    pub advices: Vec<Advice>,
    /// 5 个基础教诲的転成（融合）升级效果（`single_mode_15_fusion_effect`）
    pub fusion: Vec<FusionEffect>,
    /// 進捗発表会 / 本番的永久 buff（`single_mode_15_check_point_effect` 20 行聚合成 7 条）
    pub check_points: Vec<CheckPointBuff>,
    /// 進捗発表会 / 本番的一次性奖励（6 条：`schedule` 1–3 × 成功 / 大成功）
    ///
    /// TBD: 成功档来自 `memo` 2026-10-31（中可信度），无实测样本（见 `stubs.md`）。
    pub check_point_rewards: Vec<Reward>,
    /// 育成终了一次性奖励（2 条：全成功 / 全大成功）
    pub career_end_rewards: Vec<Reward>,
    /// 本番演出（`single_mode_15_finals_act_effect` 4 行）
    pub finals_act: FinalsActEffect,
    /// 友人卡出行 5 段 → 演目 Lv+1（`single_mode_15_outing_effect`）
    pub outing: Vec<OutingEffect>,
    /// 進捗発表会 / 本番回合（`single_mode_15_schedule`，0 基）
    pub schedule: Vec<CheckPointSchedule>,
    /// 教诲可调整窗口（`single_mode_15_advice_recommend`，0 基）
    pub advice_windows: Vec<i32>,
    /// 3 次合宿窗口 + 固定演目 Lv+1 结算点
    pub camp: CampData,
    /// 规则常量（gauge 上限 / 入账 / 训练 Lv / 羁绊基础值）
    pub rules: Rules,
}

/// 属性上限与编成位数
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// 五维基础上限（速 / 耐 / 力 / 根 / 智；官方 + memo 口径，非 mdb `max_*`）
    pub five_status_limit_base: Array5,
    /// 五维绝对上限（上限突破后）
    pub five_status_limit_upper: Array5,
    /// 普通支援卡位数（友人另占 1 位）
    pub support_num: i32,
    /// 技能点上限
    pub max_skill_point: i32,
}

/// 一条演目
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Act {
    /// 演目主键（1–5）
    pub act_type: i32,
    /// 演目稽古命令（5101–5104）
    pub command_id: i32,
    /// 对应普通训练下标（0–4：速 / 耐 / 力 / 根 / 智）
    pub train: usize,
    /// 编成外神驹的 chara_id
    pub npc_chara_id: u32,
}

/// 训练基础值
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingData {
    /// 普通训练：`[训练 0-4][等级 0-4] -> [速 耐 力 根 智 SP 体力]`
    ///
    /// TBD: Lv2–5 来自 `memo` 2026-10-31（中可信度），待真机复核（见 `stubs.md`）。
    pub normal: Vec<Vec<[i32; 7]>>,
    /// 演目稽古：按时期两档，各 5 行（行序同训练下标）
    pub keiko: KeikoTraining,
}

/// 演目稽古基础值（当前只启用 Junior 档）
///
/// 时期切换点 = turn 23 的進捗発表会（「演目排练强化」），成功 / 大成功都切档（见 `mechanism.md` §3.2）。
/// **只有两档**：第二档（`classic`）覆盖 Classic 与 Senior，无第三档（U02 2026-11-01 收口）；
/// **基础值没有 Lv2–10 维度**（`command_level` 只查演目效果 / UI，见 `intake.md` §3.2）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeikoTraining {
    /// Junior 档（= `mdb` 5101–5105）
    pub junior: Vec<[i32; 7]>,
    // TBD: Classic 及以后档（= 第二档，覆盖 Classic / Senior）暂时下架；S5 恢复字段 / 资产节 / 校验；
    // 数值见 `docs/plan/intake.md` §3.2（速 21 / 耐 20 / 力 20 / 根 23 / 智 14，SP 10/12/12/12/9，体力 -25 / +5）。
    // pub classic: Vec<[i32; 7]>,
}

/// 演目 Lv 效果（5 条演目共用；`condition_type=1/5` 的项 = 编成卡类型 ≥5 时生效）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActLevelEffect {
    /// 演目 Lv（1–10）
    pub level: i32,
    /// 全基础能力 +N（无条件）
    pub all_status_bonus: i32,
    /// 友情加成 %（无条件）
    pub friendship: i32,
    /// SP 练习效果 %（5 种卡）
    pub sp_training: i32,
    /// Hint 事件发生（5 种卡）
    pub hint_event: bool,
    /// 全 Hint 发动（5 种卡，Lv4 起）
    pub all_hint: bool,
    /// SP 获上限 +N（5 种卡，Lv4 起）
    pub sp_limit: i32,
    /// 基础能力获上限 +N（5 种卡，Lv9 起）
    pub status_limit: i32,
    /// TBD: `act_effect 201` 的恒定隐藏值 30（语义未确认，保留原值，见 `stubs.md`）
    pub sp_training_hidden: i32,
}

/// 一种教诲
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advice {
    /// 教诲 id（101–1001）
    pub id: i32,
    /// 1 = 基础槽（被融合对象）；2 = 追加教诲（融合对象，不单独占槽）
    pub fusion_type: i32,
    /// 传授批次（1 = 第 1 次 / 2 = 第 2 次 / 3 = 第 3 次）
    pub add_schedule_type: i32,
    /// 类型化效果（每种教诲自带的等级表）
    pub effect: AdviceEffect,
}

/// 教诲效果（按教诲种类区分；`levels[Lv-1]`）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdviceEffect {
    /// 101 絆の教え
    Friendship { levels: Vec<FriendshipLevel> },
    /// 201 得意率の教え
    Deyilv { levels: Vec<DeyilvLevel> },
    /// 301 ヒントの教え
    Hint { levels: Vec<HintLevel> },
    /// 401 助力の教え
    Assist { levels: Vec<AssistLevel> },
    /// 501 体力の教え
    Vital { levels: Vec<VitalLevel> },
    /// 601 特化の教え
    Special { levels: Vec<SpecialLevel> },
    /// 701 研鑽の教え
    Kensan { levels: Vec<KensanLevel> },
    /// 801 協力の教え
    Cooperate { levels: Vec<CooperateLevel> },
    /// 901 不屈の教え
    Unyielding { levels: Vec<UnyieldingLevel> },
    /// 1001 両道の教え
    Dual { levels: Vec<DualLevel> },
}

impl AdviceEffect {
    /// 等级行数（正常为 10）
    #[must_use]
    pub fn levels_len(&self) -> usize {
        match self {
            Self::Friendship { levels } => levels.len(),
            Self::Deyilv { levels } => levels.len(),
            Self::Hint { levels } => levels.len(),
            Self::Assist { levels } => levels.len(),
            Self::Vital { levels } => levels.len(),
            Self::Special { levels } => levels.len(),
            Self::Kensan { levels } => levels.len(),
            Self::Cooperate { levels } => levels.len(),
            Self::Unyielding { levels } => levels.len(),
            Self::Dual { levels } => levels.len(),
        }
    }
}

/// 絆の教え：絆 +3 平摊 + 稽古主基础 %
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FriendshipLevel {
    /// 全编组卡绊 +N（平摊、不按人数叠加）
    pub gauge: i32,
    /// 演目稽古主基础 +N%
    pub keiko_main_bonus: i32,
}

/// 得意率の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeyilvLevel {
    /// 得意率 +N
    pub deyilv: i32,
}

/// ヒントの教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HintLevel {
    /// 灵感发生率 +N%
    pub hint_rate: i32,
}

/// 助力の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistLevel {
    /// 非友情训练效果 +N%
    pub training_effect: i32,
    /// 稽古获得「助力の支え」（3 次内 +5%）
    pub support_assist: bool,
}

/// 体力の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VitalLevel {
    /// 演目稽古体力消耗 -N%
    pub keiko_vital_cost_drop: i32,
}

/// 特化の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecialLevel {
    /// 主基础获得量 +N%
    pub main_gain: i32,
    /// 基础能力获上限 +N
    pub status_limit: i32,
}

/// 研鑽の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KensanLevel {
    /// 「追加の稽古」发生率 +N%（1 回合 1 次共通掷骰，不按人数加算）
    pub rate: i32,
}

/// 協力の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CooperateLevel {
    /// 友情加成 +N%
    pub friendship: i32,
    /// 该演目每个担当（卡+NPC）额外登场处数
    pub split: i32,
}

/// 不屈の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnyieldingLevel {
    /// 体力 ≥N 时失败率 -100%（0 = 无条件）
    pub fail_vital_threshold: i32,
    /// 演目稽古训练效果 +N%（Lv8 起）
    pub keiko_training_effect: i32,
}

/// 両道の教え
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DualLevel {
    /// 副基础获得量 +N%
    pub sub_gain: i32,
    /// 演目稽古训练效果 +N%
    pub keiko_training_effect: i32,
}

/// 一个基础教诲的転成（融合）升级效果
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionEffect {
    /// 被融合的基础教诲 id（101–501）
    pub advice_id: i32,
    /// 类型化升级效果
    pub effect: FusionEffectKind,
}

/// 転成升级效果（与基础教诲种类对应）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FusionEffectKind {
    /// 101 絆 → 稽古主基础 +N%；合宿中每个担当各额外 1 处
    Friendship {
        keiko_main_bonus: i32,
        camp_extra_person: i32,
    },
    /// 201 得意率 → 得意率 +N；稽古友情 +N%
    Deyilv { deyilv: i32, keiko_friendship: i32 },
    /// 301 ヒント → 灵感率 +N%；稽古灵感 Lv +N
    Hint { hint_rate: i32, hint_level: i32 },
    /// 401 助力 → 非友情训练效果 +N%；稽古获得助力支撑
    Assist {
        training_effect: i32,
        support_assist: bool,
    },
    /// 501 体力 → 稽古体力消耗 -N%
    Vital { keiko_vital_cost_drop: i32 },
}

/// 進捗発表会结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckPointResult {
    /// 未达成（只用于第 1 回之前的基线行）
    #[serde(rename = "none")]
    NotReached,
    /// 成功
    Success,
    /// 大成功（全部演目达标）
    GreatSuccess,
}

/// 進捗発表会 / 本番的永久 buff（`駿大祭ボーナス`）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPointBuff {
    /// 场次：0 = 第 1 回前基线 / 1 = 第 1 回 / 2 = 第 2 回 / 3 = 本番
    pub schedule: i32,
    /// 结果档
    pub result: CheckPointResult,
    /// 友情加成 %
    pub friendship: i32,
    /// 得意率 +N
    pub deyilv: i32,
    /// 灵感发生率 +N%
    pub hint_rate: i32,
    /// 比赛加成 +N%
    pub race_bonus: i32,
}

/// 一次性奖励的结果档
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardResult {
    /// 成功 / 全成功
    Success,
    /// 大成功 / 全大成功
    GreatSuccess,
}

/// 一条一次性奖励（発表会 / 本番 / 育成终了共用）
///
/// 发表会 / 本番的场次由 `schedule` 表示（1–3，对应 `MatsuriData::schedule[schedule-1].turn`）；
/// 育成终了没有场次，`schedule` 省略。粉丝奖励不建模（本项目不处理粉丝机制），数值见
/// `docs/plan/intake.md` §3.6。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reward {
    /// 场次（1–3；育成终了省略）
    #[serde(default)]
    pub schedule: Option<i32>,
    /// 结果档
    pub result: RewardResult,
    /// 五维各 +N
    #[serde(default)]
    pub status: i32,
    /// 技能点 +N
    #[serde(default)]
    pub skill_pt: i32,
    /// 体力 +N
    #[serde(default)]
    pub vital: i32,
    /// 干劲 +N
    #[serde(default)]
    pub motivation: i32,
    /// 全部普通训练 Lv +N（不含演目稽古）
    #[serde(default)]
    pub training_level_up: i32,
    /// 理事长绊 +N
    #[serde(default)]
    pub president_friendship: i32,
    /// 每个演目追加担当人数（第 2 回大成功 +1）
    #[serde(default)]
    pub act_member_add: i32,
    /// 技能灵感 Lv +N
    #[serde(default)]
    pub hints: Vec<HintReward>,
    /// 前几回也大成功时追加的灵感 Lv（仅本番大成功使用）
    #[serde(default)]
    pub hints_all_great: Vec<HintReward>,
}

/// 一次性奖励里的技能灵感
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HintReward {
    /// 技能名（官方日文名）
    pub skill: String,
    /// 灵感 Lv 增量
    pub level: i32,
}

/// 本番演出
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalsActEffect {
    /// 体力 +N
    pub vital: i32,
    /// 干劲 +N
    pub motivation: i32,
    /// 失败率 -N%
    pub fail_rate_drop: i32,
    /// 全部演目稽古追加登场人数（除友人 / 团体外的编成卡各 +1）
    pub extra_member: i32,
}

/// 友人卡出行效果（每段 +1 演目 Lv）
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutingEffect {
    /// 友人支援卡 id（30323 = SSR 吾妻 / 10149 = R 吾妻）
    pub card_id: u32,
    /// 出行段数（1–5）
    pub story_step: i32,
    /// 演目 Lv 增量
    pub add_level: i32,
}

/// 進捗発表会 / 本番回合
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPointSchedule {
    /// 回合（0 基）
    pub turn: i32,
    /// 需要达标的演目数
    pub need_act_num: i32,
    /// 各演目需要的 Lv
    pub need_act_level: i32,
}

/// 合宿窗口与结算点
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampData {
    /// 3 次合宿窗口（0 基、闭区间）
    pub windows: Vec<CampWindow>,
    /// 固定演目 Lv+1 结算回合（= 各窗口 end，不随顺延推迟）
    pub level_up_turns: Vec<i32>,
}

/// 一个合宿窗口
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampWindow {
    /// 起始回合（0 基）
    pub start: i32,
    /// 结束回合（0 基、含）
    pub end: i32,
}

/// 规则常量
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// 演目 gauge 上限
    pub act_gauge_max: i32,
    /// gauge 入账
    pub act_gauge_gain: ActGaugeGain,
    /// 普通训练每 N 次升 1 级
    pub train_level_up_count: i32,
    /// 普通训练等级上限
    pub normal_train_level_max: i32,
    /// 演目 Lv 上限（R / 不带吾妻；本仓离线模拟不支持该组合，见建局友人卡门禁）
    pub act_level_max: i32,
    /// 演目 Lv 上限（SSR 吾妻）
    pub act_level_max_with_friend: i32,
    /// 羁绊基础值
    pub friendship_gain: FriendshipGain,
}

/// gauge 入账规则
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActGaugeGain {
    /// 演目担当每参与 1 人
    pub member: i32,
    /// 闪彩（彩圈）担当
    pub friendship: i32,
    /// 目标比赛（按 N 次 +1 入账）
    pub target_race: i32,
    /// 休息 / 外出 / 非目标比赛
    pub other: i32,
}

/// 羁绊基础值
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FriendshipGain {
    /// 参加友人卡
    pub friend: i32,
    /// 参加普通卡
    pub card: i32,
    /// 不参加
    pub absent: i32,
    /// 絆の教え（平摊）
    pub lesson: i32,
    /// 愛嬌（合计再加，只加一次）
    pub charm: i32,
}

// ---------------------------------------------------------------------------
// 载入与校验
// ---------------------------------------------------------------------------

impl MatsuriData {
    /// 从宿主注入的数据根载入 `scenario_matsuri.json`
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join(MATSURI_DATA_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("读取剧本数据失败: {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("剧本数据来源: {}", path.display()))
    }

    /// 解析并校验剧本数据（测试 / 内嵌场景复用）
    pub fn parse(text: &str) -> Result<Self> {
        let data: Self = serde_json::from_str(text).context("解析剧本数据失败")?;
        data.validate()?;
        Ok(data)
    }

    /// 载入期校验：形状 / 索引一致性 / 等级表单调性
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.scenario_id == SCENARIO_ID,
            "scenario_id 应为 {SCENARIO_ID}，实际 {}",
            self.scenario_id
        );
        self.validate_limits()?;
        self.validate_acts()?;
        self.validate_training()?;
        self.validate_act_effects()?;
        self.validate_advices()?;
        self.validate_fusion()?;
        self.validate_check_points()?;
        self.validate_check_point_rewards()?;
        self.validate_finals_act()?;
        self.validate_outing()?;
        self.validate_schedule()?;
        self.validate_advice_windows()?;
        self.validate_camp()?;
        self.validate_rules()?;
        Ok(())
    }

    /// 属性上限：非负、绝对上限不低于基础上限、编成位数与技能点上限为正
    fn validate_limits(&self) -> Result<()> {
        let lim = &self.limits;
        for (i, (&base, &upper)) in lim
            .five_status_limit_base
            .iter()
            .zip(&lim.five_status_limit_upper)
            .enumerate()
        {
            ensure!(base > 0, "limits.five_status_limit_base[{i}] 应为正");
            ensure!(
                upper >= base,
                "limits.five_status_limit_upper[{i}]={upper} 不应低于基础上限 {base}"
            );
        }
        ensure!(lim.support_num > 0, "limits.support_num 应为正");
        ensure!(lim.max_skill_point > 0, "limits.max_skill_point 应为正");
        Ok(())
    }

    /// 演目：5 条、`act_type` 1–5 连续、命令 5101–5105、训练位与神驹不重复
    fn validate_acts(&self) -> Result<()> {
        ensure!(
            self.acts.len() == ACT_COUNT,
            "acts 应有 {ACT_COUNT} 条，实际 {}",
            self.acts.len()
        );
        let mut npc = Vec::new();
        for (i, act) in self.acts.iter().enumerate() {
            ensure!(
                act.act_type == i as i32 + 1,
                "acts[{i}].act_type 应为 {}，实际 {}",
                i + 1,
                act.act_type
            );
            ensure!(
                act.command_id == 5101 + i as i32,
                "acts[{i}].command_id 应为 {}，实际 {}",
                5101 + i as i32,
                act.command_id
            );
            ensure!(act.train < ACT_COUNT, "acts[{i}].train 越界: {}", act.train);
            ensure!(act.npc_chara_id > 0, "acts[{i}].npc_chara_id 应为正");
            npc.push(act.npc_chara_id);
        }
        npc.sort_unstable();
        ensure!(
            npc.windows(2).all(|w| w[0] != w[1]),
            "acts 的神驹 chara_id 重复"
        );
        Ok(())
    }

    /// 训练表：普通训练 5 训练 × 等级行；稽古两档各 5 行
    fn validate_training(&self) -> Result<()> {
        let normal = &self.training.normal;
        ensure!(
            normal.len() == ACT_COUNT,
            "training.normal 应有 {ACT_COUNT} 个训练，实际 {}",
            normal.len()
        );
        let level_max = self.rules.normal_train_level_max as usize;
        ensure!(level_max > 0, "rules.normal_train_level_max 应为正");
        for (train, levels) in normal.iter().enumerate() {
            ensure!(
                levels.len() == level_max,
                "training.normal[{train}] 应有 {level_max} 个等级，实际 {}",
                levels.len()
            );
            for (level, row) in levels.iter().enumerate() {
                ensure!(
                    row[6] != 0,
                    "training.normal[{train}][{level}] 体力消耗不应为 0"
                );
            }
        }
        let keiko = &self.training.keiko;
        ensure!(
            keiko.junior.len() == ACT_COUNT,
            "training.keiko.junior 应有 {ACT_COUNT} 行，实际 {}",
            keiko.junior.len()
        );
        Ok(())
    }

    /// 演目 Lv 效果：10 级连续、主要数值非降序、全基础恒 +10
    fn validate_act_effects(&self) -> Result<()> {
        ensure!(
            self.act_effects.len() == ACT_LEVEL_MAX,
            "act_effects 应有 {ACT_LEVEL_MAX} 级，实际 {}",
            self.act_effects.len()
        );
        let mut prev = [i32::MIN; 5];
        for (i, eff) in self.act_effects.iter().enumerate() {
            ensure!(
                eff.level == i as i32 + 1,
                "act_effects[{i}].level 应为 {}，实际 {}",
                i + 1,
                eff.level
            );
            ensure!(
                eff.all_status_bonus > 0,
                "act_effects[{i}].all_status_bonus 应为正"
            );
            ensure!(eff.hint_event, "act_effects[{i}] 应发生 Hint 事件");
            let cur = [
                eff.all_status_bonus,
                eff.friendship,
                eff.sp_training,
                eff.sp_limit,
                eff.status_limit,
            ];
            ensure!(
                cur.iter().zip(prev).all(|(a, b)| *a >= b),
                "act_effects[{i}] 的 Lv 效果出现回退（应随 Lv 非降序）"
            );
            prev = cur;
        }
        Ok(())
    }

    /// 教诲：10 种、id 顺序、批次分组、等级表行数与逐级单调性
    fn validate_advices(&self) -> Result<()> {
        ensure!(
            self.advices.len() == ADVICE_COUNT,
            "advices 应有 {ADVICE_COUNT} 种，实际 {}",
            self.advices.len()
        );
        for (i, advice) in self.advices.iter().enumerate() {
            ensure!(
                advice.id == ADVICE_IDS[i],
                "advices[{i}].id 应为 {}，实际 {}",
                ADVICE_IDS[i],
                advice.id
            );
            let (fusion_type, add_schedule_type) = match advice.id {
                101..=501 => (1, 1),
                601 | 701 => (2, 2),
                _ => (2, 3),
            };
            ensure!(
                advice.fusion_type == fusion_type,
                "advices[{i}].fusion_type 应为 {fusion_type}，实际 {}",
                advice.fusion_type
            );
            ensure!(
                advice.add_schedule_type == add_schedule_type,
                "advices[{i}].add_schedule_type 应为 {add_schedule_type}，实际 {}",
                advice.add_schedule_type
            );
            ensure!(
                advice.effect.levels_len() == ACT_LEVEL_MAX,
                "advices[{i}] 的等级表应有 {ACT_LEVEL_MAX} 行，实际 {}",
                advice.effect.levels_len()
            );
            validate_advice_effect(advice)?;
        }
        Ok(())
    }

    /// 転成：5 个基础教诲、id 与效果种类一一对应
    fn validate_fusion(&self) -> Result<()> {
        ensure!(
            self.fusion.len() == ACT_COUNT,
            "fusion 应有 {ACT_COUNT} 条，实际 {}",
            self.fusion.len()
        );
        for (i, fusion) in self.fusion.iter().enumerate() {
            ensure!(
                fusion.advice_id == ADVICE_IDS[i],
                "fusion[{i}].advice_id 应为 {}，实际 {}",
                ADVICE_IDS[i],
                fusion.advice_id
            );
            match (fusion.advice_id, &fusion.effect) {
                (
                    101,
                    FusionEffectKind::Friendship {
                        keiko_main_bonus,
                        camp_extra_person,
                    },
                ) => {
                    ensure!(*keiko_main_bonus > 0, "fusion 101 的稽古主基础应为正");
                    ensure!(*camp_extra_person > 0, "fusion 101 的合宿额外人数应为正");
                }
                (
                    201,
                    FusionEffectKind::Deyilv {
                        deyilv,
                        keiko_friendship,
                    },
                ) => {
                    ensure!(
                        *deyilv > 0 && *keiko_friendship > 0,
                        "fusion 201 的效果应为正"
                    );
                }
                (
                    301,
                    FusionEffectKind::Hint {
                        hint_rate,
                        hint_level,
                    },
                ) => {
                    ensure!(*hint_rate > 0 && *hint_level > 0, "fusion 301 的效果应为正");
                }
                (
                    401,
                    FusionEffectKind::Assist {
                        training_effect,
                        support_assist,
                    },
                ) => {
                    ensure!(*training_effect > 0, "fusion 401 的训练效果应为正");
                    ensure!(*support_assist, "fusion 401 应获得助力支撑");
                }
                (
                    501,
                    FusionEffectKind::Vital {
                        keiko_vital_cost_drop,
                    },
                ) => {
                    ensure!(
                        *keiko_vital_cost_drop > 0,
                        "fusion 501 的体力消耗下降应为正"
                    );
                }
                (id, effect) => bail!("fusion 的 id {id} 与效果种类不匹配: {effect:?}"),
            }
        }
        Ok(())
    }

    /// 発表会永久 buff：7 条、场次 / 结果顺序固定、数值非降序
    fn validate_check_points(&self) -> Result<()> {
        const EXPECT: [(i32, CheckPointResult); 7] = [
            (0, CheckPointResult::NotReached),
            (1, CheckPointResult::Success),
            (1, CheckPointResult::GreatSuccess),
            (2, CheckPointResult::Success),
            (2, CheckPointResult::GreatSuccess),
            (3, CheckPointResult::Success),
            (3, CheckPointResult::GreatSuccess),
        ];
        ensure!(
            self.check_points.len() == EXPECT.len(),
            "check_points 应有 {} 条，实际 {}",
            EXPECT.len(),
            self.check_points.len()
        );
        let mut prev = [i32::MIN; 4];
        for (i, buff) in self.check_points.iter().enumerate() {
            let (schedule, result) = EXPECT[i];
            ensure!(
                buff.schedule == schedule && buff.result == result,
                "check_points[{i}] 应为场次 {schedule} / {result:?}，实际 {} / {:?}",
                buff.schedule,
                buff.result
            );
            let cur = [
                buff.friendship,
                buff.deyilv,
                buff.hint_rate,
                buff.race_bonus,
            ];
            ensure!(
                cur.iter().zip(prev).all(|(a, b)| *a >= b),
                "check_points[{i}] 的 buff 出现回退（应随场次 / 结果非降序）"
            );
            prev = cur;
        }
        Ok(())
    }

    /// 一次性奖励：8 条、阶段 / 结果顺序固定、数值非负、灵感 Lv 为正
    fn validate_check_point_rewards(&self) -> Result<()> {
        // 场次 = 1..3（对应 `schedule` 三条），各成功 / 大成功两档
        const EXPECT: [(i32, RewardResult); 6] = [
            (1, RewardResult::Success),
            (1, RewardResult::GreatSuccess),
            (2, RewardResult::Success),
            (2, RewardResult::GreatSuccess),
            (3, RewardResult::Success),
            (3, RewardResult::GreatSuccess),
        ];
        ensure!(
            self.check_point_rewards.len() == EXPECT.len(),
            "check_point_rewards 应有 {} 条，实际 {}",
            EXPECT.len(),
            self.check_point_rewards.len()
        );
        for (i, (reward, (schedule, result))) in
            self.check_point_rewards.iter().zip(EXPECT).enumerate()
        {
            ensure!(
                reward.schedule == Some(schedule),
                "check_point_rewards[{i}].schedule 应为 {schedule}，实际 {:?}",
                reward.schedule
            );
            ensure!(
                reward.result == result,
                "check_point_rewards[{i}].result 应为 {result:?}，实际 {:?}",
                reward.result
            );
            validate_reward("check_point_rewards", i, reward)?;
        }
        ensure!(
            self.career_end_rewards.len() == 2,
            "career_end_rewards 应有 2 条（全成功 / 全大成功），实际 {}",
            self.career_end_rewards.len()
        );
        for (i, (reward, result)) in self
            .career_end_rewards
            .iter()
            .zip([RewardResult::Success, RewardResult::GreatSuccess])
            .enumerate()
        {
            ensure!(
                reward.schedule.is_none(),
                "career_end_rewards[{i}] 不应带 schedule（育成终了无场次）"
            );
            ensure!(
                reward.result == result,
                "career_end_rewards[{i}].result 应为 {result:?}，实际 {:?}",
                reward.result
            );
            validate_reward("career_end_rewards", i, reward)?;
        }
        Ok(())
    }

    /// 本番演出：失败率下降在 0–100、其余非负
    fn validate_finals_act(&self) -> Result<()> {
        let fin = &self.finals_act;
        ensure!(fin.vital >= 0, "finals_act.vital 不应为负");
        ensure!(fin.motivation >= 0, "finals_act.motivation 不应为负");
        ensure!(
            (0..=100).contains(&fin.fail_rate_drop),
            "finals_act.fail_rate_drop 应在 0–100，实际 {}",
            fin.fail_rate_drop
        );
        ensure!(fin.extra_member >= 0, "finals_act.extra_member 不应为负");
        Ok(())
    }

    /// 出行：2 张友人卡 × 5 段、段数 1–5 连续、每段 +1
    fn validate_outing(&self) -> Result<()> {
        ensure!(
            self.outing.len() == 10,
            "outing 应有 10 行，实际 {}",
            self.outing.len()
        );
        let mut cards = Vec::new();
        for out in &self.outing {
            ensure!(out.add_level > 0, "outing 的 add_level 应为正");
            ensure!(
                (1..=5).contains(&out.story_step),
                "outing.story_step 应为 1–5，实际 {}",
                out.story_step
            );
            if !cards.contains(&out.card_id) {
                cards.push(out.card_id);
            }
        }
        ensure!(
            cards.len() == 2,
            "outing 应覆盖 2 张友人卡，实际 {}",
            cards.len()
        );
        for card in cards {
            let mut steps: Vec<i32> = self
                .outing
                .iter()
                .filter(|out| out.card_id == card)
                .map(|out| out.story_step)
                .collect();
            steps.sort_unstable();
            ensure!(
                steps == (1..=5).collect::<Vec<_>>(),
                "outing 卡 {card} 的出行段数应为 1–5"
            );
        }
        Ok(())
    }

    /// 進捗発表会 / 本番：3 条、回合升序、需要演目数一致
    fn validate_schedule(&self) -> Result<()> {
        ensure!(
            self.schedule.len() == CHECK_POINT_COUNT,
            "schedule 应有 {CHECK_POINT_COUNT} 条，实际 {}",
            self.schedule.len()
        );
        let mut prev_turn = i32::MIN;
        let mut prev_level = i32::MIN;
        for (i, item) in self.schedule.iter().enumerate() {
            ensure!(
                item.turn > prev_turn,
                "schedule[{i}].turn 应严格升序（{} -> {}）",
                prev_turn,
                item.turn
            );
            ensure!(
                item.turn <= crate::MAX_TURN,
                "schedule[{i}].turn 越界: {}",
                item.turn
            );
            ensure!(item.need_act_num > 0, "schedule[{i}].need_act_num 应为正");
            ensure!(
                item.need_act_level >= prev_level,
                "schedule[{i}].need_act_level 应非降序"
            );
            prev_turn = item.turn;
            prev_level = item.need_act_level;
        }
        Ok(())
    }

    /// 教诲窗口：6 个、升序、落在回合范围内
    fn validate_advice_windows(&self) -> Result<()> {
        ensure!(
            self.advice_windows.len() == ADVICE_WINDOW_COUNT,
            "advice_windows 应有 {ADVICE_WINDOW_COUNT} 个，实际 {}",
            self.advice_windows.len()
        );
        let mut prev = i32::MIN;
        for (i, &turn) in self.advice_windows.iter().enumerate() {
            ensure!(
                turn > prev,
                "advice_windows[{i}] 应严格升序（{prev} -> {turn}）"
            );
            ensure!(
                (0..=crate::MAX_TURN).contains(&turn),
                "advice_windows[{i}] 越界: {turn}"
            );
            prev = turn;
        }
        Ok(())
    }

    /// 合宿：3 个窗口升序不重叠、结算回合 = 各窗口结束回合
    fn validate_camp(&self) -> Result<()> {
        let camp = &self.camp;
        ensure!(
            camp.windows.len() == CAMP_COUNT,
            "camp.windows 应有 {CAMP_COUNT} 个，实际 {}",
            camp.windows.len()
        );
        ensure!(
            camp.level_up_turns.len() == CAMP_COUNT,
            "camp.level_up_turns 应有 {CAMP_COUNT} 个，实际 {}",
            camp.level_up_turns.len()
        );
        let mut prev_end = i32::MIN;
        for (i, window) in camp.windows.iter().enumerate() {
            ensure!(
                window.start > prev_end,
                "camp.windows[{i}] 应从上一个窗口之后开始（{prev_end} -> {}）",
                window.start
            );
            ensure!(
                window.end >= window.start,
                "camp.windows[{i}].end 不应早于 start"
            );
            ensure!(
                camp.level_up_turns[i] == window.end,
                "camp.level_up_turns[{i}] 应等于窗口结束回合 {}，实际 {}",
                window.end,
                camp.level_up_turns[i]
            );
            prev_end = window.end;
        }
        Ok(())
    }

    /// 规则常量：上限 / 入账 / 训练 Lv / 羁绊基础值均为正或非负
    fn validate_rules(&self) -> Result<()> {
        let rules = &self.rules;
        ensure!(rules.act_gauge_max > 0, "rules.act_gauge_max 应为正");
        let gain = &rules.act_gauge_gain;
        for (name, value) in [
            ("member", gain.member),
            ("friendship", gain.friendship),
            ("target_race", gain.target_race),
            ("other", gain.other),
        ] {
            ensure!(value > 0, "rules.act_gauge_gain.{name} 应为正");
        }
        ensure!(
            rules.train_level_up_count > 0,
            "rules.train_level_up_count 应为正"
        );
        ensure!(
            rules.normal_train_level_max > 0,
            "rules.normal_train_level_max 应为正"
        );
        ensure!(
            rules.act_level_max <= rules.act_level_max_with_friend,
            "rules.act_level_max 不应高于带友人卡的上限"
        );
        let friendship = &rules.friendship_gain;
        for (name, value) in [
            ("friend", friendship.friend),
            ("card", friendship.card),
            ("absent", friendship.absent),
            ("lesson", friendship.lesson),
            ("charm", friendship.charm),
        ] {
            ensure!(value >= 0, "rules.friendship_gain.{name} 不应为负");
        }
        Ok(())
    }
}

/// 一条一次性奖励的通用数值校验（列表名 + 下标用于报错）
fn validate_reward(list: &str, index: usize, reward: &Reward) -> Result<()> {
    for (name, value) in [
        ("status", reward.status),
        ("skill_pt", reward.skill_pt),
        ("vital", reward.vital),
        ("motivation", reward.motivation),
        ("training_level_up", reward.training_level_up),
        ("president_friendship", reward.president_friendship),
        ("act_member_add", reward.act_member_add),
    ] {
        ensure!(value >= 0, "{list}[{index}].{name} 不应为负: {value}");
    }
    for hint in reward.hints.iter().chain(&reward.hints_all_great) {
        ensure!(!hint.skill.is_empty(), "{list}[{index}] 的技能名不应为空");
        ensure!(
            hint.level > 0,
            "{list}[{index}] 的灵感 Lv 应为正: {}",
            hint.level
        );
    }
    Ok(())
}

/// 按教诲种类校验逐级数值（种类与 id 已在 `validate_advices` 对齐）
fn validate_advice_effect(advice: &Advice) -> Result<()> {
    let id = advice.id;
    match (id, &advice.effect) {
        (101, AdviceEffect::Friendship { levels }) => {
            ensure!(
                levels.iter().all(|level| level.gauge == 3),
                "advice 101 的绊 +3 应平摊恒为 3"
            );
            ensure_ascending(
                "advice 101 的 keiko_main_bonus",
                levels.iter().map(|l| l.keiko_main_bonus),
            )?;
        }
        (201, AdviceEffect::Deyilv { levels }) => {
            ensure_ascending("advice 201 的 deyilv", levels.iter().map(|l| l.deyilv))?;
        }
        (301, AdviceEffect::Hint { levels }) => {
            ensure_ascending(
                "advice 301 的 hint_rate",
                levels.iter().map(|l| l.hint_rate),
            )?;
        }
        (401, AdviceEffect::Assist { levels }) => {
            ensure_ascending(
                "advice 401 的 training_effect",
                levels.iter().map(|l| l.training_effect),
            )?;
            ensure!(
                levels.iter().all(|level| level.support_assist),
                "advice 401 每级都应获得助力支撑"
            );
        }
        (501, AdviceEffect::Vital { levels }) => {
            ensure_ascending(
                "advice 501 的 keiko_vital_cost_drop",
                levels.iter().map(|l| l.keiko_vital_cost_drop),
            )?;
        }
        (601, AdviceEffect::Special { levels }) => {
            for (i, level) in levels.iter().enumerate() {
                ensure!(
                    level.main_gain == level.status_limit,
                    "advice 601 的 Lv{} 主基础获得量与上限应同值",
                    i + 1
                );
            }
            ensure_ascending(
                "advice 601 的 main_gain",
                levels.iter().map(|l| l.main_gain),
            )?;
        }
        (701, AdviceEffect::Kensan { levels }) => {
            ensure_ascending("advice 701 的 rate", levels.iter().map(|l| l.rate))?;
        }
        (801, AdviceEffect::Cooperate { levels }) => {
            ensure_ascending(
                "advice 801 的 friendship",
                levels.iter().map(|l| l.friendship),
            )?;
            ensure!(
                levels.iter().all(|level| level.split > 0),
                "advice 801 每级都应给该演目担当额外登场"
            );
        }
        (901, AdviceEffect::Unyielding { levels }) => {
            ensure!(
                levels.iter().all(|level| level.fail_vital_threshold >= 0),
                "advice 901 的体力阈值不应为负"
            );
            for pair in levels.windows(2) {
                ensure!(
                    pair[0].fail_vital_threshold > pair[1].fail_vital_threshold,
                    "advice 901 的体力阈值应随 Lv 递减"
                );
            }
            ensure_ascending(
                "advice 901 的 keiko_training_effect",
                levels.iter().map(|l| l.keiko_training_effect),
            )?;
        }
        (1001, AdviceEffect::Dual { levels }) => {
            ensure_ascending("advice 1001 的 sub_gain", levels.iter().map(|l| l.sub_gain))?;
            ensure_ascending(
                "advice 1001 的 keiko_training_effect",
                levels.iter().map(|l| l.keiko_training_effect),
            )?;
        }
        (id, effect) => bail!("advices 的 id {id} 与效果种类不匹配: {effect:?}"),
    }
    Ok(())
}

/// 数值序列非降序校验
fn ensure_ascending(name: &str, values: impl Iterator<Item = i32>) -> Result<()> {
    let mut prev = i32::MIN;
    for value in values {
        ensure!(value >= prev, "{name} 应非降序（{prev} -> {value}）");
        prev = value;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 内联单测
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 仓内资产文本（测试 cwd 在 crate 目录，数据根经 testing 取仓根）
    fn repo_text() -> String {
        std::fs::read_to_string(crate::testing::data_dir().join(MATSURI_DATA_FILE))
            .expect("仓内剧本资产应存在")
    }

    /// 把仓内资产解析成 JSON 值，交给调用方改坏后再走一遍完整解析 + 校验
    fn parse_mutated(mutate: impl FnOnce(&mut serde_json::Value)) -> Result<MatsuriData> {
        let mut value: serde_json::Value =
            serde_json::from_str(&repo_text()).expect("资产应是合法 JSON");
        mutate(&mut value);
        MatsuriData::parse(&value.to_string())
    }

    #[test]
    fn parse_repo_data_ok() {
        let data = MatsuriData::load(crate::testing::data_dir()).expect("仓内资产应可载入");
        assert_eq!(data.scenario_id, 15);
        assert_eq!(data.acts.len(), 5);
        assert_eq!(data.training.normal.len(), 5);
        assert_eq!(data.training.keiko.junior.len(), 5);
        assert_eq!(data.act_effects.len(), 10);
        assert_eq!(data.advices.len(), 10);
        assert_eq!(data.fusion.len(), 5);
        assert_eq!(data.check_points.len(), 7);
        assert_eq!(data.check_point_rewards.len(), 6);
        assert_eq!(data.career_end_rewards.len(), 2);
        assert_eq!(data.outing.len(), 10);
        assert_eq!(data.schedule.len(), 3);
        assert_eq!(data.advice_windows.len(), 6);
    }

    #[test]
    fn unknown_field_rejected() {
        let bad = repo_text().replace("\"scenario_id\"", "\"scenario_id_typo\"");
        assert!(MatsuriData::parse(&bad).is_err(), "未知字段应报错");
    }

    #[test]
    fn bad_row_width_rejected() {
        let bad = repo_text().replacen("[12, 0, 2, 0, 0, 7, -20]", "[12, 0, 2, 0, 0, 7]", 1);
        assert!(MatsuriData::parse(&bad).is_err(), "六值行应报错");
    }

    #[test]
    fn wrong_act_count_rejected() {
        let result = parse_mutated(|value| {
            value["acts"].as_array_mut().unwrap().pop();
        });
        assert!(result.is_err(), "缺少演目应报错");
    }

    #[test]
    fn act_effects_regression_rejected() {
        let result = parse_mutated(|value| {
            value["act_effects"][5]["friendship"] = serde_json::json!(1);
        });
        assert!(result.is_err(), "演目 Lv 效果回退应报错");
    }

    #[test]
    fn advice_level_count_rejected() {
        let result = parse_mutated(|value| {
            value["advices"][0]["effect"]["levels"]
                .as_array_mut()
                .unwrap()
                .pop();
        });
        assert!(result.is_err(), "教诲等级行不足应报错");
    }

    #[test]
    fn advice_kind_mismatch_rejected() {
        let result = parse_mutated(|value| {
            value["advices"][0]["id"] = serde_json::json!(201);
        });
        assert!(result.is_err(), "教诲 id 与效果种类不匹配应报错");
    }

    #[test]
    fn fusion_kind_mismatch_rejected() {
        let result = parse_mutated(|value| {
            value["fusion"][0]["advice_id"] = serde_json::json!(201);
        });
        assert!(result.is_err(), "転成 id 与效果种类不匹配应报错");
    }

    #[test]
    fn check_point_buff_regression_rejected() {
        let result = parse_mutated(|value| {
            value["check_points"][3]["friendship"] = serde_json::json!(99);
        });
        assert!(result.is_err(), "発表会 buff 回退应报错");
    }

    #[test]
    fn reward_schedule_order_rejected() {
        let result = parse_mutated(|value| {
            value["check_point_rewards"][0]["schedule"] = serde_json::json!(2);
        });
        assert!(result.is_err(), "一次性奖励场次顺序错误应报错");
    }

    #[test]
    fn career_end_reward_with_schedule_rejected() {
        let result = parse_mutated(|value| {
            value["career_end_rewards"][0]["schedule"] = serde_json::json!(3);
        });
        assert!(result.is_err(), "育成终了奖励不应带场次");
    }

    #[test]
    fn keiko_classic_disabled_rejected() {
        let result = parse_mutated(|value| {
            value["training"]["keiko"]["classic"] = serde_json::json!([[21, 0, 3, 0, 0, 10, -25]]);
        });
        assert!(result.is_err(), "Classic 稽古档当前下架，未知字段应报错");
    }

    #[test]
    fn schedule_order_rejected() {
        let result = parse_mutated(|value| {
            value["schedule"][0]["turn"] = serde_json::json!(67);
        });
        assert!(result.is_err(), "赛程回合乱序应报错");
    }

    #[test]
    fn camp_settlement_point_rejected() {
        let result = parse_mutated(|value| {
            value["camp"]["level_up_turns"][1] = serde_json::json!(40);
        });
        assert!(result.is_err(), "合宿结算点与窗口不一致应报错");
    }

    #[test]
    fn limits_order_rejected() {
        let result = parse_mutated(|value| {
            value["limits"]["five_status_limit_upper"][0] = serde_json::json!(1);
        });
        assert!(result.is_err(), "绝对上限低于基础上限应报错");
    }
}
