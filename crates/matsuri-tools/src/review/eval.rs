//! `review eval`：单点重算（候选表 + regret + 配对 CI + T(n) + 缓存）
//!
//! 只从目标回合的**单帧**还原局面，搜索自己 rollout 到终局；不逐帧回放整局。
//! 种子与在线同口径（`(master, turn, Probe)`），所以同局面下在线决策与 eval 重算可比。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use matsuri_ai::{OnlineSession, ParsedFrame, Route, parse_final_score};
use matsuri_core::{
    Action, MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, SearchSummary,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use umaai::protocol::final_score::FinalAction;
use umaai_core::rng::{StreamTag, derive_seed};
use umasim::game::{Game, Trainer};

use super::discover::GameFiles;
use super::facts::open_session;

/// 缓存 schema（改动输出结构时递增）
const SCHEMA: u32 = 1;

/// `review eval` 参数
pub struct EvalArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 目标回合
    pub turn: i32,
    /// 同回合第几帧（缺省 = 第一个可决策帧）
    pub seq: Option<u32>,
    /// 每候选 rollout 次数
    pub n: u32,
    /// 输出目录；`None` = 与 `open` 同规则
    pub out: Option<&'a Path>,
    /// 数据根；`None` = 仓内 `data/`
    pub data_root: Option<&'a Path>,
    /// 忽略缓存强制重算
    pub force: bool,
    /// 批量：本局所有有决策面的回合都算（忽略 `turn`）
    pub all: bool,
}

/// 一次 eval 的结果（同时是缓存文件格式）
#[derive(Debug, Serialize, Deserialize)]
pub struct EvalReport {
    /// 结构版本
    pub schema: u32,
    /// 局号
    pub game: u64,
    /// 回合
    pub turn: i32,
    /// 同回合帧序号
    pub seq: u32,
    /// 决策类型（`train` / `advice` / `event`）
    pub kind: String,
    /// 每候选 rollout 次数
    pub n: u32,
    /// 决策流种子
    pub seed: u64,
    /// 代码版本
    pub version: String,
    /// 生效卡组档
    pub deck: Option<String>,
    /// 实际选择
    pub chosen: Option<CandidateRef>,
    /// 搜索最优
    pub best: Option<CandidateRef>,
    /// `best − chosen`（加权分口径）
    pub regret: Option<f64>,
    /// 配对差 95% 区间半宽
    pub ci95: Option<f64>,
    /// 配对样本数
    pub paired_n: usize,
    /// 本回合期望分（官方口径 T(n)）
    pub t_n: Option<f64>,
    /// 候选全表
    pub candidates: Vec<CandidateRow>,
    /// rollout 健康度
    pub rollout: Option<HealthRow>,
    /// 备注（chosen 不可判 / 走 fallback 等）
    pub note: Option<String>,
    /// 搜索耗时
    pub elapsed_ms: u64,
}

/// 候选引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRef {
    /// 候选下标（完整候选空间）
    pub index: usize,
    /// 描述
    pub description: String,
    /// 加权分
    pub score: f64,
}

/// 候选表一行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRow {
    /// 候选下标（完整候选空间）
    pub index: usize,
    /// 描述
    pub description: String,
    /// 加权分
    pub score: f64,
    /// 样本标准差
    pub stdev: f64,
    /// 样本数
    pub n: u32,
    /// 终局五维均值
    pub five: [f64; 5],
    /// 终局 PT 均值
    pub pt: f64,
}

/// rollout 健康度落盘格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthRow {
    /// 计划 rollout 总数
    pub planned: usize,
    /// 成功数
    pub succeeded: usize,
    /// 报错数
    pub failed: usize,
    /// 最高育成失败率
    pub max_career_failed_rate: f64,
}

/// 批量重算：本局每个有决策面的回合各算一点（同一缓存目录）
pub fn eval_all(args: EvalArgs<'_>) -> Result<Vec<(PathBuf, EvalReport)>> {
    let files = GameFiles::scan(args.input, args.game)?;
    let mut turns: Vec<i32> = files
        .frames
        .iter()
        .filter_map(|frame| parse_frame_name(&frame.name).map(|(turn, _)| turn))
        .collect();
    turns.sort_unstable();
    turns.dedup();
    let mut out = Vec::new();
    for turn in turns {
        let point = EvalArgs {
            input: args.input,
            game: args.game,
            turn,
            seq: None,
            n: args.n,
            out: args.out,
            data_root: args.data_root,
            force: args.force,
            all: false,
        };
        match eval(point) {
            Ok(result) => out.push(result),
            Err(error) => eprintln!("跳过 T{turn}：{error:#}"),
        }
    }
    Ok(out)
}

/// 跑一次单点重算，返回（结果路径，结果）
pub fn eval(args: EvalArgs<'_>) -> Result<(PathBuf, EvalReport)> {
    let files = GameFiles::scan(args.input, args.game)?;
    let data_root = args
        .data_root
        .map_or_else(matsuri_core::testing::data_dir, Path::to_path_buf);
    matsuri_ai::session::ensure_data(&data_root)?;
    let engine = umaai::Config::load_with_overlay_file(&data_root, ".")?;
    let data = Arc::new(MatsuriData::load(&data_root)?);
    let base = MatsuriConfig::load_with_overlay_file(&data_root, ".")?;
    let master = engine.sim.seed;
    let seed = derive_seed(master, &[args.turn as u64, StreamTag::Probe.tag()]);
    let version = env!("CARGO_PKG_VERSION").to_string();

    let out = args.out.map_or_else(
        || {
            if args.input.is_dir() {
                args.input.join("review")
            } else {
                args.input.with_extension("review")
            }
        },
        Path::to_path_buf,
    );

    if !args.force
        && let Some((path, cached)) = read_cache(&out, &files, args.turn, args.seq)
        && cached.schema == SCHEMA
        && cached.turn == args.turn
        && cached.n == args.n
        && cached.seed == seed
        && cached.version == version
    {
        return Ok((path, cached));
    }

    let mut session = open_session(&engine, Arc::clone(&data), base.clone())?;
    let (contents, seq, frame) = pick_frame(&mut session, &files, args.turn, args.seq)?;
    let deck = session.deck_profile().cloned();
    let mut config = match &deck {
        Some(profile) => base.for_profile(&profile.name)?,
        None => base,
    };
    config.search.search_n = args.n as usize;
    // 配对差需要按 rollout 序号对齐的原始分（引擎默认关闭，省内存）
    let mut search_cfg = engine.search.clone();
    search_cfg.record_ordered_rollouts = true;
    let trainer = MatsuriSearchTrainer::new(config.clone(), search_cfg)?;
    let mut rng = StdRng::seed_from_u64(seed);
    let started = Instant::now();

    let game = session.replay_frame(&contents)?;
    let (kind, summary, actions, pre) = run_decision(&trainer, &game, &frame, &config, &mut rng)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let history = history_actions(&files)?;
    let event_choices = frame.status.story.as_ref().map(|story| story.choices.len());
    let chosen_index = actual_choice(
        &files,
        &game,
        &kind,
        args.turn,
        &history,
        actions.as_deref(),
        pre.as_ref(),
        event_choices,
    );
    let report = build_report(ReportInput {
        game: files.game,
        turn: args.turn,
        seq,
        kind,
        n: args.n,
        seed,
        version,
        deck: deck.map(|profile| profile.name),
        summary,
        chosen_index,
        trainer: &trainer,
        elapsed_ms,
    });

    let dir = out.join("recompute");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("turn{:02}_{seq}.json", args.turn));
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    Ok((path, report))
}

/// 选目标帧：指定 `seq` 精确取；否则取该回合第一个可决策帧
fn pick_frame(
    session: &mut OnlineSession,
    files: &GameFiles,
    turn: i32,
    seq: Option<u32>,
) -> Result<(String, u32, ParsedFrame)> {
    let mut rows: Vec<(u32, String)> = files
        .frames
        .iter()
        .filter_map(|frame| {
            let (frame_turn, frame_seq) = parse_frame_name(&frame.name)?;
            (frame_turn == turn).then(|| (frame_seq, frame.name.clone()))
        })
        .collect();
    rows.sort_by_key(|(seq, _)| *seq);
    if rows.is_empty() {
        bail!("局包里没有 T{turn} 的回合帧");
    }
    if let Some(want) = seq {
        rows.retain(|(seq, _)| *seq == want);
        if rows.is_empty() {
            bail!("T{turn} 没有 seq={want} 的帧");
        }
    }
    let mut skipped = Vec::new();
    for (frame_seq, name) in &rows {
        let Some(frame) = files.frames.iter().find(|frame| frame.name == *name) else {
            continue;
        };
        match session.replay_frame(&frame.contents) {
            Ok(game) => {
                let parsed = ParsedFrame::parse(&frame.contents)?;
                match matsuri_ai::online::route(&game, &parsed) {
                    Route::Skip(reason) => skipped.push(format!("{name}：{reason}")),
                    _ => return Ok((frame.contents.clone(), *frame_seq, parsed)),
                }
            }
            Err(error) => skipped.push(format!("{name}：{error:#}")),
        }
    }
    bail!(
        "T{turn} 没有可决策帧（跳过 {} 帧）：{}",
        skipped.len(),
        skipped.join("；")
    )
}

/// 执行一次决策（与在线同口径），返回（类型，摘要，候选，上下文）
fn run_decision(
    trainer: &MatsuriSearchTrainer,
    game: &MatsuriGame,
    frame: &ParsedFrame,
    config: &MatsuriConfig,
    rng: &mut StdRng,
) -> Result<DecisionRun> {
    match matsuri_ai::online::route(game, frame) {
        Route::Skip(reason) => bail!("目标帧不可决策：{reason}"),
        Route::Train => {
            let actions = game.list_actions()?;
            trainer.select_action(game, &actions, rng)?;
            Ok((
                "train".to_string(),
                trainer.last_search(),
                Some(actions),
                None,
            ))
        }
        Route::Advice(window) => {
            let mut search = game.clone();
            if !search.advice.acquired || search.advice.slots == [0; 5] {
                search.advice.acquired = true;
                search.advice.slots = [101, 201, 301, 401, 501];
                search.advice.fused = [0; 5];
            }
            if search.is_fusion_window(window) {
                let pairs = if window == 2 {
                    &config.deck.fusion.year2
                } else {
                    &config.deck.fusion.year3
                };
                let already = pairs
                    .iter()
                    .all(|pair| search.advice.fused.contains(&pair.target));
                if !already {
                    search.apply_fusion_pairs(pairs)?;
                }
            }
            search.stage = matsuri_ai::online::advice_stage(&search);
            let ctx = AdviceCtx {
                slots: search.advice.slots,
                fused: search.advice.fused,
            };
            let actions = search.advice_actions();
            trainer.select_action(&search, &actions, rng)?;
            Ok((
                "advice".to_string(),
                trainer.last_search(),
                Some(actions),
                Some(ctx),
            ))
        }
        Route::Event => {
            let event = matsuri_ai::online::pending_event(game)
                .ok_or_else(|| anyhow!("事件帧取不到待选事件"))?
                .clone();
            let mut search = game.clone();
            search.stage = matsuri_ai::online::event_stage(game, frame);
            search.event_queue = vec![event.clone()];
            search.event_cursor = 0;
            let choices = event.choices.clone();
            trainer.select_event_choice(&search, &event, &choices, rng)?;
            Ok(("event".to_string(), trainer.last_search(), None, None))
        }
    }
}

/// 一次决策的产出：（类型，搜索摘要，候选列表，教诲上下文）
type DecisionRun = (
    String,
    Option<SearchSummary>,
    Option<Vec<Action>>,
    Option<AdviceCtx>,
);

/// 教诲决策前的槽位上下文（实际方案匹配用）
struct AdviceCtx {
    /// 分配前槽内容
    slots: [i32; 5],
    /// 分配前転成叠加
    fused: [i32; 5],
}

/// 终局帧里的玩家动作
fn history_actions(files: &GameFiles) -> Result<Vec<FinalAction>> {
    let Some(final_frame) = files.final_frame.as_ref() else {
        return Ok(Vec::new());
    };
    let frame = parse_final_score(&final_frame.contents)?;
    Ok(frame.actions.unwrap_or_default())
}

/// 实际选择在候选空间里的下标
#[allow(clippy::too_many_arguments)]
fn actual_choice(
    files: &GameFiles,
    game: &MatsuriGame,
    kind: &str,
    turn: i32,
    history: &[FinalAction],
    actions: Option<&[Action]>,
    advice: Option<&AdviceCtx>,
    event_choices: Option<usize>,
) -> Option<usize> {
    match kind {
        "train" => {
            let actions = actions?;
            let action = actual_train_action(files, game, turn, history)?;
            actions.iter().position(|candidate| *candidate == action)
        }
        "advice" => {
            let actions = actions?;
            let ctx = advice?;
            // 实际方案取回合**末帧**（授权帧的槽位还没落位，末帧才是玩家最终分配）
            let (actual_slots, actual_fused) = advice_actual(files, turn)?;
            actions.iter().position(|candidate| {
                let Action::AssignAdvice(assign) = candidate else {
                    return false;
                };
                let slots: [i32; 5] =
                    std::array::from_fn(|act| ctx.slots[usize::from(assign.0[act])]);
                let fused: [i32; 5] =
                    std::array::from_fn(|act| ctx.fused[usize::from(assign.0[act])]);
                slots == actual_slots && fused == actual_fused
            })
        }
        "event" => {
            // 同回合可能有多条 check_event（事件链），帧的 storyId 与请求 event_id 不同源，
            // 无法可靠对齐 → 只有「恰好一条」时才判实际选择，否则留空不猜。
            let rows: Vec<&FinalAction> = history
                .iter()
                .filter(|action| action.endpoint == "check_event" && action.turn == turn)
                .collect();
            let [row] = rows.as_slice() else {
                return None;
            };
            let choice = field(row, "choice_number")?;
            let choice = usize::try_from(choice).ok()?;
            // 真机 check_event.choice_number 是 1 基（T11 两选项给 2），超界即按 1 基回退
            match event_choices {
                Some(len) if choice < len => Some(choice),
                _ => choice.checked_sub(1),
            }
        }
        _ => None,
    }
}

/// 该回合的实际教诲方案（回合末帧的槽位，与还原层同口径）
fn advice_actual(files: &GameFiles, turn: i32) -> Option<([i32; 5], [i32; 5])> {
    let frame = files.frames.iter().rev().find(|frame| {
        parse_frame_name(&frame.name).is_some_and(|(frame_turn, _)| frame_turn == turn)
    })?;
    let parsed = ParsedFrame::parse(&frame.contents).ok()?;
    let mut slots = [0i32; 5];
    let mut fused = [0i32; 5];
    for act in 0..5 {
        let Some(row) = parsed.festa.act(act) else {
            continue;
        };
        if row.fusion_id == 0 {
            slots[act] = row.advice_id;
        } else {
            slots[act] = row.fusion_id;
            fused[act] = row.advice_id;
        }
    }
    Some((slots, fused))
}

/// 该回合实际训练动作（命令 → Action）
fn actual_train_action(
    files: &GameFiles,
    game: &MatsuriGame,
    turn: i32,
    history: &[FinalAction],
) -> Option<Action> {
    let commands: Vec<&FinalAction> = history
        .iter()
        .filter(|action| action.endpoint == "exec_command" && action.turn == turn)
        .collect();
    if history
        .iter()
        .any(|action| action.endpoint == "race_entry" && action.turn == turn)
    {
        return Some(Action::Race);
    }
    let action = commands.last().copied()?;
    let cmd = field(action, "command_id").map(|value| value as i32)?;
    if (5101..=5105).contains(&cmd) {
        return Some(Action::ActTrain((cmd - 5101) as u8));
    }
    let ctype = field(action, "command_type").map(|value| value as i32);
    let group = field(action, "command_group_id").map(|value| value as i32);
    if ctype == Some(3) || group == Some(301) || cmd == 0 {
        return Some(Action::Rest);
    }
    // 普通训练：命令编码语义未经证实，用预览与实际增量反查训练位
    let actual = turn_actual(files, turn)?;
    infer_train(game, &actual).map(|train| Action::Train(train as u8))
}

/// 该回合的实际 7 项增量（五维 + PT + 体力）：本回合末帧 vs 下一回合首帧
fn turn_actual(files: &GameFiles, turn: i32) -> Option<[i32; 7]> {
    let base = files.frames.iter().rev().find(|frame| {
        parse_frame_name(&frame.name).is_some_and(|(frame_turn, _)| frame_turn == turn)
    })?;
    let next = files.frames.iter().find(|frame| {
        parse_frame_name(&frame.name).is_some_and(|(frame_turn, _)| frame_turn > turn)
    })?;
    let base = ParsedFrame::parse(&base.contents).ok()?;
    let next = ParsedFrame::parse(&next.contents).ok()?;
    let mut out = [0i32; 7];
    for axis in 0..5 {
        out[axis] = next.status.five_status[axis] - base.status.five_status[axis];
    }
    out[5] = next.status.skill_pt - base.status.skill_pt;
    out[6] = next.status.vital - base.status.vital;
    Some(out)
}

/// 用训练预览反查训练位（0–4）
fn infer_train(game: &MatsuriGame, actual: &[i32; 7]) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for train in 0..5 {
        let Ok(layers) = game.preview_training_layers(train, false) else {
            continue;
        };
        // `ActionValue.status_pt` 六位：五维 + PT（体力不在其中）
        let total = layers.total.status_pt;
        let mut dist = (actual[5] - total[5]).abs();
        for axis in 0..5 {
            dist += (actual[axis] - total[axis]).abs();
        }
        if best.is_none_or(|(_, best_dist)| dist < best_dist) {
            best = Some((train, dist));
        }
    }
    best.filter(|(_, dist)| *dist <= 20).map(|(train, _)| train)
}

/// 汇总结果
struct ReportInput<'a> {
    game: u64,
    turn: i32,
    seq: u32,
    kind: String,
    n: u32,
    seed: u64,
    version: String,
    deck: Option<String>,
    summary: Option<SearchSummary>,
    chosen_index: Option<usize>,
    trainer: &'a MatsuriSearchTrainer,
    elapsed_ms: u64,
}

/// 组装落盘结果
fn build_report(input: ReportInput<'_>) -> EvalReport {
    let ReportInput {
        game,
        turn,
        seq,
        kind,
        n,
        seed,
        version,
        deck,
        summary,
        chosen_index,
        trainer,
        elapsed_ms,
    } = input;
    let health = trainer.last_health();
    let mut note = None;
    let (candidates, best, chosen, regret, ci95, paired_n, t_n) = match summary {
        Some(summary) => {
            let candidates: Vec<CandidateRow> = summary
                .candidates
                .iter()
                .map(|candidate| CandidateRow {
                    index: candidate.index,
                    description: candidate.description.clone(),
                    score: candidate.score,
                    stdev: candidate.stdev,
                    n: candidate.n,
                    five: candidate.five,
                    pt: candidate.pt,
                })
                .collect();
            let best_position = summary
                .candidates
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.score.total_cmp(&b.score))
                .map(|(position, _)| position);
            let best = best_position.and_then(|position| {
                summary
                    .candidates
                    .get(position)
                    .map(|candidate| CandidateRef {
                        index: candidate.index,
                        description: candidate.description.clone(),
                        score: candidate.score,
                    })
            });
            let chosen_position = chosen_index.and_then(|index| {
                summary
                    .candidates
                    .iter()
                    .position(|candidate| candidate.index == index)
            });
            let chosen = chosen_position.and_then(|position| {
                summary
                    .candidates
                    .get(position)
                    .map(|candidate| CandidateRef {
                        index: candidate.index,
                        description: candidate.description.clone(),
                        score: candidate.score,
                    })
            });
            if chosen_index.is_some() && chosen.is_none() {
                note = Some("实际动作不在候选空间（搜索剪枝 / 未映射）".to_string());
            }
            let regret = best
                .as_ref()
                .zip(chosen.as_ref())
                .map(|(best, chosen)| best.score - chosen.score);
            let (ci95, paired_n) = match (trainer.last_ordered(), best_position, chosen_position) {
                (Some(ordered), Some(best), Some(chosen)) => paired_ci(&ordered, best, chosen),
                _ => (None, 0),
            };
            (
                candidates,
                best,
                chosen,
                regret,
                ci95,
                paired_n,
                Some(summary.expected_score),
            )
        }
        None => {
            note = Some("本次走了手写 fallback（无搜索分）".to_string());
            (Vec::new(), None, None, None, None, 0, None)
        }
    };
    if chosen_index.is_none() && note.is_none() && kind == "event" {
        note = Some("该回合有多条/没有 check_event，实际选择无法与当前事件对齐".to_string());
    };
    EvalReport {
        schema: SCHEMA,
        game,
        turn,
        seq,
        kind,
        n,
        seed,
        version,
        deck,
        chosen,
        best,
        regret,
        ci95,
        paired_n,
        t_n,
        candidates,
        rollout: health.map(|health| HealthRow {
            planned: health.planned,
            succeeded: health.succeeded,
            failed: health.failed,
            max_career_failed_rate: health.max_career_failed_rate(),
        }),
        note,
        elapsed_ms,
    }
}

/// 配对差 95% 区间：逐 rollout 序号求差（CRN 下方差远小于独立近似）
fn paired_ci(
    ordered: &umasim::search::OrderedRollouts,
    best: usize,
    chosen: usize,
) -> (Option<f64>, usize) {
    let (Some(best_row), Some(chosen_row)) = (
        ordered.per_candidate.get(best),
        ordered.per_candidate.get(chosen),
    ) else {
        return (None, 0);
    };
    let diffs: Vec<f64> = best_row
        .iter()
        .zip(chosen_row.iter())
        .filter_map(|(best, chosen)| best.zip(*chosen).map(|(best, chosen)| best - chosen))
        .collect();
    let n = diffs.len();
    if n < 2 {
        return (None, n);
    }
    let mean = diffs.iter().sum::<f64>() / n as f64;
    let variance = diffs.iter().map(|diff| (diff - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let se = (variance / n as f64).sqrt();
    (Some(t95(n - 1) * se), n)
}

/// 双侧 95% t 临界值（df ≤ 30 查表，更大用正态 1.96）
fn t95(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    TABLE.get(df.saturating_sub(1)).copied().unwrap_or(1.96)
}

/// 帧文件名 → （回合，同回合序号）
pub fn parse_frame_name(name: &str) -> Option<(i32, u32)> {
    let base = name.trim_end_matches(".json");
    let after = base.split_once("_turn")?.1;
    let mut parts = after.split('_');
    let turn = parts.next()?.parse::<i32>().ok()?;
    let seq = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Some((turn, seq))
}

/// 读缓存（路径由局目录 / 回合 / 序号决定）
fn read_cache(
    out: &Path,
    files: &GameFiles,
    turn: i32,
    seq: Option<u32>,
) -> Option<(PathBuf, EvalReport)> {
    let dir = out.join("recompute");
    let mut best: Option<(PathBuf, EvalReport)> = None;
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy().to_string();
        if !name.starts_with(&format!("turn{turn:02}_")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(report) = serde_json::from_str::<EvalReport>(&text) else {
            continue;
        };
        if report.game != files.game {
            continue;
        }
        if let Some(want) = seq
            && report.seq != want
        {
            continue;
        }
        best = Some((path, report));
    }
    best
}

/// 取端点私有字段的整数值
fn field(action: &FinalAction, key: &str) -> Option<i64> {
    action.fields.get(key).and_then(serde_json::Value::as_i64)
}
