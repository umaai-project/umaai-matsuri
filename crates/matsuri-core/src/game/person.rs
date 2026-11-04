//! 剧本人头（`MatsuriPerson`）：引擎通用人头 + 剧本自持信息（担当 / 来源 / 可用回合）
//!
//! **`PersonOrigin` 与引擎 `PersonType` 的关系**：`PersonType` 是引擎契约的语义分类（缺省可用性 /
//! 不在率 / 分布顺序 / 红点 / 闪彩 / 羁绊点击等由它决定，剧本不能扩展）；`PersonOrigin` 是剧本侧
//! 标签（来源 / 是否参与担当 / 加入条件），构造时**单向决定** `PersonType`：
//!
//! | PersonOrigin | PersonType | 说明 |
//! |---|---|---|
//! | `Card` | `Card` | 编成普通支援卡（含 link 卡）：参与担当分配 |
//! | `Friend` | `ScenarioCard` | 剧本友人（吾妻）：不参与担当、不占训练位卡配额 |
//! | `Guest` | `Npc` | 编成外神驹（两批共用；加入时点 / 条件由 `avail_turn` / `needs_great_success` 区分） |
//! | `Yayoi` | `Yayoi` | 理事长：按引擎基础局默认处理（沿用引擎术语） |
//! | `Reporter` | `Reporter` | 记者：按引擎基础局默认处理 |
//!
//! 运行期引擎侧默认逻辑只读 `PersonType`；剧本侧逻辑只看 `PersonOrigin`（不用 `PersonType` 反推）。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use umaai_core::Data;
use umasim::game::base::BasePerson;
use umasim::game::{PersonType, SupportCard};

/// 人头来源（剧本侧标签；与引擎 `PersonType` 的映射见文件头）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonOrigin {
    /// 编成普通支援卡（含 link 卡）：参与担当分配
    #[default]
    Card,
    /// 剧本友人（吾妻）：不参与担当、不占训练位卡配额
    Friend,
    /// 编成外神驹（两批共用；批次靠可用回合 / 大成功条件区分）
    Guest,
    /// 理事长（沿用引擎术语 `Yayoi`；按引擎基础局默认处理）
    Yayoi,
    /// 记者（按引擎基础局默认处理）
    Reporter,
}

/// 剧本人头：引擎通用人头 + 剧本自持字段
///
/// 契约 `Person` 的 impl 在 [`traits`](super::traits)（契约 trait impl 不拆文件）；
/// 本文件只放结构、构造与 inherent 查询。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatsuriPerson {
    /// 引擎通用人头（类型 / 得意 / 羁绊 / 红点 / card_id）；契约 impl 逐项委托
    pub(crate) base: BasePerson,
    /// 担当演目下标（0–4；卡 / 编成外神驹），不参与担当（吾妻 / 理事长 / 记者）为 `None`
    pub act: Option<usize>,
    /// 来源 / 剧本标签
    pub origin: PersonOrigin,
    /// 可用起始回合（卡 0 / 吾妻 2 / 第 1 批神驹 2 / 第 2 批 48 / 理事长与记者按引擎默认）
    pub avail_turn: i32,
    /// 是否要求 Classic 進捗発表会（`check_points[1]`）大成功才可用（第 2 批神驹）
    pub needs_great_success: bool,
}

impl MatsuriPerson {
    /// 由编成支援卡构造（友人卡 → `Friend`，其余 → `Card`；均处于卡组前缀、与 deck 同序）
    pub(crate) fn from_card(card: &SupportCard) -> Result<Self> {
        let base = BasePerson::try_from(card).context("由支援卡构造人头失败")?;
        let origin = if card.card_type >= 5 {
            PersonOrigin::Friend
        } else {
            PersonOrigin::Card
        };
        Ok(Self {
            base,
            act: None,
            origin,
            avail_turn: if origin == PersonOrigin::Friend { 2 } else { 0 },
            needs_great_success: false,
        })
    }

    /// 编成外神驹（无得意率 / 无红点 / 无羁绊面板）
    pub(crate) fn guest(
        chara_id: u32,
        act: usize,
        avail_turn: i32,
        needs_great_success: bool,
    ) -> Self {
        Self {
            base: BasePerson {
                person_index: 0,
                person_type: PersonType::Npc,
                train_type: -1,
                chara_id,
                friendship: 0,
                is_hint: false,
                card_id: None,
            },
            act: Some(act),
            origin: PersonOrigin::Guest,
            avail_turn,
            needs_great_success,
        }
    }

    /// 理事长人头（沿用引擎术语；按引擎基础局默认处理）
    pub(crate) fn yayoi() -> Self {
        Self {
            base: BasePerson::yayoi(),
            act: None,
            origin: PersonOrigin::Yayoi,
            avail_turn: 0,
            needs_great_success: false,
        }
    }

    /// 记者人头（按引擎基础局默认处理）
    pub(crate) fn reporter() -> Self {
        Self {
            base: BasePerson::reporter(),
            act: None,
            origin: PersonOrigin::Reporter,
            avail_turn: 0,
            needs_great_success: false,
        }
    }

    /// 人头序号（`add_person` 按当前人数重编号）
    pub(crate) fn set_person_index(&mut self, index: i32) {
        self.base.person_index = index;
    }

    /// 角色 ID（担当分配 / 同 chara 唯一性校验用）
    #[must_use]
    pub fn chara_id(&self) -> u32 {
        self.base.chara_id
    }

    /// 人头短名（终端展示用；编成外神驹显示 `[NPC]` + 角色名前两字）
    #[must_use]
    pub fn short_name(&self) -> String {
        match self.origin {
            PersonOrigin::Guest => {
                let name = Data::get()
                    .map(|data| {
                        data.get_chara_name(self.base.chara_id)
                            .chars()
                            .take(2)
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                format!("[NPC]{name}")
            }
            _ => self.base.short_name(),
        }
    }
}
