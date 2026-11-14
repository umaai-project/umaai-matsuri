//! 诊断：NPC 名单的回合区间对账（真机帧 vs 离线模拟 vs 还原局面）
//!
//! 输出 CSV：tag,turn,source,total,guests,guests_available,frame_guests

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use matsuri_ai::{FrameSource, OnlineSession, TrainerKind};
use matsuri_core::testing::data_dir;
use matsuri_core::{MAX_TURN, MatsuriConfig, MatsuriData, MatsuriTrainer, build_game};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::FrameKind;
use umaai_core::Data;
use umasim::game::Game;

fn frames_dir() -> Option<PathBuf> {
    let root = data_dir().parent()?.parent()?.to_path_buf();
    let dir = root.join("Matsuri_AI/.portable/PluginData/SendGameStatusPlugin");
    dir.is_dir().then_some(dir)
}

fn row(tag: &str, turn: i32, source: &str, game: &matsuri_core::MatsuriGame, frame_guests: usize) {
    let persons = &game.view().board.persons;
    let guests: Vec<_> = persons
        .iter()
        .filter(|p| format!("{:?}", p.origin) == "Guest")
        .collect();
    let available = guests.iter().filter(|p| p.available).count();
    println!(
        "{tag},{turn},{source},{},{},{available},{frame_guests}",
        persons.len(),
        guests.len()
    );
}

#[test]
fn diag_npc_roster() -> Result<()> {
    let root = data_dir();
    Data::init(&root)?;
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").context("载入引擎配置")?;
    let matsuri_cfg = MatsuriConfig::load_with_overlay_file(&root, ".").context("载入剧本配置")?;
    let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据")?);

    let mut game = build_game(&cfg, Arc::clone(&data))?;
    game.set_rule_master(7);
    let hand = MatsuriTrainer::new(matsuri_cfg.clone());
    let mut rng = StdRng::seed_from_u64(11);
    let mut last = -1;
    for _ in 0..=(MAX_TURN as usize + 1) * 6 {
        if game.stage == matsuri_core::Stage::Train && game.turn() != last {
            last = game.turn();
            row("SIM", game.turn(), "-", &game, 0);
        }
        if game.next() {
            game.run_stage(&hand, &mut rng)?;
        } else {
            break;
        }
        if game.career_failed {
            break;
        }
    }

    let Some(dir) = frames_dir() else {
        eprintln!("跳过帧侧：目录不存在");
        return Ok(());
    };
    let mut matsuri_cfg = matsuri_cfg;
    matsuri_cfg.search.search_n = 1;
    let mut sess = OnlineSession::new(&cfg, data, matsuri_cfg, TrainerKind::Search, false)?;
    let mut source = FrameSource::open_replay(&dir)?;
    while let Some(frame) = source.next_frame(Duration::ZERO)? {
        if frame.kind == FrameKind::FinalScore {
            continue;
        }
        // 夹具里可能混有通不过编成门禁的局：跳过该帧（整局都会报同一错误）
        let out = match sess.handle_frame(&frame.contents) {
            Ok(out) => out,
            Err(err) if format!("{err:#}").contains("编成不合规") => {
                eprintln!("跳过编成不合规的帧：{err:#}");
                continue;
            }
            Err(err) => return Err(err),
        };
        let book: serde_json::Value = serde_json::from_str(&frame.contents)?;
        let frame_guests = book["festa"]["persons"].as_array().map_or(0, |rows| {
            rows.iter()
                .filter(|person| person["targetId"].as_i64().unwrap_or(0) >= 1000)
                .count()
        });
        let src = book["baseGame"]["source"].as_str().unwrap_or("?");
        row("FRAME", out.turn, src, &out.game, frame_guests);
    }
    Ok(())
}
