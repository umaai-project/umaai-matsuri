//! 帧 diff 反推：教诲落位 / 転成 / 学技能 / 追加稽古 / 实际动作
//!
//! 只读帧与终局 `actions`，不重算估值。口径「效果可核、过程不评」：只判生效结果
//! （落在哪个 / 融合成什么 / 花了多少点），不判操作序列。
//! 训练位由**帧内预览与前后帧增量**反查，不依赖命令编码表（编码语义未经证实）。

use matsuri_core::restore_displayed_gain;
use serde::Serialize;
use umaai::protocol::final_score::FinalAction;

/// 「追加の稽古」事件 id（`400015106`，研鑽演目触发）
pub const KENSAN_EVENT_ID: i32 = 400015106;

/// 普通训练命令区间（`101–106`；真机实测 106 亦为训练类）
const TRAIN_CMD_MIN: i32 = 101;
/// 普通训练命令上限
const TRAIN_CMD_MAX: i32 = 106;
/// 演目稽古命令下限（`5101–5105`，演目下标 = 命令 − 下限）
const KEIKO_CMD_MIN: i32 = 5101;
/// 演目稽古命令上限
const KEIKO_CMD_MAX: i32 = 5105;
/// 训练预览的轴数（速 / 耐 / 力 / 根 / 智 / 技能点 / 体力）
const AXES: usize = 7;

/// 一帧的取数快照（由 `facts` 组装，反推层只读）
#[derive(Debug, Clone)]
pub struct TraceRow {
    /// 帧文件名
    pub name: String,
    /// 回合（0 基）
    pub turn: i32,
    /// 帧内 `isAfterTraining`（真机会粘连，只作参考）
    pub is_after_training: bool,
    /// 帧来源（`command` / `event` / `special` / `load`）
    pub source: String,
    /// 帧内 `storyId`（事件帧才有）
    pub story_id: Option<i32>,
    /// 帧内剩余技能点**原值**（不是本局累计获得）
    pub skill_pt: i32,
    /// 体力
    pub vital: i32,
    /// 干劲 `[1, 5]`
    pub motivation: i32,
    /// 五维（帧原值，≥1200 时为折半表示值）
    pub five: [i32; 5],
    /// 各演目 Lv
    pub act_level: [i32; 5],
    /// 教诲槽（0 = 未分配）
    pub advice: [i32; 5],
    /// 転成叠加（0 = 未転成）
    pub fusion: [i32; 5],
    /// 本回合按排练处理的演目
    pub practice: [bool; 5],
    /// 研鑽绑定演目（未転成 701 时为 `None`）
    pub kensan_act: Option<usize>,
    /// 本回合是否延长期合宿
    pub camp_extended: bool,
    /// 本回合進捗発表会场次（0–2）
    pub check_point: Option<usize>,
    /// 已用友人出行段数
    pub out_used: i32,
    /// 帧内训练预览
    pub training: Vec<TrainPreview>,
    /// 帧内继承增量（累计数组）
    pub inherit_gains: Vec<i32>,
}

/// 帧内一条训练预览（`baseGame.training{command_id}`）
#[derive(Debug, Clone)]
pub struct TrainPreview {
    /// 命令 id（`101–106` / `5101–5105`）
    pub cmd: String,
    /// 失败率（协议原值）
    pub failure_rate: f32,
    /// 7 项总增量（下层 + 上层，未做 1200 衰减）
    pub total: [i32; AXES],
}

/// 反推总表
#[derive(Debug, Default, Serialize)]
pub struct Derive {
    /// 教诲落位（每个演目首现新 id 一条）
    pub advice: Vec<AdviceSlot>,
    /// 転成记录
    pub fusion: Vec<FusionRecord>,
    /// 学技能（剩余点下降）
    pub skill: SkillReport,
    /// 追加稽古
    pub kensan: KensanReport,
    /// 实际动作序列（终局帧 `actions`）
    pub decisions: Vec<DecisionRow>,
    /// 训练回合（含预览匹配）
    pub training: Vec<TrainRecord>,
    /// 继承（逐次）
    pub inherit: Vec<InheritStep>,
}

/// 一条教诲落位
#[derive(Debug, Serialize)]
pub struct AdviceSlot {
    /// 演目下标（0–4）
    pub act: usize,
    /// 回合
    pub turn: i32,
    /// 教诲 id
    pub advice_id: i32,
    /// 变更前的 id（0 = 首现）
    pub prev: i32,
}

/// 一条転成记录
#[derive(Debug, Serialize)]
pub struct FusionRecord {
    /// 演目下标
    pub act: usize,
    /// 回合
    pub turn: i32,
    /// 基础教诲 id（槽身份）
    pub base: i32,
    /// 転成后 id
    pub target: i32,
}

/// 学技能汇总
#[derive(Debug, Default, Serialize)]
pub struct SkillReport {
    /// 终局帧 `skillPtSpent`（本局累计花掉）
    pub spent_total: i32,
    /// 剩余点下降的回合
    pub events: Vec<SkillSpend>,
}

/// 一次剩余技能点下降
#[derive(Debug, Serialize)]
pub struct SkillSpend {
    /// 回合
    pub turn: i32,
    /// 下降前剩余点
    pub from: i32,
    /// 下降后剩余点
    pub to: i32,
}

/// 追加稽古汇总
#[derive(Debug, Default, Serialize)]
pub struct KensanReport {
    /// 触发次数
    pub triggers: usize,
    /// 选 1「加练」
    pub executed: usize,
    /// 选 2「休息」
    pub wasted: usize,
    /// 选择不可得（actions 未采集该事件）
    pub rested: usize,
    /// 明细
    pub items: Vec<KensanItem>,
}

/// 一次追加稽古触发
#[derive(Debug, Serialize)]
pub struct KensanItem {
    /// 回合
    pub turn: i32,
    /// 研鑽绑定演目
    pub act: Option<usize>,
    /// 玩家选择（`1` 加练 / `2` 休息；`None` = actions 缺记录）
    pub choice: Option<i32>,
    /// 是否真的走了该演目稽古（`None` = 后续动作不可判）
    pub executed: Option<bool>,
    /// 判定说明
    pub note: String,
}

/// 玩家实际动作（终局帧 `actions` 归类）
#[derive(Debug, Serialize)]
pub struct DecisionRow {
    /// 回合（0 基）
    pub turn: i32,
    /// 类型（`train` / `rest` / `race` / `event` / `other`）
    pub kind: String,
    /// 训练命令 id（训练类才有）
    pub cmd: Option<i32>,
    /// 稽古演目（`5101–5105` 才有）
    pub keiko: Option<usize>,
    /// 事件 id
    pub event_id: Option<i32>,
    /// 事件选项号
    pub choice: Option<i32>,
}

/// 一条训练记录（实际动作 + 帧预览匹配）
#[derive(Debug, Serialize)]
pub struct TrainRecord {
    /// 回合
    pub turn: i32,
    /// 实际命令 id
    pub cmd: i32,
    /// 稽古演目（普通训练为 `None`）
    pub keiko: Option<usize>,
    /// 匹配到的预览命令（`None` = 未匹配）
    pub matched: Option<String>,
    /// 匹配命令的失败率
    pub failure_rate: Option<f32>,
    /// 预览 7 项（期望）
    pub expected: Option<[i32; AXES]>,
    /// 前后帧实际 7 项（五维 + 技能点 + 体力）
    pub actual: Option<[i32; AXES]>,
    /// 是否判定为训练失败（期望非零、实际五维全零）
    pub failed: bool,
    /// 预览帧是否 `isAfterTraining`（`true` = 参考值，不作判据）
    pub base_after: bool,
}

/// 一次继承
#[derive(Debug, Serialize)]
pub struct InheritStep {
    /// 回合（首个记录到该次增量的帧）
    pub turn: i32,
    /// 该次增量
    pub gain: i32,
}

/// 由帧轨迹与终局动作构建反推总表
pub fn build(trace: &[TraceRow], actions: &[FinalAction], spent_total: i32) -> Derive {
    let decisions = decisions(actions);
    let training = training(trace, actions);
    let kensan = kensan_report(trace, actions, &training);
    Derive {
        advice: advice_slots(trace),
        fusion: fusions(trace),
        skill: skill_report(trace, spent_total),
        kensan,
        training,
        decisions,
        inherit: inherits(trace),
    }
}

/// 每回合的末帧（同回合多帧折叠）
fn turn_end(trace: &[TraceRow]) -> Vec<&TraceRow> {
    let mut out: Vec<&TraceRow> = Vec::new();
    for row in trace {
        match out.last() {
            Some(last) if last.turn == row.turn => {
                let index = out.len() - 1;
                out[index] = row;
            }
            _ => out.push(row),
        }
    }
    out
}

/// 教诲落位：逐回合比对槽位 id，首现新 id 记该演目的生效回合
fn advice_slots(trace: &[TraceRow]) -> Vec<AdviceSlot> {
    let mut out = Vec::new();
    let mut prev = [0i32; 5];
    // 同回合多帧（先授后调）只取回合末状态，避免把一次分配拆成两轮
    for row in turn_end(trace) {
        for act in 0..5 {
            let now = row.advice[act];
            if now != prev[act] && now != 0 {
                out.push(AdviceSlot {
                    act,
                    turn: row.turn,
                    advice_id: now,
                    prev: prev[act],
                });
            }
            prev[act] = now;
        }
    }
    out
}

/// 転成：融合叠加首次从 0 变非 0 的回合
fn fusions(trace: &[TraceRow]) -> Vec<FusionRecord> {
    let mut out = Vec::new();
    let mut prev = [0i32; 5];
    for row in turn_end(trace) {
        for act in 0..5 {
            let now = row.fusion[act];
            if now != prev[act] && now != 0 {
                out.push(FusionRecord {
                    act,
                    turn: row.turn,
                    base: row.advice[act],
                    target: now,
                });
            }
            prev[act] = now;
        }
    }
    out
}

/// 学技能：剩余点下降的回合（帧原值口径）
fn skill_report(trace: &[TraceRow], spent_total: i32) -> SkillReport {
    let mut report = SkillReport {
        spent_total,
        events: Vec::new(),
    };
    let mut prev: Option<i32> = None;
    for row in trace {
        if let Some(from) = prev
            && row.skill_pt < from
        {
            report.events.push(SkillSpend {
                turn: row.turn,
                from,
                to: row.skill_pt,
            });
        }
        prev = Some(row.skill_pt);
    }
    report
}

/// 继承：帧内 `inheritGains` 是累计数组，按长度增长切出每次增量
fn inherits(trace: &[TraceRow]) -> Vec<InheritStep> {
    let mut out = Vec::new();
    let mut prev_len = 0usize;
    for row in trace {
        if row.inherit_gains.len() > prev_len {
            for gain in &row.inherit_gains[prev_len..] {
                out.push(InheritStep {
                    turn: row.turn,
                    gain: *gain,
                });
            }
            prev_len = row.inherit_gains.len();
        } else if row.inherit_gains.len() < prev_len {
            // 读档 / 换段后累计数组回退：以本帧为准重新对齐
            prev_len = row.inherit_gains.len();
        }
    }
    out
}

/// 终局动作归类
fn decisions(actions: &[FinalAction]) -> Vec<DecisionRow> {
    let mut out = Vec::new();
    for action in actions {
        let turn = action.turn;
        let row = match action.endpoint.as_str() {
            "exec_command" => {
                let cmd = field(action, "command_id").map(|value| value as i32);
                let ctype = field(action, "command_type").map(|value| value as i32);
                let group = field(action, "command_group_id").map(|value| value as i32);
                let kind = match cmd {
                    Some(cmd) if is_train(cmd) => "train",
                    Some(0) if ctype == Some(3) || group == Some(301) => "rest",
                    _ if ctype == Some(3) => "rest",
                    _ => "other",
                };
                DecisionRow {
                    turn,
                    kind: kind.to_string(),
                    cmd,
                    keiko: cmd.and_then(keiko_of),
                    event_id: None,
                    choice: None,
                }
            }
            "check_event" => DecisionRow {
                turn,
                kind: "event".to_string(),
                cmd: None,
                keiko: None,
                event_id: field(action, "event_id").map(|value| value as i32),
                choice: field(action, "choice_number").map(|value| value as i32),
            },
            "race_entry" => DecisionRow {
                turn,
                kind: "race".to_string(),
                cmd: None,
                keiko: None,
                event_id: None,
                choice: None,
            },
            _ => continue,
        };
        out.push(row);
    }
    out
}

/// 训练回合：实际命令 + 同回合预览匹配
fn training(trace: &[TraceRow], actions: &[FinalAction]) -> Vec<TrainRecord> {
    let mut out = Vec::new();
    for action in actions {
        if action.endpoint != "exec_command" {
            continue;
        }
        let Some(cmd) = field(action, "command_id").map(|value| value as i32) else {
            continue;
        };
        if !is_train(cmd) {
            continue;
        }
        let turn = action.turn;
        // 基线帧：优先本回合「训练前」且带预览的帧；兜底取本回合最后一帧预览
        let base = trace
            .iter()
            .rev()
            .find(|row| row.turn == turn && !row.is_after_training && !row.training.is_empty())
            .or_else(|| {
                trace
                    .iter()
                    .rev()
                    .find(|row| row.turn == turn && !row.training.is_empty())
            });
        let next = trace.iter().find(|row| row.turn > turn);
        let actual = base.zip(next).map(|(base, next)| actual_gain(base, next));
        // 命令 id 与帧内预览键同源：先精确查，键缺失才退回距离匹配
        let direct = base.and_then(|base| {
            base.training
                .iter()
                .find(|preview| preview.cmd == cmd.to_string())
        });
        let mut matched = None;
        let mut failure_rate = None;
        let mut expected = None;
        let mut failed = false;
        if let (Some(base), Some(actual)) = (base, actual) {
            let index = match_preview(base, &actual)
                .map(|(index, _)| index)
                .filter(|index| base.training[*index].cmd == cmd.to_string())
                .or_else(|| {
                    direct.and_then(|preview| {
                        base.training.iter().position(|row| row.cmd == preview.cmd)
                    })
                })
                .or_else(|| match_preview(base, &actual).map(|(index, _)| index));
            if let Some(index) = index {
                let row = &base.training[index];
                matched = Some(row.cmd.clone());
                failure_rate = Some(row.failure_rate);
                expected = Some(row.total);
                let force = row.total[..5].iter().any(|value| *value != 0);
                failed = force && actual[..5].iter().all(|value| *value == 0);
            }
        }
        out.push(TrainRecord {
            turn,
            cmd,
            keiko: keiko_of(cmd),
            matched,
            failure_rate,
            expected,
            actual,
            failed,
            base_after: base.is_some_and(|row| row.is_after_training),
        });
    }
    out
}

/// 追加稽古：触发帧 `storyId`（训练**之后**弹出）+ 同回合训练是否就是研鑽演目
///
/// 事件在稽古结算后弹出（选 1 = 加练、选 2 = 休息，选择当场结算），
/// 所以「执行」判的是**触发回合自身的训练**，而不是下一回合的主行动。
fn kensan_report(
    trace: &[TraceRow],
    actions: &[FinalAction],
    training: &[TrainRecord],
) -> KensanReport {
    let mut report = KensanReport::default();
    let mut seen = std::collections::BTreeSet::new();
    for row in trace
        .iter()
        .filter(|row| row.story_id == Some(KENSAN_EVENT_ID))
    {
        if !seen.insert(row.turn) {
            continue;
        }
        let act = row.kensan_act;
        let choice = actions
            .iter()
            .find(|action| {
                action.endpoint == "check_event"
                    && action.turn == row.turn
                    && field(action, "event_id") == Some(i64::from(KENSAN_EVENT_ID))
            })
            .and_then(|action| field(action, "choice_number"))
            .map(|value| value as i32);
        // 事件是「训练后弹窗」：选 1 = 加练（当场 −5 体 / 智 +20），选 2 = 休息。
        // actions 未采集该事件时不猜选择，只陈述同回合训练命令供对账。
        let executed = choice.map(|value| value == 1);
        let note = match choice {
            Some(1) => "选 1 加练".to_string(),
            Some(_) => "选 2 休息".to_string(),
            None => {
                let cmd = training
                    .iter()
                    .find(|record| record.turn == row.turn)
                    .map_or_else(String::new, |record| record.cmd.to_string());
                if cmd.is_empty() {
                    "actions 未采集该事件；该回合无训练记录".to_string()
                } else {
                    format!("actions 未采集该事件；同回合训练命令 {cmd}")
                }
            }
        };
        report.triggers += 1;
        match executed {
            Some(true) => report.executed += 1,
            Some(false) => report.wasted += 1,
            None => report.rested += 1,
        }
        report.items.push(KensanItem {
            turn: row.turn,
            act,
            choice,
            executed,
            note,
        });
    }
    report
}

/// 前后帧实际 7 项增量
///
/// 帧 `fiveStatus` 是**真值**（≥1200 不减半），所以这里直接做差；
/// 折半只发生在 `baseGame.training` 预览上，见 [`match_preview`]。
fn actual_gain(base: &TraceRow, next: &TraceRow) -> [i32; AXES] {
    let mut out = [0; AXES];
    for axis in 0..5 {
        out[axis] = next.five[axis] - base.five[axis];
    }
    out[5] = next.skill_pt - base.skill_pt;
    out[6] = next.vital - base.vital;
    out
}

/// 用前后帧增量反查实际训练位（返回预览下标与距离）
///
/// 预览是 `ReviseOver1200` 表示值：先把每轴还原成真值再比。匹配取
/// 五维绝对差 + 体力 3 倍权重的最小者，阈值 20 只用来排除"根本不是这一项"。
fn match_preview(base: &TraceRow, actual: &[i32; AXES]) -> Option<(usize, i32)> {
    let mut best: Option<(usize, i32)> = None;
    for (index, row) in base.training.iter().enumerate() {
        let mut dist = 3 * (actual[6] - row.total[6]).abs();
        for axis in 0..5 {
            let expected = restore_displayed_gain(base.five[axis], row.total[axis]);
            dist += (actual[axis] - expected).abs();
        }
        if best.is_none_or(|(_, best)| dist < best) {
            best = Some((index, dist));
        }
    }
    best.filter(|(_, dist)| *dist <= 20)
}

/// 命令是否训练类（普通训练 / 演目稽古）
fn is_train(cmd: i32) -> bool {
    (TRAIN_CMD_MIN..=TRAIN_CMD_MAX).contains(&cmd) || keiko_of(cmd).is_some()
}

/// 稽古命令 → 演目下标
fn keiko_of(cmd: i32) -> Option<usize> {
    (KEIKO_CMD_MIN..=KEIKO_CMD_MAX)
        .contains(&cmd)
        .then_some((cmd - KEIKO_CMD_MIN) as usize)
}

/// 取端点私有字段的整数值
fn field(action: &FinalAction, key: &str) -> Option<i64> {
    action.fields.get(key).and_then(serde_json::Value::as_i64)
}
