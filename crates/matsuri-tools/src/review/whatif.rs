//! `review whatif`：从某回合局面分叉，对比「历史实际动作」与若干「替代动作」之后的走向
//!
//! 分支只分一次：根动作不同，之后各支都交给手写训练员继续；同 `master` 下回合流按
//! `(master, turn, tag)` 派生，各支在 T+k 的随机骨架一致，差异来自状态与消耗路径。
//! `--horizon 0` 表示跑满终局，另给终局分。

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};
use matsuri_ai::{ParsedFrame, Route};
use matsuri_core::{Action, MatsuriConfig, MatsuriData, MatsuriGame, MatsuriTrainer};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::Serialize;
use umaai_core::gamedata::{EventChoice, EventData};
use umaai_core::rng::{StreamTag, derive_seed};
use umasim::game::{Game, Trainer};
use umasim::search::FlatSearchGame;

use super::discover::GameFiles;
use super::eval::{AdviceCtx, actual_choice, history_actions, parse_frame_name};
use super::facts::{display, open_session};

/// `review whatif` 参数
pub struct WhatifArgs<'a> {
    /// 局目录或局包 zip
    pub input: &'a Path,
    /// 多局混放时指定局号
    pub game: Option<u64>,
    /// 分叉回合
    pub turn: i32,
    /// 替代动作名（逗号分隔多个）
    pub swap: &'a str,
    /// 分叉后观察的回合数（`0` = 跑满终局）
    pub horizon: i32,
    /// 重复组数（每组换 `master`）
    pub k: u32,
    /// 输出目录
    pub out: Option<&'a Path>,
    /// 数据根
    pub data_root: Option<&'a Path>,
}

/// 一行回合快照
#[derive(Debug, Clone, Serialize)]
pub struct StepRow {
    /// 回合
    pub turn: i32,
    /// 本回合动作
    pub action: String,
    /// 体力
    pub vital: i32,
    /// 干劲
    pub motivation: i32,
    /// 五维真值
    pub five: [i32; 5],
    /// 五维查表得分（评分分量，与属性值不同口径）
    pub five_score: i32,
    /// 演目 Lv
    pub act_level: [i32; 5],
    /// 技能点（未花掉的资源）
    pub skill_pt: i32,
    /// PT（评分分量）
    pub pt: i32,
    /// 技能分（评分分量）
    pub skill: i32,
    /// 官方口径评分
    pub score: f64,
}

/// 一条分支
#[derive(Debug, Clone, Serialize)]
pub struct Branch {
    /// 分支标签
    pub label: String,
    /// 根动作
    pub root_action: String,
    /// 逐回合快照（第一组明细）
    pub rows: Vec<StepRow>,
    /// 终局分（`--horizon 0` 跑满时才有；K 组均值）
    pub final_score: Option<f64>,
}

/// 单回合的状态均值
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Snapshot {
    /// 体力
    pub vital: f64,
    /// 干劲
    pub motivation: f64,
    /// 五维属性总和
    pub five: f64,
    /// 五维各自属性均值（速耐力根智）
    pub five_parts: [f64; 5],
    /// 五维得分总和
    pub five_score: f64,
    /// 演目 Lv 总和
    pub act: f64,
    /// 技能点
    pub skill_pt: f64,
    /// PT（评分分量）
    pub pt: f64,
    /// 技能分（评分分量）
    pub skill: f64,
    /// 官方口径评分
    pub score: f64,
}

/// 单回合的动作分布与状态均值
#[derive(Debug, Clone, Serialize)]
pub struct TurnDist {
    /// 回合
    pub turn: i32,
    /// 基线分支各动作次数（降序）
    pub actual: Vec<(String, u32)>,
    /// 替代分支各动作次数（降序）
    pub alternate: Vec<(String, u32)>,
    /// 基线状态均值
    pub actual_mean: Option<Snapshot>,
    /// 替代状态均值
    pub alternate_mean: Option<Snapshot>,
}

/// 一次 whatif 的结果
#[derive(Debug, Serialize)]
pub struct WhatifReport {
    /// 局号
    pub game: u64,
    /// 分叉回合
    pub turn: i32,
    /// 观察窗口（`0` = 跑满终局）
    pub horizon: i32,
    /// 重复组数
    pub k: u32,
    /// 基线动作来源（`history` = 帧反推 / `default` = 策略默认）
    pub actual_source: String,
    /// 基线分支
    pub actual: Branch,
    /// 替代分支
    pub alternates: Vec<Branch>,
    /// 各替代的首个动作分歧回合
    pub diverge_turns: Vec<Option<i32>>,
    /// 各替代的逐回合动作分布
    pub dists: Vec<Vec<TurnDist>>,
}

/// 跑一次 whatif，落盘 `whatif.json` 并返回（路径，结果）
pub fn whatif(args: WhatifArgs<'_>) -> Result<(PathBuf, WhatifReport)> {
    let files = GameFiles::scan(args.input, args.game)?;
    let data_root = args
        .data_root
        .map_or_else(matsuri_core::testing::data_dir, Path::to_path_buf);
    matsuri_ai::session::ensure_data(&data_root)?;
    let engine = umaai::Config::load_with_overlay_file(&data_root, ".")?;
    let data = Arc::new(MatsuriData::load(&data_root)?);
    let config = MatsuriConfig::load_with_overlay_file(&data_root, ".")?;
    let mut session = open_session(&engine, Arc::clone(&data), config.clone())?;

    // 同回合可能多帧（事件待选 / 教诲分配 / 训练）：取第一个可决策的 Action 帧
    let mut picked: Option<(ParsedFrame, MatsuriGame)> = None;
    let mut frames: Vec<_> = files
        .frames
        .iter()
        .filter(|frame| parse_frame_name(&frame.name).is_some_and(|(turn, _)| turn == args.turn))
        .collect();
    frames.sort_by_key(|frame| parse_frame_name(&frame.name).map_or(u32::MAX, |(_, seq)| seq));
    if frames.is_empty() {
        bail!("局包里没有 T{} 的回合帧", args.turn);
    }
    for frame in frames {
        let Ok(parsed) = ParsedFrame::parse(&frame.contents) else {
            continue;
        };
        let Ok(game) = session.replay_frame(&frame.contents) else {
            continue;
        };
        match matsuri_ai::online::route(&game, &parsed) {
            // 教诲帧优先：同回合常同时有训练帧与教诲分配帧（special + ps=51）
            Route::Advice(_) => {
                picked = Some((parsed, game));
                break;
            }
            Route::Train if picked.is_none() => {
                picked = Some((parsed, game));
            }
            _ => {}
        }
    }
    let (parsed, mut base) = picked.ok_or_else(|| {
        anyhow::anyhow!(
            "T{} 没有可决策的 Action 帧（事件待选或结算中间态），current whatif 不支持",
            args.turn
        )
    })?;

    // 按路由取候选空间：训练走 list_actions，教诲走 120 排列，事件不支持
    let mut advice_ctx: Option<AdviceCtx> = None;
    let (kind, actions) = match matsuri_ai::online::route(&base, &parsed) {
        Route::Train => ("train", base.list_actions()?),
        Route::Advice(window) => {
            if !base.advice.acquired || base.advice.slots == [0; 5] {
                base.advice.acquired = true;
                base.advice.slots = [101, 201, 301, 401, 501];
                base.advice.fused = [0; 5];
            }
            if base.is_fusion_window(window) {
                let pairs = if window == 2 {
                    &config.deck.fusion.year2
                } else {
                    &config.deck.fusion.year3
                };
                if !pairs
                    .iter()
                    .all(|pair| base.advice.fused.contains(&pair.target))
                {
                    base.apply_fusion_pairs(pairs)?;
                }
            }
            base.stage = matsuri_ai::online::advice_stage(&base);
            advice_ctx = Some(AdviceCtx {
                slots: base.advice.slots,
                fused: base.advice.fused,
            });
            ("advice", base.advice_actions())
        }
        Route::Event => bail!("whatif 暂不支持事件回合（根动作是选项而非 Action）"),
        Route::Skip(reason) => bail!("目标帧不可决策：{reason}"),
    };

    let history = history_actions(&files)?;
    let choices = parsed
        .status
        .story
        .as_ref()
        .map(|story| story.choices.len());
    let actual_position = actual_choice(
        &files,
        &base,
        kind,
        args.turn,
        &history,
        Some(&actions),
        advice_ctx.as_ref(),
        choices,
    );
    let actual_source = if actual_position.is_some() {
        "history".to_string()
    } else {
        "default".to_string()
    };

    let swaps: Vec<&str> = args
        .swap
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    if swaps.is_empty() {
        bail!("--swap 为空");
    }
    if actions.len() <= 1 {
        let only = actions
            .first()
            .map_or_else(|| "无候选".to_string(), |action| label_of(&base, action));
        let hint = if base.plan.advice_window.is_some() {
            "（该回合是教诲窗口，但局包里没有教诲分配帧 special+ps=51）"
        } else {
            ""
        };
        bail!(
            "T{} 只有 1 个候选（{only}）：强制比赛 / 本番回合没有替代可选{hint}",
            args.turn
        );
    }
    let labels: Vec<String> = actions
        .iter()
        .map(|action| label_of(&base, action))
        .collect();
    let swap_positions: Vec<usize> = swaps
        .iter()
        .map(|name| resolve_action(&labels, name))
        .collect::<Result<Vec<_>>>()?;

    let master = engine.sim.seed;
    let stop_turn = if args.horizon <= 0 {
        i32::MAX
    } else {
        args.turn + args.horizon
    };
    let trainer = MatsuriTrainer::new(config.clone());

    let mut actual_groups: Vec<Vec<StepRow>> = Vec::new();
    let mut alternate_groups: Vec<Vec<Vec<StepRow>>> = vec![Vec::new(); swaps.len()];
    let mut actual_finals: Vec<f64> = Vec::new();
    let mut alternate_finals: Vec<Vec<f64>> = vec![Vec::new(); swaps.len()];
    let mut actual_root = String::new();
    let mut alternate_roots: Vec<String> = vec![String::new(); swaps.len()];
    for group in 0..args.k.max(1) {
        let seed = derive_seed(
            master,
            &[
                args.turn as u64,
                0xA1 + u64::from(group),
                StreamTag::Probe.tag(),
            ],
        );
        let (rows, root, final_score) =
            run_branch(&base, &actions, actual_position, &trainer, stop_turn, seed)?;
        if group == 0 {
            actual_root = root;
        }
        actual_groups.push(rows);
        if let Some(score) = final_score {
            actual_finals.push(score);
        }
        for (index, position) in swap_positions.iter().enumerate() {
            let (rows, root, final_score) =
                run_branch(&base, &actions, Some(*position), &trainer, stop_turn, seed)?;
            if group == 0 {
                alternate_roots[index] = root;
            }
            alternate_groups[index].push(rows);
            if let Some(score) = final_score {
                alternate_finals[index].push(score);
            }
        }
    }

    let mean = |values: &[f64]| -> Option<f64> {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let alternates: Vec<Branch> = swaps
        .iter()
        .enumerate()
        .map(|(index, name)| Branch {
            label: format!("替代（{name}）"),
            root_action: alternate_roots[index].clone(),
            rows: alternate_groups[index].first().cloned().unwrap_or_default(),
            final_score: mean(&alternate_finals[index]),
        })
        .collect();
    let diverge_turns: Vec<Option<i32>> = (0..swaps.len())
        .map(|index| first_diverge(&actual_groups[0], &alternate_groups[index][0]))
        .collect();
    let dists: Vec<Vec<TurnDist>> = (0..swaps.len())
        .map(|index| aggregate(&actual_groups, &alternate_groups[index]))
        .collect();

    let actual_label = if actual_source == "history" {
        "历史实际"
    } else {
        "策略默认"
    };
    let report = WhatifReport {
        game: files.game,
        turn: args.turn,
        horizon: args.horizon,
        k: args.k,
        actual_source,
        actual: Branch {
            label: actual_label.to_string(),
            root_action: actual_root,
            rows: actual_groups.first().cloned().unwrap_or_default(),
            final_score: mean(&actual_finals),
        },
        alternates,
        diverge_turns,
        dists,
    };

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
    std::fs::create_dir_all(&out)?;
    let path = out.join("whatif.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    Ok((path, report))
}

/// 跑一条分支，返回（逐回合快照，根动作描述，终局分）
fn run_branch(
    base: &MatsuriGame,
    actions: &[Action],
    forced: Option<usize>,
    trainer: &MatsuriTrainer,
    stop_turn: i32,
    seed: u64,
) -> Result<(Vec<StepRow>, String, Option<f64>)> {
    let mut sim = base.clone();
    sim.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    let root_index = match forced {
        Some(index) => index,
        None => trainer.select_action(&sim, actions, &mut rng)?,
    };
    let root = actions[root_index].clone();
    let root_label = label_of(&sim, &root);
    let mut rows = vec![snapshot(&sim, &root)];
    sim.apply_root_action(&root, &mut rng)?;
    sim.finish_root_stage(&root, trainer, &mut rng)?;
    let trace = TracePolicy {
        inner: trainer,
        rows: RefCell::new(Vec::new()),
    };
    while sim.next() {
        if sim.turn() > stop_turn {
            break;
        }
        sim.run_stage(&trace, &mut rng)?;
    }
    rows.extend(trace.rows.into_inner());
    let final_score = if stop_turn == i32::MAX {
        sim.search_score(1.0).ok()
    } else {
        None
    };
    Ok((rows, root_label, final_score))
}

/// 逐回合记录决策的训练员包装
struct TracePolicy<'a> {
    inner: &'a MatsuriTrainer,
    rows: RefCell<Vec<StepRow>>,
}

impl Trainer<MatsuriGame> for TracePolicy<'_> {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let index = self.inner.select_action(game, actions, rng)?;
        self.rows.borrow_mut().push(snapshot(game, &actions[index]));
        Ok(index)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

/// 动作展示名（教诲用槽位映射，与 `eval` 候选描述同口径）
fn label_of(game: &MatsuriGame, action: &Action) -> String {
    match action {
        Action::AssignAdvice(assign) => assign.describe(&game.advice.slots, &game.advice.fused),
        other => other.decision_label(),
    }
}

/// 回合快照
fn snapshot(game: &MatsuriGame, action: &Action) -> StepRow {
    let view = game.view();
    let parts = game.uma().score_parts().ok();
    StepRow {
        turn: view.turn.turn,
        action: label_of(game, action),
        vital: view.uma.vital,
        motivation: view.uma.motivation,
        // 帧 / view 传的是真实值，统一转显示值（超 1200 减半）后再展示
        five: std::array::from_fn(|axis| display(view.uma.five_status[axis])),
        five_score: parts.map_or(0, |parts| parts.five_status.iter().sum()),
        act_level: view.resources.act_level,
        skill_pt: view.uma.skill_pt,
        pt: parts.map_or(0, |parts| parts.pt),
        skill: parts.map_or(0, |parts| parts.skill),
        score: game.search_score(1.0).unwrap_or(f64::NAN),
    }
}

/// 按动作名解析（精确 → 唯一前缀；歧义或缺失报错并列出候选）
fn resolve_action(labels: &[String], query: &str) -> Result<usize> {
    let exact: Vec<usize> = labels
        .iter()
        .enumerate()
        .filter(|(_, label)| label.as_str() == query)
        .map(|(index, _)| index)
        .collect();
    match exact.len() {
        1 => return Ok(exact[0]),
        0 => {}
        _ => bail!("「{query}」命中多个候选：{}", list(labels, &exact)),
    }
    let prefix: Vec<usize> = labels
        .iter()
        .enumerate()
        .filter(|(_, label)| label.starts_with(query))
        .map(|(index, _)| index)
        .collect();
    match prefix.len() {
        1 => Ok(prefix[0]),
        0 => {
            let all: Vec<usize> = (0..labels.len()).collect();
            bail!("本回合候选里没有「{query}」；可选：{}", list(labels, &all))
        }
        _ => bail!("「{query}」前缀匹配到多个候选：{}", list(labels, &prefix)),
    }
}

/// 候选名列表
fn list(labels: &[String], positions: &[usize]) -> String {
    positions
        .iter()
        .filter_map(|position| labels.get(*position))
        .cloned()
        .collect::<Vec<_>>()
        .join("；")
}

/// 按回合聚合各组的动作分布
fn aggregate(actual: &[Vec<StepRow>], alternate: &[Vec<StepRow>]) -> Vec<TurnDist> {
    let mut turns: BTreeSet<i32> = BTreeSet::new();
    for rows in actual.iter().chain(alternate.iter()) {
        for row in rows {
            turns.insert(row.turn);
        }
    }
    turns
        .into_iter()
        .map(|turn| TurnDist {
            turn,
            actual: count(actual, turn),
            alternate: count(alternate, turn),
            actual_mean: mean_rows(actual, turn),
            alternate_mean: mean_rows(alternate, turn),
        })
        .collect()
}

/// 单回合某分支的动作计数（降序）
fn count(groups: &[Vec<StepRow>], turn: i32) -> Vec<(String, u32)> {
    let mut map: BTreeMap<String, u32> = BTreeMap::new();
    for rows in groups {
        if let Some(row) = rows.iter().find(|row| row.turn == turn) {
            *map.entry(row.action.clone()).or_default() += 1;
        }
    }
    let mut rows: Vec<(String, u32)> = map.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows
}

/// 单回合某分支的状态均值；该回合无样本返回 `None`
fn mean_rows(groups: &[Vec<StepRow>], turn: i32) -> Option<Snapshot> {
    let rows: Vec<&StepRow> = groups
        .iter()
        .filter_map(|rows| rows.iter().find(|row| row.turn == turn))
        .collect();
    if rows.is_empty() {
        return None;
    }
    let n = rows.len() as f64;
    let avg = |pick: &dyn Fn(&StepRow) -> f64| rows.iter().map(|row| pick(row)).sum::<f64>() / n;
    Some(Snapshot {
        vital: avg(&|row| f64::from(row.vital)),
        motivation: avg(&|row| f64::from(row.motivation)),
        five: avg(&|row| row.five.iter().sum::<i32>() as f64),
        five_parts: std::array::from_fn(|axis| avg(&|row| f64::from(row.five[axis]))),
        five_score: avg(&|row| f64::from(row.five_score)),
        act: avg(&|row| row.act_level.iter().sum::<i32>() as f64),
        skill_pt: avg(&|row| f64::from(row.skill_pt)),
        pt: avg(&|row| f64::from(row.pt)),
        skill: avg(&|row| f64::from(row.skill)),
        score: avg(&|row| row.score),
    })
}

/// 两条分支的首个动作分歧回合
fn first_diverge(actual: &[StepRow], alternate: &[StepRow]) -> Option<i32> {
    actual
        .iter()
        .zip(alternate.iter())
        .find(|(a, b)| a.turn == b.turn && a.action != b.action)
        .map(|(a, _)| a.turn)
}
