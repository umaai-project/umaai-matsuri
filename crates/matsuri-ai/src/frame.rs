//! 帧解析：`thisTurn.json` 的剧本段 + 通用段 + 训练预览真值
//!
//! 分工严格：**通用段**交引擎 [`parse_base_status`]（它容忍未知键、字段漂移在解析处炸），
//! 本模块只做三件事：
//!
//! 1. 取出 `festa` 段（[`FestaState`]）——缺段 = 不是本剧本的帧，**不退化成默认状态**；
//! 2. 校验 `baseGame.scenarioId`（骏大祭 = 15）；
//! 3. 顺手把 `baseGame.training`（训练预览真值）读成 `map<指令 id, {failureRate, lower, upper}>`，
//!    供 `--verify-training` 验算（`G13`：**只验算，不进还原 / 不进决策**）。

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result, anyhow, bail};
use matsuri_core::game::festa::FestaState;
use serde::Deserialize;
use umaai::BaseStatus;
use umaai::parse_base_status;

/// 骏大祭剧本 ID
pub const MATSURI_SCENARIO_ID: u32 = 15;

/// 帧信封（只读本模块关心的两段；其余交引擎）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    /// 剧本段（缺 = 不是本剧本的帧）
    #[serde(default)]
    festa: Option<FestaState>,
    /// 通用段（只取 `training`，其余字段引擎已解析过）
    #[serde(default)]
    base_game: Option<BaseGameExtra>,
}

/// 通用段里本模块额外需要的字段
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaseGameExtra {
    /// 训练预览协议真值（仅骏大祭填充；`{command_id: {failureRate, lower, upper}}`）
    #[serde(default)]
    training: Option<BTreeMap<String, TrainingFrame>>,
    /// 生病病种（游戏 `chara_effect_id` 原值，1–6，142 表口径）；旧插件不给 → 空
    #[serde(default)]
    ill_effects: Vec<i32>,
    /// 待选事件的 `story_id`（`evt.story_id` 原值）；单选项 / 无事件帧不给
    ///
    /// 插件只透传 id：效果表命中时另有 `story`，**未收录时只有本字段**，
    /// 事件与选项数据由接收端按自己的表还原。
    #[serde(default)]
    story_id: Option<i32>,
}

/// 一条训练预览真值
///
/// 两段都是**7 项定长**：速 / 耐 / 力 / 根 / 智 / 技能点 / 体力（负数为消耗）；
/// `lower` 取游戏下层、`upper` 取剧本上层，逐项相加才是总增量，且**未做 1200 衰减**。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingFrame {
    /// 失败率（协议原值）
    #[serde(default)]
    pub failure_rate: f32,
    /// 下层增量
    #[serde(default)]
    pub lower: Vec<i32>,
    /// 上层增量
    #[serde(default)]
    pub upper: Vec<i32>,
}

impl TrainingFrame {
    /// 两层逐项相加（长度不等时按较短者截断，`preview` 侧恒 7 项）
    #[must_use]
    pub fn total(&self) -> Vec<i32> {
        self.lower
            .iter()
            .zip(self.upper.iter())
            .map(|(low, up)| low + up)
            .collect()
    }
}

/// 解析好的一帧
#[derive(Debug, Clone)]
pub struct ParsedFrame {
    /// 通用段（引擎口径）
    pub status: BaseStatus,
    /// 剧本段
    pub festa: FestaState,
    /// 训练预览真值（`G13` 只验算用；决策链路不读）
    pub training: BTreeMap<String, TrainingFrame>,
    /// 帧原文摘要（`G7` 幂等：同内容不重复决策）
    pub digest: u64,
    /// 生病病种（游戏 `chara_effect_id` 原值，1–6，142 表口径）；空 = 帧未给（沿用 `isIll` 布尔）
    pub ill_effects: Vec<i32>,
    /// 待选事件的 `story_id`（插件只发 id，选项数据由接收端自持）；无事件帧为 `None`
    pub story_id: Option<i32>,
}

impl ParsedFrame {
    /// 解析帧原文
    ///
    /// 两次解析同一份 JSON（引擎通用段一次、本模块一次）：帧不大，换来「引擎解析规则
    /// 只有一份」——本模块不复制 `baseGame` 的 46 个字段。
    pub fn parse(contents: &str) -> Result<Self> {
        let status = parse_base_status(contents).context("帧解析失败")?;
        if status.scenario_id != MATSURI_SCENARIO_ID {
            bail!(
                "不是骏大祭的帧：scenarioId={}（本剧本只接受 {MATSURI_SCENARIO_ID}）",
                status.scenario_id
            );
        }
        let envelope: Envelope = serde_json::from_str(contents).context("帧解析失败（信封）")?;
        let festa = envelope
            .festa
            .ok_or_else(|| anyhow!("帧内没有 `festa` 段：不是骏大祭的帧（拒绝计算）"))?;
        let extra = envelope.base_game.unwrap_or_default();
        let training = extra.training.unwrap_or_default();
        Ok(Self {
            status,
            festa,
            training,
            digest: digest_of(contents),
            ill_effects: extra.ill_effects,
            story_id: extra.story_id,
        })
    }

    /// 育成局号（缺 `single_mode_chara_id` 时退化到马娘 ID）
    #[must_use]
    pub fn epoch_key(&self) -> u64 {
        self.status.epoch_key()
    }

    /// 是否「动作已执行」的帧（还原到 `AfterTrain`，不是 `Train`）
    #[must_use]
    pub fn is_after_training(&self) -> bool {
        self.status.is_after_training
    }

    /// 帧来源（`load / command / event / special`）
    #[must_use]
    pub fn source(&self) -> &str {
        self.status.source.as_deref().unwrap_or("")
    }
}

/// 内容摘要（同一份原文 → 同一摘要；`G7` 幂等键之一）
fn digest_of(contents: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    contents.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: &str = r#"{
        "festa": {
          "acts": [{"actType":1,"gauge":5,"level":6,"needGauge":5,"adviceId":601,"fusionId":101,"outingGainLevel":0}],
          "maxActLevel": 10, "campRemain": 1, "assistSupportLeft": 1, "fatigueRecovery": false,
          "checkPoints": [{"scheduleId":1,"resultType":2}],
          "trainLevelTotal": [3,2,2,2,3], "kensan": [false,false,false,false,true],
          "persons": [{"targetId":1,"actType":1,"origin":"card"}]
        },
        "baseGame": {
          "scenarioId": 15, "umaId": 105902, "turn": 40, "vital": 77, "maxVital": 108,
          "motivation": 5,
          "fiveStatus":[1382,739,902,733,727], "fiveStatusLimit":[3288,2504,2516,2238,2414],
          "skillPt": 300, "skillScore": 0, "totalHints": 20, "trainLevelCount":[3,1,0,1,2],
          "ptScoreRate": 1.0, "failureRateBias": 0, "isIll": false, "isQieZhe": false,
          "isAiJiao": false, "zhongMaBlueCount":[0,0,0,0,0], "isRacing": false,
          "cardId": [303233,302424,302831,303044,302894,303114],
          "persons": [{"personType":1,"charaId":9052,"friendship":40},
                      {"personType":2,"charaId":1129,"friendship":50},
                      {"personType":2,"charaId":1022,"friendship":30},
                      {"personType":2,"charaId":1058,"friendship":20},
                      {"personType":2,"charaId":1138,"friendship":35},
                      {"personType":2,"charaId":1146,"friendship":10}],
          "personDistribution": [[0,1,5,1009,-1],[3,1096,-1,-1,-1],[-1,-1,-1,-1,-1],
                                 [-1,-1,-1,-1,-1],[-1,-1,-1,-1,-1]],
          "lockedTrainingId": -1, "friendship_noncard_yayoi": 30,
          "friendship_noncard_reporter": 20, "friend_stage": 2, "friend_outgoingUsed": 1,
          "playing_state": 1, "source": "command", "isAfterTraining": false,
          "single_mode_chara_id": 6310, "islegal": true,
          "training": {"5101": {"failureRate": 0, "lower": [50,0,22,0,0,66,-19],
                                "upper": [57,10,13,10,10,55,0]}}
        }
    }"#;

    /// 层1：真机片段可解析；剧本段 / 通用段 / 训练真值都到位
    #[test]
    fn parse_machine_frame() {
        let frame = ParsedFrame::parse(FRAME).expect("真机片段应可解析");
        assert_eq!(frame.status.turn, 40);
        assert_eq!(frame.epoch_key(), 6310);
        assert_eq!(frame.source(), "command");
        assert!(!frame.is_after_training());
        assert_eq!(frame.festa.camp_remain, Some(1));
        let training = frame.training.get("5101").expect("应带训练真值");
        assert_eq!(training.total(), vec![107, 10, 35, 10, 10, 121, -19]);
    }

    /// 层1：事件帧只有 `storyId`、没有 `story`（插件未收录效果表）也能解析
    #[test]
    fn parse_story_id_without_story() {
        // 只换 source（帧里已有 source，重复键 serde 会报错）+ 追加 storyId
        let frame = ParsedFrame::parse(&FRAME.replace(
            "\"source\": \"command\"",
            "\"source\": \"event\", \"storyId\": 400015106",
        ))
        .expect("事件帧应可解析");
        assert_eq!(frame.story_id, Some(400015106));
        assert_eq!(frame.source(), "event");
        assert!(frame.status.story.is_none(), "未收录事件没有效果数据");
    }

    /// 层1：帧带 `illEffects` 时按病种解析（引擎通用段容忍该新增键）
    #[test]
    fn parse_ill_effects() {
        let frame = ParsedFrame::parse(&FRAME.replace(
            "\"isIll\": false",
            "\"isIll\": true, \"illEffects\": [1, 3]",
        ))
        .expect("带病种的帧应可解析");
        assert_eq!(frame.ill_effects, vec![1, 3]);
    }

    /// 层1：非本剧本（scenarioId 不符 / 无 festa 段）→ 明确报错，不退化成默认状态
    #[test]
    fn reject_other_scenario() {
        let other = FRAME.replace("\"scenarioId\": 15", "\"scenarioId\": 14");
        let err = ParsedFrame::parse(&other).expect_err("别的剧本应拒绝");
        assert!(
            format!("{err:#}").contains("scenarioId"),
            "报错要带 scenarioId"
        );

        // 通用段合法、但删掉 `festa` 段
        let mut value: serde_json::Value = serde_json::from_str(FRAME).expect("测试帧是合法 JSON");
        value.as_object_mut().expect("顶层是对象").remove("festa");
        let no_festa = serde_json::to_string(&value).expect("序列化");
        let err = ParsedFrame::parse(&no_festa).expect_err("缺 festa 段应拒绝");
        assert!(
            format!("{err:#}").contains("festa"),
            "报错要指明缺 festa 段"
        );
    }

    /// 层1：同内容 → 同摘要；改一个字节 → 摘要变化（G7 幂等键）
    #[test]
    fn digest_is_content_based() {
        let a = ParsedFrame::parse(FRAME).expect("解析");
        let b = ParsedFrame::parse(FRAME).expect("解析");
        assert_eq!(a.digest, b.digest);
        let changed = FRAME.replace("\"turn\": 40", "\"turn\": 41");
        let c = ParsedFrame::parse(&changed).expect("解析");
        assert_ne!(a.digest, c.digest);
    }
}
