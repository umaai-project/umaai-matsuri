//! 人头名单与担当分配（建局 / NPC 加入时的一次性动作）
//!
//! 五步流水线（link 钉位 → 卡按类型本位 → 外派链 → NPC 每演目 1 人 → 带吾妻余额 1 人）见
//! `docs/matsuri_memo_cn.md` §2.2；本文件只决定「谁担当哪个演目」，**每回合站位由 `distribute.rs` 决定**。
//!
//! 随机非 link 角色的离线口径：真机从马娘池随机抓；本模拟**从 `chara_id` 1001 起按序取**
//! 第一个满足条件者（非剧本 link / 非育成马娘 / 未出现过）。**同 `chara_id` 不允许两个人头**
//! （同 chara 只能是 distribution 里的分身占位）。

use std::collections::HashSet;

use anyhow::{Result, anyhow, bail, ensure};
use umaai_core::Data;

use super::MatsuriGame;
use super::deck::{DeckWarning, inspect_deck};
use super::person::MatsuriPerson;

/// 每个训练位最多站 2 张卡（只存在于担当分配；进入每回合站位后只受每训练位 5 人上限约束）
const CARD_SLOTS_PER_ACT: usize = 2;
/// 演目 / 训练位数量
const ACT_COUNT: usize = 5;
/// 编成外神驹第 1 批加入回合（0 基）
const GUEST_FIRST_TURN: i32 = 2;
/// 编成外神驹第 2 批可用回合（turn 47 结算后；0 基）
const GUEST_SECOND_TURN: i32 = 48;

/// 外派链：卡属性 → 依次尝试的演目（`docs/matsuri_memo_cn.md` §2.2(a)④）
fn dispatch_chain(card_type: i32) -> &'static [usize] {
    match card_type {
        0 => &[3, 1],
        1 => &[0, 3],
        2 => &[4, 0],
        3 => &[2, 4],
        4 => &[1, 2],
        _ => &[],
    }
}

/// 卡侧担当分配（①②③）：返回每张卡的担当演目（友人卡不参与 → `None`）
///
/// `card_types` 与 `link_acts` 与卡组同序；`link_acts[i] = Some(act)` 表示该卡是剧本 link 卡
/// （按 `chara_id` 钉在固定演目）。容量是**整位 2 张**（不分属性，友人卡不占名额）：
/// ① link 钉位 → ② 其余卡进自己类型的训练位（满则跳过）→ ③ 外派链改道（遇满位继续跳）。
pub(crate) fn assign_card_acts(
    card_types: &[i32],
    link_acts: &[Option<usize>],
) -> Result<Vec<Option<usize>>> {
    ensure!(
        card_types.len() == link_acts.len(),
        "卡类型与链接表长度不一致"
    );
    let count = card_types.len();
    let mut assigned = vec![None; count];
    let mut slots = [0usize; ACT_COUNT];
    // ① link 卡钉位（不看卡类型）
    for index in 0..count {
        if card_types[index] >= 5 {
            continue; // 友人卡不参与担当
        }
        if let Some(act) = link_acts[index] {
            ensure!(act < ACT_COUNT, "link 演目越界: {act}");
            ensure!(
                slots[act] < CARD_SLOTS_PER_ACT,
                "link 卡超出整位容量: act {act}"
            );
            assigned[index] = Some(act);
            slots[act] += 1;
        }
    }
    // ② 其余卡按类型本位
    for index in 0..count {
        if assigned[index].is_some() || card_types[index] >= 5 {
            continue;
        }
        let card_type = card_types[index];
        if !(0..ACT_COUNT as i32).contains(&card_type) {
            bail!("非法支援卡类型: {card_type}");
        }
        let act = card_type as usize;
        if slots[act] < CARD_SLOTS_PER_ACT {
            assigned[index] = Some(act);
            slots[act] += 1;
        }
    }
    // ③ 外派链：遇满位继续跳（不绕去空位、最多两跳）
    for index in 0..count {
        if assigned[index].is_some() || card_types[index] >= 5 {
            continue;
        }
        let mut placed = false;
        for &act in dispatch_chain(card_types[index]) {
            if slots[act] < CARD_SLOTS_PER_ACT {
                assigned[index] = Some(act);
                slots[act] += 1;
                placed = true;
                break;
            }
        }
        ensure!(
            placed,
            "外派链找不到空位：第 {index} 张卡（type {}）",
            card_types[index]
        );
    }
    Ok(assigned)
}

/// NPC 余额层：带吾妻时多出的 1 人投给「卡 + NPC 人数最少」的演目；并列按 速→耐→力→毅→智
pub(crate) fn balance_guest_act(
    cards_per_act: &[usize; ACT_COUNT],
    npc_per_act: &[usize; ACT_COUNT],
) -> usize {
    (0..ACT_COUNT)
        .min_by_key(|&act| (cards_per_act[act] + npc_per_act[act], act))
        .unwrap_or(0)
}

/// 随机非 link 马娘取人（离线口径）：从 `chara_id` 1001 起按序取第一个满足条件者
fn pick_guest_chara(
    used: &HashSet<u32>,
    trained_chara: u32,
    link_chars: &HashSet<u32>,
) -> Result<u32> {
    let data = Data::get()?;
    let mut pool: Vec<u32> = data
        .uma
        .keys()
        .filter_map(|key| key.parse::<u32>().ok())
        .map(|game_id| game_id / 100)
        .collect();
    pool.sort_unstable();
    pool.dedup();
    pool.into_iter()
        .find(|chara| {
            *chara != trained_chara && !link_chars.contains(chara) && !used.contains(chara)
        })
        .ok_or_else(|| anyhow!("没有可用的随机非 link 马娘（编成外神驹补人失败）"))
}

impl MatsuriGame {
    /// 建局人头名单：卡组人头（`persons` 前缀与 `deck` 同序）+ 理事长 + 记者 + 编成外神驹，
    /// 并完成担当分配（建局 / NPC 加入时的一次性动作）
    pub(crate) fn init_roster(&mut self) -> Result<()> {
        let trained_chara = self.base.uma.chara_id();
        // 编成检测：禁止项拒绝建局；警告项随局面 / View 下发（宿主负责展示）
        let report = inspect_deck(&self.deck, &self.data, trained_chara)?;
        ensure!(report.is_ok(), "编成不合规：{}", report.error_message());
        self.deck_warnings = report.warnings;
        let card_acts = report.card_acts;
        let link_chars = self.link_chara_ids();
        // ① 卡组人头（含友人卡；契约布局要求前缀与 deck 同序）
        let mut persons = Vec::with_capacity(self.deck.len() + 13);
        let mut used: HashSet<u32> = HashSet::new();
        for card in &self.deck {
            let person = MatsuriPerson::from_card(card)?;
            ensure!(
                used.insert(person.chara_id()),
                "编成含同角色支援卡（同 chara_id 只能是分身）: {}",
                person.chara_id()
            );
            persons.push(person);
        }
        ensure!(
            !used.contains(&trained_chara),
            "育成马娘与编成支援卡同角色（同 chara_id 只能是分身）"
        );
        // ② 卡侧担当分配（①②③；检测阶段已算好，直接复用）
        let mut cards_per_act = [0usize; ACT_COUNT];
        for (person, act) in persons.iter_mut().zip(card_acts) {
            person.act = act;
            if let Some(act) = act {
                cards_per_act[act] += 1;
            }
        }
        // ③ 理事长 / 记者（按引擎基础局默认处理）
        persons.push(MatsuriPerson::yayoi());
        persons.push(MatsuriPerson::reporter());
        // ④ 编成外神驹第 1 批：5 名固定 + 带吾妻余额 1（本剧本强制带 SSR 吾妻 → 恒 6 名）
        let mut npc_per_act = [0usize; ACT_COUNT];
        for (act, info) in self.data.acts.iter().enumerate() {
            let fixed = info.npc_chara_id;
            let chara = if used.contains(&fixed) || fixed == trained_chara {
                pick_guest_chara(&used, trained_chara, &link_chars)?
            } else {
                fixed
            };
            used.insert(chara);
            npc_per_act[act] += 1;
            persons.push(MatsuriPerson::guest(chara, act, GUEST_FIRST_TURN, false));
        }
        let balance_act = balance_guest_act(&cards_per_act, &npc_per_act);
        let balance_chara = pick_guest_chara(&used, trained_chara, &link_chars)?;
        used.insert(balance_chara);
        npc_per_act[balance_act] += 1;
        persons.push(MatsuriPerson::guest(
            balance_chara,
            balance_act,
            GUEST_FIRST_TURN,
            false,
        ));
        // W10：各演目「卡 + 第 1 批 NPC」人物数极差 >=2 → 担当分布不均衡警告
        let persons_per_act: Vec<usize> = cards_per_act
            .iter()
            .zip(npc_per_act.iter())
            .map(|(cards, npcs)| cards + npcs)
            .collect();
        let min = persons_per_act.iter().copied().min().unwrap_or(0);
        let max = persons_per_act.iter().copied().max().unwrap_or(0);
        if max >= min + 2 {
            self.deck_warnings
                .push(DeckWarning::UnevenActPersons { min, max });
        }
        // ⑤ 第 2 批：Classic 進捗発表会大成功时 +5（每演目 1 人），可用回合 48
        for act in 0..ACT_COUNT {
            let chara = pick_guest_chara(&used, trained_chara, &link_chars)?;
            used.insert(chara);
            persons.push(MatsuriPerson::guest(chara, act, GUEST_SECOND_TURN, true));
        }
        // ⑥ 落盘（按加入顺序重编 person_index）
        self.persons.clear();
        for person in persons {
            self.add_person(person);
        }
        Ok(())
    }

    /// 剧本 link 角色（5 神驹 + 吾妻）：不能作为「随机非 link 角色」候选
    fn link_chara_ids(&self) -> HashSet<u32> {
        let mut ids: HashSet<u32> = self.data.acts.iter().map(|act| act.npc_chara_id).collect();
        if let Some(friend) = self.deck.iter().find(|card| card.card_type >= 5) {
            ids.insert(friend.data.chara_id);
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每演目担当卡数
    fn act_counts(acts: &[Option<usize>]) -> [usize; ACT_COUNT] {
        let mut counts = [0usize; ACT_COUNT];
        for act in acts.iter().flatten() {
            counts[*act] += 1;
        }
        counts
    }

    #[test]
    fn card_assign_five_speed() {
        // 5 张速度卡（无 link）→ 速 2 / 毅力 2 / 耐力 1（memo §2.2(a) 算例 1）
        let types = [0, 0, 0, 0, 0];
        let links = [None; 5];
        let acts = assign_card_acts(&types, &links).expect("分配应成功");
        assert_eq!(act_counts(&acts), [2, 1, 0, 2, 0]);
    }

    #[test]
    fn card_assign_link_pins_and_dispatches() {
        // 智大和（link 钉速位）+ 3 速 + 2 毅 → 速 2 / 耐 2 / 毅 2（memo §2.2(a) 算例 2）
        let types = [4, 0, 0, 0, 3, 3];
        let links = [Some(0), None, None, None, None, None];
        let acts = assign_card_acts(&types, &links).expect("分配应成功");
        assert_eq!(acts[0], Some(0), "link 钉在固定演目");
        assert_eq!(act_counts(&acts), [2, 2, 0, 2, 0]);
    }

    #[test]
    fn card_assign_reference_deck_shape() {
        // 参考卡组形状：速 2 / 耐 1 / 力 1（link 力钉位）/ 智 1 → 速 2 / 耐 1 / 力 1 / 毅 0 / 智 1
        let types = [0, 0, 1, 2, 4];
        let links = [None, None, None, Some(2), None];
        let acts = assign_card_acts(&types, &links).expect("分配应成功");
        assert_eq!(act_counts(&acts), [2, 1, 1, 0, 1]);
    }

    #[test]
    fn guest_balance_follows_min_persons() {
        // 卡数 2/1/1/0/1 + 固定 NPC 各 1 → 毅力位 1 人最少 → 余额给毅力（并列按 速→耐→力→毅→智）
        let cards = [2, 1, 1, 0, 1];
        let npcs = [1, 1, 1, 1, 1];
        assert_eq!(balance_guest_act(&cards, &npcs), 3);
        // 全并列时按 速→耐→力→毅→智 取第一个
        assert_eq!(balance_guest_act(&[1, 1, 1, 1, 1], &npcs), 0);
    }
}
