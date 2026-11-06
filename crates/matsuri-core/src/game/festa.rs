//! 骏大祭帧的剧本段（`thisTurn.json::festa`）：协议 DTO + 逐字段判空
//!
//! 定义放 core 而不放 `matsuri-ai`：核心的还原层（[`super::restore`]）与反向导出都要用它，
//! ai 侧只做信封解析。**结构照抄协议**（`camelCase`、不加 `deny_unknown_fields`——
//! 插件新增字段不得让整帧解析失败）。
//!
//! ## 「未知」与「零值」要分开
//!
//! 协议里有三类字段不能按 0 处理（`unwrap_or(0)` 会把「本帧没给」当成「真值 0」）：
//!
//! | 字段 | 0 / 空的含义 |
//! |---|---|
//! | `campRemain` | **0 = 本次合宿已结束**（有效值）；缺字段 = 未知 |
//! | `assistSupportLeft` | 0 = 没有助力支撑（有效值）；缺字段 = 未知 |
//! | `checkPoints` | 空 = 尚未结算过発表会（有效值）；缺字段 = 未知 |
//! | `trainLevelTotal` | **整组 0 = 本帧无有效等级**（合宿期 `command` 帧、`load/event/special`
//!   帧都是整组 0，见 `docs/plan/online_wiring.md` §3.6 / `G12`）；缺字段 = 未知 |
//! | `kensan` | `[false; 5]` 是有效值（本回合没掷中） |
//!
//! 前三个用 `Option` 区分「缺字段」；`trainLevelTotal` 既用 `Option` 也用「逐位 > 0」判空
//! （见 [`FestaState::train_level`]）。
//!
//! ## 与状态字段的口径差
//!
//! `acts[].needGauge` / `acts[].outingGainLevel` / `maxActLevel` **不进局面状态**：
//! 三者在本仓都由静态资产决定（`rules.act_gauge_max` / 融合表 / `rules.act_level_max_with_friend`），
//! 帧值只作 `--verify-training` 的一致性校验。理由见方案 §2 #7 与实施记录。

use serde::Deserialize;
use umaai_core::Array5;

/// 演目数量（`actType` 1–5 ↔ 演目 / 训练位下标 0–4）
pub const ACT_COUNT: usize = 5;

/// 剧本段（`festa`，9 个字段）
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FestaState {
    /// 5 条演目（协议按 `act_type` 升序给出，但**不保证**，按下标读会错位）
    #[serde(default)]
    pub acts: Vec<FestaAct>,
    /// 演目 Lv 上限（真机 10，与 `rules.act_level_max_with_friend` 同源）
    #[serde(default)]
    pub max_act_level: i32,
    /// 合宿剩余回合（服务端**存储值**；0 = 本次合宿已结束，`None` = 帧未给）
    #[serde(default)]
    pub camp_remain: Option<i32>,
    /// 助力支撑有无（状态 47；`1` = 有，按 `G4` 口径折算成局面 1 回合）
    #[serde(default)]
    pub assist_support_left: Option<i32>,
    /// 疲劳回复 flag（状态 45 有无）
    #[serde(default)]
    pub fatigue_recovery: bool,
    /// 進捗発表会 / 本番结果（`scheduleId` **1 基**，数组顺序不保证；`None` = 帧未给）
    #[serde(default)]
    pub check_points: Option<Vec<FestaCheckPoint>>,
    /// 各训练位等级（含発表会 +1；整组 0 = 本帧无有效值，`None` = 帧未给）
    #[serde(default)]
    pub train_level_total: Option<Array5>,
    /// 各训练位「追加の稽古」掷中标记（引擎侧只取 OR）
    #[serde(default)]
    pub kensan: Option<[bool; ACT_COUNT]>,
    /// 人头（**只含 `actType > 0` 的成员**：卡 / 编成外神驹；友人 / 理事长 / 记者不在内）
    #[serde(default)]
    pub persons: Vec<FestaPerson>,
}

/// 一条演目（`FestaFrame.acts[]`）
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FestaAct {
    /// 演目号（1–5；对应演目 / 训练位下标 `actType - 1`）
    #[serde(default)]
    pub act_type: i32,
    /// 当前演目槽
    #[serde(default)]
    pub gauge: i32,
    /// 演目 Lv（**1 基**；0 = 帧未给）
    #[serde(default)]
    pub level: i32,
    /// 升级所需槽（真机恒 5；不进状态，只作校验）
    #[serde(default)]
    pub need_gauge: i32,
    /// 当前教诲 id（未転成 101–501；転成后 601–1001）
    #[serde(default)]
    pub advice_id: i32,
    /// 転成来源：`0` = 未転成；非 0 = 指回原基础教诲 101–501
    #[serde(default)]
    pub fusion_id: i32,
    /// 友人出行演目 Lv 增益（不进状态，只作校验）
    #[serde(default)]
    pub outing_gain_level: i32,
}

/// 一场発表会 / 本番的结果
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FestaCheckPoint {
    /// 场次（**1 基**：1 = 第 1 回 / 2 = 第 2 回 / 3 = 本番）
    #[serde(default)]
    pub schedule_id: i32,
    /// 结果档（真机 `1` = 成功 / `2` = 大成功；`0` 的含义待真机采样，见 `G3`）
    #[serde(default)]
    pub result_type: i32,
}

/// 一个人头（`festa.persons[]`）
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FestaPerson {
    /// 协议人头 id：`1–6` = 卡位（`targetId - 1` 即卡组下标）；`>= 1000` = 编成外神驹的 `chara_id`
    #[serde(default)]
    pub target_id: i32,
    /// 担当演目（1–5；0 / 缺失 = 不参与担当，仅占人数）
    #[serde(default)]
    pub act_type: i32,
    /// 来源（`card` / `friend` / `guest` / `yayoi` / `reporter`）
    #[serde(default)]
    pub origin: String,
}

impl FestaState {
    /// 按 `actType` 取演目（1–5 → 0–4）；越界 / 缺失返回 `None`
    #[must_use]
    pub fn act(&self, index: usize) -> Option<&FestaAct> {
        self.acts
            .iter()
            .find(|act| act.act_type == index as i32 + 1)
    }

    /// 某训练位的协议等级（`tlt[i] > 0` 才算有效值；整组 0 / 缺字段 → `None`）
    #[must_use]
    pub fn train_level(&self, train: usize) -> Option<i32> {
        let total = self.train_level_total?;
        let level = *total.get(train)?;
        (level > 0).then_some(level)
    }

    /// 本回合是否掷中「追加の稽古」（逐位 OR；缺字段 = 否）
    #[must_use]
    pub fn kensan_any(&self) -> bool {
        self.kensan
            .is_some_and(|kensan| kensan.iter().any(|hit| *hit))
    }

    /// 某场発表会的结果档（`schedule_id` 1 基）；未结算 / 未给 → `None`
    #[must_use]
    pub fn check_point_result(&self, schedule_id: i32) -> Option<i32> {
        self.check_points
            .as_ref()?
            .iter()
            .find(|point| point.schedule_id == schedule_id)
            .map(|point| point.result_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 层1：真机片段照原样解析（字段名 / 类型 / 判空口径）
    #[test]
    fn parse_machine_frame() {
        let raw = r#"{
            "acts": [
              {"actType":1,"gauge":5,"level":6,"needGauge":5,"adviceId":601,"fusionId":101,"outingGainLevel":0},
              {"actType":2,"gauge":2,"level":6,"needGauge":5,"adviceId":501,"fusionId":0,"outingGainLevel":0}
            ],
            "maxActLevel": 10,
            "campRemain": 1,
            "assistSupportLeft": 1,
            "fatigueRecovery": false,
            "checkPoints": [{"scheduleId":1,"resultType":2}],
            "trainLevelTotal": [3,2,2,2,3],
            "kensan": [false,false,false,false,true],
            "persons": [
              {"targetId":1,"actType":1,"origin":"card"},
              {"targetId":1009,"actType":1,"origin":"guest"}
            ]
        }"#;
        let festa: FestaState = serde_json::from_str(raw).expect("真机片段应可解析");
        assert_eq!(festa.max_act_level, 10);
        assert_eq!(festa.camp_remain, Some(1));
        assert_eq!(festa.assist_support_left, Some(1));
        assert_eq!(festa.check_point_result(1), Some(2));
        assert_eq!(festa.train_level(0), Some(3));
        assert!(festa.kensan_any());
        assert_eq!(festa.persons.len(), 2);
        assert_eq!(festa.act(0).map(|act| act.advice_id), Some(601));
        assert_eq!(festa.act(1).map(|act| act.gauge), Some(2));
        assert_eq!(festa.act(5), None, "越界演目取不到");
    }

    /// 层1：未知字段不得让整帧失败（插件可以加字段）
    #[test]
    fn tolerate_unknown_fields() {
        let raw = r#"{"acts":[],"campRemain":0,"futureField":{"a":1}}"#;
        let festa: FestaState = serde_json::from_str(raw).expect("未知键应被忽略");
        assert_eq!(festa.camp_remain, Some(0), "0 是有效值（合宿已结束）");
        assert_eq!(festa.assist_support_left, None, "缺字段 ≠ 0");
        assert_eq!(festa.check_points, None, "缺字段 ≠ 空数组");
        assert_eq!(festa.train_level(0), None, "缺字段无等级");
    }

    /// 层1：`trainLevelTotal` 整组 0 视为无有效值（合宿期 command 帧 / 非 command 帧）
    #[test]
    fn zero_train_level_is_unknown() {
        let raw = r#"{"trainLevelTotal":[0,0,0,0,0],"kensan":[true,false,false,false,false]}"#;
        let festa: FestaState = serde_json::from_str(raw).expect("解析");
        for train in 0..ACT_COUNT {
            assert_eq!(festa.train_level(train), None, "0 = 本帧无有效等级");
        }
        assert!(festa.kensan_any(), "kensan 只用于标注，不参与等级判定");
    }
}
