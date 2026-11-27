//! 搜索蒸馏档经 bench game 加载的回归覆盖。
//!
//! 断言只锁「关系」（同目录 `distilled.toml` == 文件加载、关闭 == 纯手写、零权重 == 纯手写），
//! 不锁具体分数，避免数据 / 版本漂移导致门禁误红。
use std::process::{Command, Output};

fn game(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bench"))
        .current_dir(matsuri_core::testing::repo_root())
        .args(["game", "--games", "1", "--seed", "500000", "--warmup", "0"])
        .args(args)
        .output()
        .expect("运行 bench game")
}

fn score(out: &Output) -> i32 {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|line| line.starts_with("500000,"))
        .and_then(|line| line.split(',').nth(1))
        .and_then(|raw| raw.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "缺少基准行：stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
}

#[test]
fn policy_sources_agree_and_switch_off_returns_to_plain() {
    const POLICY: &str = "benchmark/policies/distilled-v1.json";
    let inline = game(&["--search-n", "0"]);
    let file = game(&["--distilled-policy", POLICY]);
    let plain = game(&["--search-n", "0", "--no-distilled"]);
    for out in [&inline, &file, &plain] {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|line| line == "seed,score,pt,turn,elapsed_us"),
            "逐局 CSV 头必须保持不变"
        );
    }
    assert_eq!(
        score(&inline),
        score(&file),
        "同目录 distilled.toml 与文件加载必须同分"
    );
    assert_ne!(
        score(&inline),
        score(&plain),
        "--no-distilled 必须真的关闭蒸馏（防止参数被静默忽略）"
    );
}

#[test]
fn zero_weights_match_plain_trainer() {
    // cargo 为集成测试提供的可写临时目录（不依赖进程 TEMP 环境）
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("distilled_zero");
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("zero.json");
    let groups = (0..15)
        .map(|_| "[0.0, 0.0, 0.0]")
        .collect::<Vec<_>>()
        .join(", ");
    let board = (0..2)
        .map(|_| "[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]")
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        &path,
        format!("{{\"groups\": [{groups}], \"board\": [{board}]}}"),
    )
    .expect("写零权重");
    let path_str = path.to_string_lossy().to_string();
    let zero = game(&["--distilled-policy", path_str.as_str()]);
    let plain = game(&["--search-n", "0", "--no-distilled"]);
    assert!(
        zero.status.success(),
        "{}",
        String::from_utf8_lossy(&zero.stderr)
    );
    assert_eq!(score(&zero), score(&plain), "零权重必须退化为纯手写");
}

#[test]
fn conflicting_flags_are_rejected() {
    const POLICY: &str = "benchmark/policies/distilled-v1.json";
    let search_conflict = game(&["--distilled-policy", POLICY, "--search-n", "64"]);
    assert!(!search_conflict.status.success());
    assert!(String::from_utf8_lossy(&search_conflict.stderr).contains("--distilled-policy"));

    let off_conflict = game(&["--distilled-policy", POLICY, "--no-distilled"]);
    assert!(!off_conflict.status.success());
    assert!(String::from_utf8_lossy(&off_conflict.stderr).contains("--no-distilled"));
}
