//! 诊断：搜索档**实际教诲分配** vs 配置落位预设（`[decks.*.advice]`）
//!
//! 手动跑（release）：
//! `MATSURI_ADVICE_GAMES=10 MATSURI_ADVICE_N=2048 cargo test --release --offline \
//!   -p matsuri-core --test diag_advice_choice -- --ignored --nocapture`
//!
//! 口径：每局按 `bench` 的推进序、决策流种子 = 规则主种子；记录 6 个教诲窗口
//! （T2 / T11 / T35 / T59 在 Beginning，T23 / T47 在 End）最终生效的**有效 id**
//! （已融合），与 `preset_advice_ids(window)` 同口径逐位比较（位置 = 演目 速/耐/力/根/智）。
//! 「可行」= 该窗口的 120 个候选里存在与预设等价的方案（801 硬约束会剔除一部分）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use matsuri_core::game::advice::advice_display_name;
use matsuri_core::testing::data_dir;
use matsuri_core::{
    Action, MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, Qie,
    build_game, deck_comp,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};

/// 六个数诲窗口（回合号；顺序与 `AdvicePreset` 的 t2/t11/t23/t35/t47/t59 一致）
const WINDOWS: [i32; 6] = [2, 11, 23, 35, 47, 59];

/// 一次被采纳的教诲分配（融合后有效 id，按演目 0..4）
struct Entry {
    win: usize,
    ids: [i32; 5],
    preset: Option<[i32; 5]>,
    feasible: bool,
}

/// 回合号 → 窗口下标
fn window_of_turn(turn: i32) -> Option<usize> {
    WINDOWS.iter().position(|t| *t == turn)
}

/// id 组 → 展示串（`牵绊/擅长率/体力/灵感/助力`）
fn layout(ids: &[i32]) -> String {
    ids.iter()
        .map(|id| advice_display_name(*id))
        .collect::<Vec<_>>()
        .join("/")
}

/// 记录每次教诲决策的训练员包装（内层可以是搜索档或手写档）
struct Recorder<T> {
    inner: T,
    log: Mutex<Vec<Entry>>,
}

impl<T: Trainer<MatsuriGame>> Recorder<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<Entry> {
        self.log
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}

impl<T: Trainer<MatsuriGame>> Trainer<MatsuriGame> for Recorder<T> {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let index = self.inner.select_action(game, actions, rng)?;
        if let Some(win) = window_of_turn(game.turn())
            && let Some(Action::AssignAdvice(assign)) = actions.get(index)
        {
            let ids = game.effective_advice_ids(assign);
            let preset = game.preset_advice_ids(win);
            let feasible = preset.is_some_and(|preset| {
                actions.iter().any(|action| match action {
                    Action::AssignAdvice(other) => game.effective_advice_ids(other) == preset,
                    _ => false,
                })
            });
            if let Ok(mut log) = self.log.lock() {
                log.push(Entry {
                    win,
                    ids,
                    preset,
                    feasible,
                });
            }
        }
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

/// 一局（与 `bench` 的 `play_with_engine` 同序）
fn play(
    engine: &umaai::Config,
    config: &MatsuriConfig,
    data: &Arc<MatsuriData>,
    seed: u64,
    trainer: &impl Trainer<MatsuriGame>,
) -> Result<()> {
    let mut game = build_game(engine, Arc::clone(data))
        .context("建局失败")?
        .with_config(config.clone());
    game.set_rule_master(seed);
    let mut rng = StdRng::seed_from_u64(seed);
    game.run_stage(trainer, &mut rng)?;
    while game.next() {
        game.run_stage(trainer, &mut rng)?;
    }
    game.on_simulation_end(trainer, &mut rng)?;
    Ok(())
}

/// 单窗口统计
#[derive(Default)]
struct Stat {
    games: usize,
    hit: usize,
    feasible: usize,
    per_act: [usize; 5],
    diff: usize,
    layouts: BTreeMap<Vec<i32>, usize>,
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// 打印六个窗口的统计（`detail` = 是否展开实际布局分布）
fn report(entries: &[Entry], detail: bool) {
    let mut stats: Vec<Stat> = (0..WINDOWS.len()).map(|_| Stat::default()).collect();
    for entry in entries {
        let stat = &mut stats[entry.win];
        stat.games += 1;
        *stat.layouts.entry(entry.ids.to_vec()).or_insert(0) += 1;
        if entry.feasible {
            stat.feasible += 1;
        }
        if let Some(preset) = entry.preset {
            let diff = entry
                .ids
                .iter()
                .zip(preset.iter())
                .filter(|(chosen, want)| chosen != want)
                .count();
            if diff == 0 {
                stat.hit += 1;
            }
            stat.diff += diff;
            for (act, chosen) in entry.ids.iter().enumerate() {
                if *chosen == preset[act] {
                    stat.per_act[act] += 1;
                }
            }
        }
    }
    for (win, stat) in stats.iter().enumerate() {
        if stat.games == 0 {
            println!("T{:<3} 无记录", WINDOWS[win]);
            continue;
        }
        let preset = entries
            .iter()
            .find(|entry| entry.win == win)
            .and_then(|entry| entry.preset);
        let preset = preset.map_or_else(|| "（不可解析）".to_string(), |ids| layout(&ids));
        let mean = stat.diff as f64 / stat.games as f64;
        println!(
            "T{:<3} 预设 {:<28} 命中 {}/{}  可行 {}/{}  逐位(速耐力根智) {}/{}/{}/{}/{}  Δ均 {:.2}",
            WINDOWS[win],
            preset,
            stat.hit,
            stat.games,
            stat.feasible,
            stat.games,
            stat.per_act[0],
            stat.per_act[1],
            stat.per_act[2],
            stat.per_act[3],
            stat.per_act[4],
            mean
        );
        if detail {
            let mut rows: Vec<(&Vec<i32>, &usize)> = stat.layouts.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            let text = rows
                .iter()
                .map(|(ids, count)| format!("{} ×{}", layout(ids.as_slice()), count))
                .collect::<Vec<_>>()
                .join(" ; ");
            println!("     实际 {text}");
        }
    }
}

/// 主入口：两档卡组 × 搜索档 10 局 × N=2048，附带同局手写档参考
#[test]
#[ignore = "手动诊断：默认 10 局 × N=2048，约 3 分钟"]
fn advice_choice_vs_config_preset() -> Result<()> {
    let root = data_dir();
    Data::init(&root).context("引擎静态数据初始化失败")?;
    let engine = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置失败")?;
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据失败")?);
    let base = MatsuriConfig::load_with_overlay_file(&root, ".")
        .context("载入剧本配置失败")?
        .for_deck(deck_comp(&engine.sim.cards).context("卡组构成解析失败")?);

    let games = env_usize("MATSURI_ADVICE_GAMES", 10);
    let search_n = env_usize("MATSURI_ADVICE_N", 2048);
    let seed0 = 73_200u64;
    let decks = [
        ("a-normal", 105_902u32, Qie::None, 0i32),
        ("b-ideal", 106_302u32, Qie::Xiaoqie, 2i32),
    ];

    for (name, uma, qie, shield) in decks {
        let mut config = base.clone();
        config.pre.qie = qie;
        config.pre.positive_thinking = shield;
        config.search.search_n = search_n;
        let mut deck_engine = engine.clone();
        deck_engine.sim.uma = uma;

        let trainer = Recorder::new(
            MatsuriSearchTrainer::new(
                config.clone(),
                deck_engine.search.clone().with_search_n(search_n),
            )
            .context("构造搜索训练员失败")?,
        );
        for game in 0..games {
            play(&deck_engine, &config, &data, seed0 + game as u64, &trainer)?;
        }
        println!(
            "\n== {name} uma={uma} qie={qie:?} shield={shield} | 搜索 N={search_n} | {games} 局（seed0={seed0}）=="
        );
        report(&trainer.take(), true);

        let hand = Recorder::new(MatsuriTrainer::new(config.clone()));
        for game in 0..games {
            play(&deck_engine, &config, &data, seed0 + game as u64, &hand)?;
        }
        println!("\n-- {name} 手写参考（同 {games} 局）--");
        report(&hand.take(), false);
    }
    Ok(())
}
