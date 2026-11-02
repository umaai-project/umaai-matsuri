//! 训练位排列指标：**卡参与「自己类型」训练的比例**
//!
//! 口径与报文侧一致（`E:/HJM/jidianbeiAI0.22/out/hachimi_raw`，413 条训练命令）：
//! 对每个训练类型 t，看该训练位的参与者里有没有卡种 == t 的支援卡。
//! 报文实测 **73.1%**（零假设 35.6%，z=+15.9）；分类型 速 94.7 / 力 88.6 / 智 82.0 / 耐 78.6。
//!
//! ⚠️ 报文**没有逐格布局**（角色在 `training_partner_array` 里的位置是分散的），
//! 所以「卡在某一格」不可直接测，只能测「训练 t 的参与者里有没有 t 类卡」。
//!
//! 引擎底座那套 `weights[train_type] += deyilv`（把 cardDB 的 `deYiLv` 数值当权重）
//! 在报文里**没有剂量关系**（10→58.0%、43→65.8%、60→61.5%、75→47.1%，无单调性），
//! 不能照抄；真机的偏置是「按卡种站位」这一分类效应（见 `SELF_SLOT_P`）。

use matsuri_core::testing::sample_game;
use rand::SeedableRng;
use rand::rngs::StdRng;
use umasim::game::{Game, Person};

/// 报文实测值（零假设 35.6%）
const TARGET: f64 = 0.731;

/// 跑 N 回合的排列，返回 (命中数, 观测数)
fn measure(rounds: usize, seed: u64) -> (usize, usize) {
    let mut game = sample_game();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut hit = 0usize;
    let mut total = 0usize;
    for _ in 0..rounds {
        game.distribute_all(&mut rng).unwrap();
        for t in 0..5usize {
            total += 1;
            let matched = game.distribution()[t].iter().any(|pi| {
                let i = *pi as usize;
                i < game.persons().len() && game.persons()[i].train_type() == t as i32
            });
            if matched {
                hit += 1;
            }
        }
    }
    (hit, total)
}

#[test]
fn arrangement_metric_matches_packet_observation() {
    let (hit, total) = measure(400, 0);
    let metric = hit as f64 / total as f64;
    println!("ARRANGE_METRIC {hit}/{total} = {metric:.4}");
    // 对标报文实测 73.1%；留 2 个点的余量，卡组/权重改动时能立刻看出来
    assert!(
        (metric - TARGET).abs() <= 0.02,
        "落位指标 {metric:.4} 偏离报文实测 {TARGET:.4} 超过 2 个点"
    );
}

#[test]
fn arrangement_metric_is_stable_across_seeds() {
    for seed in [1u64, 7, 20261101] {
        let (hit, total) = measure(200, seed);
        let metric = hit as f64 / total as f64;
        assert!(
            (metric - TARGET).abs() <= 0.03,
            "seed {seed} 的落位指标 {metric:.4} 偏离目标 {TARGET:.4}"
        );
    }
}
