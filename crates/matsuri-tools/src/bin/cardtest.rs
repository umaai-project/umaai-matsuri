//! 测卡工具：卡池扫描 / 卡组装配 / 多卡组同种子配对跑测
//!
//! 用法（仓根执行，先 `cargo build --release --offline -p matsuri-tools`）：
//! - `cardtest pool [--type 0-4] [--top 12]` 卡池榜（面板分 / 得意率 / 羁绊 / 链接钉位 / 剔除项）
//! - `cardtest build --comp all --pick panel --out decks/panel.csv` 按构成码装配卡组
//! - `cardtest build --comp 21101 --pick measured --topk 6 --games 400` 逐槽实测贪婪
//! - `cardtest run --decks decks/panel.csv --games 1000 --seed 80000` 同种子配对跑测
//! - `cardtest ... --matsuri-config <完整配置>` 测卡 / 扫参专用：显式载入完整
//!   `matsuri_config.toml`（含 `[decks.*]` 变体），不读仓内默认层；玩家 CLI 不暴露该入口
//!
//! 口径：卡组 = 5 训练位 + SSR 吾妻（满破）；构成码 5 位 = 速 / 耐 / 力 / 根 / 智张数；
//! `idrank = card_id * 10 + 4`；卡组顺序固定「类型 0→4 升序 + 友人最后」（与默认层同序）。
//! 选卡：`panel` = 情×2 + 干×0.5 + 训（次键 `card_id` 降序，纯静态剪枝）；
//! `measured` = panel 前 topk 逐槽同种子配对实测贪婪。链接卡只在「卡类型 == 钉位演目」时进池。

use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use matsuri_core::game::deck::inspect_deck;
use matsuri_core::{
    Action, MatsuriConfig, MatsuriData, MatsuriGame, MatsuriTrainer, Qie, build_game,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Array5;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umaai_core::stats::{Correct, paired, paired_runs_for_effect};
use umasim::game::{Game, SupportCard, Trainer};

/// 仓内数据根（相对仓根）
const DATA_ROOT: &str = "data";
/// 演目 / 训练位名称（0–4：速 / 耐 / 力 / 根 / 智）
const ACT_NAMES: [&str; 5] = ["速", "耐", "力", "根", "智"];
/// 满破档（`idrank` 低位）
const MAX_RANK: u32 = 4;
/// 默认种子块起点（与 `sweep` / 基线用过的块错开）
const DEFAULT_SEED: u64 = 80_000;
/// `run` 默认局数
const DEFAULT_RUN_GAMES: usize = 1000;
/// `build --pick measured` 默认局数（配对差分 sd≈3100 → 4000 局可分辨约 140 分）
const DEFAULT_BUILD_GAMES: usize = 4000;
/// 实测档每槽候选宽度（含当前卡）
const DEFAULT_TOPK: usize = 6;
/// 标准卡组构成（用户给定口径：速耐力根智张数 + SSR 吾妻）
const PRESETS: [(&str, [usize; 5]); 4] = [
    ("speed_build", [2, 1, 1, 0, 1]),
    ("stamina_build", [1, 2, 1, 0, 1]),
    ("guts_build", [2, 0, 1, 1, 1]),
    ("wisdom_build", [1, 1, 1, 0, 2]),
];

// ---------------------------------------------------------------------------
// 运行环境
// ---------------------------------------------------------------------------

/// 装载数据与配置（每进程一次）
struct Env {
    engine: umaai::Config,
    data: Arc<MatsuriData>,
    config: MatsuriConfig,
}

impl Env {
    /// 载入数据与配置。
    ///
    /// `--matsuri-config` 是**测卡 / 扫参专用**入口：给了就读显式完整配置，
    /// 不走仓内默认层。玩家终端 / online 不暴露这个入口，`[decks.*]` 的玩家覆盖
    /// 限制仍由 `MatsuriConfigOverlay` 保持。
    fn load(args: &[String]) -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("crate 目录上溯两级即仓根")?;
        let data_root = root.join(DATA_ROOT);
        Data::init(&data_root).context("引擎静态数据初始化失败")?;
        let engine =
            umaai::Config::load_with_overlay_file(&data_root, ".").context("载入引擎配置失败")?;
        let data = Arc::new(MatsuriData::load(&data_root).context("载入剧本数据失败")?);
        let mut config = match parse_arg(args, "--matsuri-config") {
            Some(path) => MatsuriConfig::load(&path, None)
                .with_context(|| format!("载入剧本配置失败: {path}"))?,
            None => MatsuriConfig::load_with_overlay_file(&data_root, ".")
                .context("载入剧本配置失败")?,
        };
        // 本工具只测手写档：默认层已开搜索，这里就地关掉
        if config.search.search_n != 0 {
            eprintln!(
                "# 手写档工具：忽略 search_n={}，本次按 0 跑",
                config.search.search_n
            );
            config.search.search_n = 0;
        }
        Ok(Self {
            engine,
            data,
            config,
        })
    }

    /// 担当马娘的角色 id（`uma_id / 100`）
    fn trained_chara(&self) -> u32 {
        self.engine.sim.uma / 100
    }

    /// 换卡组后的引擎配置（其余局面条件不变）
    fn engine_with(&self, deck: &[u32]) -> umaai::Config {
        let mut engine = self.engine.clone();
        engine.sim.cards = deck.to_vec();
        engine
    }
}

// ---------------------------------------------------------------------------
// 卡池
// ---------------------------------------------------------------------------

/// 卡池条目（满破档物化）
#[derive(Debug, Clone)]
struct CardRep {
    /// 卡 ID × 10 + 满破档
    idrank: u32,
    /// 卡 ID
    card_id: u32,
    /// 角色 ID
    chara_id: u32,
    /// 卡类型（0 速 / 1 耐 / 2 力 / 3 根 / 4 智）
    card_type: i32,
    /// 卡名
    name: String,
    /// 静态面板分（情×2 + 干×0.5 + 训）
    panel: f64,
    /// 友情加成
    qing: f32,
    /// 干劲加成
    gan: i32,
    /// 训练加成
    xun: i32,
    /// 得意率
    deyi: f32,
    /// 初始羁绊
    jiban: i32,
    /// 赛后加成
    saihou: i32,
    /// 剧本链接钉位（chara 命中 `acts[].npc_chara_id` 时给出演目下标）
    link_act: Option<usize>,
    /// 剔除原因（`None` = 可进组）
    reject: Option<String>,
}

impl CardRep {
    /// 是否可进组
    fn eligible(&self) -> bool {
        self.reject.is_none()
    }
}

/// 满破档 idrank
fn idrank(card_id: u32) -> u32 {
    card_id * 10 + MAX_RANK
}

/// 剧本友人位（SSR 吾妻）：`outing` 表里 rarity 3 的最新一张
fn azuma_card_id(data: &MatsuriData) -> Result<u32> {
    let static_data = Data::get()?;
    let mut best: Option<u32> = None;
    for outing in &data.outing {
        let Ok(card) = static_data.get_card(outing.card_id) else {
            continue;
        };
        if card.rarity != 3 {
            continue;
        }
        if best.is_none_or(|old| outing.card_id > old) {
            best = Some(outing.card_id);
        }
    }
    best.context("剧本友人卡里找不到 SSR 吾妻")
}

/// 扫描卡池：SSR 训练卡，剔除担当马娘；链接超位的卡保留但标剔除原因
fn build_pool(env: &Env) -> Result<Vec<CardRep>> {
    let static_data = Data::get()?;
    let link_acts: Vec<(u32, usize)> = env
        .data
        .acts
        .iter()
        .enumerate()
        .map(|(act, info)| (info.npc_chara_id, act))
        .collect();
    let trained = env.trained_chara();
    let mut pool = Vec::new();
    for card in static_data.card.values() {
        if card.rarity != 3 || !(0..5).contains(&card.card_type) {
            continue;
        }
        if card.chara_id == trained {
            continue;
        }
        let Some(value) = card.card_value.get(MAX_RANK as usize) else {
            continue;
        };
        let panel = f64::from(value.youqing) * 2.0
            + f64::from(value.ganjing) * 0.5
            + f64::from(value.xunlian);
        let link_act = link_acts
            .iter()
            .find(|(chara, _)| *chara == card.chara_id)
            .map(|(_, act)| *act);
        // 链接卡：只有卡类型与钉位演目一致才进池，否则测出来是外派效应
        let reject = match link_act {
            Some(act) if act as i32 != card.card_type => {
                Some(format!("链接钉到「{}」", ACT_NAMES[act]))
            }
            _ => None,
        };
        pool.push(CardRep {
            idrank: idrank(card.card_id),
            card_id: card.card_id,
            chara_id: card.chara_id,
            card_type: card.card_type,
            name: card.card_name.clone(),
            panel,
            qing: value.youqing,
            gan: value.ganjing,
            xun: value.xunlian,
            deyi: value.deyilv,
            jiban: value.initial_jiban,
            saihou: value.saihou,
            link_act,
            reject,
        });
    }
    // 可进组优先，其次面板分降序、卡 ID 降序（同分取新卡）
    pool.sort_by(|a, b| {
        b.eligible()
            .cmp(&a.eligible())
            .then(b.panel.total_cmp(&a.panel))
            .then(b.card_id.cmp(&a.card_id))
    });
    Ok(pool)
}

/// 构成码 → 每类型张数（5 位十进制，速耐力根智）
fn parse_comp(raw: &str) -> Result<[usize; 5]> {
    let digits: Vec<char> = raw.chars().collect();
    ensure!(
        digits.len() == 5,
        "构成码须为 5 位（速耐力根智），收到 {raw}"
    );
    let mut comp = [0usize; 5];
    for (slot, digit) in digits.iter().enumerate() {
        let value = digit
            .to_digit(10)
            .with_context(|| format!("构成码含非数字字符: {raw}"))? as usize;
        ensure!(value <= 5, "构成码单类型张数不能超过 5: {raw}");
        comp[slot] = value;
    }
    let total: usize = comp.iter().sum();
    ensure!(total == 5, "构成码训练卡合计须为 5（实际 {total}）: {raw}");
    Ok(comp)
}

/// 构成码文本（`[2,1,1,0,1]` → `21101`）
fn comp_code(comp: [usize; 5]) -> String {
    comp.iter().map(usize::to_string).collect()
}

/// 训练位 → 卡类型（卡组顺序：类型 0→4 升序）
fn slot_types(comp: [usize; 5]) -> Vec<i32> {
    let mut types = Vec::new();
    for (card_type, count) in comp.iter().enumerate() {
        for _ in 0..*count {
            types.push(card_type as i32);
        }
    }
    types
}

/// 装配一套卡组（训练位按面板分降序取，剔除重复人物，末位补 SSR 吾妻）
fn assemble(pool: &[CardRep], comp: [usize; 5], azuma: u32) -> Result<Vec<u32>> {
    let mut used: Vec<u32> = Vec::new();
    let mut deck: Vec<u32> = Vec::new();
    for (card_type, count) in comp.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let mut taken = 0usize;
        for card in pool
            .iter()
            .filter(|card| card.eligible() && card.card_type == card_type as i32)
        {
            if used.contains(&card.chara_id) {
                continue;
            }
            deck.push(card.idrank);
            used.push(card.chara_id);
            taken += 1;
            if taken == *count {
                break;
            }
        }
        ensure!(
            taken == *count,
            "「{}」类型可用卡不足：需 {count} 张，只取到 {taken} 张",
            ACT_NAMES[card_type]
        );
    }
    deck.push(idrank(azuma));
    Ok(deck)
}

/// 从卡组 idrank 反算构成码（训练位按卡类型计数，友人卡不计）
fn comp_of_deck(deck: &[u32]) -> [usize; 5] {
    let mut comp = [0usize; 5];
    let Ok(static_data) = Data::get() else {
        return comp;
    };
    for id in deck {
        if let Ok(card) = static_data.get_card(id / 10)
            && (0..5).contains(&card.card_type)
        {
            comp[card.card_type as usize] += 1;
        }
    }
    comp
}

/// 按 idrank 取池内条目（友人位不在池里，返回 `None`）
fn pool_card(pool: &[CardRep], idrank_value: u32) -> Option<&CardRep> {
    pool.iter().find(|card| card.idrank == idrank_value)
}

/// 卡组种类数（训练卡类型集合 + 友人位 1 张）
fn deck_kinds(pool: &[CardRep], deck: &[u32]) -> usize {
    let types: std::collections::BTreeSet<i32> = deck
        .iter()
        .filter_map(|id| pool_card(pool, *id))
        .map(|card| card.card_type)
        .collect();
    types.len() + 1
}

/// 建局前门禁：`SupportCard` 构造 + `inspect_deck`（禁止项直接报错）
fn validate_deck(env: &Env, deck: &[u32]) -> Result<matsuri_core::DeckReport> {
    let cards = deck
        .iter()
        .map(|&id| SupportCard::new(id))
        .collect::<Result<Vec<_>>>()?;
    let report = inspect_deck(&cards, &env.data, env.trained_chara())?;
    ensure!(report.is_ok(), "编成被拒：{}", report.error_message());
    Ok(report)
}

// ---------------------------------------------------------------------------
// 跑局
// ---------------------------------------------------------------------------

/// 一局终局观测（评分七分量 + 五维终值 + 演目 Lv）
#[derive(Debug, Clone, Copy)]
struct Observation {
    score: i32,
    pt: i32,
    skill: i32,
    five_score: Array5,
    five: Array5,
    acts: Array5,
}

/// 决策分布（手写档每回合选了什么）
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    train: [u32; 5],
    keiko: [u32; 5],
    rest: u32,
    race: u32,
    outing: u32,
    friend: u32,
    clinic: u32,
    advice: u32,
}

impl Counts {
    fn add(&mut self, action: &Action) {
        match action {
            Action::Train(index) => self.train[usize::from(*index).min(4)] += 1,
            Action::ActTrain(index) => self.keiko[usize::from(*index).min(4)] += 1,
            Action::Race => self.race += 1,
            Action::Rest => self.rest += 1,
            Action::NormalOuting => self.outing += 1,
            Action::FriendOuting => self.friend += 1,
            Action::Clinic => self.clinic += 1,
            Action::AssignAdvice(_) => self.advice += 1,
        }
    }
}

/// 计数的训练员包装：决策语义与 `MatsuriTrainer` 逐位一致，只额外统计动作分布
struct CountingTrainer {
    inner: MatsuriTrainer,
    counts: RefCell<Counts>,
}

impl CountingTrainer {
    fn new(config: MatsuriConfig) -> Self {
        Self {
            inner: MatsuriTrainer::new(config),
            counts: RefCell::new(Counts::default()),
        }
    }

    fn into_counts(self) -> Counts {
        self.counts.into_inner()
    }
}

impl Trainer<MatsuriGame> for CountingTrainer {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let chosen = self.inner.select_action(game, actions, rng)?;
        if let Some(action) = actions.get(chosen) {
            self.counts.borrow_mut().add(action);
        }
        Ok(chosen)
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

/// 按 Runner 同序推进一局（规则主种子 = 决策流 = seed）
fn play(
    env: &Env,
    engine: &umaai::Config,
    config: &MatsuriConfig,
    seed: u64,
) -> Result<(Observation, Counts)> {
    let mut game = build_game(engine, env.data.clone())?.with_config(config.clone());
    game.set_rule_master(seed);
    let trainer = CountingTrainer::new(config.clone());
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(&trainer, &mut rng)?;
    while game.next() {
        game.run_stage(&trainer, &mut rng)?;
    }
    game.on_simulation_end(&trainer, &mut rng)?;
    let parts = game.uma().score_parts()?;
    let observation = Observation {
        score: parts.total(),
        pt: game.uma().total_pt()?,
        skill: parts.skill,
        five_score: parts.five_status,
        five: game.uma().five_status,
        acts: game.acts.level,
    };
    Ok((observation, trainer.into_counts()))
}

/// 一套卡组在种子块上的逐局观测（并行跑；同种子块 = 与其它卡组可直接配对）
fn run_deck(
    env: &Env,
    deck: &[u32],
    config: &MatsuriConfig,
    profile: Option<&str>,
    games: usize,
    seed: u64,
    threads: usize,
) -> Result<Vec<(Observation, Counts)>> {
    let engine = env.engine_with(deck);
    let config = match profile {
        Some(name) if config.decks.resolve_name(name).is_some() => config.for_profile(name)?,
        _ => {
            let comp = matsuri_core::deck_comp(deck).context("卡组构成解析失败")?;
            config.for_deck(comp)
        }
    };
    let config = &config;
    let next = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<(Observation, Counts)>>> = Mutex::new(vec![None; games]);
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let next = &next;
            let slots = &slots;
            let engine = &engine;
            handles.push(scope.spawn(move || -> Result<()> {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= games {
                        return Ok(());
                    }
                    let outcome = play(env, engine, config, seed + index as u64)?;
                    if let Ok(mut guard) = slots.lock() {
                        guard[index] = Some(outcome);
                    }
                }
            }));
        }
        for handle in handles {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("跑局线程 panic"))??;
        }
        Ok(())
    })?;
    let mut out = Vec::with_capacity(games);
    for (index, slot) in slots
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let Some(item) = slot else {
            bail!("第 {index} 局缺结果");
        };
        out.push(item);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 统计
// ---------------------------------------------------------------------------

/// 样本均值（空样本给 0）
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// 样本标准差（n < 2 给 0）
fn stdev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let var = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
}

/// 一组卡组的跑测结果
struct DeckRun {
    name: String,
    comp: String,
    pick: String,
    deck: Vec<u32>,
    observations: Vec<Observation>,
    counts: Vec<Counts>,
    warnings: Vec<String>,
}

impl DeckRun {
    fn scores(&self) -> Vec<f64> {
        self.observations
            .iter()
            .map(|obs| f64::from(obs.score))
            .collect()
    }

    fn pts(&self) -> Vec<f64> {
        self.observations
            .iter()
            .map(|obs| f64::from(obs.pt))
            .collect()
    }

    /// 五维 / 演目 Lv 等按轴取均值
    fn axis_mean(&self, map: impl Fn(&Observation) -> Array5) -> [f64; 5] {
        let mut sums = [0.0f64; 5];
        for obs in &self.observations {
            let values = map(obs);
            for (slot, value) in sums.iter_mut().zip(values.iter()) {
                *slot += f64::from(*value);
            }
        }
        let n = self.observations.len().max(1) as f64;
        sums.map(|sum| sum / n)
    }

    fn count_mean(&self, map: impl Fn(&Counts) -> [u32; 5]) -> [f64; 5] {
        let mut sums = [0.0f64; 5];
        for counts in &self.counts {
            let values = map(counts);
            for (slot, value) in sums.iter_mut().zip(values.iter()) {
                *slot += f64::from(*value);
            }
        }
        let n = self.counts.len().max(1) as f64;
        sums.map(|sum| sum / n)
    }

    fn scalar_mean(&self, map: impl Fn(&Counts) -> u32) -> f64 {
        if self.counts.is_empty() {
            return 0.0;
        }
        self.counts.iter().map(|c| f64::from(map(c))).sum::<f64>() / self.counts.len() as f64
    }
}

// ---------------------------------------------------------------------------
// 子命令：pool / build / run
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    // 诊断开关关闭：diag! 连 format_args 都不执行
    umaai_core::diag::set_enabled(false);
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("pool");
    let mut env = Env::load(&args)?;
    apply_fixture(&mut env, &args)?;
    match command {
        "pool" => cmd_pool(&env, &args),
        "build" => cmd_build(&env, &args),
        "run" => cmd_run(&env, &args),
        other => bail!("未知子命令: {other}（可选 pool / build / run）"),
    }
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

/// 局外条件覆盖：`--uma` / `--qie` / `--shield`（与 `bench` 同名同义）
fn apply_fixture(env: &mut Env, args: &[String]) -> Result<()> {
    if let Some(raw) = parse_arg(args, "--uma") {
        env.engine.sim.uma = raw.parse().with_context(|| format!("--uma 非法: {raw}"))?;
    }
    if let Some(raw) = parse_arg(args, "--qie") {
        env.config.pre.qie = match raw.as_str() {
            "none" => Qie::None,
            "xiaoqie" => Qie::Xiaoqie,
            "qiezhe" => Qie::Qiezhe,
            other => bail!("--qie 只能取 none / xiaoqie / qiezhe（收到 {other}）"),
        };
    }
    if let Some(raw) = parse_arg(args, "--shield") {
        env.config.pre.positive_thinking = raw
            .parse()
            .with_context(|| format!("--shield 非法: {raw}"))?;
    }
    Ok(())
}

/// 并行度：`--threads 0` = 机器并行度
fn resolve_threads(args: &[String]) -> Result<usize> {
    let raw = parse_usize(args, "--threads", 0)?;
    if raw > 0 {
        return Ok(raw);
    }
    Ok(std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(8))
}

/// 卡池榜：`cardtest pool [--type 0-4] [--top 12]`
fn cmd_pool(env: &Env, args: &[String]) -> Result<()> {
    let top = parse_usize(args, "--top", 12)?;
    let only_type = parse_arg(args, "--type")
        .map(|raw| {
            raw.parse::<i32>()
                .with_context(|| format!("--type 非法: {raw}"))
        })
        .transpose()?;
    let pool = build_pool(env)?;
    let azuma = azuma_card_id(&env.data)?;
    println!(
        "# uma={} 担当chara={} 吾妻={} 满破档 idrank=card_id*10+4 池={} 张（SSR 训练卡）",
        env.engine.sim.uma,
        env.trained_chara(),
        idrank(azuma),
        pool.len()
    );
    for card_type in 0..5i32 {
        if let Some(only) = only_type
            && only != card_type
        {
            continue;
        }
        let rows: Vec<&CardRep> = pool
            .iter()
            .filter(|card| card.card_type == card_type)
            .collect();
        let usable = rows.iter().filter(|card| card.eligible()).count();
        println!(
            "## {}（{}/{} 可用）",
            ACT_NAMES[card_type as usize],
            usable,
            rows.len()
        );
        for card in rows.iter().take(top) {
            println!(
                "  {} panel={:.1} 情{} 干{} 训{} 得{} 羁{} 赛{} 链接={} {}{}",
                card.idrank,
                card.panel,
                card.qing,
                card.gan,
                card.xun,
                card.deyi,
                card.jiban,
                card.saihou,
                card.link_act
                    .map_or_else(|| "-".to_string(), |act| ACT_NAMES[act].to_string()),
                card.name,
                card.reject
                    .as_ref()
                    .map_or_else(String::new, |reason| format!("  [剔除:{reason}]"))
            );
        }
    }
    Ok(())
}

/// 卡组装配：`cardtest build --comp all|21101[,12101] --pick panel|measured`
fn cmd_build(env: &Env, args: &[String]) -> Result<()> {
    let pick = parse_arg(args, "--pick").unwrap_or_else(|| "panel".to_string());
    ensure!(
        pick == "panel" || pick == "measured",
        "--pick 只能取 panel / measured（收到 {pick}）"
    );
    let topk = parse_usize(args, "--topk", DEFAULT_TOPK)?.max(1);
    let games = parse_usize(args, "--games", DEFAULT_BUILD_GAMES)?.max(1);
    let seed = parse_usize(args, "--seed", DEFAULT_SEED as usize)? as u64;
    let threads = resolve_threads(args)?;
    let comps = comps_arg(args)?;
    let pool = build_pool(env)?;
    let azuma = azuma_card_id(&env.data)?;

    let mut lines: Vec<String> = Vec::new();
    println!(
        "# pick={pick} uma={} 吾妻={} 满破档 games={games} seed={seed} threads={threads}",
        env.engine.sim.uma,
        idrank(azuma)
    );
    if args.iter().any(|arg| arg == "--with-default") {
        // 仓内默认卡组（手写训练员的调参局面）——配对跑测的天然参照臂，构成码按实际卡面反算
        let default_comp = comp_of_deck(&env.engine.sim.cards);
        lines.push(format!(
            "default_{},{},default,{}",
            comp_code(default_comp),
            comp_code(default_comp),
            env.engine
                .sim
                .cards
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    for (name, comp) in comps {
        let started = Instant::now();
        let deck = match pick.as_str() {
            "measured" => greedy_deck(env, &pool, comp, azuma, topk, games, seed, threads)?,
            _ => assemble(&pool, comp, azuma)?,
        };
        let report = validate_deck(env, &deck)?;
        let types = slot_types(comp);
        let summary = deck
            .iter()
            .zip(types.iter().map(|t| ACT_NAMES[*t as usize]).chain(["友"]))
            .map(|(id, label)| format!("{label}{id}"))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{} {} kinds={} warnings={} elapsed={:.1}s  {summary}",
            name,
            comp_code(comp),
            deck_kinds(&pool, &deck),
            report.warnings.len(),
            started.elapsed().as_secs_f64()
        );
        for warning in &report.warnings {
            println!("   ! {warning}");
        }
        lines.push(format!(
            "{name},{},{pick},{}",
            comp_code(comp),
            deck.iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(path) = parse_arg(args, "--out") {
        write_decks(&path, &lines)?;
        println!("# 已写入 {path}");
    }
    Ok(())
}

/// `--comp all` = 四套标准构成；否则解析逗号分隔的构成码（名字 = 构成码）
fn comps_arg(args: &[String]) -> Result<Vec<(String, [usize; 5])>> {
    let raw = parse_arg(args, "--comp").unwrap_or_else(|| "all".to_string());
    if raw == "all" {
        return Ok(PRESETS
            .iter()
            .map(|(name, comp)| ((*name).to_string(), *comp))
            .collect());
    }
    raw.split(',')
        .filter(|item| !item.is_empty())
        .map(|item| {
            let comp = parse_comp(item)?;
            Ok((item.to_string(), comp))
        })
        .collect()
}

/// 逐槽实测贪婪：起点 = panel 档，每槽在 panel 前 topk 里选配对均值最高者
#[allow(clippy::too_many_arguments)]
fn greedy_deck(
    env: &Env,
    pool: &[CardRep],
    comp: [usize; 5],
    azuma: u32,
    topk: usize,
    games: usize,
    seed: u64,
    threads: usize,
) -> Result<Vec<u32>> {
    let mut deck = assemble(pool, comp, azuma)?;
    let types = slot_types(comp);
    for (slot, card_type) in types.iter().enumerate() {
        let used: Vec<u32> = deck
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != slot)
            .filter_map(|(_, id)| pool_card(pool, *id).map(|card| card.chara_id))
            .collect();
        let candidates: Vec<&CardRep> = pool
            .iter()
            .filter(|card| card.eligible() && card.card_type == *card_type)
            .filter(|card| !used.contains(&card.chara_id))
            .take(topk)
            .collect();
        ensure!(!candidates.is_empty(), "第 {slot} 槽没有可用候选");

        let mut base_scores = run_deck(env, &deck, &env.config, None, games, seed, threads)?
            .iter()
            .map(|(obs, _)| f64::from(obs.score))
            .collect::<Vec<_>>();
        let mut best = deck[slot];
        let mut pair_sds: Vec<f64> = Vec::new();
        println!(
            "  [补 {} 第 {slot} 槽] 候选 {}（{games} 局，seed {seed}）",
            ACT_NAMES[*card_type as usize],
            candidates.len()
        );
        for candidate in candidates {
            if candidate.idrank == deck[slot] {
                println!(
                    "    {} 当前 {:<16} mean={:.1}",
                    candidate.idrank,
                    candidate.name,
                    mean(&base_scores)
                );
                continue;
            }
            let mut trial = deck.clone();
            trial[slot] = candidate.idrank;
            let scores = run_deck(env, &trial, &env.config, None, games, seed, threads)?
                .iter()
                .map(|(obs, _)| f64::from(obs.score))
                .collect::<Vec<_>>();
            let stats = paired(&base_scores, &scores)?;
            pair_sds.push(stats.sd);
            println!(
                "    {} {:<16} mean={:.1} Δ={:+.1} ±{:.1} W/L/T={}/{}/{}{}",
                candidate.idrank,
                candidate.name,
                mean(&scores),
                stats.mean,
                stats.ci95,
                stats.wins,
                stats.losses,
                stats.ties,
                if stats.beyond_zero() && stats.mean > 0.0 {
                    ""
                } else {
                    "  (未越过零)"
                }
            );
            // 只在配对区间越过零时替换：否则等于拿噪声当收益
            if stats.mean > 0.0 && stats.beyond_zero() {
                best = candidate.idrank;
                base_scores = scores;
            }
        }
        if pair_sds.is_empty() {
            continue;
        }
        let sd = mean(&pair_sds);
        println!(
            "    分辨率：配对差分 sd≈{sd:.0} → 本次 {games} 局可分辨约 {:.0} 分",
            2.8 * sd / (games as f64).sqrt()
        );
        if best != deck[slot] {
            deck[slot] = best;
            println!("    => 换成 {best}");
        }
    }
    Ok(deck)
}

/// 卡组文件写入（表头 + 每行一套）
fn write_decks(path: &str, lines: &[String]) -> Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let body = format!(
        "name,comp,pick,slot0,slot1,slot2,slot3,slot4,friend\n{}\n",
        lines.join("\n")
    );
    std::fs::write(path, body).with_context(|| format!("写入 {path} 失败"))
}

/// 一行卡组文件：`name,comp,pick,6×idrank`
struct DeckLine {
    name: String,
    comp: String,
    pick: String,
    deck: Vec<u32>,
}

fn read_decks(path: &str) -> Result<Vec<DeckLine>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("读取 {path} 失败"))?;
    let mut out = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        if cells.first().is_some_and(|cell| *cell == "name") {
            continue;
        }
        ensure!(
            cells.len() >= 5,
            "{} 第 {} 行列数不足: {line}",
            path,
            line_no + 1
        );
        let deck = cells[3..]
            .iter()
            .map(|cell| cell.parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("{path} 第 {} 行 idrank 非法: {line}", line_no + 1))?;
        out.push(DeckLine {
            name: cells[0].to_string(),
            comp: cells.get(1).copied().unwrap_or("").to_string(),
            pick: cells.get(2).copied().unwrap_or("").to_string(),
            deck,
        });
    }
    ensure!(!out.is_empty(), "{path} 里没有卡组行");
    Ok(out)
}

/// 多卡组同种子配对跑测：`cardtest run --decks decks/panel.csv --games 1000`
fn cmd_run(env: &Env, args: &[String]) -> Result<()> {
    let path = parse_arg(args, "--decks")
        .or_else(|| parse_arg(args, "--in"))
        .unwrap_or_else(|| "decks/panel.csv".to_string());
    let games = parse_usize(args, "--games", DEFAULT_RUN_GAMES)?.max(1);
    let seed = parse_usize(args, "--seed", DEFAULT_SEED as usize)? as u64;
    let threads = resolve_threads(args)?;
    let correct =
        Correct::parse(&parse_arg(args, "--correct").unwrap_or_else(|| "none".to_string()))
            .with_context(|| "--correct 只能取 none / bonferroni / holm")?;
    let mut lines = read_decks(&path)?;
    if let Some(raw) = parse_arg(args, "--ref")
        && let Some(index) = lines.iter().position(|line| line.name == raw)
    {
        let reference = lines.remove(index);
        lines.insert(0, reference);
    }

    let started = Instant::now();
    let mut runs: Vec<DeckRun> = Vec::new();
    for line in lines {
        let report = validate_deck(env, &line.deck)?;
        let observations = run_deck(
            env,
            &line.deck,
            &env.config,
            Some(&line.name),
            games,
            seed,
            threads,
        )?;
        let mut deck_run = DeckRun {
            name: line.name,
            comp: line.comp,
            pick: line.pick,
            deck: line.deck,
            counts: observations.iter().map(|(_, counts)| *counts).collect(),
            observations: observations.iter().map(|(obs, _)| *obs).collect(),
            warnings: report.warnings.iter().map(ToString::to_string).collect(),
        };
        if !deck_run.warnings.is_empty() {
            deck_run.warnings.sort();
        }
        runs.push(deck_run);
    }
    let elapsed = started.elapsed();

    print_run(&runs, games, seed, threads, &correct)?;
    println!(
        "# 墙钟 {:.1}s（{} 套 × {games} 局）",
        elapsed.as_secs_f64(),
        runs.len()
    );
    if let Some(out) = parse_arg(args, "--out") {
        write_summary(&out, &runs, &correct)?;
        println!("# 已写入 {out}");
    }
    Ok(())
}

/// 打印对比表 + 归因表 + 决策分布
fn print_run(
    runs: &[DeckRun],
    games: usize,
    seed: u64,
    threads: usize,
    correct: &Correct,
) -> Result<()> {
    let Some(reference) = runs.first() else {
        return Ok(());
    };
    println!(
        "# games={games} seed={seed} threads={threads} ref={} correct={}",
        reference.name,
        correct.name()
    );
    let ref_scores = reference.scores();
    println!(
        "{:<24} {:>5} {:>9} {:>7} {:>9} {:>9} {:>8} {:>10} {:>7}",
        "deck", "n", "score", "sd", "pt", "Δvsref", "ci95", "W/L/T", "p_adj"
    );
    let mut comparisons = Vec::new();
    for run in runs {
        comparisons.push(paired(&ref_scores, &run.scores())?);
    }
    let adjusted = correct.adjust(&comparisons);
    for (index, run) in runs.iter().enumerate() {
        let scores = run.scores();
        let row = &adjusted[index];
        println!(
            "{:<24} {:>5} {:>9.1} {:>7.1} {:>9.1} {:>9} {:>8} {:>10} {:>7}",
            run.name,
            scores.len(),
            mean(&scores),
            stdev(&scores),
            mean(&run.pts()),
            format!("{:+.1}", row.mean),
            format!("±{:.1}", row.ci),
            format!(
                "{}/{}/{}",
                comparisons[index].wins, comparisons[index].losses, comparisons[index].ties
            ),
            format!("{:.4}", row.p_adj)
        );
    }

    println!("# 归因（均值）：skill / 五维分项 / 五维终值 / 演目Lv");
    for run in runs {
        let five_score = run.axis_mean(|obs| obs.five_score);
        let five = run.axis_mean(|obs| obs.five);
        let acts = run.axis_mean(|obs| obs.acts);
        println!(
            "  {:<24} skill={:>6.0} 分项[{}] 五维[{}] Lv[{}]",
            run.name,
            mean(
                &run.observations
                    .iter()
                    .map(|obs| f64::from(obs.skill))
                    .collect::<Vec<_>>()
            ),
            fmt_axes(&five_score),
            fmt_axes(&five),
            fmt_axes(&acts)
        );
    }

    println!("# 决策分布（局均次数）：训练 5 档 / 稽古 5 档 / 其它");
    for run in runs {
        let train = run.count_mean(|counts| counts.train);
        let keiko = run.count_mean(|counts| counts.keiko);
        println!(
            "  {:<24} 训练[{}] 稽古[{}] 休息={:.1} 比赛={:.1} 出行={:.1} 友人={:.1} 治病={:.1} 教诲={:.1}",
            run.name,
            fmt_axes(&train),
            fmt_axes(&keiko),
            run.scalar_mean(|counts| counts.rest),
            run.scalar_mean(|counts| counts.race),
            run.scalar_mean(|counts| counts.outing),
            run.scalar_mean(|counts| counts.friend),
            run.scalar_mean(|counts| counts.clinic),
            run.scalar_mean(|counts| counts.advice)
        );
    }

    let warnings: Vec<String> = runs
        .iter()
        .filter(|run| !run.warnings.is_empty())
        .map(|run| format!("{}={}", run.name, run.warnings.join("；")))
        .collect();
    if !warnings.is_empty() {
        println!("# 编成警告：{}", warnings.join(" | "));
    }

    let sd = stdev(&comparisons.iter().map(|row| row.sd).collect::<Vec<_>>());
    if sd > 0.0 {
        let paired_sd = mean(&comparisons.iter().map(|row| row.sd).collect::<Vec<_>>());
        println!(
            "# 分辨率：配对差分 sd≈{paired_sd:.0} → 分辨 100 分需约 {} 局/档；分辨 50 分需约 {} 局/档",
            paired_runs_for_effect(paired_sd, 100.0),
            paired_runs_for_effect(paired_sd, 50.0)
        );
    }
    Ok(())
}

/// 五轴均值格式化（`速/耐/力/根/智` 顺序，一位小数）
fn fmt_axes(values: &[f64; 5]) -> String {
    values
        .iter()
        .map(|value| format!("{value:.1}"))
        .collect::<Vec<_>>()
        .join("/")
}

/// 汇总 CSV（一行一套卡组）
fn write_summary(path: &str, runs: &[DeckRun], correct: &Correct) -> Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let Some(reference) = runs.first() else {
        return Ok(());
    };
    let ref_scores = reference.scores();
    let mut comparisons = Vec::with_capacity(runs.len());
    for run in runs {
        comparisons.push(paired(&ref_scores, &run.scores())?);
    }
    let adjusted = correct.adjust(&comparisons);
    let mut out = String::from(
        "name,comp,pick,deck,n,score_mean,score_sd,pt_mean,skill_mean,\
         five_score0,five_score1,five_score2,five_score3,five_score4,\
         five0,five1,five2,five3,five4,acts0,acts1,acts2,acts3,acts4,\
         delta,ci95,wins,losses,ties,p_adj,warnings\n",
    );
    for (index, run) in runs.iter().enumerate() {
        let scores = run.scores();
        let row = &adjusted[index];
        let five_score = run.axis_mean(|obs| obs.five_score);
        let five = run.axis_mean(|obs| obs.five);
        let acts = run.axis_mean(|obs| obs.acts);
        out.push_str(&format!(
            "{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},\
             {:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},\
             {:.3},{:.3},{},{},{},{:.6},{}\n",
            run.name,
            run.comp,
            run.pick,
            run.deck
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            scores.len(),
            mean(&scores),
            stdev(&scores),
            mean(&run.pts()),
            mean(
                &run.observations
                    .iter()
                    .map(|obs| f64::from(obs.skill))
                    .collect::<Vec<_>>()
            ),
            five_score[0],
            five_score[1],
            five_score[2],
            five_score[3],
            five_score[4],
            five[0],
            five[1],
            five[2],
            five[3],
            five[4],
            acts[0],
            acts[1],
            acts[2],
            acts[3],
            acts[4],
            row.mean,
            row.ci,
            comparisons[index].wins,
            comparisons[index].losses,
            comparisons[index].ties,
            row.p_adj,
            run.warnings.join(" ")
        ));
    }
    std::fs::write(path, out).with_context(|| format!("写入 {path} 失败"))
}
