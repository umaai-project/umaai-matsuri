//! 手动训练员（S8：机制核对面板）
//!
//! 用途：机制核对时由人逐决策选择。每次需要决策前打印完整面板：回合结构 / 五维与状态 /
//! 演目与教诲 / 発表会与永久 buff / 合宿 / 友人链 / 事件回显 / 人头分布 / 动作候选（带训练预览
//! 与失败率）。事件选择时打印选项组与各分支数值。
//!
//! 只依赖标准输入输出（无颜色、无终端重依赖）；颜色与表格归终端前端。本模式下引擎侧
//! `PrintFrontend` 静默（`Verbosity::Quiet`），面板是唯一信息源。

use std::cell::Cell;
use std::fmt::Write as _;
use std::io::{self, Write};

use anyhow::{Context, Result, anyhow};
use matsuri_core::MAX_TURN;
use matsuri_core::data::CheckPointResult;
use matsuri_core::game::{Action, MatsuriGame, PersonOrigin};
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{FriendOutState, Game, Person, Trainer};

/// 训练 / 五维轴名（简称）
const AXIS_NAMES: [&str; 5] = ["速", "耐", "力", "根", "智"];

/// 教诲 id → 中文简称
fn advice_name(id: i32) -> &'static str {
    match id {
        101 => "絆",
        201 => "得意率",
        301 => "灵感",
        401 => "助力",
        501 => "体力",
        601 => "特化",
        701 => "研钻",
        801 => "协力",
        901 => "不屈",
        1001 => "両道",
        _ => "—",
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

/// 事件结果档名（`ChoiceResult` 的整数值）
fn choice_result_name(result: i32) -> &'static str {
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
fn year_name(turn: i32) -> &'static str {
    match turn {
        0..=23 => "Junior",
        24..=47 => "Classic",
        48..=MAX_TURN => "Senior",
        _ => "?",
    }
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

/// 局面数值快照（算「上一步后变化」）
#[derive(Debug, Clone, Copy, PartialEq)]
struct Snapshot {
    turn: i32,
    five: [i32; 5],
    pt: i32,
    vital: i32,
    motivation: i32,
}

impl Snapshot {
    fn of(game: &MatsuriGame) -> Self {
        let uma = game.uma();
        Self {
            turn: game.turn(),
            five: uma.five_status,
            pt: uma.skill_pt,
            vital: uma.vital,
            motivation: uma.motivation,
        }
    }
}

/// 手动训练员：面板状态（跨提示保留）
pub struct ManualTrainer {
    /// 上次提示时的数值快照（用于「上一步后变化」）
    last: Cell<Option<Snapshot>>,
    /// 已回显到的事件序号
    last_event_seq: Cell<u64>,
}

impl ManualTrainer {
    /// 新建手动训练员
    pub fn new() -> Self {
        Self {
            last: Cell::new(None),
            last_event_seq: Cell::new(0),
        }
    }

    /// 读取一个落在 `0..len` 的下标；空行重试，`q` 退出
    fn read_index(prompt: &str, len: usize) -> Result<usize> {
        let stdin = io::stdin();
        loop {
            print!("{prompt}");
            io::stdout().flush().context("刷新标准输出失败")?;
            let mut line = String::new();
            if stdin.read_line(&mut line).context("读取标准输入失败")? == 0 {
                return Err(anyhow!("标准输入已关闭"));
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            if text.eq_ignore_ascii_case("q") {
                return Err(anyhow!("手动训练员退出"));
            }
            match text.parse::<usize>() {
                Ok(idx) if idx < len => return Ok(idx),
                _ => println!(
                    "无效输入：请输入 0..{} 之间的编号（q 退出）",
                    len.saturating_sub(1)
                ),
            }
        }
    }

    /// 本回合类型标签（目标赛 / 発表会 / 合宿 / 教诲窗口 / 强制休息 / URA …）
    fn turn_kind(game: &MatsuriGame, actions: &[Action]) -> String {
        let plan = &game.plan;
        let mut tags: Vec<String> = Vec::new();
        if plan.race {
            tags.push(
                if plan.ura_race {
                    "URA决赛"
                } else {
                    "目标赛"
                }
                .to_string(),
            );
        } else if actions.iter().any(|action| matches!(action, Action::Race)) {
            tags.push("可自选比赛".to_string());
        }
        if plan.finals() {
            tags.push("本番".to_string());
        } else if let Some(check_point) = plan.check_point {
            tags.push(format!("発表会{}", check_point + 1));
        }
        if plan.camp_zone {
            tags.push(format!(
                "合宿{}年{}",
                plan.camp_year.map_or(0, |year| year + 1),
                if plan.camp_extended {
                    "·延长"
                } else if plan.camp_regular {
                    "·常规"
                } else {
                    ""
                }
            ));
        }
        if plan.camp_rest {
            tags.push("合宿版休息".to_string());
        }
        if plan.camp_level_up {
            tags.push("合宿结算·全演目+1".to_string());
        }
        if plan.advice_window.is_some() {
            tags.push("教诲窗口".to_string());
        }
        if game.forced_rest {
            tags.push("强制休息".to_string());
        }
        if plan.ura_period {
            tags.push("URA收尾".to_string());
        }
        if tags.is_empty() {
            "普通回合".to_string()
        } else {
            tags.join(" / ")
        }
    }

    /// 事件回显：打印「自上次提示以来」新结算的事件
    fn write_new_events(&self, game: &MatsuriGame, out: &mut String) {
        let last_seq = self.last_event_seq.get();
        let fresh: Vec<_> = game
            .event_log
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
                record.turn, record.id, record.name, record.choice, detail
            );
        }
        self.last_event_seq.set(max_seq);
    }

    /// 上一步后的数值变化
    fn write_delta(&self, game: &MatsuriGame, out: &mut String) {
        let Some(last) = self.last.get() else {
            return;
        };
        let now = Snapshot::of(game);
        let range = if last.turn == now.turn {
            "本回合上一步后".to_string()
        } else {
            format!("T{} 后", last.turn)
        };
        let axes: Vec<String> = (0..5)
            .filter(|axis| now.five[*axis] != last.five[*axis])
            .map(|axis| format!("{}{:+}", AXIS_NAMES[axis], now.five[axis] - last.five[axis]))
            .collect();
        let _ = writeln!(
            out,
            "【变化】{range}：{} 体力{:+} 干劲{:+} PT{:+}",
            if axes.is_empty() {
                "五维±0".to_string()
            } else {
                axes.join(" ")
            },
            now.vital - last.vital,
            now.motivation - last.motivation,
            now.pt - last.pt
        );
    }

    /// 五维 / 体力 / 状态
    fn write_uma(&self, game: &MatsuriGame, out: &mut String) {
        let data = &game.data;
        let uma = game.uma();
        let consts = Data::get().map(|data| &data.consts).ok();
        let motivation = consts
            .and_then(|consts| {
                consts
                    .motivation_names
                    .get(uma.motivation.max(1) as usize - 1)
            })
            .map_or("?", String::as_str);
        let axes: Vec<String> = (0..5)
            .map(|axis| {
                format!(
                    "{} {}/{}",
                    AXIS_NAMES[axis], uma.five_status[axis], uma.five_status_limit[axis]
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "【数值】体力 {}/{}  干劲 {}({})  PT {}  灵感合计 {}  比赛加成 +{}%",
            uma.vital,
            uma.max_vital,
            motivation,
            uma.motivation,
            uma.skill_pt,
            uma.total_hints,
            uma.race_bonus
        );
        let _ = writeln!(out, "       {}", axes.join("  "));
        let mut states: Vec<String> = Vec::new();
        let flags = uma.flags.to_string();
        if !flags.is_empty() {
            states.push(flags);
        }
        if game.fatigue_recovery {
            states.push("疲劳回复".to_string());
        }
        if game.kensan_pending {
            states.push("追加稽古待触发".to_string());
        }
        if game.assist_support_left > 0 {
            states.push(format!("助力支撑×{}", game.assist_support_left));
        }
        if game.plan.advice_window.is_some() {
            states.push("教诲窗口".to_string());
        }
        if let Some(train) = game.base.last_train {
            states.push(format!(
                "上次训练:{}",
                AXIS_NAMES.get(train).copied().unwrap_or("?")
            ));
        }
        if data.camp.windows.len() != 3 {
            states.push("合宿窗口数据异常".to_string());
        }
        let _ = writeln!(
            out,
            "【状态】{}",
            if states.is_empty() {
                "无".to_string()
            } else {
                states.join("、")
            }
        );
    }

    /// 演目（gauge / Lv / 亮起 / 绑定教诲）与発表会 / 永久 buff
    fn write_acts(&self, game: &MatsuriGame, out: &mut String) {
        let view = game.view();
        let rules = &game.data.rules;
        out.push_str("【演目】");
        for act in 0..5 {
            let level = view.act_level[act];
            let gauge = view.act_gauge[act];
            let base = view.advice_slots[act];
            let fused = view.advice_fused[act];
            let _ = write!(
                out,
                "\n  [{act}] {}稽古  Lv{}/{}  槽 {}/{}  {}",
                AXIS_NAMES[game.data.acts[act].train],
                level,
                view.act_level_max,
                gauge,
                rules.act_gauge_max,
                if gauge >= rules.act_gauge_max {
                    "亮起"
                } else {
                    "—"
                },
            );
            let _ = write!(
                out,
                "  教诲 {}({})",
                if base == 0 {
                    "—".to_string()
                } else {
                    base.to_string()
                },
                advice_name(base)
            );
            if fused != 0 {
                let _ = write!(out, " + 転成 {}({})", fused, advice_name(fused));
            }
        }
        out.push('\n');
        let _ = writeln!(
            out,
            "【発表会】[1]{} [2]{} [3]{}  本番 {}  永久buff 友情+{}% 得意+{}(含基线) 灵感+{}% 比赛+{}%  訓練Lv {:?}",
            check_point_name(view.check_points[0]),
            check_point_name(view.check_points[1]),
            check_point_name(view.check_points[2]),
            if view.finals_done { "已" } else { "未" },
            view.presentation_friendship,
            view.presentation_deyilv,
            view.presentation_hint_rate,
            view.presentation_race_bonus,
            view.train_level_bonus
        );
    }

    /// 合宿 / 友人链 / 赛程 / 编成警告
    fn write_context(&self, game: &MatsuriGame, out: &mut String) {
        if game.plan.camp_zone {
            let year = game.plan.camp_year.unwrap_or(0);
            let total = game
                .data
                .camp
                .windows
                .get(year)
                .map_or(0, |window| window.end - window.start + 1);
            let _ = writeln!(
                out,
                "【合宿】第{}年 {}  已计入 {}/{} 回合{}",
                year + 1,
                if game.plan.camp_extended {
                    "延长期"
                } else {
                    "常规窗口"
                },
                game.camp.counted,
                total,
                if game.plan.camp_level_up {
                    "  本回合全演目+1"
                } else {
                    ""
                }
            );
        } else {
            let _ = writeln!(out, "【合宿】非合宿回合");
        }
        let out_used = game.friend.out_used.iter().filter(|used| **used).count();
        let next_step = game
            .friend
            .out_used
            .iter()
            .position(|used| !*used)
            .map_or(0, |step| step + 1);
        let next_event = if next_step == 0 {
            String::new()
        } else {
            Data::get()
                .ok()
                .and_then(|data| data.events.friend_events.get(&next_step.to_string()))
                .map_or(String::new(), |event| {
                    format!("（#{} {}）", event.id, event.name)
                })
        };
        let _ = writeln!(
            out,
            "【友人】{}  出行 {}/5{}  下一段 {}{}  疲劳回复 {}",
            out_state_name(game.friend.out_state),
            out_used,
            if out_used == 5 { "（已用完）" } else { "" },
            next_step,
            next_event,
            if game.fatigue_recovery { "有" } else { "无" }
        );
        let goals: Vec<String> = (game.turn()..=MAX_TURN)
            .filter(|turn| game.uma().is_race_turn(*turn))
            .map(|turn| format!("T{turn}"))
            .collect();
        let _ = writeln!(
            out,
            "【目标赛】{}",
            if goals.is_empty() {
                "无后续目标赛".to_string()
            } else {
                goals.join(" ")
            }
        );
        let races: Vec<String> = (game.turn()..=MAX_TURN)
            .filter(|turn| !game.uma().is_race_turn(*turn))
            .filter_map(|turn| {
                let grade = *game.race_grades.get(turn as usize)?;
                (grade > 0).then(|| format!("T{turn}({})", grade_name(grade)))
            })
            .take(4)
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
        if !game.deck_warnings.is_empty() {
            let warnings: Vec<String> = game
                .deck_warnings
                .iter()
                .map(ToString::to_string)
                .map(|warning| format!("⚠{warning}"))
                .collect();
            let _ = writeln!(out, "【编成】{}", warnings.join(" "));
        }
    }

    /// 人头分布（按训练位）
    fn write_distribution(&self, game: &MatsuriGame, out: &mut String) {
        out.push_str("【分布】\n");
        let distribution = game.distribution();
        for (train, axis) in AXIS_NAMES.iter().enumerate() {
            let members: Vec<String> = distribution
                .get(train)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .filter(|index| **index >= 0)
                .filter_map(|index| game.persons().get(*index as usize))
                .map(|person| {
                    let star = match person.origin {
                        PersonOrigin::Card => "",
                        PersonOrigin::Friend => "★",
                        PersonOrigin::Guest => "神",
                        PersonOrigin::Yayoi => "理",
                        PersonOrigin::Reporter => "记",
                    };
                    format!(
                        "{}{}{}(羁绊{})",
                        star,
                        person.short_name(),
                        if person.hint() { "●" } else { "" },
                        person.friendship()
                    )
                })
                .collect();
            let _ = writeln!(
                out,
                "  {}训练位 Lv{}(计数{})（{}人）：{}",
                axis,
                game.train_level(train),
                game.base.train_level_count[train],
                members.len(),
                if members.is_empty() {
                    "空".to_string()
                } else {
                    members.join(" ")
                }
            );
        }
        let others: Vec<String> = game
            .persons()
            .iter()
            .filter(|person| matches!(person.origin, PersonOrigin::Yayoi | PersonOrigin::Reporter))
            .map(|person| format!("{}(羁绊{})", person.short_name(), person.friendship()))
            .collect();
        if !others.is_empty() {
            let _ = writeln!(out, "  其他：{}", others.join(" "));
        }
    }

    /// 单个动作的说明（训练带成功收益预览与失败率）
    fn describe_action(&self, game: &MatsuriGame, action: &Action) -> String {
        match action {
            Action::Train(train) => {
                let train = usize::from(*train);
                let name = AXIS_NAMES.get(train).copied().unwrap_or("?");
                match game.preview_training(train, false) {
                    Ok(preview) => format!(
                        "{name}训练  成功: {}  失败率 {:.1}%",
                        preview.value, preview.failure_rate
                    ),
                    Err(err) => format!("{name}训练（预览失败: {err}）"),
                }
            }
            Action::ActTrain(act) => {
                let act = usize::from(*act);
                let train = game.data.acts.get(act).map_or(act, |act| act.train);
                let level = game.view().act_level.get(act).copied().unwrap_or(0);
                match game.preview_training(train, true) {
                    Ok(preview) => format!(
                        "演目{}（{}稽古 Lv{}）  成功: {}  失败率 {:.1}%",
                        act + 1,
                        AXIS_NAMES.get(train).copied().unwrap_or("?"),
                        level,
                        preview.value,
                        preview.failure_rate
                    ),
                    Err(err) => format!("演目{act}稽古（预览失败: {err}）"),
                }
            }
            Action::Race => {
                let uma = game.uma();
                if game.plan.ura_race {
                    "URA 决赛（奖励按引擎）".to_string()
                } else if game.plan.race {
                    format!("目标赛（比赛加成 +{}%）", uma.race_bonus)
                } else {
                    format!("自选比赛（比赛加成 +{}%）", uma.race_bonus)
                }
            }
            Action::Rest => {
                if game.plan.camp_rest {
                    "合宿版休息（体力 +40 / 干劲 +1）".to_string()
                } else {
                    "普通休息（体力回复；不改干劲）".to_string()
                }
            }
            Action::NormalOuting => "普通出行（随机出行事件：体力 / 属性 / 干劲）".to_string(),
            Action::FriendOuting => {
                let step = game
                    .friend
                    .out_used
                    .iter()
                    .position(|used| !*used)
                    .map_or(0, |step| step + 1);
                let name = Data::get()
                    .ok()
                    .and_then(|data| data.events.friend_events.get(&step.to_string()))
                    .map_or(String::new(), |event| {
                        format!("：#{} {}", event.id, event.name)
                    });
                format!("友人出行第 {step} 段{name}（最低演目槽 Lv+1 + 段事件）")
            }
            Action::Clinic => "治病（体力 +20；清空生病位与不擅长训练）".to_string(),
        }
    }

    /// 打印局面面板 + 动作候选（纯文本）
    fn show_board(&self, game: &MatsuriGame, actions: &[Action]) -> Result<String> {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "════════════════════════════════════════════════════════════════"
        );
        let _ = writeln!(
            out,
            "T{}/{} {}  阶段 {}({})  类型: {}",
            game.turn(),
            game.max_turn(),
            year_name(game.turn()),
            game.stage().get(),
            stage_name(game.stage().get()),
            Self::turn_kind(game, actions),
        );
        self.write_new_events(game, &mut out);
        self.write_delta(game, &mut out);
        self.write_uma(game, &mut out);
        self.write_acts(game, &mut out);
        self.write_context(game, &mut out);
        self.write_distribution(game, &mut out);
        out.push_str("【动作】\n");
        for (idx, action) in actions.iter().enumerate() {
            let _ = writeln!(
                out,
                "  [{idx}] {:<6} {}",
                action.to_string(),
                self.describe_action(game, action)
            );
        }
        out.push_str("（输入编号选择；q 退出）\n");
        // 更新面板状态：下次提示显示本步之后的变化 / 新事件
        self.last.set(Some(Snapshot::of(game)));
        Ok(out)
    }

    /// 打印事件选项组（每组 = 一个可选分支，组内是该分支的可能结果）
    fn show_choices(event: Option<&EventData>, choices: &[Vec<EventChoice>]) -> String {
        let mut out = String::new();
        match event {
            Some(event) => {
                let _ = writeln!(
                    out,
                    "──────── 事件 #{} {}  [{}] ────────",
                    event.id,
                    event.name,
                    if event.player_select {
                        "需选择"
                    } else {
                        "自动"
                    }
                );
            }
            None => out.push_str("──────── 事件选项 ────────\n"),
        }
        for (idx, group) in choices.iter().enumerate() {
            let _ = write!(out, "  [{idx}]");
            if group.is_empty() {
                out.push_str(" （空组）");
            }
            for choice in group {
                let _ = write!(
                    out,
                    "  {}%",
                    if choice.prob == 0 { 100 } else { choice.prob }
                );
                if choice.result != 0 {
                    let _ = write!(out, "[{}]", choice_result_name(choice.result));
                }
                let value = choice.value.to_string();
                if !value.is_empty() {
                    let _ = write!(out, " {value}");
                }
                if let Some(extra) = &choice.extra_status {
                    let _ = write!(out, " 额外[{extra}]");
                }
                if let Some(flags) = &choice.add_flags
                    && !flags.to_string().is_empty()
                {
                    let _ = write!(out, " 状态+[{}]", flags);
                }
                if let Some(flags) = &choice.remove_flags
                    && !flags.to_string().is_empty()
                {
                    let _ = write!(out, " 状态-[{}]", flags);
                }
            }
            out.push('\n');
        }
        out
    }
}

impl Default for ManualTrainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer<MatsuriGame> for ManualTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        println!("{}", self.show_board(game, actions)?);
        Self::read_index("> 选择动作编号: ", actions.len())
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        println!("{}", Self::show_choices(None, choices));
        Self::read_index("> 选择事件选项编号: ", choices.len())
    }

    fn select_event_choice(
        &self,
        _game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        println!("{}", Self::show_choices(Some(event), choices));
        Self::read_index("> 选择事件选项编号: ", choices.len())
    }
}
