//! 诊断埋点守门：整局必须产出「关键状态变化」日志，且运行时静默不位移结果
//!
//! 独立测试进程（`tests/` 每文件一个二进制），因此可以安全安装一次性捕获日志器；
//! `DiagGuard` 是进程级开关，本文件内全部断言串行执行，不与其他测试并行冲突。
//!
//! 覆盖口径：回合开始 / 分布 / 动作 / 训练后 / 事件结算 / 教诲 / 発表会 / 决策 都要有；
//! `DiagGuard::suppress()` 期间零产出，且同一主种子下终局局面与开日志时逐位一致。

use std::sync::{Mutex, Once};

use log::{Level, LevelFilter, Log, Metadata, Record};
use matsuri_core::testing::sample_game;
use matsuri_core::{MAX_TURN, MatsuriConfig, MatsuriGame, MatsuriTrainer};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai_core::diag::DiagGuard;
use umasim::game::Game;

/// 捕获到的日志记录：`(target, 文本)`
static LINES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// 一次性安装捕获日志器
static INSTALL: Once = Once::new();

/// 捕获 `info` 级别日志（诊断日志都是 info）
struct CaptureLogger;

impl Log for CaptureLogger {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        meta.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let mut lines = LINES.lock().unwrap_or_else(|err| err.into_inner());
            lines.push((record.target().to_string(), record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

/// 安装日志器（进程内只装一次）
fn init_capture() {
    INSTALL.call_once(|| {
        log::set_boxed_logger(Box::new(CaptureLogger)).expect("测试进程内首次安装日志器");
        log::set_max_level(LevelFilter::Info);
    });
    LINES.lock().unwrap_or_else(|err| err.into_inner()).clear();
}

/// 取当前捕获的日志文本
fn captured() -> Vec<(String, String)> {
    LINES.lock().unwrap_or_else(|err| err.into_inner()).clone()
}

/// 同种子跑满整局（含终局奖励），返回终局局面
fn run_full_game() -> MatsuriGame {
    let mut game = sample_game();
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    for _ in 0..=MAX_TURN {
        for _ in 0..5 {
            game.run_stage(&trainer, &mut decision)
                .expect("阶段推进失败");
            game.next();
        }
    }
    game.on_simulation_end(&trainer, &mut decision)
        .expect("终局结算失败");
    game
}

/// 统计含某个标记的日志行数
fn count(lines: &[(String, String)], needle: &str) -> usize {
    lines
        .iter()
        .filter(|(_, text)| text.contains(needle))
        .count()
}

/// 整局诊断守门：关键状态变化全部有日志 + 静默不位移结果
#[test]
fn diag_logs_key_state_changes_without_changing_results() {
    init_capture();
    let game_with_logs = run_full_game();
    let lines = captured();
    // ① 全部走诊断 target（`diag!` 的 "diagnostic"）
    assert!(
        !lines.is_empty(),
        "整局应产出诊断日志（是否未装日志器 / feature 被裁掉？）"
    );
    assert!(
        lines.iter().all(|(target, _)| target == "diagnostic"),
        "诊断日志 target 应为 diagnostic：{:?}",
        lines
            .iter()
            .map(|(target, _)| target)
            .collect::<std::collections::HashSet<_>>()
    );
    // ② 关键埋点齐备（失败信息带上实际计数，便于定位回归）
    let expect = [
        ("回合开始", "【回合】T"),
        ("分布", "【分布】"),
        ("动作结算", "【动作】"),
        ("训练后处理", "【训练后】"),
        ("事件结算", "【事件结算】"),
        ("演目槽", "【演目槽】"),
        ("教诲", "【教诲】"),
        ("発表会", "【発表会】"),
        ("训练员决策", "【决策】"),
    ];
    for (name, needle) in expect {
        let n = count(&lines, needle);
        assert!(n > 0, "诊断缺少「{name}」埋点（标记 `{needle}`）");
    }
    assert_eq!(
        count(&lines, "【回合】T"),
        (MAX_TURN + 1) as usize,
        "每回合 Begin 一行回合摘要"
    );
    assert_eq!(count(&lines, "【発表会】"), 3, "三场発表会 / 本番各一行");
    // ③ 抽样关键状态变化行：T0 回合摘要 / T2 首次授予 / 分布人数
    let all = lines
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>();
    assert!(
        all.iter().any(|text| text.contains("T0 Begin")),
        "应含 T0 回合开始行"
    );
    assert!(
        all.iter().any(|text| text.contains("首次授予")),
        "应含 T2 首次教诲授予行"
    );
    assert!(
        all.iter().any(|text| text.contains("【分布】T2 ")),
        "应含 T2 分布摘要"
    );
    println!(
        "诊断埋点：{} 行（回合 {} / 発表会 {} / 动作 {} / 决策 {}）",
        lines.len(),
        count(&lines, "【回合】T"),
        count(&lines, "【発表会】"),
        count(&lines, "【动作】"),
        count(&lines, "【决策】")
    );

    // ④ 运行时静默：零产出，且同种子终局局面逐位一致（诊断只读）
    let before = captured().len();
    let game_silent = {
        let _guard = DiagGuard::suppress();
        let game = run_full_game();
        assert_eq!(
            captured().len(),
            before,
            "DiagGuard 静默期间不得产出诊断日志"
        );
        game
    };
    assert_eq!(
        game_with_logs, game_silent,
        "诊断输出不得位移结算结果（同种子终局局面应完全一致）"
    );
}
