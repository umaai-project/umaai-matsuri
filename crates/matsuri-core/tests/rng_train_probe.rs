//! Paired normal-stage and search-root training, including the after-training event queue.

use anyhow::{Result, anyhow};
use matsuri_core::testing::sample_game;
use matsuri_core::{Action, MatsuriGame, Stage};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use umaai_core::gamedata::EventChoice;
use umasim::game::{Game, Trainer};
use umasim::search::FlatSearchGame;

struct SpeedOnly;

impl Trainer<MatsuriGame> for SpeedOnly {
    fn select_action(&self, _: &MatsuriGame, actions: &[Action], _: &mut StdRng) -> Result<usize> {
        actions
            .iter()
            .position(|a| *a == Action::Train(0))
            .ok_or_else(|| anyhow!("speed training missing"))
    }

    fn select_choice(
        &self,
        _: &MatsuriGame,
        _: &[Vec<EventChoice>],
        _: &mut StdRng,
    ) -> Result<usize> {
        Ok(0)
    }
}

#[test]
fn root_and_live_train_share_failures_and_events() -> Result<()> {
    let mut root = sample_game();
    let mut prepare = StdRng::seed_from_u64(7);
    while root.stage != Stage::Train {
        root.run_stage(&SpeedOnly, &mut prepare)?;
        assert!(root.next());
    }
    root.uma_mut().vital = 25;
    let mut outcomes = [0; 3];
    let mut after_events = 0;
    for seed in 0..128 {
        let mut live = root.fork_for_rollout(seed);
        live.record_events = true;
        let mut branch = live.clone();
        let mut clean = StdRng::seed_from_u64(123);
        let mut noisy = clean.clone();
        for _ in 0..5 {
            noisy.next_u64();
        }
        let untouched = noisy.clone();
        live.run_stage(&SpeedOnly, &mut clean)?;
        branch.apply_root_action(&Action::Train(0), &mut noisy)?;
        assert_eq!(noisy, untouched, "root consumed decision RNG: seed={seed}");
        assert_eq!(live, branch, "root outcome or RNG cursor: seed={seed}");
        let success = branch.train_level_count[0] > root.train_level_count[0];
        let severe = branch.uma().flags.bad_trainer;
        let outcome = match (success, severe) {
            (true, _) => 0,
            (false, false) => 1,
            (false, true) => 2,
        };
        outcomes[outcome] += 1;
        after_events += branch.unresolved_events.len();
        assert!(live.next() && branch.next());
        assert_eq!(branch.stage, Stage::AfterTrain);
        live.run_stage(&SpeedOnly, &mut clean)?;
        branch.run_stage(&SpeedOnly, &mut noisy)?;
        assert_eq!(
            live, branch,
            "after-training events or RNG cursor: seed={seed}"
        );
    }
    println!("success/failure/severe={outcomes:?}, queued events={after_events}");
    assert!(outcomes.iter().all(|&n| n > 0));
    assert!(after_events > 0);
    Ok(())
}
