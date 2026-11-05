//! FlatSearch 搜索档：整局可复现 / 搜索预算 / 事件队列恢复的代码正确性
//!
//! CI 只跑 `search_n=1` 的小预算；1024 真实预算用 `#[ignore]` 手动跑。

use anyhow::Result;
use matsuri_core::testing::sample_game;
use matsuri_core::{MAX_TURN, MatsuriConfig, MatsuriSearchTrainer, MatsuriTrainer, MatsuriView};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umasim::game::{Game, Trainer};
use umasim::search::{FlatSearchGame, SearchConfig};

/// 整局观测：评分 / PT / 终局快照
type Outcome = (i32, i32, MatsuriView);

/// 用指定训练员跑满一局（固定主种子）
fn run(seed: u64, trainer: &impl Trainer<matsuri_core::MatsuriGame>) -> Result<Outcome> {
    let mut game = sample_game();
    game.set_rule_master(seed);
    let mut decision = StdRng::seed_from_u64(7);
    for _ in 0..=MAX_TURN {
        for _ in 0..5 {
            game.run_stage(trainer, &mut decision)?;
            if !game.next() {
                break;
            }
        }
        if game.career_failed || game.turn() >= MAX_TURN {
            break;
        }
    }
    game.on_simulation_end(trainer, &mut decision)?;
    Ok((
        game.uma().calc_score()?,
        game.uma().total_pt()?,
        game.view(),
    ))
}

fn search_config(n: usize) -> SearchConfig {
    SearchConfig::default().with_search_n(n).with_ucb(false)
}

/// 三类决策全开、小预算：同种子两次逐位一致
#[test]
fn search_full_game_is_reproducible() -> Result<()> {
    let mut cfg = MatsuriConfig::default();
    cfg.search.search_n = 1;
    let trainer = MatsuriSearchTrainer::new(cfg, search_config(1))?;
    let a = run(61444, &trainer)?;
    let b = run(61444, &trainer)?;
    assert_eq!(a, b, "同种子搜索档应逐位一致");
    Ok(())
}

/// 搜索开关逐类关闭：关闭后与手写档同种子逐位一致
#[test]
fn search_scopes_off_match_handwritten() -> Result<()> {
    let mut cfg = MatsuriConfig::default();
    cfg.search.search_n = 1;
    cfg.search.train = false;
    cfg.search.advice = false;
    cfg.search.event = false;
    let trainer = MatsuriSearchTrainer::new(cfg, search_config(1))?;
    let hand = MatsuriTrainer::new(MatsuriConfig::default());
    assert_eq!(run(61444, &trainer)?, run(61444, &hand)?);
    Ok(())
}

/// rollout 分支静默事件明细：记录开关不影响决策与终局，fork 后 `event_log` 为空
#[test]
fn rollout_fork_skips_event_log_without_changing_game() -> Result<()> {
    let mut base = sample_game();
    base.set_rule_master(61444);
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());

    let mut normal = base.clone();
    let mut quiet = base.clone();
    quiet.record_events = false;
    let mut rng_normal = StdRng::seed_from_u64(7);
    let mut rng_quiet = StdRng::seed_from_u64(7);
    normal.run_full_game(&trainer, &mut rng_normal)?;
    quiet.run_full_game(&trainer, &mut rng_quiet)?;
    assert_eq!(
        normal.uma().calc_score()?,
        quiet.uma().calc_score()?,
        "事件明细开关不应改变评分"
    );
    assert_eq!(normal.uma().total_pt()?, quiet.uma().total_pt()?);
    assert!(!normal.event_log.is_empty(), "正常局面应记录事件");
    assert!(quiet.event_log.is_empty(), "静默局面不应记录事件");

    let mut forked = base.fork_for_rollout(61444);
    assert!(!forked.record_events, "fork_for_rollout 应关闭事件明细");
    forked.run_full_game(&trainer, &mut rng_quiet)?;
    assert!(forked.event_log.is_empty(), "rollout 分支不应记录事件");
    Ok(())
}

/// 单候选不搜：教练分配窗口之外的强制赛也走手写
#[test]
fn single_candidate_skips_search() -> Result<()> {
    let mut cfg = MatsuriConfig::default();
    cfg.search.search_n = 1;
    let trainer = MatsuriSearchTrainer::new(cfg, search_config(1))?;
    let game = sample_game();
    let actions = vec![matsuri_core::Action::Race];
    let mut rng = StdRng::seed_from_u64(1);
    let index = trainer.select_action(&game, &actions, &mut rng)?;
    assert_eq!(index, 0);
    Ok(())
}

/// 1024 真实预算：不进 CI，手动跑 `cargo test --release -- --ignored`
#[test]
#[ignore = "1024 每候选 rollout 的整局验证，手动执行"]
fn search_1024_full_game_is_reproducible() -> Result<()> {
    let mut cfg = MatsuriConfig::default();
    cfg.search.search_n = 1024;
    let trainer = MatsuriSearchTrainer::new(cfg, search_config(1024))?;
    let a = run(61444, &trainer)?;
    let b = run(61444, &trainer)?;
    assert_eq!(a, b, "1024 档同种子应逐位一致");
    Ok(())
}
