//! 节奏基线：模拟局逐回合采集「友人解锁 / 出行」与「演目 Lv 上升」的轨迹。
//!
//! 手动跑（仓根，先 cargo build --release --offline -p matsuri-tools --bin rhythm）：
//! - rhythm --deck speed_build  --search-n 2048 --games 50
//! - rhythm --deck wisdom_build --search-n 2048 --games 50
//! - rhythm --deck speed_build  --search-n 0    --games 50（手写档同种子对照）
//!
//! 采集点 = select_action 首次进入该回合时取 view() 快照（上一回合 End 结算后）；
//! 所有「到达」时刻统一记「可见回合 − 1」（快照是决策前）。产物写
//! benchmark/_local/acts_friend/（原始 CSV 不入库）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use matsuri_core::game::Action;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, Qie,
    SearchCandidate, SearchSummary, build_game, deck_comp,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umasim::game::{FriendOutState, Game, Trainer};

/// 两套卡组夹具（benchmark/decks/*.csv 主行：5 张训练卡 + 友人 30323）
const DECKS: [(&str, [u32; 6]); 2] = [
    (
        "speed_build",
        [303174, 303114, 303244, 303224, 303204, 303234],
    ),
    (
        "wisdom_build",
        [303114, 303244, 303224, 303194, 302384, 303234],
    ),
];

/// 年末观察点：第一 / 二 / 三年结束后的首个决策点
const YEAR_ENDS: [i32; 3] = [24, 48, 68];
/// 発表会回合与要求的演目 Lv 下限（B1 同口径）
const MILESTONES: [(i32, i32); 3] = [(23, 3), (47, 5), (67, 6)];
/// A1 观察的演目 Lv 档
const REACH_LEVELS: [i32; 3] = [6, 8, 10];

/// 每回合决策前的只读快照
#[derive(Clone, Copy)]
struct Snap {
    lv: [i32; 5],
    out_state: u8,
    used: usize,
    camp_up: bool,
    camp_active: bool,
}

/// 一个回合的采集记录（act = 该回合实际执行的动作，教诲不覆盖）
struct TurnRec {
    turn: i32,
    snap: Snap,
    act: Option<String>,
}

/// 单局采集结果
struct Run {
    seed: u64,
    score: i32,
    pt: i32,
    final_lv: [i32; 5],
    recs: Vec<TurnRec>,
    outings: Vec<OutingRow>,
}

/// 一局分析出的行
struct GameRow {
    seed: u64,
    score: i32,
    pt: i32,
    final_lv: [i32; 5],
    lv_sum: i32,
    t_unlock: Option<i32>,
    t_out: [Option<i32>; 5],
    done5: bool,
    done5_before23: bool,
    reach: [[Option<i32>; 3]; 5],
    year_lv: [[Option<i32>; 5]; 3],
    milestone_ok: [bool; 3],
    keiko_ok: usize,
    keiko_fail: usize,
    events: Vec<EventRow>,
}

/// 一次演目 Lv +1 的归因
#[derive(Clone, Copy)]
struct EventRow {
    turn: i32,
    act: usize,
    source: Source,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Keiko = 0,
    Camp = 1,
    Outing = 2,
    Other = 3,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Source::Keiko => "keiko",
            Source::Camp => "camp",
            Source::Outing => "outing",
            Source::Other => "other",
        }
    }
}

/// 装载剧本数据（引擎配置单独按档覆写卡组 / 马娘）
struct Env {
    root: PathBuf,
    data: Arc<MatsuriData>,
}

impl Env {
    fn load() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("crate 目录上溯两级即仓根")?
            .to_path_buf();
        let data_root = root.join("data");
        Data::init(&data_root).context("引擎静态数据初始化失败")?;
        let data = Arc::new(MatsuriData::load(&data_root).context("载入剧本数据失败")?);
        Ok(Self { root, data })
    }
}

/// 回合内动作标签：教诲记 advice（不覆盖执行动作）
fn action_label(action: &Action) -> String {
    match action {
        Action::Train(slot) => format!("train{slot}"),
        Action::ActTrain(act) => format!("keiko{act}"),
        Action::Race => "race".to_string(),
        Action::Rest => "rest".to_string(),
        Action::NormalOuting => "normal_outing".to_string(),
        Action::FriendOuting => "friend_outing".to_string(),
        Action::Clinic => "clinic".to_string(),
        Action::AssignAdvice(_) => "advice".to_string(),
    }
}

/// 友人状态 → 0 未点击 / 1 已点击未解锁 / 2 已解锁 / 3 离开
fn out_state_code(state: FriendOutState) -> u8 {
    match state {
        FriendOutState::UnClicked => 0,
        FriendOutState::BeforeUnlock => 1,
        FriendOutState::AfterUnlock => 2,
        FriendOutState::Away => 3,
    }
}

/// 搜索档取最近一次候选表（手写档没有候选分）
trait SearchProbe {
    fn last_summary(&self) -> Option<SearchSummary>;
}

impl SearchProbe for MatsuriSearchTrainer {
    fn last_summary(&self) -> Option<SearchSummary> {
        self.last_search()
    }
}

impl SearchProbe for MatsuriTrainer {
    fn last_summary(&self) -> Option<SearchSummary> {
        None
    }
}

/// 一次「含出行候选」的决策点记录（只用于诊断出行候选的估分位置）
struct OutingRow {
    turn: i32,
    rank: usize,
    out_score: f64,
    best_score: f64,
    diff: f64,
    stdev: f64,
    cands: usize,
    selected: bool,
    chosen_label: String,
    best_label: String,
    out_label: String,
}

/// 逐回合快照包装（内层为搜索或手写训练员）
struct RhythmTrainer<T> {
    inner: T,
    turns: Mutex<Vec<TurnRec>>,
    outings: Mutex<Vec<OutingRow>>,
}

impl<T> RhythmTrainer<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            turns: Mutex::new(Vec::new()),
            outings: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<TurnRec> {
        self.turns
            .lock()
            .map(|mut turns| std::mem::take(&mut *turns))
            .unwrap_or_default()
    }

    fn take_outings(&self) -> Vec<OutingRow> {
        self.outings
            .lock()
            .map(|mut rows| std::mem::take(&mut *rows))
            .unwrap_or_default()
    }

    /// 记录该决策点里出行候选的分数与排名（仅搜索档有候选分）
    fn record_outing(&self, turn: i32, actions: &[Action], chosen: usize, summary: &SearchSummary) {
        let Some(outing_index) = actions
            .iter()
            .position(|action| matches!(action, Action::FriendOuting))
        else {
            return;
        };
        let Some(outing) = summary
            .candidates
            .iter()
            .find(|candidate| candidate.index == outing_index)
        else {
            return;
        };
        let mut ranked: Vec<&SearchCandidate> = summary.candidates.iter().collect();
        ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
        let rank = ranked
            .iter()
            .position(|candidate| candidate.index == outing_index)
            .map_or(ranked.len(), |position| position + 1);
        let best = ranked.first().copied();
        let row = OutingRow {
            turn,
            rank,
            out_score: outing.score,
            best_score: best.map_or(outing.score, |candidate| candidate.score),
            diff: best.map_or(0.0, |candidate| candidate.score - outing.score),
            stdev: outing.stdev,
            cands: summary.candidates.len(),
            selected: chosen == outing_index,
            chosen_label: actions.get(chosen).map(action_label).unwrap_or_default(),
            best_label: best
                .map(|candidate| candidate.description.clone())
                .unwrap_or_default(),
            out_label: outing.description.clone(),
        };
        if let Ok(mut rows) = self.outings.lock() {
            rows.push(row);
        }
    }
}

impl<T: Trainer<MatsuriGame> + SearchProbe> Trainer<MatsuriGame> for RhythmTrainer<T> {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let index = self.inner.select_action(game, actions, rng)?;
        let turn = game.turn();
        let label = actions.get(index).map(action_label);
        let snapshot = {
            let view = game.view();
            Snap {
                lv: view.resources.act_level,
                out_state: out_state_code(view.friend.out_state),
                used: view.friend.out_used.iter().filter(|used| **used).count(),
                camp_up: view.camp.level_up,
                camp_active: view.camp.active,
            }
        };
        if let Some(summary) = self.inner.last_summary() {
            self.record_outing(turn, actions, index, &summary);
        }
        if let Ok(mut turns) = self.turns.lock() {
            match turns.last_mut() {
                Some(last) if last.turn == turn => {
                    if let Some(label) = label.filter(|label| label != "advice") {
                        last.act = Some(label);
                    }
                }
                _ => {
                    let act = label.filter(|label| label != "advice");
                    turns.push(TurnRec {
                        turn,
                        snap: snapshot,
                        act,
                    });
                }
            }
        }
        Ok(index)
    }

    fn select_choice(
        &self,
        game: &MatsuriGame,
        choices: &[Vec<umaai_core::gamedata::EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &MatsuriGame,
        event: &umaai_core::gamedata::EventData,
        choices: &[Vec<umaai_core::gamedata::EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

/// 推进一局（与 bench 的 play_with_engine 同序）
fn play_one<T: Trainer<MatsuriGame>>(
    env: &Env,
    engine: &umaai::Config,
    config: &MatsuriConfig,
    seed: u64,
    trainer: &T,
) -> Result<(i32, i32, [i32; 5])> {
    let mut game = build_game(engine, env.data.clone())
        .context("建局失败")?
        .with_config(config.clone());
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(trainer, &mut rng)?;
    while game.next() {
        game.run_stage(trainer, &mut rng)?;
    }
    game.on_simulation_end(trainer, &mut rng)?;
    let view = game.view();
    Ok((
        game.uma().calc_score()?,
        game.uma().total_pt()?,
        view.resources.act_level,
    ))
}

/// 批量跑局并收集逐回合记录
fn run_games<T: Trainer<MatsuriGame> + SearchProbe>(
    env: &Env,
    engine: &umaai::Config,
    config: &MatsuriConfig,
    trainer: &RhythmTrainer<T>,
    games: usize,
    seed0: u64,
) -> Result<Vec<Run>> {
    let mut runs = Vec::with_capacity(games);
    for index in 0..games {
        let seed = seed0 + index as u64;
        let (score, pt, final_lv) = play_one(env, engine, config, seed, trainer)?;
        runs.push(Run {
            seed,
            score,
            pt,
            final_lv,
            recs: trainer.take(),
            outings: trainer.take_outings(),
        });
    }
    Ok(runs)
}

/// 单局 → 指标行（含 Lv 增量归因）
fn analyze(run: &Run) -> GameRow {
    let recs = &run.recs;
    let t_unlock = recs
        .iter()
        .find(|rec| rec.snap.out_state >= 2)
        .map(|rec| rec.turn - 1);
    let t_out: [Option<i32>; 5] = std::array::from_fn(|k| {
        recs.iter()
            .find(|rec| rec.snap.used > k)
            .map(|rec| rec.turn - 1)
    });
    let reach: [[Option<i32>; 3]; 5] = std::array::from_fn(|act| {
        std::array::from_fn(|i| {
            recs.iter()
                .find(|rec| rec.snap.lv[act] >= REACH_LEVELS[i])
                .map(|rec| rec.turn - 1)
        })
    });
    let year_lv: [[Option<i32>; 5]; 3] = std::array::from_fn(|year| {
        std::array::from_fn(|act| {
            recs.iter()
                .find(|rec| rec.turn == YEAR_ENDS[year])
                .map(|rec| rec.snap.lv[act])
        })
    });
    let milestone_ok: [bool; 3] = std::array::from_fn(|i| {
        let (turn, need) = MILESTONES[i];
        recs.iter()
            .find(|rec| rec.turn == turn)
            .is_some_and(|rec| rec.snap.lv.iter().all(|level| *level >= need))
    });

    let mut events = Vec::new();
    let mut keiko_ok = 0;
    let mut keiko_fail = 0;
    for pair in recs.windows(2) {
        let prev = &pair[0];
        let next = &pair[1];
        for act in 0..5 {
            let delta = next.snap.lv[act] - prev.snap.lv[act];
            if delta > 0 {
                let keiko_label = format!("keiko{act}");
                let source = if prev.snap.camp_up {
                    Source::Camp
                } else {
                    match prev.act.as_deref() {
                        Some("friend_outing") => Source::Outing,
                        Some(label) if label == keiko_label => Source::Keiko,
                        _ => Source::Other,
                    }
                };
                if source == Source::Keiko {
                    keiko_ok += 1;
                }
                for _ in 0..delta {
                    events.push(EventRow {
                        turn: prev.turn,
                        act,
                        source,
                    });
                }
            } else if prev.act.as_deref() == Some(&format!("keiko{act}"))
                && !prev.snap.camp_up
                && !prev.snap.camp_active
            {
                keiko_fail += 1;
            }
        }
    }

    GameRow {
        seed: run.seed,
        score: run.score,
        pt: run.pt,
        final_lv: run.final_lv,
        lv_sum: run.final_lv.iter().sum(),
        t_unlock,
        t_out,
        done5: t_out[4].is_some(),
        done5_before23: t_out[4].is_some_and(|turn| turn <= 23),
        reach,
        year_lv,
        milestone_ok,
        keiko_ok,
        keiko_fail,
        events,
    }
}

/// 线性插值分位（空集返回 NaN）
fn quantile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let pos = q * (values.len() as f64 - 1.0);
    let low = pos.floor() as usize;
    let high = pos.ceil() as usize;
    if low == high {
        return values[low];
    }
    let weight = pos - low as f64;
    values[low] * (1.0 - weight) + values[high] * weight
}

/// 分位串 P10/P50/P90
fn fmt_p10_50_90(values: &[f64]) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    let mut copy = values.to_vec();
    let p10 = quantile(&mut copy, 0.10);
    let mut copy = values.to_vec();
    let p50 = quantile(&mut copy, 0.50);
    let mut copy = values.to_vec();
    let p90 = quantile(&mut copy, 0.90);
    format!("{p10:.1}/{p50:.1}/{p90:.1}")
}

/// 中位数
fn median(values: &[f64]) -> f64 {
    let mut copy = values.to_vec();
    quantile(&mut copy, 0.5)
}

fn parse_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn parse_usize(args: &[String], name: &str, default: usize) -> Result<usize> {
    match parse_arg(args, name) {
        Some(raw) => raw.parse().with_context(|| format!("{name} 非法: {raw}")),
        None => Ok(default),
    }
}

fn parse_i32(args: &[String], name: &str, default: i32) -> Result<i32> {
    match parse_arg(args, name) {
        Some(raw) => raw.parse().with_context(|| format!("{name} 非法: {raw}")),
        None => Ok(default),
    }
}

fn main() -> Result<()> {
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let env = Env::load()?;

    let deck_name = parse_arg(&args, "--deck").unwrap_or_else(|| "speed_build".to_string());
    let Some((_, deck)) = DECKS.iter().find(|(name, _)| *name == deck_name) else {
        bail!("未知 --deck {deck_name}（可选 speed_build / wisdom_build）");
    };
    let games = parse_usize(&args, "--games", 50)?;
    let search_n = parse_usize(&args, "--search-n", 2048)?;
    let seed0 = parse_usize(&args, "--seed", 73_200)? as u64;
    let uma = parse_usize(&args, "--uma", 105_902)? as u32;
    let shield = parse_i32(&args, "--shield", 0)?;
    let qie = match parse_arg(&args, "--qie").as_deref() {
        None | Some("none") => Qie::None,
        Some("xiaoqie") => Qie::Xiaoqie,
        Some("qiezhe") => Qie::Qiezhe,
        Some(other) => bail!("--qie 只能取 none / xiaoqie / qiezhe（收到 {other}）"),
    };
    let out_dir = parse_arg(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env.root
                .join("benchmark")
                .join("_local")
                .join("acts_friend")
        });
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("创建输出目录失败: {}", out_dir.display()))?;

    let data_root = env.root.join("data");
    let mut engine =
        umaai::Config::load_with_overlay_file(&data_root, ".").context("载入引擎配置失败")?;
    engine.sim.uma = uma;
    engine.sim.cards = deck.to_vec();
    let comp = deck_comp(&engine.sim.cards).context("卡组构成解析失败")?;
    let base = MatsuriConfig::load_with_overlay_file(&data_root, ".")
        .context("载入剧本配置失败")?
        .for_deck(comp);
    let mut config = base.clone();
    config.pre.qie = qie;
    config.pre.positive_thinking = shield;
    config.search.search_n = search_n;
    let profile = base.decks.profile_name(comp).to_string();
    let trainer_name = if search_n == 0 { "hand" } else { "search" };

    println!(
        "# deck={deck_name} profile={profile} comp={comp:?} trainer={trainer_name} n={search_n} games={games} seed0={seed0} uma={uma} qie={qie:?} shield={shield}"
    );
    let started = std::time::Instant::now();
    let runs = if search_n > 0 {
        let inner = MatsuriSearchTrainer::new(
            config.clone(),
            engine.search.clone().with_search_n(search_n),
        )
        .context("构造搜索训练员失败")?;
        let trainer = RhythmTrainer::new(inner);
        run_games(&env, &engine, &config, &trainer, games, seed0)?
    } else {
        let trainer = RhythmTrainer::new(MatsuriTrainer::new(config.clone()));
        run_games(&env, &engine, &config, &trainer, games, seed0)?
    };
    let elapsed = started.elapsed().as_secs_f64();

    let rows: Vec<GameRow> = runs.iter().map(analyze).collect();
    write_games_csv(&out_dir, &deck_name, trainer_name, search_n, &rows)?;
    write_events_csv(&out_dir, &deck_name, trainer_name, search_n, &rows)?;
    write_outing_csv(&out_dir, &deck_name, trainer_name, search_n, &runs)?;
    report(&rows, elapsed);
    println!("\n# 产物目录：{}", out_dir.display());
    Ok(())
}

fn csv_name(deck: &str, trainer: &str, n: usize, suffix: &str) -> String {
    format!("{deck}_{trainer}{n}_{suffix}.csv")
}

fn write_games_csv(
    dir: &Path,
    deck: &str,
    trainer: &str,
    n: usize,
    rows: &[GameRow],
) -> Result<()> {
    let path = dir.join(csv_name(deck, trainer, n, "games"));
    let mut text = String::from(
        "seed,score,pt,lv_sum,lv_s,lv_p,lv_w,lv_g,lv_i,t_unlock,t_out1,t_out2,t_out3,t_out4,t_out5,done5,done5_before23,keiko_ok,keiko_fail,milestone23,milestone47,milestone67\n",
    );
    for row in rows {
        let lv = row.final_lv;
        let out = |turn: Option<i32>| turn.map_or_else(|| "-1".to_string(), |v| v.to_string());
        text.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            row.seed,
            row.score,
            row.pt,
            row.lv_sum,
            lv[0],
            lv[1],
            lv[2],
            lv[3],
            lv[4],
            out(row.t_unlock),
            out(row.t_out[0]),
            out(row.t_out[1]),
            out(row.t_out[2]),
            out(row.t_out[3]),
            out(row.t_out[4]),
            u8::from(row.done5),
            u8::from(row.done5_before23),
            row.keiko_ok,
            row.keiko_fail,
            u8::from(row.milestone_ok[0]),
            u8::from(row.milestone_ok[1]),
            u8::from(row.milestone_ok[2]),
        ));
    }
    std::fs::write(&path, text).with_context(|| format!("写 {} 失败", path.display()))?;

    // 演目到达与年末等级另出一张表，便于直接做分位
    let reach_path = dir.join(csv_name(deck, trainer, n, "reach"));
    let mut reach_text =
        String::from("seed,act,reach6,reach8,reach10,lv_at_24,lv_at_48,lv_at_68\n");
    for row in rows {
        for act in 0..5 {
            let cell =
                |value: Option<i32>| value.map_or_else(|| "-1".to_string(), |v| v.to_string());
            reach_text.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                row.seed,
                act,
                cell(row.reach[act][0]),
                cell(row.reach[act][1]),
                cell(row.reach[act][2]),
                cell(row.year_lv[0][act]),
                cell(row.year_lv[1][act]),
                cell(row.year_lv[2][act]),
            ));
        }
    }
    std::fs::write(&reach_path, reach_text)
        .with_context(|| format!("写 {} 失败", reach_path.display()))?;
    Ok(())
}

fn write_events_csv(
    dir: &Path,
    deck: &str,
    trainer: &str,
    n: usize,
    rows: &[GameRow],
) -> Result<()> {
    let path = dir.join(csv_name(deck, trainer, n, "level_events"));
    let mut text = String::from("seed,turn,act,source\n");
    for row in rows {
        for event in &row.events {
            text.push_str(&format!(
                "{},{},{},{}\n",
                row.seed,
                event.turn,
                event.act,
                event.source.name()
            ));
        }
    }
    std::fs::write(&path, text).with_context(|| format!("写 {} 失败", path.display()))?;
    Ok(())
}

/// 出行候选的估分位置（只有搜索档有数据；手写档无候选分）
fn write_outing_csv(dir: &Path, deck: &str, trainer: &str, n: usize, runs: &[Run]) -> Result<()> {
    let path = dir.join(csv_name(deck, trainer, n, "outing_choices"));
    let mut text = String::from(
        "seed,turn,rank,cands,selected,out_score,best_score,diff,stdev,chosen,best,out
",
    );
    for run in runs {
        for row in &run.outings {
            let clean = |value: &str| value.replace(',', " ");
            text.push_str(&format!(
                "{},{},{},{},{},{:.1},{:.1},{:.1},{:.1},{},{},{}
",
                run.seed,
                row.turn,
                row.rank,
                row.cands,
                u8::from(row.selected),
                row.out_score,
                row.best_score,
                row.diff,
                row.stdev,
                clean(&row.chosen_label),
                clean(&row.best_label),
                clean(&row.out_label),
            ));
        }
    }
    std::fs::write(&path, text).with_context(|| format!("写 {} 失败", path.display()))?;
    Ok(())
}

/// 汇总输出：友人节奏 / 演目到达 / 年末等级 / 归因 / 终局
fn report(rows: &[GameRow], elapsed: f64) {
    let n = rows.len();
    println!("\n## 友人（吾妻）n={n}");
    let mut unlock = Vec::new();
    let mut unlock_before11 = 0;
    let mut first_out = Vec::new();
    let mut done5 = Vec::new();
    let mut done5_count = 0;
    let mut done5_before23 = 0;
    for row in rows {
        if let Some(turn) = row.t_unlock {
            unlock.push(f64::from(turn));
            if turn <= 11 {
                unlock_before11 += 1;
            }
        }
        if let Some(turn) = row.t_out[0] {
            first_out.push(f64::from(turn));
        }
        if let Some(turn) = row.t_out[4] {
            done5.push(f64::from(turn));
            done5_count += 1;
        }
        if row.done5_before23 {
            done5_before23 += 1;
        }
    }
    println!(
        "解锁回合 P10/P50/P90 = {} | T11 前解锁 {}/{}",
        fmt_p10_50_90(&unlock),
        unlock_before11,
        n
    );
    println!(
        "首段出行 P10/P50/P90 = {} | 5 段走满 P10/P50/P90 = {} | 走满 {}/{} | 第一年内走满 {}/{}",
        fmt_p10_50_90(&first_out),
        fmt_p10_50_90(&done5),
        done5_count,
        n,
        done5_before23,
        n
    );
    let gaps: Vec<f64> = (1..5)
        .map(|k| {
            let mut values = Vec::new();
            for row in rows {
                if let (Some(prev), Some(next)) = (row.t_out[k - 1], row.t_out[k]) {
                    values.push(f64::from(next - prev));
                }
            }
            median(&values)
        })
        .collect();
    println!(
        "段间隔中位（第 k 段 − 第 k-1 段）= {:.1}/{:.1}/{:.1}/{:.1}",
        gaps[0], gaps[1], gaps[2], gaps[3]
    );

    println!("\n## 演目到达（A1：只报 Lv6/8/10）");
    let names = ["速", "耐", "力", "根", "智"];
    for (act, name) in names.iter().enumerate() {
        let mut cells = Vec::new();
        for (index, level) in REACH_LEVELS.iter().enumerate() {
            let mut values = Vec::new();
            let mut missed = 0;
            for row in rows {
                match row.reach[act][index] {
                    Some(turn) => values.push(f64::from(turn)),
                    None => missed += 1,
                }
            }
            cells.push(format!(
                "Lv{level} {} 未达 {missed}/{n}",
                fmt_p10_50_90(&values)
            ));
        }
        println!("{name}：{}", cells.join(" | "));
    }

    println!("\n## 年末等级（A2）与発表会达标");
    for (year, turn) in YEAR_ENDS.iter().enumerate() {
        let mut medians = Vec::new();
        for act in 0..5 {
            let values: Vec<f64> = rows
                .iter()
                .filter_map(|row| row.year_lv[year][act].map(f64::from))
                .collect();
            medians.push(format!("{:.1}", median(&values)));
        }
        println!(
            "T{turn}（第 {} 年末）各演目 Lv 中位（速耐力根智）= {}",
            year + 1,
            medians.join("/")
        );
    }
    for (index, (turn, need)) in MILESTONES.iter().enumerate() {
        let ok = rows.iter().filter(|row| row.milestone_ok[index]).count();
        println!("T{turn} 全演目 ≥{need}：{ok}/{n}");
    }

    println!("\n## Lv 增量归因");
    let mut total = [0usize; 4];
    for row in rows {
        for event in &row.events {
            total[event.source as usize] += 1;
        }
    }
    let sum: usize = total.iter().sum();
    println!(
        "稽古 {}（{:.0}%）| 合宿 {}（{:.0}%）| 出行 {}（{:.0}%）| 其他 {}（{:.0}%）| 合计 {}",
        total[0],
        pct(total[0], sum),
        total[1],
        pct(total[1], sum),
        total[2],
        pct(total[2], sum),
        total[3],
        pct(total[3], sum),
        sum
    );
    let keiko_ok: Vec<f64> = rows.iter().map(|row| row.keiko_ok as f64).collect();
    let keiko_fail: Vec<f64> = rows.iter().map(|row| row.keiko_fail as f64).collect();
    println!(
        "每局稽古成功 {} 次 / 失败 {} 次（中位）",
        median(&keiko_ok),
        median(&keiko_fail)
    );

    println!("\n## 终局");
    let lv_sum: Vec<f64> = rows.iter().map(|row| f64::from(row.lv_sum)).collect();
    let top: Vec<f64> = rows
        .iter()
        .map(|row| row.final_lv.iter().filter(|level| **level >= 10).count() as f64)
        .collect();
    let scores: Vec<f64> = rows.iter().map(|row| f64::from(row.score)).collect();
    println!(
        "Lv 总和 P10/P50/P90 = {} | 达 Lv10 的演目数中位 {:.0} | 终局分 P50 {:.0}",
        fmt_p10_50_90(&lv_sum),
        median(&top),
        median(&scores)
    );
    println!("耗时 {elapsed:.1}s（{:.1}s/局）", elapsed / n.max(1) as f64);
}

fn pct(part: usize, sum: usize) -> f64 {
    if sum == 0 {
        0.0
    } else {
        part as f64 * 100.0 / sum as f64
    }
}
