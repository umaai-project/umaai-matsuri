//! 帧 → 局面还原（[`MatsuriGame::restore`]）与反向导出（[`MatsuriGame::export_festa`]）
//!
//! 在线档**一帧一决策**，所以还原必须是「帧 + 本地静态数据 → 局面」的**无状态函数**：
//! 不重放前序回合、不抽随机、不结算；随机分布 / 事件结果一律取真机帧的值。
//!
//! ## 两条必须保留的注释（防止后人「优化」）
//!
//! 1. **`init_persons` 的分配结果会被整段覆盖**：这里仍然调它，是为了拿到编成门禁
//!    （[`inspect_deck`](super::deck::inspect_deck)：必须带 SSR 吾妻、6 张卡、无重复…）与
//!    `deck_warnings`；卡组人头的前缀布局也由它建立。随后 `restore_persons` 按帧重写担当，
//!    离线那套「五步流水线」的随机结果在这里**不作数**。
//! 2. **不得从 `Begin` 进入**：`stage_body(Begin)` 会 `derive_plan` / `recover_fatigue` /
//!    `roll_fat_recovery` / `generate_events`（抽随机事件）/ `acquire_advice` / `roll_kensan`。
//!    真机跑到决策点时这些已经发生过，再跑一遍会重复抽事件、重复掷追加稽古。
//!    因此入口阶段由调用方给定：训练前 = [`Stage::Train`]，训练后 = [`Stage::AfterTrain`]。
//!
//! ## 与方案文档的差异（实施时按真机帧改正）
//!
//! | 项 | 文档口径 | 实际帧 / 本实现 |
//! |---|---|---|
//! | `persons[].targetId` | 「`1–6` = 卡位、`targetId-1` 即卡组下标」 | **不成立**：`targetId` 是本局服务端人头 id，开新局时友人是 1、卡从 2 起；中途加载局（friend 不进 `evaluation_info_array`）卡从 1 起。本实现**按顺序**对齐卡位（见 [`MatsuriGame::restore_persons`]） |
//! | `festa.persons` 内容 | 「只发 `actType > 0`」 | T0 帧把整份名单都发了（含 `101–115` 占位与理事长 / 记者，`actType` 全 0）；按 `actType > 0` 过滤担当即可，两边都成立 |
//! | `trainLevelTotal == 0` | 「只出现在 `load/event/special`」 | 合宿期的 `command` 帧（T13–T15）**也整组为 0**；改为逐位判空 |
//! | 神驹批次 | 第 1 批 `avail_turn = 2` / 第 2 批 `avail_turn = 48 + 需大成功` | 帧内神驹一律 `avail_turn = 0`、不设大成功门槛（出现在帧里即已可用；帧按 `targetId` 升序排列，位置推不出批次） |

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use umaai::BasePersonStatus;
use umaai_core::gamedata::EventData;
use umaai_core::{Array5, Data, Ill};
use umasim::game::Game;
use umasim::game::base::BaseGame;

use super::festa::{ACT_COUNT, FestaAct, FestaCheckPoint, FestaPerson, FestaState};
use super::person::{MatsuriPerson, PersonOrigin};
use super::{MatsuriGame, SCENARIO_START_TURN, Stage};
use crate::MatsuriConfig;
use crate::data::{CheckPointResult, MatsuriData};
use crate::diag;

/// 协议人头 id 里「NPC / 占位」段（`101–115`）：从不出现在分布，还原时丢弃
const NPC_PLACEHOLDER_RANGE: std::ops::RangeInclusive<i32> = 101..=115;
/// 协议人头 id：理事长
const PROTOCOL_ID_YAYOI: i32 = 102;
/// 协议人头 id：记者
const PROTOCOL_ID_REPORTER: i32 = 103;
/// 分布里卡位的 id 上界（0–5 → 卡组前 6 位）
const PROTOCOL_CARD_ID_MAX: i32 = 5;
/// 分布里理事长 / 记者的 id（6 / 7）
const PROTOCOL_ID_YAYOI_DIST: i32 = 6;
const PROTOCOL_ID_REPORTER_DIST: i32 = 7;
/// 编成外神驹在协议 id 空间的下界（`>= 1000` 即 `chara_id`）
const PROTOCOL_GUEST_ID_MIN: i32 = 1000;
/// 每个训练位最多站 5 人（协议定长）
const MAX_PERSONS_PER_TRAIN: usize = 5;

/// 还原输入：帧已解析出的全部材料（局面底座 + 剧本段 + 入口阶段 + 跨帧基准）
///
/// 用结构体而不是位置参数：还原需要的材料会随真机核对继续增加（例如人头羁绊），
/// 位置参数一多就容易传错顺序。
pub struct RestoreInput<'a> {
    /// 通用段还原出的引擎底座（`BaseStatus::to_base_game` 的产物）
    ///
    /// 注意：其中的 `distribution` 是**协议原值**（`0–7` / `>= 1000` 的人头 id），
    /// 还原时由本模块改写成 `person_index`（离线口径）。
    pub base: BaseGame,
    /// 剧本段
    pub festa: &'a FestaState,
    /// 入口阶段：训练前 [`Stage::Train`] / 训练后 [`Stage::AfterTrain`]
    pub stage: Stage,
    /// 帧 `baseGame.persons`（含 `charaId` 与羁绊）：用于**卡位顺序自检**，见 [`MatsuriGame::check_roster`]
    pub frame_persons: &'a [BasePersonStatus],
    /// 上一帧的有效训练等级（`G12` 基准；首帧为 `None`）
    pub prev_train_levels: Option<Array5>,
    /// 非卡人头羁绊 `(理事长, 记者)`（帧 `friendship_noncard_*`；`None` = 帧未给，保持 0）
    pub noncard_friendship: Option<(i32, i32)>,
    /// 帧 `baseGame.charaEffects`：全量状态 ID（含非病状态，插件已去 0 / 去重 / 保序）
    ///
    /// 空 = 帧未给（旧插件）或马娘无任何状态 → 沿用引擎粗粒度 `isIll`；非空时
    /// 由 [`ill_from_effects`] 只挑 142 表 1–6 病种精确还原，其余状态一律忽略。
    pub chara_effects: &'a [i32],
    /// 帧 `baseGame.storyId`（待选事件 id；插件只透传 id，选项数据由接收端自持）
    ///
    /// 命中本地事件表（本仓 `scenario_events` → 引擎五表）时注入 `unresolved_events`，
    /// 路由据此进事件决策；查不到则由上层跳过（不给训练建议）。
    pub story_id: Option<i32>,
}

/// 全量状态数组 → 生病位集：只取 `chara_effect_id`（文本库分类 **142**）的 1–6
///
/// | id | 142 名称（中文） | 位 |
/// |---|---|---|
/// | 1 | 熬夜（夜ふかし気味） | [`Ill::NIGHT`] |
/// | 2 | 懒惰成性（なまけ癖） | [`Ill::LAZY`] |
/// | 3 | 皮肤粗糙（肌あれ） | [`Ill::SKIN`] |
/// | 4 | 变胖（太り気味） | [`Ill::FAT`] |
/// | 5 | 偏头痛（片頭痛） | [`Ill::HEADACHE`] |
/// | 6 | 不擅长练习（練習ベタ） | [`Ill::BAD_TRAINER`] |
///
/// **不要按 [`Ill`] 的位序推**：142 表是日文 UI 的列表顺序（熬夜在前、变胖在第 4），
/// 与位序不同。另注：事件数据里的 `add_flags.ill` 用的是**位集**（`1` = 变胖），
/// 与本函数的协议 id 是两套编号，别混用。输入是全量 `charaEffects`，1–6 以外（切者 / 正向思考 / 助力支撑 / 情热领域等）一律忽略。
fn ill_from_effects(ids: &[i32]) -> Ill {
    let mut ill = Ill::NONE;
    for id in ids {
        let kind = match id {
            1 => Ill::NIGHT,
            2 => Ill::LAZY,
            3 => Ill::SKIN,
            4 => Ill::FAT,
            5 => Ill::HEADACHE,
            6 => Ill::BAD_TRAINER,
            _ => continue,
        };
        ill.insert(kind);
    }
    ill
}

/// 按 `storyId` 找事件数据：**本仓剧本资产优先**，其后引擎五表（`story` / `uma` / `card` /
/// `friend` / `system`）
///
/// 插件端只透传 `story_id`（效果表未收录时不发 `story`），事件身份与选项数完全由接收端
/// 按自己的数据判断。`festa` / 追加稽古 / 友人出行第三段这类剧本关键事件都在
/// `scenario_events` 与 `friend_events` 里，命中即可正常做事件决策。
fn lookup_event(id: u32, scenario: &[EventData]) -> Option<EventData> {
    if let Some(event) = scenario.iter().find(|event| event.id == id) {
        return Some(event.clone());
    }
    let data = Data::get().ok()?;
    let events = &data.events;
    events
        .story_events
        .iter()
        .chain(events.uma_events.iter())
        .chain(events.card_events.iter())
        .chain(events.friend_events.values())
        .chain(events.system_events.values())
        .find(|event| event.id == id)
        .cloned()
}

impl MatsuriGame {
    /// 用一帧还原出可继续决策的局面
    ///
    /// 四步（顺序不能换）：
    ///
    /// 1. 复用会话已建好的 `BaseGame`（马娘 / 卡组 / 继承 / 回合 / flags / 友人 / 分布 / 训练计数）；
    /// 2. 复用建局的静态装配（[`Self::assemble`]）+ `init_persons`（编成门禁 / `deck_warnings`）；
    /// 3. **覆盖**剧本字段：人头、演目、教诲、合宿、発表会、助力、追加稽古、训练 Lv 加成、疲劳回复；
    /// 4. 派生 `plan` / `forced_rest` / `career_failed` 与入口阶段。
    pub fn restore(
        input: RestoreInput<'_>,
        data: Arc<MatsuriData>,
        config: MatsuriConfig,
    ) -> Result<Self> {
        let RestoreInput {
            base,
            festa,
            stage,
            frame_persons,
            prev_train_levels,
            noncard_friendship,
            chara_effects,
            story_id,
        } = input;
        let turn = base.turn;
        // 事件帧：插件可能只发 id（效果表未收录时没有 `story`），先按 id 查本地表
        let pending = story_id
            .filter(|id| *id > 0)
            .and_then(|id| lookup_event(id as u32, &data.scenario_events));
        let mut game = Self::assemble(base, data)?;
        // 病种：协议通用段只有 `isIll` 布尔（引擎落 `Ill::UNKNOWN`），帧带非空 `charaEffects`
        // 时按 142 表挑病种覆盖——变胖（`FAT`）只有这条路才认得出来。
        // 数组里只有 buff / 其它状态时 `ill_from_effects` 返回 `NONE`，同样覆盖粗粒度标记。
        if !chara_effects.is_empty() {
            let ill = ill_from_effects(chara_effects);
            let flags = &mut game.uma_mut().flags;
            flags.ill = ill;
            // 引擎不变量：`bad_trainer` 与 `Ill::BAD_TRAINER` 同置同清
            flags.bad_trainer |= ill.contains(Ill::BAD_TRAINER);
        }
        // 局外状态（小切 / 正向思考）以帧为准（`to_base_game` 已写进 flags），
        // 这里**不再**套 `with_config` 的 `[pre]` 段，否则会重复施加。
        game.config = config;
        // 注释 1：这里的分配随后被整段覆盖；它只负责编成门禁与 `deck_warnings`。
        game.init_persons()?;

        if turn >= SCENARIO_START_TURN {
            game.check_roster(frame_persons)?;
            game.restore_persons(festa, noncard_friendship)?;
        }
        // 剧本机制未启动（T0 / T1）：名单保持建局结果，担当 / 演目 / 教诲都不从帧取。
        game.restore_acts(festa);
        game.restore_advice(festa);
        game.restore_camp(festa);
        game.restore_check_points(festa);
        game.restore_train_levels(festa, prev_train_levels);
        game.assist_support_left = festa
            .assist_support_left
            .unwrap_or(0)
            .clamp(0, game.data.rules.assist_support.count.max(0));
        game.kensan_pending = festa.kensan_any();
        game.fatigue_recovery = festa.fatigue_recovery;

        // 人头建完才能把分布从协议 id 翻成 `person_index`
        let raw_distribution = std::mem::take(&mut game.base.distribution);
        game.base.distribution = game.map_distribution(&raw_distribution);

        // 事件帧注入：**本仓事件数据优先**——数值与选项数以本地表为准，帧 `story` 只提供选项文本。
        // 因此引擎按 `story` 建过的同 id 事件会被本地那份覆盖；本地查不到才回退帧数据，
        // 两者都没有则由路由层跳过（事件帧的训练 / 分布 / 等级都是残值，不能拿来做训练建议）
        if let Some(id) = story_id.filter(|id| *id > 0) {
            match pending {
                Some(event) => {
                    match game
                        .base
                        .unresolved_events
                        .iter_mut()
                        .find(|queued| queued.id == id as u32)
                    {
                        Some(slot) => {
                            let frame_groups = slot.choices.len();
                            *slot = event;
                            if frame_groups == slot.choices.len() {
                                diag!("【事件帧】T{turn} storyId={id} 按本地事件表覆盖帧数据");
                            } else {
                                diag!(
                                    "【事件帧】T{turn} storyId={id} 选项数不一致：帧 {frame_groups} 组 / 本地 {} 组，按本地",
                                    slot.choices.len()
                                );
                            }
                        }
                        None => {
                            diag!("【事件帧】T{turn} storyId={id} 按本地事件表注入");
                            game.base.unresolved_events.push(event);
                        }
                    }
                }
                None => diag!("【事件帧】T{turn} storyId={id} 未收录（本地事件表查不到）"),
            }
        }

        // 注释 2：入口阶段由调用方给定，绝不从 `Begin` 进来
        game.stage = stage;
        game.plan = game.derive_plan();
        game.forced_rest = super::stage::is_forced_rest_turn(game.base.uma.chara_id(), turn);
        game.career_failed = game.restored_career_failed(turn);
        Ok(game)
    }

    /// 反向导出：把局面折回一帧剧本段（round-trip / 工具面用）
    ///
    /// 口径与协议对齐：`persons` 只导**参与担当**的成员（`actType > 0`），`checkPoints` 只导
    /// **已结算**的场次，`trainLevelTotal` 导**当前有效等级**（不是 `click/4+1`）。
    #[must_use]
    pub fn export_festa(&self) -> FestaState {
        let acts = (0..ACT_COUNT)
            .map(|index| {
                let base_slot = self.advice.slots[index];
                let fused_slot = self.advice.fused[index];
                // 与还原口径互逆：未転成 `adviceId = 基础槽`；已転成 `adviceId = 転成槽`、`fusionId = 基础槽`
                let (advice_id, fusion_id) = if fused_slot != 0 {
                    (fused_slot, base_slot)
                } else {
                    (base_slot, 0)
                };
                FestaAct {
                    act_type: index as i32 + 1,
                    gauge: self.acts.gauge[index],
                    level: self.acts.level[index],
                    need_gauge: self.act_gauge_max(),
                    advice_id,
                    fusion_id,
                    outing_gain_level: 0,
                }
            })
            .collect();
        let check_points = (0..3)
            .filter(|index| self.check_points[*index] != CheckPointResult::NotReached)
            .map(|index| FestaCheckPoint {
                schedule_id: index as i32 + 1,
                result_type: match self.check_points[index] {
                    CheckPointResult::GreatSuccess => 2,
                    CheckPointResult::Success => 1,
                    CheckPointResult::NotReached => 0,
                },
            })
            .collect();
        let persons = self
            .persons
            .iter()
            .enumerate()
            .filter_map(|(index, person)| {
                let act_type = person.act? as i32 + 1;
                let origin = match person.origin {
                    PersonOrigin::Card => "card",
                    PersonOrigin::Friend => "friend",
                    PersonOrigin::Guest => "guest",
                    PersonOrigin::Yayoi => "yayoi",
                    PersonOrigin::Reporter => "reporter",
                };
                // 卡位：协议 id = 卡组下标 + 1（本仓导出侧的自定口径，只用于 round-trip）
                let target_id = match person.origin {
                    PersonOrigin::Card | PersonOrigin::Friend => index as i32 + 1,
                    PersonOrigin::Yayoi => PROTOCOL_ID_YAYOI,
                    PersonOrigin::Reporter => PROTOCOL_ID_REPORTER,
                    PersonOrigin::Guest => person.chara_id() as i32,
                };
                Some(FestaPerson {
                    target_id,
                    act_type,
                    origin: origin.to_string(),
                })
            })
            .collect();
        let train_level_total = Some(std::array::from_fn(|train| self.train_level(train) as i32));
        FestaState {
            acts,
            max_act_level: self.act_level_cap(),
            camp_remain: Some(self.camp_remain()),
            assist_support_left: Some(self.assist_support_left),
            fatigue_recovery: self.fatigue_recovery,
            check_points: Some(check_points),
            train_level_total,
            kensan: Some([self.kensan_pending; ACT_COUNT]),
            persons,
        }
    }

    /// 本次合宿剩余回合（`campRemain` 的逆运算；不在合宿期 → 0）
    fn camp_remain(&self) -> i32 {
        let Some(year) = self.camp.year else {
            return 0;
        };
        let Some(window) = self.data.camp.windows.get(year) else {
            return 0;
        };
        let total = window.end - window.start + 1;
        (total - self.camp.counted as i32).max(0)
    }

    /// 卡位顺序自检（**编成门禁的第二道**）
    ///
    /// `to_base_game` 按**数组下标**把 `baseGame.persons[i].friendship` 写进 `deck[i]`，
    /// 也就是假定「帧人头顺序 = 卡组顺序」。这条假设必须当着真机帧验证：
    /// 对不齐就直接报错，而不是带着错位的羁绊去算训练值。
    ///
    /// **身份以本地卡表为准**（2026-11-11）：小黑板与 AI 的数据各自更新，新支援卡常是
    /// AI 侧先有、小黑板侧还没有——插件 `ApplyPersonCharaIds` 查不到角色就把 `charaId`
    /// 留 0（协议里该字段本就可选、只用于解名字）。这是「帧端未知」不是「数据矛盾」，
    /// 跳过该位即可；帧端给出非 0 却与本地不符才是真人头错位，照旧拒绝。
    ///
    /// 身份之外再守一道**结构校验**：`personType`（1 = 友人 / 2 = 普通支援卡）由插件按
    /// `cardId` 直接算出，不依赖卡名表，因此 `charaId` 全为 0 时依然成立。
    fn check_roster(&self, frame_persons: &[BasePersonStatus]) -> Result<()> {
        if frame_persons.is_empty() {
            bail!("帧内缺少 baseGame.persons：无法校验卡位顺序（拒绝计算）");
        }
        ensure!(
            frame_persons.len() == self.deck.len(),
            "帧内人头数 {} 与卡组张数 {} 不一致（拒绝计算）",
            frame_persons.len(),
            self.deck.len()
        );
        for (index, (person, card)) in frame_persons.iter().zip(self.deck.iter()).enumerate() {
            if person.chara_id == 0 {
                diag!(
                    "【卡位自检】第 {} 位 charaId=0（小黑板卡表未收录 card {}），按本地 chara_id={} 继续",
                    index + 1,
                    card.card_id,
                    card.data.chara_id
                );
            } else {
                ensure!(
                    person.chara_id == card.data.chara_id,
                    "第 {} 位人头对不上：帧内 chara_id={}，卡组 card_id={} 的 chara_id={}（帧人头顺序与卡组不一致，拒绝计算）",
                    index + 1,
                    person.chara_id,
                    card.card_id,
                    card.data.chara_id
                );
            }
            // `0` = 未加载 / 语义未定，不参与比对；只认 1 / 2 两种卡位语义
            let frame_friend = match person.person_type {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            };
            if let Some(frame_friend) = frame_friend {
                ensure!(
                    frame_friend == (card.card_type >= 5),
                    "第 {} 位卡位类型对不上：帧内是{}，卡组 card_id={} 是{}（帧人头顺序与卡组不一致，拒绝计算）",
                    index + 1,
                    if frame_friend {
                        "友人卡"
                    } else {
                        "普通支援卡"
                    },
                    card.card_id,
                    if card.card_type >= 5 {
                        "友人卡"
                    } else {
                        "普通支援卡"
                    }
                );
            }
        }
        Ok(())
    }

    /// 按帧重建人头名单（卡位担当 / 理事长 / 记者 / 编成外神驹）
    ///
    /// **卡位映射**：协议 `targetId` 是本局服务端人头 id，开新局时友人占 1、卡从 2 起；
    /// 中途加载局里友人不进 `evaluation_info_array`，卡从 1 起。两种排法差一位，所以
    /// **不能做 `targetId - 1` 的算术**，改为：按 `targetId` 升序走一遍帧人头，`origin == card`
    /// 依次落到卡组里**非友人**卡位上（卡组顺序）；`origin == friend` 只标记友人不参与担当。
    fn restore_persons(
        &mut self,
        festa: &FestaState,
        noncard_friendship: Option<(i32, i32)>,
    ) -> Result<()> {
        let deck_len = self.deck.len();
        // 帧没说担当 = 没有担当：先把卡位清空（离线五步流水线的结果不作数）
        for person in self.persons.iter_mut().take(deck_len) {
            person.act = None;
        }
        let card_slots: Vec<usize> = (0..deck_len)
            .filter(|index| self.persons[*index].origin == PersonOrigin::Card)
            .collect();
        let mut card_slots = card_slots.into_iter();

        let mut guests: BTreeMap<u32, Option<usize>> = BTreeMap::new();
        let mut entries: Vec<&FestaPerson> = festa.persons.iter().collect();
        entries.sort_by_key(|person| person.target_id);
        for person in entries {
            match classify_person(person) {
                PersonKind::Friend => {} // 友人（吾妻）不参与担当
                PersonKind::Card => {
                    let Some(index) = card_slots.next() else {
                        bail!(
                            "帧内卡位人头多于卡组非友人卡（targetId={}，卡组 {deck_len} 张）",
                            person.target_id
                        );
                    };
                    self.persons[index].act = act_index(person.act_type);
                }
                PersonKind::Yayoi | PersonKind::Reporter => {}
                PersonKind::Placeholder => {}
                PersonKind::Guest => {
                    let chara_id = u32::try_from(person.target_id)
                        .with_context(|| format!("帧内神驹 targetId 非法：{}", person.target_id))?;
                    // 神驹的 targetId 就是角色 id，本地必须有这个角色才谈得上算他的加成；
                    // 缺数据是「AI 侧落后于游戏版本」，处置是让玩家更新（与卡位 charaId=0 不同）
                    ensure!(
                        Data::get()?.get_chara_name(chara_id) != "未知",
                        "帧内编成外神驹 targetId={chara_id} 本地没有该角色（无法计算）：请更新 AI 数据后重试，或等待上游更新"
                    );
                    guests.insert(chara_id, act_index(person.act_type));
                }
            }
        }
        // 只在分布里出现、帧名单里没有 actType 的神驹：占一个人头、不给担当
        for row in &self.base.distribution {
            for id in row {
                if *id >= PROTOCOL_GUEST_ID_MIN {
                    let chara_id = *id as u32;
                    guests.entry(chara_id).or_insert(None);
                }
            }
        }

        let cards: Vec<MatsuriPerson> = self.persons.iter().take(deck_len).cloned().collect();
        let (yayoi_friendship, reporter_friendship) = noncard_friendship.unwrap_or((0, 0));
        let mut yayoi = MatsuriPerson::yayoi();
        yayoi.base.friendship = yayoi_friendship;
        let mut reporter = MatsuriPerson::reporter();
        reporter.base.friendship = reporter_friendship;

        self.persons.clear();
        for person in cards {
            self.add_person(person);
        }
        self.add_person(yayoi);
        self.add_person(reporter);
        for (chara_id, act) in guests {
            // `guest()` 要求先给一个演目；不参与担当的占位随后改回 `None`
            let mut person = MatsuriPerson::guest(chara_id, act.unwrap_or(0), 0, false);
            person.act = act;
            self.add_person(person);
        }
        Ok(())
    }

    /// 演目槽 / 演目 Lv（按 `actType` 落位，不信数组下标）
    ///
    /// `actType` 越界（协议新增了本仓没有的演目）只跳过该项：其余演目的槽位与等级仍然可用，
    /// 不值得为一条多出来的记录拒绝整帧。
    fn restore_acts(&mut self, festa: &FestaState) {
        let gauge_max = self.act_gauge_max();
        let level_cap = self.act_level_cap();
        for act in &festa.acts {
            let Some(index) = act_index_of(act.act_type) else {
                diag!(
                    "【演目】T{} actType={} 越界（应为 1–{ACT_COUNT}），忽略该项",
                    self.base.turn,
                    act.act_type
                );
                continue;
            };
            self.acts.gauge[index] = act.gauge.clamp(0, gauge_max);
            // `level == 0` = 本帧未给（真机运行期恒 ≥1），保持默认 Lv1
            if act.level > 0 {
                self.acts.level[index] = act.level.clamp(1, level_cap);
            }
        }
    }

    /// 教诲槽：`fusionId == 0` → 未転成，`slots = adviceId`；否则 `slots = fusionId`、`fused = adviceId`
    fn restore_advice(&mut self, festa: &FestaState) {
        for act in &festa.acts {
            let Some(index) = act_index_of(act.act_type) else {
                continue;
            };
            if act.fusion_id == 0 {
                self.advice.slots[index] = act.advice_id;
                self.advice.fused[index] = 0;
            } else {
                self.advice.slots[index] = act.fusion_id;
                self.advice.fused[index] = act.advice_id;
            }
        }
        self.advice.acquired = self.base.turn >= SCENARIO_START_TURN
            || self.advice.slots.iter().any(|slot| *slot != 0);
    }

    /// 合宿进度：`campRemain` 是存储值，反推 `(year, counted)`
    ///
    /// `counted = 窗口长度 − remain`；`year = max{ i | window[i].start ≤ turn }`（`remain > 0` 时）。
    /// `remain == 0` 表示本次合宿已在上一回合结束 → 不在合宿期。
    fn restore_camp(&mut self, festa: &FestaState) {
        self.camp = super::stage::CampState::default();
        let remain = festa.camp_remain.unwrap_or(0);
        if remain <= 0 {
            return;
        }
        let turn = self.base.turn;
        let windows = &self.data.camp.windows;
        let Some(year) = windows.iter().rposition(|window| window.start <= turn) else {
            return;
        };
        let window = &windows[year];
        let total = (window.end - window.start + 1).max(1);
        self.camp = super::stage::CampState {
            year: Some(year),
            counted: (total - remain).max(0) as usize,
        };
    }

    /// 進捗発表会 / 本番结果（`scheduleId` 1 基；数组顺序不保证，按 id 落位）
    ///
    /// 越界的 `scheduleId` 只跳过该条（本仓只有 3 场），不让一条多出来的记录否决整帧。
    fn restore_check_points(&mut self, festa: &FestaState) {
        if let Some(points) = &festa.check_points {
            for point in points {
                let index = point
                    .schedule_id
                    .checked_sub(1)
                    .and_then(|id| usize::try_from(id).ok())
                    .filter(|index| *index < 3);
                let Some(index) = index else {
                    diag!(
                        "【発表会】T{} scheduleId={} 越界（应为 1–3），忽略该项",
                        self.base.turn,
                        point.schedule_id
                    );
                    continue;
                };
                self.check_points[index] = match point.result_type {
                    2 => CheckPointResult::GreatSuccess,
                    1 => CheckPointResult::Success,
                    // 0 / 其它：真机未采样到（`G3`），按「未达成」处理——不涨発表会 buff，
                    // 也不会误触发第 2 批神驹的大成功门槛
                    _ => CheckPointResult::NotReached,
                };
            }
        }
        self.finals_done = self.check_points[2] != CheckPointResult::NotReached;
    }

    /// 训练等级加成：以帧等级为**真值**反解 `train_level_bonus`
    ///
    /// 不变式（M1 守门）：还原后 `train_level(i) == tlt[i]`（`tlt[i] > 0` 时）。
    /// `tlt[i] == 0` 视为「本帧无有效等级」（合宿期 `command` 帧、`load/event/special` 帧整组为 0），
    /// 逐位沿用上一帧等级；首帧没有基准时退回「设施计数/4 + 1」。
    fn restore_train_levels(&mut self, festa: &FestaState, prev: Option<Array5>) {
        let up = self.data.rules.train_level_up_count.max(1);
        let cap = self.max_normal_train_level() as i32;
        for train in 0..ACT_COUNT {
            let from_clicks = self.base.train_level_count[train] / up + 1;
            let level = match festa.train_level(train) {
                Some(total) => total.clamp(1, cap),
                None => prev
                    .map_or(from_clicks, |levels| levels[train])
                    .clamp(1, cap),
            };
            self.train_level_bonus[train] = (level - from_clicks).max(0);
        }
    }

    /// 育成失败：区间已过末回合且场数未达标（帧仍在 → 正常应恒 `false`）
    fn restored_career_failed(&self, turn: i32) -> bool {
        self.base.uma.free_races.iter().any(|free| {
            (free.end_turn as i32) < turn && self.base.uma.count_free_race(free) < free.count
        })
    }

    /// 分布：协议人头 id → `person_index`（引擎按 `persons` 下标解释分布）
    ///
    /// `to_base_game` 把帧的 `personDistribution` **原样**拷进底座，而引擎的
    /// `count_training_persons` / `calc_training_buff` 都按 `persons` 下标取人——
    /// 不翻这一层，编成外神驹（id ≥ 1000）会被当成越界下标静默丢弃，人数加成与神驹担当全丢。
    ///
    /// **单点翻不出来只丢弃该人头**（`diag!` 留痕），不拒绝整帧：少一个人只影响该训练位的
    /// 人数加成，整帧拒绝会让这一局彻底没有建议。可能触发的情形：老插件把神驹压成 `8`、
    /// 协议新增本仓还不认识的编号段、一行超过协议定长 5 人。
    fn map_distribution(&self, raw: &[Vec<i32>]) -> Vec<Vec<i32>> {
        let deck_len = self.deck.len() as i32;
        let yayoi = self.person_index_of(PersonOrigin::Yayoi);
        let reporter = self.person_index_of(PersonOrigin::Reporter);
        let mut out = Vec::with_capacity(raw.len());
        for (train, row) in raw.iter().enumerate() {
            if row.len() > MAX_PERSONS_PER_TRAIN {
                diag!(
                    "【分布】T{} 第 {train} 号训练位给了 {} 人（协议定长 {MAX_PERSONS_PER_TRAIN}），按前 {MAX_PERSONS_PER_TRAIN} 人取",
                    self.base.turn,
                    row.len()
                );
            }
            let mut mapped = Vec::with_capacity(row.len().min(MAX_PERSONS_PER_TRAIN));
            for id in row.iter().take(MAX_PERSONS_PER_TRAIN) {
                if *id < 0 {
                    continue; // 空位哨兵：离线分布只放实际人头
                }
                if NPC_PLACEHOLDER_RANGE.contains(id) {
                    continue; // 协议占位 / NPC：从不出现在分布
                }
                let index = if (0..=PROTOCOL_CARD_ID_MAX).contains(id) {
                    if *id >= deck_len {
                        diag!(
                            "【分布】T{} 卡位 id={id} 超出卡组张数 {deck_len}，丢弃该人头",
                            self.base.turn
                        );
                        continue;
                    }
                    *id as usize
                } else if *id == PROTOCOL_ID_YAYOI_DIST {
                    let Some(index) = yayoi else {
                        diag!("【分布】T{} 理事长人头未建，丢弃该人头", self.base.turn);
                        continue;
                    };
                    index
                } else if *id == PROTOCOL_ID_REPORTER_DIST {
                    let Some(index) = reporter else {
                        diag!("【分布】T{} 记者人头未建，丢弃该人头", self.base.turn);
                        continue;
                    };
                    index
                } else if *id >= PROTOCOL_GUEST_ID_MIN {
                    let chara_id = *id as u32;
                    let Some(index) = self.persons.iter().position(|person| {
                        person.origin == PersonOrigin::Guest && person.chara_id() == chara_id
                    }) else {
                        diag!(
                            "【分布】T{} 神驹 id={chara_id} 未登记进人头，丢弃该人头",
                            self.base.turn
                        );
                        continue;
                    };
                    index
                } else {
                    diag!(
                        "【分布】T{} 无法归类的人头 id={id}（协议编号段本仓不识别），丢弃该人头",
                        self.base.turn
                    );
                    continue;
                };
                mapped.push(index as i32);
            }
            out.push(mapped);
        }
        out
    }

    /// 某来源人头的下标（就地查找；`None` = 人头未建）
    fn person_index_of(&self, origin: PersonOrigin) -> Option<usize> {
        self.persons
            .iter()
            .position(|person| person.origin == origin)
    }
}

/// 帧人头的语义分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersonKind {
    /// 编成支援卡
    Card,
    /// 剧本友人（吾妻）
    Friend,
    /// 理事长
    Yayoi,
    /// 记者
    Reporter,
    /// 编成外神驹
    Guest,
    /// 协议占位 / NPC（`101–115`）
    Placeholder,
}

/// 按 `origin` 分类，`origin` 缺失时退回 `targetId` 编号空间（`G11`）
///
/// `origin == "guest"` 也要看编号：剧本启动前的帧会把 `101–115` 的 NPC 占位一并标成
/// `guest`（`actType = 0`），它们不是编成外神驹，必须丢弃。
fn classify_person(person: &FestaPerson) -> PersonKind {
    match person.origin.as_str() {
        "card" => PersonKind::Card,
        "friend" => PersonKind::Friend,
        "yayoi" => PersonKind::Yayoi,
        "reporter" => PersonKind::Reporter,
        "guest" => {
            if person.target_id >= PROTOCOL_GUEST_ID_MIN {
                PersonKind::Guest
            } else {
                PersonKind::Placeholder
            }
        }
        _ => {
            let id = person.target_id;
            if id >= PROTOCOL_GUEST_ID_MIN {
                PersonKind::Guest
            } else if id == PROTOCOL_ID_YAYOI {
                PersonKind::Yayoi
            } else if id == PROTOCOL_ID_REPORTER {
                PersonKind::Reporter
            } else if NPC_PLACEHOLDER_RANGE.contains(&id) {
                PersonKind::Placeholder
            } else if (1..=6).contains(&id) {
                PersonKind::Card
            } else {
                PersonKind::Placeholder
            }
        }
    }
}

/// `actType`（1–5）→ 演目 / 训练位下标（0–4）；`0` 与越界 → `None`（不参与担当）
fn act_index(act_type: i32) -> Option<usize> {
    act_index_of(act_type)
}

/// 同 [`act_index`]，语义化的别名（用于必填场景的对话）
fn act_index_of(act_type: i32) -> Option<usize> {
    let index = usize::try_from(act_type).ok()?.checked_sub(1)?;
    (index < ACT_COUNT).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act_index_maps_one_based() {
        assert_eq!(act_index(1), Some(0));
        assert_eq!(act_index(5), Some(4));
        assert_eq!(act_index(0), None, "0 = 不参与担当");
        assert_eq!(act_index(6), None, "越界");
        assert_eq!(act_index(-1), None);
    }

    #[test]
    fn classify_falls_back_to_id_space() {
        let person = |target_id: i32, origin: &str| FestaPerson {
            target_id,
            act_type: 0,
            origin: origin.to_string(),
        };
        assert_eq!(classify_person(&person(3, "card")), PersonKind::Card);
        assert_eq!(classify_person(&person(1, "friend")), PersonKind::Friend);
        // 剧本启动前的帧把 101–115 占位也标成 guest：编号 < 1000 → 不是编成外神驹
        assert_eq!(
            classify_person(&person(101, "guest")),
            PersonKind::Placeholder
        );
        assert_eq!(classify_person(&person(1009, "guest")), PersonKind::Guest);
        // origin 缺失时按编号空间兜底
        assert_eq!(classify_person(&person(1009, "")), PersonKind::Guest);
        assert_eq!(classify_person(&person(102, "")), PersonKind::Yayoi);
        assert_eq!(classify_person(&person(103, "")), PersonKind::Reporter);
        assert_eq!(classify_person(&person(104, "")), PersonKind::Placeholder);
        assert_eq!(classify_person(&person(4, "")), PersonKind::Card);
    }
}
