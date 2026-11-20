//! 诊断：模拟 hint 总等级（`total_hints`）逐回合曲线与前 30 回合来源分解。
//!
//! 手动跑（release，仓根；默认 20 局手写档、seed 73200 起、打印 turn 0..29）：
//! `PROBE_GAMES=20 PROBE_SEED0=73200 PROBE_TURNS=30 cargo test --release --offline \
//!   -p matsuri-core --test tmp_probe_hint_curve -- --ignored --nocapture`
//!
//! 口径：与 `bench` 同序推进（规则主种子 = 决策流种子 = seed0+i）。回合末 = 该回合
//! 最后一次 `run_stage` 之后、`turn` 变化之前的状态；事件 hint 按数据 8002=3 级、
//! 8003=6 级折算，残差（含友人 5 级与 T66 进化 +1）归入「训练启发」。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use matsuri_core::testing::data_dir;
use matsuri_core::{
    Action, MatsuriConfig, MatsuriData, MatsuriGame, MatsuriTrainer, build_game, deck_comp,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};

/// 回合末观测
#[derive(Clone, Copy)]
struct Row {
    turn: i32,
    hints: i32,
    ev: [u32; 3],
}

/// 8002/8003 与灵感事件（id 101）计数快照
fn snap(game: &MatsuriGame) -> [u32; 3] {
    [8002u32, 8003u32, 101u32].map(|id| game.base.events.get(&id).copied().unwrap_or(0))
}

/// 记录每回合实际动作的训练员包装（只观察，不改变决策）
struct Recorder<T> {
    inner: T,
    actions: Mutex<BTreeMap<i32, String>>,
}

impl<T: Trainer<MatsuriGame>> Trainer<MatsuriGame> for Recorder<T> {
    fn select_action(
        &self,
        game: &MatsuriGame,
        actions: &[Action],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let index = self.inner.select_action(game, actions, rng)?;
        if let Some(action) = actions.get(index)
            && !matches!(action, Action::AssignAdvice(_))
            && let Ok(mut map) = self.actions.lock()
        {
            map.insert(game.turn(), action.to_string());
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

/// 记录一次观测：同回合覆盖为最新，换回合时把上一回合末压栈
fn observe(rows: &mut Vec<Row>, cur: &mut Option<Row>, game: &MatsuriGame) {
    let row = Row {
        turn: game.turn(),
        hints: game.uma().total_hints,
        ev: snap(game),
    };
    if let Some(prev) = cur {
        if prev.turn == row.turn {
            *cur = Some(row);
            return;
        }
        rows.push(*prev);
    }
    *cur = Some(row);
}

#[test]
#[ignore = "手动诊断：默认 20 局手写档前 30 回合（PROBE_GAMES / PROBE_SEED0 / PROBE_TURNS 可覆盖）"]
fn probe_hint_curve() -> Result<()> {
    let root = data_dir();
    Data::init(&root).context("引擎静态数据初始化失败")?;
    let mut engine =
        umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置失败")?;
    // 可选：按真机局换马娘 / 卡组（PROBE_UMA / PROBE_CARDS="a,b,c"）
    if let Ok(uma) = std::env::var("PROBE_UMA") {
        engine.sim.uma = uma.parse().context("PROBE_UMA 非法")?;
    }
    if let Ok(cards) = std::env::var("PROBE_CARDS") {
        engine.sim.cards = cards
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("PROBE_CARDS 非法")?;
    }
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据失败")?);
    let mut base = MatsuriConfig::load_with_overlay_file(&root, ".")
        .context("载入剧本配置失败")?
        .for_deck(deck_comp(&engine.sim.cards).context("卡组构成解析失败")?);
    base.search.search_n = 0;

    let games: u64 = std::env::var("PROBE_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let seed0: u64 = std::env::var("PROBE_SEED0")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(73_200);
    let max_turn: i32 = std::env::var("PROBE_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    println!(
        "# uma={} qie={:?} games={games} seed0={seed0} turns=0..{}",
        engine.sim.uma,
        base.pre.qie,
        max_turn - 1
    );
    println!("seed,turn,hints,d_total,ev8002,ev8003,ev_hint,train_hint,hint_events,action");

    for i in 0..games {
        let seed = seed0 + i;
        let mut game = build_game(&engine, Arc::clone(&data))?.with_config(base.clone());
        game.set_rule_master(seed);
        let mut rng = StdRng::seed_from_u64(seed);
        let trainer = Recorder {
            inner: MatsuriTrainer::new(base.clone()),
            actions: Mutex::new(BTreeMap::new()),
        };

        let mut rows = Vec::new();
        let mut cur = None;
        game.run_stage(&trainer, &mut rng)?;
        observe(&mut rows, &mut cur, &game);
        while game.next() {
            game.run_stage(&trainer, &mut rng)?;
            observe(&mut rows, &mut cur, &game);
        }
        if let Some(last) = cur {
            rows.push(last);
        }

        let mut prev: Option<Row> = None;
        for row in rows.iter().filter(|row| row.turn < max_turn) {
            let (d_total, d2, d3, ev_hint, hint_events) = match prev {
                Some(p) => {
                    let d2 = row.ev[0].saturating_sub(p.ev[0]);
                    let d3 = row.ev[1].saturating_sub(p.ev[1]);
                    let dh = row.ev[2].saturating_sub(p.ev[2]);
                    let ev = d2 * 3 + d3 * 6;
                    (row.hints - p.hints, d2, d3, ev as i32, dh)
                }
                None => (0, 0, 0, 0, 0),
            };
            let train_hint = d_total - ev_hint;
            let action = trainer
                .actions
                .lock()
                .ok()
                .and_then(|map| map.get(&row.turn).cloned())
                .unwrap_or_default()
                .replace(',', "_");
            println!(
                "{seed},{},{},{d_total},{d2},{d3},{ev_hint},{train_hint},{hint_events},{action}",
                row.turn, row.hints
            );
            prev = Some(*row);
        }
    }
    Ok(())
}
