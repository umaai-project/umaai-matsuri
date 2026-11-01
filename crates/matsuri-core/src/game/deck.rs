//! 编成检测（禁止项 / 警告项）
//!
//! 担当机制对编成非常敏感：**禁止项**直接拒绝建局（`newgame` 返回 `Err`）；**警告项**不阻止
//! 建局，随 `MatsuriGame::deck_warnings` / `MatsuriView` 下发给宿主展示。
//! 规则来源见 `docs/matsuri_memo_cn.md` §2.2（担当编排 / link / 外派链 / 余额）。
//!
//! 宿主可在建局前先调 [`inspect_deck`] 自查（用 `SupportCard::new(idrank)` 构造卡组）。

use std::collections::{HashMap, HashSet};
use std::fmt::Display;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use umasim::game::SupportCard;

use super::roster::assign_card_acts;
use crate::data::MatsuriData;

/// 演目 / 训练位名称（0–4：速 / 耐 / 力 / 毅 / 智）
const ACT_NAMES: [&str; 5] = ["速度", "耐力", "力量", "毅力", "智力"];
/// 编成里的友人卡张数（本剧本强制带 SSR 吾妻）
const FRIEND_CARD_COUNT: usize = 1;

/// 演目 / 训练位名称（越界回退为下标文本）
fn act_name(act: usize) -> String {
    ACT_NAMES
        .get(act)
        .map_or_else(|| format!("#{act}"), |name| (*name).to_string())
}

/// 支援卡类型名称（0 速 / 1 耐 / 2 力 / 3 毅 / 4 智 / 越界回退为下标文本）
fn type_name(card_type: i32) -> String {
    usize::try_from(card_type)
        .ok()
        .and_then(|index| ACT_NAMES.get(index))
        .map_or_else(|| format!("#{card_type}"), |name| (*name).to_string())
}

// ---------------------------------------------------------------------------
// 禁止项
// ---------------------------------------------------------------------------

/// 编成禁止项：出现任意一条即拒绝建局
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckError {
    /// 没带 SSR 吾妻（友人位空缺）
    MissingSsrAzuma,
    /// 友人卡多于 1 张
    MultipleFriendCards { count: usize },
    /// 带了 SSR 吾妻以外的友人卡（R 吾妻 / 其他友人）
    FriendCardNotSsrAzuma { card_id: u32 },
    /// 卡组不符合通用规则（必须 6 张 / 无重复人物 / 不含担当马娘 / 卡类型正确）
    GeneralRuleViolation { reasons: Vec<String> },
}

impl Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSsrAzuma => write!(f, "没带 SSR 吾妻（本剧本仅支持带 SSR 吾妻的编成）"),
            Self::MultipleFriendCards { count } => write!(f, "友人卡多于 1 张（实际 {count} 张）"),
            Self::FriendCardNotSsrAzuma { card_id } => write!(
                f,
                "带了 SSR 吾妻以外的友人卡（card_id {card_id}）：本剧本仅支持 SSR 吾妻"
            ),
            Self::GeneralRuleViolation { reasons } => {
                write!(f, "卡组不符合通用规则：{}", reasons.join("；"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 警告项
// ---------------------------------------------------------------------------

/// 编成警告项：不阻止建局，需在宿主（终端 / Web / 在线端）提示
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckWarning {
    /// 编成支援卡类型少于 5 种（含友人）：演目 Lv 效果的「5 种卡」列无法发动
    FewTypeKinds { kinds: usize },
    /// 剧本 link 卡的固定演目与其卡类型不一致（如速重炮被钉在力量演目）
    LinkTypeMismatch {
        card_id: u32,
        card_type: i32,
        act: usize,
    },
    /// 同一类型卡 >= 3 张：整位容量 2 张，必然产生外派
    TypeConcentration { card_type: i32, count: usize },
    /// 卡被外派到非本职演目（担当位置溢出到别的训练）
    CardDispatched {
        card_id: u32,
        card_type: i32,
        act: usize,
    },
    /// 各演目「卡 + 第 1 批 NPC」人物数极差 >= 2：担当分布不均衡
    UnevenActPersons { min: usize, max: usize },
}

impl Display for DeckWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FewTypeKinds { kinds } => write!(
                f,
                "编成支援卡类型只有 {kinds} 种（含友人；<5）：演目 Lv 效果的「5 种卡」列无法发动"
            ),
            Self::LinkTypeMismatch {
                card_id,
                card_type,
                act,
            } => write!(
                f,
                "剧本链接卡 card_id {card_id}（{}类型）被钉在「{}」演目：类型与固定演目不一致",
                type_name(*card_type),
                act_name(*act),
            ),
            Self::TypeConcentration { card_type, count } => write!(
                f,
                "「{}」类型卡共 {count} 张（≥3）：整位容量 2 张，必然产生外派",
                type_name(*card_type)
            ),
            Self::CardDispatched {
                card_id,
                card_type,
                act,
            } => write!(
                f,
                "卡 card_id {card_id}（{}类型）被外派到「{}」演目：担当位置溢出到别的训练",
                type_name(*card_type),
                act_name(*act)
            ),
            Self::UnevenActPersons { min, max } => write!(
                f,
                "各演目「卡+NPC」人物数为 {min}–{max}（极差 ≥2）：担当分布不均衡"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// 检测结果
// ---------------------------------------------------------------------------

/// 编成检测报告
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckReport {
    /// 禁止项（非空则拒绝建局）
    pub errors: Vec<DeckError>,
    /// 警告项（不阻止建局）
    pub warnings: Vec<DeckWarning>,
    /// 卡侧担当分配结果（与 deck 同序；友人卡 `None`）——建局复用，避免重复计算
    pub(crate) card_acts: Vec<Option<usize>>,
}

impl DeckReport {
    /// 是否通过禁止项
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// 禁止项合并成一条错误信息（`newgame` 报错用）
    #[must_use]
    pub fn error_message(&self) -> String {
        self.errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("；")
    }
}

/// 检查一套编成：返回禁止项 / 警告项 / 卡侧担当分配结果
///
/// `trained_chara` 是育成马娘的角色 id（`uma_id / 100`），用于「不含担当马娘」校验。
/// 「神驹顶替」（link = 固定演目 / 育成马娘是 link）属合理情况，本函数不警告；
/// 分布失衡（`UnevenActPersons`）在名单构建（含 NPC 余额）后由 `roster` 追加。
pub fn inspect_deck(
    deck: &[SupportCard],
    data: &MatsuriData,
    trained_chara: u32,
) -> Result<DeckReport> {
    let expect_total = data.limits.support_num as usize + FRIEND_CARD_COUNT;
    let friend_ids: Vec<u32> = data.outing.iter().map(|outing| outing.card_id).collect();
    let mut errors: Vec<DeckError> = Vec::new();
    let mut friends = 0usize;
    let mut seen_chara: HashSet<u32> = HashSet::new();
    let mut rule_reasons: Vec<String> = Vec::new();
    if deck.len() != expect_total {
        rule_reasons.push(format!("必须 {expect_total} 张（实际 {} 张）", deck.len()));
    }
    for card in deck {
        match card.card_type {
            0..=4 => {}
            5 => {
                friends += 1;
                let is_ssr_azuma = card.data.rarity == 3 && friend_ids.contains(&card.data.card_id);
                if !is_ssr_azuma {
                    errors.push(DeckError::FriendCardNotSsrAzuma {
                        card_id: card.data.card_id,
                    });
                }
            }
            card_type => rule_reasons.push(format!(
                "卡类型不合法（card_id {}，card_type={card_type}；本剧本不模拟团队卡）",
                card.data.card_id
            )),
        }
        if !seen_chara.insert(card.data.chara_id) {
            rule_reasons.push(format!("存在重复人物（chara_id {}）", card.data.chara_id));
        }
        if card.data.chara_id == trained_chara {
            rule_reasons.push(format!("包含担当马娘本人（chara_id {trained_chara}）"));
        }
    }
    if friends == 0 {
        errors.push(DeckError::MissingSsrAzuma);
    } else if friends > 1 {
        errors.push(DeckError::MultipleFriendCards { count: friends });
    }
    if !rule_reasons.is_empty() {
        rule_reasons.sort();
        rule_reasons.dedup();
        errors.push(DeckError::GeneralRuleViolation {
            reasons: rule_reasons,
        });
    }
    dedup_errors(&mut errors);
    if !errors.is_empty() {
        return Ok(DeckReport {
            errors,
            warnings: Vec::new(),
            card_acts: vec![None; deck.len()],
        });
    }
    // 卡侧担当分配（①②③）
    let card_types: Vec<i32> = deck.iter().map(|card| card.card_type).collect();
    let link_act_of: HashMap<u32, usize> = data
        .acts
        .iter()
        .enumerate()
        .map(|(act, info)| (info.npc_chara_id, act))
        .collect();
    let link_acts: Vec<Option<usize>> = deck
        .iter()
        .map(|card| link_act_of.get(&card.data.chara_id).copied())
        .collect();
    let card_acts = assign_card_acts(&card_types, &link_acts)?;
    // 警告项
    let mut warnings: Vec<DeckWarning> = Vec::new();
    // W4：编成支援卡类型少于 5 种（含友人；演目 Lv 的「5 种卡」列）
    let kinds: HashSet<i32> = deck.iter().map(|card| card.card_type).collect();
    if kinds.len() < 5 {
        warnings.push(DeckWarning::FewTypeKinds { kinds: kinds.len() });
    }
    // W2：同一类型卡 >= 3 张
    let mut type_count = [0usize; 5];
    for card in deck {
        if (0..5).contains(&card.card_type) {
            type_count[card.card_type as usize] += 1;
        }
    }
    for (card_type, count) in type_count.iter().enumerate() {
        if *count >= 3 {
            warnings.push(DeckWarning::TypeConcentration {
                card_type: card_type as i32,
                count: *count,
            });
        }
    }
    // W1：link 卡类型与固定演目不一致；W3：卡被外派到非本职演目
    for (index, card) in deck.iter().enumerate() {
        if card.card_type >= 5 {
            continue;
        }
        let Some(act) = card_acts[index] else {
            continue;
        };
        if link_acts[index].is_some() {
            if card.card_type != act as i32 {
                warnings.push(DeckWarning::LinkTypeMismatch {
                    card_id: card.data.card_id,
                    card_type: card.card_type,
                    act,
                });
            }
        } else if card.card_type != act as i32 {
            warnings.push(DeckWarning::CardDispatched {
                card_id: card.data.card_id,
                card_type: card.card_type,
                act,
            });
        }
    }
    Ok(DeckReport {
        errors,
        warnings,
        card_acts,
    })
}

/// 去重禁止项（保留首次出现顺序）
fn dedup_errors(errors: &mut Vec<DeckError>) {
    let mut unique: Vec<DeckError> = Vec::with_capacity(errors.len());
    for error in errors.drain(..) {
        if !unique.contains(&error) {
            unique.push(error);
        }
    }
    *errors = unique;
}
