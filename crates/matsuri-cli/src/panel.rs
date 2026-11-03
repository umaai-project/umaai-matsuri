//! 手动训练员（S8：机制核对面板）
//!
//! 用途：机制核对时由人逐决策选择。每次动作决策前打印完整面板：回合结构 / 数值与状态 /
//! 剧本资源与教诲（advice）分配（演目槽 · Lv · 教诲→転成 · 担当人物）/ 彩色训练明细表
//! （每训练一列，格内列出该位置全部人头：短名、未满 100 的羁绊、Hint 亮黄叹号；友人绿 /
//! 彩圈卡亮绿 / 普通支援卡白 / NPC 灰）/ 合宿·友人链·赛程 / 动作候选（成功收益预览、失败率）。
//! 事件选择时打印选项组与各分支数值；动作选择在交互终端用 Inquire 列表，非 TTY 退回编号输入。
//!
//! **本模块 = 手写档 / 手动档共用的「回合面板」**（2026-11-06 抽出）：`BoardPanel::render` 打印
//! 回合状态 + 剧本资源 + 训练明细 + 动作候选；手写档 explain 模式在面板后追加候选打分表。
//! 数据源纪律不变：状态一律从 `MatsuriGame::view()` 取，本文件只做呈现；`game` 仅用于
//! 只读预览（`preview_training` / `preview_action_resources`）与开局育成配置。
//!
//! 交互依赖 Inquire（仅终端）；颜色与表格复用 umaai-cli 呈现原语（term 三重门禁：非 TTY /
//! NO_COLOR 自动退化纯文本）。本模式下引擎侧 PrintFrontend 静默（Quiet），面板是唯一信息源。

use std::cell::Cell;
use std::fmt::Write as _;

use anyhow::Result;
use matsuri_core::SCENARIO_START_TURN;
use matsuri_core::data::CheckPointResult;
use matsuri_core::game::{Action, MatsuriGame, MatsuriView, MatsuriYear, PersonOrigin};
use umaai_cli::render::table::Grid;
use umaai_cli::render::term;
use umaai_core::Data;
use umasim::game::{FriendOutState, Game};

/// 训练 / 五维轴名（简称）
const AXIS_NAMES: [&str; 5] = ["速", "耐", "力", "根", "智"];

/// 演目名（官方中文 text_data_dict 462:1–5；缺静态数据时兜底）
const ACT_NAMES: [&str; 5] = ["流镝马", "神旗争夺战", "纳舞", "大曳神舆", "飞悬"];

/// 演目短名（2 字；表格 / 动作 / 槽后缀等紧凑位置用）
const ACT_SHORT_NAMES: [&str; 5] = ["流镝", "神旗", "纳舞", "大曳", "飞悬"];

/// 教诲短名兜底（官方中文 463 组去掉「教诲」后缀；缺静态数据时用）
const ADVICE_FALLBACK: [(i32, &str); 10] = [
    (101, "牵绊"),
    (201, "擅长率"),
    (301, "灵感"),
    (401, "助力"),
    (501, "体力"),
    (601, "特化"),
    (701, "研习"),
    (801, "协助"),
    (901, "不屈"),
    (1001, "兼修"),
];

/// ANSI 前景色：友人卡
const ANSI_GREEN: &str = "32";
/// ANSI 前景色：普通支援卡（白）
const ANSI_WHITE: &str = "37";
/// ANSI 前景色：彩圈支援卡 / 达成 / 亮起
const ANSI_BRIGHT_GREEN: &str = "92";
/// ANSI 前景色：支援卡 / 资源数值
pub(crate) const ANSI_BRIGHT_CYAN: &str = "96";
/// ANSI 前景色：事件名
pub(crate) const ANSI_BRIGHT_BLUE: &str = "94";
/// ANSI 前景色：研鑽（追加稽古）担当（非彩圈时）
const ANSI_BRIGHT_MAGENTA: &str = "95";
/// ANSI 前景色：助力担当（稽古触发助力支撑）
const ANSI_YELLOW: &str = "33";
/// ANSI 前景色：Hint 叹号 / 提醒
const ANSI_BRIGHT_YELLOW: &str = "93";
/// ANSI 前景色：NPC / 无数据
const ANSI_GRAY: &str = "90";
/// ANSI 前景色：负面状态 / 减量
const ANSI_RED: &str = "31";

/// ANSI 前景色包装（color = false 时原样返回）
pub(crate) fn paint(text: &str, code: Option<&str>, color: bool) -> String {
    match code {
        Some(code) if color => format!("\x1b[{code}m{text}\x1b[0m"),
        _ => text.to_string(),
    }
}

/// 按显示宽度右补空格（CJK / ANSI 由 term::text_width 计宽；超宽原样返回）
fn pad_to(text: &str, width: usize) -> String {
    let used = term::text_width(text);
    if used >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - used))
    }
}

/// 从引擎静态文本表取官方中文名；分组 / 键缺失时返回 None
fn text_name(group: &str, key: &str) -> Option<String> {
    Data::get()
        .ok()
        .and_then(|data| data.text.get(group))
        .and_then(|entries| entries.get(key))
        .cloned()
}

/// 演目名（官方中文；缺数据时用 ACT_NAMES 兜底）
fn act_name(act: usize) -> String {
    text_name("462", &(act + 1).to_string())
        .unwrap_or_else(|| ACT_NAMES.get(act).copied().unwrap_or("演目").to_string())
}

/// 演目短名（2 字；紧凑位置用）
pub(crate) fn act_short(act: usize) -> String {
    ACT_SHORT_NAMES
        .get(act)
        .copied()
        .unwrap_or("演目")
        .to_string()
}

/// 教诲兜底短名（官方文本缺失时使用）
fn advice_fallback(id: i32) -> Option<&'static str> {
    ADVICE_FALLBACK
        .iter()
        .find(|(advice_id, _)| *advice_id == id)
        .map(|(_, name)| *name)
}

/// 教诲短名（官方中文去掉「教诲」后缀；缺数据时用 advice_fallback 兜底）
pub(crate) fn advice_name(id: i32) -> String {
    if id == 0 {
        return "—".to_string();
    }
    text_name("463", &id.to_string())
        .map(|name| name.trim_end_matches("教诲").to_string())
        .unwrap_or_else(|| advice_fallback(id).map_or_else(|| format!("#{id}"), str::to_string))
}

/// 显示属性压缩：>1200 的部分显示时减半向下取整（计分仍用原始值）
///
/// 例：原始 1600 → 显示 1400；原始上限 3179 → 显示 2189。
fn display_status(value: i32) -> i32 {
    if value <= 1200 {
        value
    } else {
        1200 + (value - 1200) / 2
    }
}

/// 训练表人物颜色：彩圈亮绿 > 助力黄 > 类型底色（研鑽不再给人头上色，改标列标题）
fn member_color(origin: PersonOrigin, shining: bool, assist_bound: bool) -> Option<&'static str> {
    if origin == PersonOrigin::Card && shining {
        Some(ANSI_BRIGHT_GREEN)
    } else if assist_bound {
        Some(ANSI_YELLOW)
    } else {
        match origin {
            PersonOrigin::Card => Some(ANSI_WHITE),
            PersonOrigin::Friend => Some(ANSI_GREEN),
            PersonOrigin::Guest | PersonOrigin::Yayoi | PersonOrigin::Reporter => Some(ANSI_GRAY),
        }
    }
}

/// 增减量色调：正绿 / 负红 / 零默认
fn delta_tone(delta: i32) -> Option<&'static str> {
    match delta {
        d if d > 0 => Some(ANSI_GREEN),
        d if d < 0 => Some(ANSI_RED),
        _ => None,
    }
}

/// 训练位下标 → 演目下标（`act_trains` 反查；防御性回退同下标）
fn act_of_train(view: &MatsuriView, train: usize) -> usize {
    view.resources
        .act_trains
        .iter()
        .position(|mapped| *mapped == train)
        .unwrap_or(train)
}

/// 人头短名（呈现层按引擎静态表解析；与 `MatsuriPerson::short_name` 同口径）
fn person_short_name(person: &matsuri_core::PersonView) -> String {
    match person.origin {
        PersonOrigin::Guest => {
            let name = Data::get().ok().map_or_else(String::new, |data| {
                data.get_chara_name(person.chara_id)
                    .chars()
                    .take(2)
                    .collect()
            });
            format!("[NPC]{name}")
        }
        PersonOrigin::Yayoi => "理事长".to_string(),
        PersonOrigin::Reporter => "记者".to_string(),
        PersonOrigin::Card | PersonOrigin::Friend => {
            if let Some(card_id) = person.card_id
                && let Ok(data) = Data::get()
                && let Ok(card) = data.get_card(card_id)
            {
                return card.short_name().to_string();
            }
            let short: String = Data::get().ok().map_or_else(String::new, |data| {
                data.get_chara_name(person.chara_id)
                    .chars()
                    .take(2)
                    .collect()
            });
            format!("[???]{short}")
        }
    }
}

/// 演目槽数字文本（x/y；满槽亮绿、差一格亮黄、有进度亮青、空灰）
fn gauge_text(gauge: i32, max: i32, color: bool) -> String {
    let max = max.max(1);
    let gauge = gauge.clamp(0, max);
    let code = if gauge >= max {
        Some(ANSI_BRIGHT_GREEN)
    } else if gauge + 1 == max {
        Some(ANSI_BRIGHT_YELLOW)
    } else if gauge > 0 {
        Some(ANSI_BRIGHT_CYAN)
    } else {
        Some(ANSI_GRAY)
    };
    paint(&format!("{gauge}/{max}"), code, color)
}

/// 演目 Lv 文本（未达下次考核门槛亮黄、已达成绿；本番后默认色）
fn act_level_text(level: i32, cap: i32, need: Option<i32>, color: bool) -> String {
    let code = match need {
        Some(need) if level >= need => Some(ANSI_GREEN),
        Some(_) => Some(ANSI_BRIGHT_YELLOW),
        None => None,
    };
    paint(&format!("{level}/{cap}"), code, color)
}

/// 教诲分配格：基础教诲亮青、転成亮绿；研鑽掷中时追加亮黄叹号
fn advice_cell(base: i32, fused: i32, kensan_pending: bool, color: bool) -> String {
    if base == 0 && fused == 0 {
        return paint("—", Some(ANSI_GRAY), color);
    }
    let mut cell = if base == 0 {
        String::new()
    } else {
        paint(
            &format!("{base} {}", advice_name(base)),
            Some(ANSI_BRIGHT_CYAN),
            color,
        )
    };
    if fused != 0 {
        if !cell.is_empty() {
            cell.push_str(" → ");
        }
        cell.push_str(&paint(
            &format!("{fused} {}", advice_name(fused)),
            Some(ANSI_BRIGHT_GREEN),
            color,
        ));
        if fused == 701 && kensan_pending {
            cell.push_str(&paint(" !", Some(ANSI_BRIGHT_YELLOW), color));
        }
    }
    cell
}

/// 演目担当人物（卡 + 编成外神驹；只列当前可用者，卡亮青 / 神驹灰）
///
/// 0–1 回合剧本机制尚未开始，没有担当分配。
fn act_members(view: &MatsuriView, act: usize, color: bool) -> String {
    if !view.turn.scenario_started {
        return paint("未分配", Some(ANSI_GRAY), color);
    }
    let members: Vec<String> = view
        .board
        .act_members
        .get(act)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|index| view.board.persons.get(*index))
        .map(|person| {
            let code = if person.origin == PersonOrigin::Card {
                Some(ANSI_BRIGHT_CYAN)
            } else {
                Some(ANSI_GRAY)
            };
            paint(&person_short_name(person), code, color)
        })
        .collect();
    if members.is_empty() {
        "—".to_string()
    } else {
        members.join(" ")
    }
}

/// 单个人头在训练位的显示（类型 / 助力着色 + Hint 亮黄叹号 + 未满 100 的羁绊）
///
/// `keiko` = 该训练位本回合是否按稽古处理（助力支撑只在稽古成功时触发）。
fn person_label(
    view: &MatsuriView,
    person_index: usize,
    shining: bool,
    keiko: bool,
    color: bool,
) -> Option<String> {
    let person = view.board.persons.get(person_index)?;
    let name = person_short_name(person);
    // 编成外神驹没有羁绊面板；其余人物未满 100 时尾附数值
    let body = if person.origin != PersonOrigin::Guest && person.friendship < 100 {
        format!("{name}{}", person.friendship)
    } else {
        name
    };
    // 助力只在稽古位标记；研鑽（追加稽古）改标训练列标题，不再给人头着色
    let assist_bound = keiko && person.act.is_some() && person.act == view.resources.assist_act;
    let body = paint(
        &body,
        member_color(person.origin, shining, assist_bound),
        color,
    );
    Some(if person.hint {
        format!("{}{body}", paint("!", Some(ANSI_BRIGHT_YELLOW), color))
    } else {
        body
    })
}

/// 该训练位参与者是否含指定演目担当（研鑽 / 助力判定同口径）
fn participant_has_act(view: &MatsuriView, train: usize, act: usize) -> bool {
    view.board.distribution.get(train).is_some_and(|members| {
        members.iter().any(|member| {
            view.board
                .persons
                .get(member.person)
                .is_some_and(|person| person.act == Some(act))
        })
    })
}

/// 一个训练位的全部人头（一格内多行；无人显示 —）
fn person_cell(view: &MatsuriView, train: usize, color: bool) -> String {
    // 该列是否按稽古处理（合宿 / URA 全列；日常仅槽满列）：助力支撑只在稽古成功时触发
    let act = act_of_train(view, train);
    let keiko = view.turn.practice.get(act).copied().unwrap_or(false);
    let members: Vec<String> = view
        .board
        .distribution
        .get(train)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|member| person_label(view, member.person, member.shining, keiko, color))
        .collect();
    if members.is_empty() {
        "—".to_string()
    } else {
        members.join("\n")
    }
}

/// 発表会 / 本番结果名
fn check_point_name(result: CheckPointResult) -> &'static str {
    match result {
        CheckPointResult::NotReached => "未",
        CheckPointResult::Success => "成功",
        CheckPointResult::GreatSuccess => "大成功",
    }
}

/// 事件结果档名（ChoiceResult 的整数值）
pub(crate) fn choice_result_name(result: i32) -> &'static str {
    match result {
        0 => "普通",
        1 => "成功",
        2 => "大成功",
        3 => "失败",
        4 => "大失败",
        _ => "?",
    }
}

/// 友人出行状态名
fn out_state_name(state: FriendOutState) -> &'static str {
    match state {
        FriendOutState::UnClicked => "未点击",
        FriendOutState::BeforeUnlock => "已点击·未解锁",
        FriendOutState::AfterUnlock => "已解锁",
        FriendOutState::Away => "离开",
    }
}

/// 年次名
fn year_name(year: MatsuriYear) -> &'static str {
    year.as_str()
}

/// 比赛等级名（等级表口径：0 = 无比赛、1 = G1 … 4 = OP）
fn grade_name(grade: i32) -> &'static str {
    match grade {
        1 => "G1",
        2 => "G2",
        3 => "G3",
        4 => "OP",
        _ => "?",
    }
}

/// 阶段号 → 名字
fn stage_name(stage: u64) -> &'static str {
    match stage {
        0 => "Begin",
        1 => "Train",
        2 => "End",
        3 => "Distribute",
        4 => "AfterTrain",
        _ => "?",
    }
}

/// 交互终端（stdin / stdout 都是 TTY）
/// 动作的资源侧后缀（演目槽增减 / 排练清槽）；无资源变化时为空
fn gauge_suffix(game: &MatsuriGame, action: &Action, color: bool) -> String {
    let preview = game.preview_action_resources(action);
    let mut parts: Vec<String> = Vec::new();
    for act in 0..5 {
        let delta = preview.gauge_delta[act];
        if delta > 0 {
            parts.push(paint(
                &format!("{}{:+}", act_short(act), delta),
                Some(ANSI_BRIGHT_GREEN),
                color,
            ));
        } else if delta < 0 {
            parts.push(paint(
                &format!("{}清空", act_short(act)),
                Some(ANSI_BRIGHT_YELLOW),
                color,
            ));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("  槽 {}", parts.join(" "))
    }
}

/// 开局育成配置（育成马娘 / 卡组 / 继承配置）；manual 档跑局前由宿主打印一次
pub(crate) fn newgame_config(game: &MatsuriGame) -> String {
    let color = term::use_color(false);
    let uma = game.uma();
    let uma_name = uma
        .get_data()
        .map_or_else(|_| format!("#{}", uma.uma_id), |row| row.name.clone());
    let deck: Vec<String> = game
        .deck()
        .iter()
        .map(|card| {
            let rarity = match card.data.rarity {
                3 => "SSR",
                2 => "SR",
                _ => "R",
            };
            format!("{}◆{}({rarity})", card.data.card_name, card.rank)
        })
        .collect();
    let inherit = &game.base.inherit;
    let blue: Vec<String> = (0..5)
        .map(|axis| format!("{}{}", AXIS_NAMES[axis], inherit.blue_count[axis]))
        .collect();
    let extra: Vec<String> = (0..5)
        .map(|axis| format!("{}{}", AXIS_NAMES[axis], inherit.extra_count[axis]))
        .chain(std::iter::once(format!("PT{}", inherit.extra_count[5])))
        .collect();
    let initial = inherit.inherit_newgame();
    let initial_text: Vec<String> = (0..5)
        .map(|axis| format!("{}{:+}", AXIS_NAMES[axis], initial[axis]))
        .collect();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "════════════════════════════════════════════════════════════════"
    );
    let _ = writeln!(
        out,
        "{}",
        paint("【育成配置】", Some(ANSI_BRIGHT_CYAN), color)
    );
    let _ = writeln!(
        out,
        "  育成马娘：{}（#{}）",
        paint(&uma_name, Some(ANSI_BRIGHT_CYAN), color),
        uma.uma_id
    );
    let _ = writeln!(out, "  卡组（{} 张）：{}", deck.len(), deck.join("  "));
    let _ = writeln!(
        out,
        "  继承：蓝因子 {} ｜ 剧本因子 {}",
        blue.join(" "),
        extra.join(" ")
    );
    let _ = writeln!(out, "        开局继承：{}", initial_text.join(" "));
    out
}

/// 局面数值快照（`MatsuriView` 派生；算「上一步后变化」）
#[derive(Debug, Clone, Copy, PartialEq)]
struct Snapshot {
    turn: i32,
    five: [i32; 5],
    pt: i32,
    total_hints: i32,
    vital: i32,
    motivation: i32,
    /// 演目槽（比赛 +4 / 休息·出行 +1 / 训练参与 / 稽古清槽的落点可核对）
    act_gauge: [i32; 5],
    act_level: [i32; 5],
}

impl Snapshot {
    fn of(view: &MatsuriView) -> Self {
        Self {
            turn: view.turn.turn,
            five: view.uma.five_status,
            pt: view.uma.skill_pt,
            total_hints: view.uma.total_hints,
            vital: view.uma.vital,
            motivation: view.uma.motivation,
            act_gauge: view.resources.act_gauge,
            act_level: view.resources.act_level,
        }
    }
}

/// 回合面板（手写档 / 手动档共用）
///
/// 持有「上一步变化 / 新事件 / 教诲分配变化」的跨提示快照；对外只读（内部 `Cell`）。
pub struct BoardPanel {
    /// 上次提示时的数值快照（用于「上一步后变化」）
    last: Cell<Option<Snapshot>>,
    /// 已回显到的事件序号
    last_event_seq: Cell<u64>,
    /// 上一个动作面板所在的教诲窗口回合计（下一次面板列分配结果）
    last_advice_window: Cell<Option<i32>>,
    /// 上次面板的教诲分配状态 (slots, fused)；用于检测首次授予 / 転成
    last_advice: Cell<Option<([i32; 5], [i32; 5])>>,
    /// 是否着色（手动档固定 false；explain / 终端按需）
    color: bool,
}

impl BoardPanel {
    /// 新建面板
    pub fn new(color: bool) -> Self {
        Self {
            last: Cell::new(None),
            last_event_seq: Cell::new(0),
            last_advice_window: Cell::new(None),
            last_advice: Cell::new(None),
            color,
        }
    }

    /// 本回合类型标签（目标赛 / 発表会 / 合宿 / 教诲窗口 / 强制休息 / URA …）
    fn turn_kind(view: &MatsuriView) -> String {
        let turn = &view.turn;
        let camp = &view.camp;
        let mut tags: Vec<String> = Vec::new();
        if turn.race {
            tags.push(
                if turn.ura_race {
                    "URA决赛"
                } else {
                    "目标赛"
                }
                .to_string(),
            );
        } else if turn.self_race_available {
            tags.push("可自选比赛".to_string());
        }
        if turn.finals {
            tags.push("本番".to_string());
        } else if let Some(check_point) = turn.check_point {
            tags.push(format!("発表会{}", check_point + 1));
        }
        if camp.active {
            tags.push(format!(
                "合宿{}年{}",
                camp.year.map_or(0, |year| year + 1),
                if camp.extended {
                    "·延长"
                } else if camp.regular {
                    "·常规"
                } else {
                    ""
                }
            ));
        }
        if camp.rest {
            tags.push("合宿版休息".to_string());
        }
        if camp.level_up {
            tags.push("合宿结算·全演目+1".to_string());
        }
        if turn.advice_window {
            tags.push("教诲窗口".to_string());
        }
        if turn.forced_rest {
            tags.push("强制休息".to_string());
        }
        if turn.interlude {
            tags.push("空窗期".to_string());
        }
        if turn.ura_period {
            tags.push("URA回合".to_string());
        }
        if tags.is_empty() {
            "普通回合".to_string()
        } else {
            tags.join(" / ")
        }
    }

    /// 事件回显：打印「自上次提示以来」新结算的事件
    fn write_new_events(&self, view: &MatsuriView, out: &mut String) {
        let color = term::use_color(false);
        let last_seq = self.last_event_seq.get();
        let fresh: Vec<_> = view
            .recent_events
            .iter()
            .filter(|record| record.seq > last_seq)
            .collect();
        let Some(max_seq) = fresh.iter().map(|record| record.seq).max() else {
            return;
        };
        out.push_str("【事件】\n");
        for record in fresh {
            let detail = record
                .detail
                .as_deref()
                .filter(|detail| !detail.is_empty())
                .map_or(String::new(), |detail| format!("  {detail}"));
            let _ = writeln!(
                out,
                "  · T{} #{} {}（选项{}）{}",
                record.turn,
                record.id,
                paint(&record.name, Some(ANSI_BRIGHT_BLUE), color),
                record.choice,
                detail
            );
        }
        self.last_event_seq.set(max_seq);
    }

    /// 上一步后的数值变化（正绿 / 负红）
    fn write_delta(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let Some(last) = self.last.get() else {
            return;
        };
        let now = Snapshot::of(view);
        let range = if last.turn == now.turn {
            "本回合上一步后".to_string()
        } else {
            format!("T{} 后", last.turn)
        };
        let axes: Vec<String> = (0..5)
            .filter(|axis| now.five[*axis] != last.five[*axis])
            .map(|axis| {
                let delta = now.five[axis] - last.five[axis];
                paint(
                    &format!("{}{:+}", AXIS_NAMES[axis], delta),
                    delta_tone(delta),
                    color,
                )
            })
            .collect();
        let vital = paint(
            &format!("体力{:+}", now.vital - last.vital),
            delta_tone(now.vital - last.vital),
            color,
        );
        let motivation = paint(
            &format!("干劲{:+}", now.motivation - last.motivation),
            delta_tone(now.motivation - last.motivation),
            color,
        );
        let pt = paint(
            &format!("PT{:+}", now.pt - last.pt),
            delta_tone(now.pt - last.pt),
            color,
        );
        let mut parts: Vec<String> = vec![if axes.is_empty() {
            "五维±0".to_string()
        } else {
            axes.join(" ")
        }];
        parts.push(vital);
        parts.push(motivation);
        parts.push(pt);
        // 灵感（Hint）收益也要进入「变化」：事件 / 训练后的 hint 在 URA 期间同样发生
        if now.total_hints != last.total_hints {
            let delta = now.total_hints - last.total_hints;
            parts.push(paint(&format!("灵感{delta:+}"), delta_tone(delta), color));
        }
        let _ = writeln!(out, "【变化】{range}：{}", parts.join(" "));
        // 演目等级变化（排练成功 / 友人出行 / 合宿结算等）
        let levels: Vec<String> = (0..5)
            .filter(|act| now.act_level[*act] != last.act_level[*act])
            .map(|act| {
                paint(
                    &format!(
                        "{} {}→{}",
                        act_short(act),
                        last.act_level[act],
                        now.act_level[act]
                    ),
                    delta_tone(now.act_level[act] - last.act_level[act]),
                    color,
                )
            })
            .collect();
        if !levels.is_empty() {
            let _ = writeln!(out, "【演目Lv】{}", levels.join(" "));
        }
        // 演目槽变化：比赛 / 休息 / 出行 / 训练参与 / 稽古清槽的落点
        let gauges: Vec<String> = (0..5)
            .filter(|act| now.act_gauge[*act] != last.act_gauge[*act])
            .map(|act| {
                // 稽古成功清槽（归零）与普通增减分开表述 / 着色
                if now.act_gauge[act] == 0 {
                    paint(
                        &format!("{}清空", act_short(act)),
                        Some(ANSI_BRIGHT_YELLOW),
                        color,
                    )
                } else {
                    paint(
                        &format!(
                            "{} {}→{}",
                            act_short(act),
                            last.act_gauge[act],
                            now.act_gauge[act]
                        ),
                        delta_tone(now.act_gauge[act] - last.act_gauge[act]),
                        color,
                    )
                }
            })
            .collect();
        if !gauges.is_empty() {
            let _ = writeln!(out, "【演目槽】{}", gauges.join(" "));
        }
    }

    /// 五维 / 体力 / 状态
    fn write_uma(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let uma = &view.uma;
        let consts = Data::get().map(|data| &data.consts).ok();
        let motivation_name = consts
            .and_then(|consts| {
                consts
                    .motivation_names
                    .get(uma.motivation.max(1) as usize - 1)
            })
            .map_or("?", String::as_str);
        let motivation_code = match uma.motivation {
            5 => Some(ANSI_BRIGHT_GREEN),
            4 => Some(ANSI_GREEN),
            3 => None,
            2 => Some(ANSI_GRAY),
            _ => Some(ANSI_RED),
        };
        let vital_pct = if uma.max_vital > 0 {
            uma.vital * 100 / uma.max_vital
        } else {
            uma.vital
        };
        let vital_code = if vital_pct <= 34 {
            Some(ANSI_RED)
        } else if vital_pct <= 49 {
            Some(ANSI_BRIGHT_YELLOW)
        } else {
            Some(ANSI_GREEN)
        };
        let vital_text = paint(
            &format!("体力 {}/{}", uma.vital, uma.max_vital),
            vital_code,
            color,
        );
        let motivation_text = paint(
            &format!("{motivation_name}({})", uma.motivation),
            motivation_code,
            color,
        );
        // 显示值按游戏口径压缩（>1200 的部分减半），计分仍用原始值
        let axes: Vec<String> = (0..5)
            .map(|axis| {
                let near_limit = uma.five_status_limit[axis] > 0
                    && uma.five_status[axis] * 10 >= uma.five_status_limit[axis] * 9;
                let text = format!(
                    "{} {}/{}",
                    AXIS_NAMES[axis],
                    display_status(uma.five_status[axis]),
                    display_status(uma.five_status_limit[axis])
                );
                paint(&text, near_limit.then_some(ANSI_BRIGHT_YELLOW), color)
            })
            .collect();
        let special = if uma.special_race_stats_bonus > 0 {
            format!(
                "（铁心 五维×{:.1} / PT×{:.2}）",
                1.0 + f64::from(uma.special_race_stats_bonus) / 100.0,
                f64::from(uma.special_race_pt_rate) / 100.0
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "【数值】{vital_text}  干劲 {motivation_text}  PT {}  灵感合计 {}  比赛加成 +{}%{special}",
            uma.skill_pt, uma.total_hints, uma.race_bonus
        );
        let _ = writeln!(out, "       {}", axes.join("  "));
        let mut states: Vec<String> = Vec::new();
        let flags = uma.flags.to_string();
        if !flags.is_empty() {
            states.push(paint(&flags, Some(ANSI_RED), color));
        }
        if view.friend.fatigue_recovery {
            states.push(paint("疲劳回复", Some(ANSI_GREEN), color));
        }
        if view.turn.advice_window {
            states.push(paint("教诲窗口", Some(ANSI_BRIGHT_YELLOW), color));
        }
        if let Some(train) = uma.last_train {
            states.push(format!(
                "上次训练:{}",
                AXIS_NAMES.get(train).copied().unwrap_or("?")
            ));
        }
        // 仅显示已置状态；全部没有时不打印该行（不用「无」占位）
        if !states.is_empty() {
            let _ = writeln!(out, "【状态】{}", states.join("、"));
        }
    }

    /// 教诲结果：首次授予 / 転成 / 改派窗口后的下一次面板列出分配结果（其后紧跟剧本资源刷新）
    fn write_advice_window(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let state = (view.resources.advice_slots, view.resources.advice_fused);
        let prev_state = self.last_advice.get();
        let prev_window = self.last_advice_window.get();
        let settled = prev_window.is_some_and(|turn| view.turn.turn == turn + 1);
        let changed = prev_state.is_some_and(|prev| prev != state);
        // 变化优先归到上一个窗口；T2 的首次授予发生在 Begin（无上窗口）→ 归到本回合
        let window = if changed {
            if settled {
                prev_window
            } else {
                Some(view.turn.turn)
            }
        } else if settled {
            prev_window
        } else {
            None
        };
        if let Some(turn) = window {
            let kind = match turn {
                2 => "首次传授",
                23 | 47 => "転成",
                _ => "改派·保持",
            };
            let parts: Vec<String> = (0..5)
                .map(|act| {
                    let label = act_name(act);
                    let base = view.resources.advice_slots[act];
                    if base == 0 {
                        return format!("{label}=—");
                    }
                    let mut cell = format!(
                        "{label}={} {}",
                        base,
                        paint(&advice_name(base), Some(ANSI_BRIGHT_CYAN), color)
                    );
                    let fused = view.resources.advice_fused[act];
                    if fused != 0 {
                        cell.push_str(&format!(
                            "→{} {}",
                            fused,
                            paint(&advice_name(fused), Some(ANSI_BRIGHT_GREEN), color)
                        ));
                    }
                    cell
                })
                .collect();
            let _ = writeln!(
                out,
                "【教诲分配】T{turn} 窗口（{kind}）结果：{}",
                parts.join(" ｜ ")
            );
        }
        self.last_advice.set(Some(state));
        // 本回合是窗口 → 记下来，下一次面板列结果；本回合已列过结果则不重复记
        let store_window = view.turn.advice_window && window.is_none();
        self.last_advice_window
            .set(store_window.then_some(view.turn.turn));
    }

    /// 剧本资源：演目表（槽 / Lv / 教诲→転成 / 担当人物）+ 発表会·永久 buff·资源状态
    fn write_resources(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let res = &view.resources;
        let need = view.schedule.next_check_point;
        let need_level = need.map(|need| need.need_act_level);
        out.push_str("【剧本资源】\n");
        if !view.turn.scenario_started {
            let note = format!(
                "（剧本机制自 T{SCENARIO_START_TURN} 开始：0–1 回合无担当分配，训练等不获取演目资源）"
            );
            let _ = writeln!(out, "  {}", paint(&note, Some(ANSI_GRAY), color));
        }
        let mut grid = Grid::new(["演目", "槽", "Lv", "教诲 → 転成", "担当"]);
        for act in 0..5 {
            let train = res.act_trains.get(act).copied().unwrap_or(act);
            let name = format!(
                "{}({})",
                act_short(act),
                AXIS_NAMES.get(train).copied().unwrap_or("?")
            );
            let slot = gauge_text(res.act_gauge[act], res.act_gauge_max, color);
            let level = act_level_text(res.act_level[act], res.act_level_max, need_level, color);
            let advice = advice_cell(
                res.advice_slots[act],
                res.advice_fused[act],
                res.kensan_pending,
                color,
            );
            let members = act_members(view, act, color);
            grid.push([name, slot, level, advice, members]);
        }
        out.push_str(&grid.render(term::resolve_width(None)));
        out.push('\n');
        // 発表会 / 本番结果、下次考核门槛
        let results: Vec<String> = res
            .check_points
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let code = match result {
                    CheckPointResult::NotReached => Some(ANSI_GRAY),
                    CheckPointResult::Success => Some(ANSI_GREEN),
                    CheckPointResult::GreatSuccess => Some(ANSI_BRIGHT_GREEN),
                };
                format!(
                    "[{}]{}",
                    index + 1,
                    paint(check_point_name(*result), code, color)
                )
            })
            .collect();
        let finals = paint(
            if res.finals_done { "已" } else { "未" },
            if res.finals_done {
                Some(ANSI_GREEN)
            } else {
                Some(ANSI_GRAY)
            },
            color,
        );
        let mut line = format!("  発表会 {} · 本番 {finals}", results.join(" "));
        if let Some(need) = need {
            let reached = res
                .act_level
                .iter()
                .filter(|level| **level >= need.need_act_level)
                .count();
            let stage = if need.is_finals {
                "本番"
            } else {
                "発表会"
            };
            let ok = reached >= need.need_act_num as usize;
            let text = format!(
                " · 下次{stage} T{} 需全演目 Lv≥{}（达成 {reached}/{}）",
                need.turn, need.need_act_level, need.need_act_num
            );
            line.push_str(&paint(
                &text,
                if ok {
                    Some(ANSI_GREEN)
                } else {
                    Some(ANSI_BRIGHT_YELLOW)
                },
                color,
            ));
        }
        let _ = writeln!(out, "{line}");
        // 永久 buff / 助力支撑 / 追加稽古 / 训练 Lv 加成
        let mut extras: Vec<String> = vec![paint(
            &format!(
                "永久buff 友情+{}% 擅长率+{} 灵感+{}% 比赛+{}%",
                res.presentation_friendship,
                res.presentation_deyilv,
                res.presentation_hint_rate,
                res.presentation_race_bonus
            ),
            Some(ANSI_BRIGHT_CYAN),
            color,
        )];
        // 助力支撑：剩余次数 + 触发演目（该演目担当参与稽古成功时获得 / 重置）
        if let Some(act) = res.assist_act {
            let text = if res.assist_support_left > 0 {
                format!(
                    "助力支撑 ×{}（触发 {}）",
                    res.assist_support_left,
                    act_short(act)
                )
            } else {
                format!("助力支撑 0（{}担当稽古→+3）", act_short(act))
            };
            extras.push(paint(&text, Some(ANSI_BRIGHT_CYAN), color));
        }
        // 追加稽古：本回合掷中且存在可触发列 → 亮黄提示（训练表列标题另标紫「追加稽古」）
        // 比赛回合等必然无训练位时掷中注定作废，不提示，避免「显示可触发却没有目标」
        if res.kensan_pending && Self::kensan_columns(view).iter().any(|marked| *marked) {
            extras.push(paint("可触发追加稽古", Some(ANSI_BRIGHT_YELLOW), color));
        }
        if res.train_level_bonus.iter().any(|bonus| *bonus != 0) {
            extras.push(format!("訓練Lv加成 {:?}", res.train_level_bonus));
        }
        let _ = writeln!(out, "  {}", extras.join(" · "));
    }

    /// 本回合掷中且该训练位参与者含研鑽担当 → 列标题标「追加稽古」（与触发判定同口径）
    fn kensan_columns(view: &MatsuriView) -> [bool; 5] {
        let res = &view.resources;
        let Some(kensan_act) = res.kensan_act else {
            return [false; 5];
        };
        if !res.kensan_pending {
            return [false; 5];
        }
        std::array::from_fn(|train| participant_has_act(view, train, kensan_act))
    }

    /// 训练明细表：每训练一列（列头 = 训练名 / 排练名 + Lv；掷中时含研鑽担当的列加紫行），
    /// 格内列出该位置全部人头
    fn write_training(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let kensan = Self::kensan_columns(view);
        let headers: Vec<String> = (0..5)
            .map(|train| {
                let act = act_of_train(view, train);
                let base = if view.turn.practice[act] {
                    paint(
                        &format!("{} Lv{}", act_short(act), view.resources.act_level[act]),
                        Some(ANSI_BRIGHT_YELLOW),
                        color,
                    )
                } else {
                    format!(
                        "{} Lv{}",
                        AXIS_NAMES.get(train).copied().unwrap_or("?"),
                        view.board.train_levels[train]
                    )
                };
                // 掷中且该列参与者含研鑽担当：列标题下加一行紫色「追加稽古」
                if kensan[train] {
                    format!(
                        "{base}\n{}",
                        paint("追加稽古", Some(ANSI_BRIGHT_MAGENTA), color)
                    )
                } else {
                    base
                }
            })
            .collect();
        let mut grid = Grid::new(headers);
        let cells: Vec<String> = (0..5)
            .map(|train| person_cell(view, train, color))
            .collect();
        let widest = cells
            .iter()
            .flat_map(|cell| cell.lines())
            .map(term::text_width)
            .max()
            .unwrap_or(0);
        let padded: Vec<String> = cells
            .iter()
            .map(|cell| {
                cell.lines()
                    .map(|line| pad_to(line, widest))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        grid.push(padded);
        out.push_str("【训练】\n");
        out.push_str(&grid.render(term::resolve_width(None)));
        out.push('\n');
        // 人物配色图例（只列本局存在的教诲来源；研鑽改标列标题，不再给人物着色）
        let legend: Vec<String> = if view.resources.assist_act.is_some() {
            vec![paint("黄=助力担当（稽古触发）", Some(ANSI_YELLOW), color)]
        } else {
            Vec::new()
        };
        if !legend.is_empty() {
            let _ = writeln!(out, "  配色：{}", legend.join("  "));
        }
    }

    /// 合宿 / 友人链 / 赛程 / 编成警告
    fn write_context(&self, view: &MatsuriView, out: &mut String, color: bool) {
        let camp = &view.camp;
        if camp.active {
            let year = camp.year.unwrap_or(0);
            let total = camp.total.unwrap_or(0);
            let level_up = if camp.level_up {
                format!("  {}", paint("本回合全演目+1", Some(ANSI_GREEN), color))
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "【合宿】第{}年 {}  已计入 {}/{} 回合{level_up}",
                paint(&(year + 1).to_string(), Some(ANSI_BRIGHT_CYAN), color),
                if camp.extended {
                    "延长期"
                } else {
                    "常规窗口"
                },
                camp.counted,
                total,
            );
        }
        let friend = &view.friend;
        let out_used = friend.out_used.iter().filter(|used| **used).count();
        let next_step = friend.next_step.map_or(0, |step| step + 1);
        let next_event = friend
            .next_step
            .and_then(|step| {
                Data::get().ok().and_then(|data| {
                    data.events
                        .friend_events
                        .get(&(step + 1).to_string())
                        .map(|event| {
                            format!(
                                "（#{} {}）",
                                event.id,
                                paint(&event.name, Some(ANSI_BRIGHT_BLUE), color)
                            )
                        })
                })
            })
            .unwrap_or_default();
        let state = match friend.out_state {
            FriendOutState::AfterUnlock => paint(
                out_state_name(friend.out_state),
                Some(ANSI_BRIGHT_CYAN),
                color,
            ),
            FriendOutState::Away => paint(out_state_name(friend.out_state), Some(ANSI_GRAY), color),
            _ => out_state_name(friend.out_state).to_string(),
        };
        let outing = paint(
            &format!(
                "出行 {out_used}/5{}",
                if out_used == 5 { "（已用完）" } else { "" }
            ),
            Some(ANSI_BRIGHT_CYAN),
            color,
        );
        // 仅在有疲劳回复时附状态；没有则不显示该项
        let fatigue = if friend.fatigue_recovery {
            format!("  {}", paint("疲劳回复", Some(ANSI_GREEN), color))
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "【友人】{state}  {outing}  下一段 {next_step}{next_event}{fatigue}"
        );
        let goals = &view.schedule.goal_races;
        let goal_text = if goals.is_empty() {
            "无后续目标赛".to_string()
        } else {
            goals
                .iter()
                .enumerate()
                .map(|(index, turn)| {
                    let text = format!("T{turn}");
                    if index == 0 {
                        paint(&text, Some(ANSI_BRIGHT_YELLOW), color)
                    } else {
                        text
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let _ = writeln!(out, "【目标赛】{goal_text}");
        let races: Vec<String> = view
            .schedule
            .self_races
            .iter()
            .take(4)
            .map(|race| format!("T{}({})", race.turn, grade_name(race.grade)))
            .collect();
        let _ = writeln!(
            out,
            "【自选赛】{}",
            if races.is_empty() {
                "无".to_string()
            } else {
                races.join(" ")
            }
        );
        if !view.deck_warnings.is_empty() {
            let warnings: Vec<String> = view
                .deck_warnings
                .iter()
                .map(ToString::to_string)
                .map(|warning| paint(&format!("⚠{warning}"), Some(ANSI_BRIGHT_YELLOW), color))
                .collect();
            let _ = writeln!(out, "【编成】{}", warnings.join(" "));
        }
    }

    /// 动作短标签（训练名 / 演目名；用于动作清单左列）
    pub(crate) fn action_label(&self, action: &Action) -> String {
        match action {
            Action::Train(train) => format!(
                "{}训练",
                AXIS_NAMES.get(usize::from(*train)).copied().unwrap_or("?")
            ),
            Action::ActTrain(act) => format!("{}稽古", act_short(usize::from(*act))),
            other => other.to_string(),
        }
    }

    /// 动作是否会获得 / 重置「助力支撑」：稽古位且参与者含助力绑定演目担当
    fn assist_mark(view: &MatsuriView, action: &Action) -> bool {
        let (train, keiko) = match action {
            Action::Train(train) => (usize::from(*train), false),
            Action::ActTrain(act) => {
                let act = usize::from(*act);
                let train = view.resources.act_trains.get(act).copied().unwrap_or(act);
                let keiko = view.turn.practice.get(act).copied().unwrap_or(false);
                (train, keiko)
            }
            _ => return false,
        };
        match (keiko, view.resources.assist_act) {
            (true, Some(assist_act)) => participant_has_act(view, train, assist_act),
            _ => false,
        }
    }

    /// 单个动作的说明（训练带成功收益预览与失败率；末尾附剧本资源增量）
    fn describe_action(
        &self,
        game: &MatsuriGame,
        view: &MatsuriView,
        action: &Action,
        color: bool,
    ) -> String {
        let body = match action {
            Action::Train(train) => {
                let train = usize::from(*train);
                match game.preview_training(train, false) {
                    Ok(preview) => format!(
                        "成功: {}  失败率 {}",
                        preview.value,
                        paint(
                            &format!("{:.1}%", preview.failure_rate),
                            (preview.failure_rate > 0.0).then_some(ANSI_BRIGHT_YELLOW),
                            color
                        )
                    ),
                    Err(err) => format!("预览失败: {err}"),
                }
            }
            Action::ActTrain(act) => {
                let act = usize::from(*act);
                let train = view.resources.act_trains.get(act).copied().unwrap_or(act);
                let level = view.resources.act_level.get(act).copied().unwrap_or(0);
                match game.preview_training(train, true) {
                    Ok(preview) => format!(
                        "Lv{}  成功: {}  失败率 {}",
                        level,
                        preview.value,
                        paint(
                            &format!("{:.1}%", preview.failure_rate),
                            (preview.failure_rate > 0.0).then_some(ANSI_BRIGHT_YELLOW),
                            color
                        )
                    ),
                    Err(err) => format!("预览失败: {err}"),
                }
            }
            Action::Race => {
                if view.turn.ura_race {
                    "URA 决赛（奖励按引擎）".to_string()
                } else if view.turn.race {
                    let special = if view.uma.special_race_stats_bonus > 0 {
                        format!(
                            "（铁心 五维×{:.1} / PT×{:.2}）",
                            1.0 + f64::from(view.uma.special_race_stats_bonus) / 100.0,
                            f64::from(view.uma.special_race_pt_rate) / 100.0
                        )
                    } else {
                        String::new()
                    };
                    format!("目标赛（比赛加成 +{}%{special}）", view.uma.race_bonus)
                } else {
                    format!("自选比赛（比赛加成 +{}%）", view.uma.race_bonus)
                }
            }
            Action::Rest => {
                if view.camp.rest {
                    "合宿版休息（体力 +40 / 干劲 +1）".to_string()
                } else {
                    "普通休息（体力回复；不改干劲）".to_string()
                }
            }
            Action::NormalOuting => "普通出行（随机出行事件：体力 / 属性 / 干劲）".to_string(),
            Action::FriendOuting => {
                let step = view.friend.next_step.map_or(0, |step| step + 1);
                let name =
                    view.friend
                        .next_step
                        .and_then(|step| {
                            Data::get().ok().and_then(|data| {
                                data.events.friend_events.get(&(step + 1).to_string()).map(
                                    |event| {
                                        format!(
                                            "：#{} {}",
                                            event.id,
                                            paint(&event.name, Some(ANSI_BRIGHT_BLUE), color)
                                        )
                                    },
                                )
                            })
                        })
                        .unwrap_or_default();
                format!("友人出行第 {step} 段{name}（最低演目槽 Lv+1 + 段事件）")
            }
            Action::Clinic => "治病（体力 +20；清空生病位与不擅长训练）".to_string(),
            Action::AssignAdvice(assign) => format!(
                "教诲分配方案（各演目 → 第几号槽）：{}",
                assign.slots().map(|slot| slot.to_string()).join(" ")
            ),
        };
        let mut suffix = gauge_suffix(game, action, color);
        if Self::assist_mark(view, action) {
            suffix.push_str(&paint("  助力支撑✓", Some(ANSI_YELLOW), color));
        }
        format!("{body}{suffix}")
    }

    /// 打印局面面板 + 动作候选
    /// 渲染一次回合面板（`scores` = 候选动作评分；手写档 explain 时追加呈现）
    pub(crate) fn render(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        scores: Option<&[(usize, f64)]>,
    ) -> Result<String> {
        let color = self.color;
        let view = game.view();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "════════════════════════════════════════════════════════════════"
        );
        let _ = writeln!(
            out,
            "T{}/{} {}  阶段 {}({})  类型: {}",
            view.turn.turn,
            view.turn.max_turn,
            year_name(view.turn.year),
            view.turn.stage.get(),
            stage_name(view.turn.stage.get()),
            Self::turn_kind(&view),
        );
        self.write_new_events(&view, &mut out);
        self.write_delta(&view, &mut out, color);
        self.write_uma(&view, &mut out, color);
        self.write_advice_window(&view, &mut out, color);
        self.write_resources(&view, &mut out, color);
        self.write_training(&view, &mut out, color);
        self.write_context(&view, &mut out, color);
        out.push_str("【动作】\n");
        for (idx, action) in actions.iter().enumerate() {
            let _ = writeln!(
                out,
                "  [{idx}] {}  {}",
                self.action_label(action),
                self.describe_action(game, &view, action, color)
            );
        }

        // 候选打分表（仅 explain 模式）：与手写打分同源，按分值降序列出
        if let Some(scores) = scores {
            out.push_str("【候选打分】\n");
            let mut sorted: Vec<&(usize, f64)> = scores.iter().collect();
            sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
            for (index, value) in sorted {
                let action = actions
                    .get(*index)
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let text = if value.is_finite() {
                    format!("{value:.1}")
                } else {
                    "—（不参与）".to_string()
                };
                let _ = writeln!(out, "  [{index}] {action} 值={text}");
            }
        }

        // 更新面板状态：下次提示显示本步之后的变化 / 新事件
        self.last.set(Some(Snapshot::of(&view)));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paint_respects_switch() {
        assert_eq!(paint("x", Some(ANSI_RED), false), "x", "关闭着色应原样返回");
        assert_eq!(paint("x", None, true), "x", "无码应原样返回");
        let painted = paint("x", Some(ANSI_RED), true);
        assert!(
            painted.contains('x') && painted.contains('\x1b'),
            "上色应保留原文: {painted:?}"
        );
    }

    #[test]
    fn test_pad_to_counts_display_width() {
        assert_eq!(pad_to("ab", 4), "ab  ");
        assert_eq!(pad_to("速度", 5), "速度 ", "CJK 按两列计宽");
        assert_eq!(pad_to("abc", 1), "abc", "超宽原样返回");
    }

    #[test]
    fn test_gauge_text_shape() {
        assert_eq!(gauge_text(0, 5, false), "0/5");
        assert_eq!(gauge_text(3, 5, false), "3/5");
        assert_eq!(gauge_text(5, 5, false), "5/5");
        assert_eq!(gauge_text(9, 5, false), "5/5", "越界按上限夹取");
        assert_eq!(gauge_text(2, 0, false), "1/1", "上限为 0 时按 1 处理");
    }

    #[test]
    fn test_advice_fallback_names() {
        assert_eq!(advice_fallback(101), Some("牵绊"));
        assert_eq!(advice_fallback(1001), Some("兼修"));
        assert_eq!(advice_fallback(999), None);
    }

    #[test]
    fn test_delta_tone() {
        assert_eq!(delta_tone(3), Some(ANSI_GREEN));
        assert_eq!(delta_tone(-3), Some(ANSI_RED));
        assert_eq!(delta_tone(0), None);
    }

    #[test]
    fn test_display_status_compression() {
        assert_eq!(display_status(0), 0);
        assert_eq!(display_status(1199), 1199, "1200 以下不变");
        assert_eq!(display_status(1200), 1200);
        assert_eq!(display_status(1201), 1200, "超出部分向下取整");
        assert_eq!(display_status(1600), 1400, "1600 显示 1400");
        assert_eq!(display_status(3179), 2189, "上限 3179 显示 2189");
    }

    #[test]
    fn test_member_color_priority() {
        // 彩圈最高：即使命中助力也保持亮绿
        assert_eq!(
            member_color(PersonOrigin::Card, true, true),
            Some(ANSI_BRIGHT_GREEN)
        );
        // 助力黄 > 类型底色（研鑽不再给人头上色）
        assert_eq!(
            member_color(PersonOrigin::Card, false, true),
            Some(ANSI_YELLOW)
        );
        assert_eq!(
            member_color(PersonOrigin::Card, false, false),
            Some(ANSI_WHITE)
        );
        assert_eq!(
            member_color(PersonOrigin::Friend, false, false),
            Some(ANSI_GREEN)
        );
        assert_eq!(
            member_color(PersonOrigin::Guest, false, false),
            Some(ANSI_GRAY)
        );
    }
}
