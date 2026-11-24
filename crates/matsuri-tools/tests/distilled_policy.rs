//! 搜索蒸馏档经 bench game 加载的回归覆盖。
use std::process::{Command, Output};

fn game(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bench"))
        .current_dir(matsuri_core::testing::repo_root())
        .args(["game", "--games", "1", "--seed", "500000", "--warmup", "0"])
        .args(args)
        .output()
        .expect("运行 bench game")
}

#[test]
fn frozen_policy_loads_and_preserves_csv_contract() {
    const POLICY: &str = "benchmark/policies/distilled-v1.json";
    for (args, expected) in [
        (["--search-n", "0"], "85273"),
        (["--distilled-policy", POLICY], "84006"),
    ] {
        let out = game(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8(out.stdout).expect("UTF-8");
        assert!(
            stdout
                .lines()
                .any(|line| line == "seed,score,pt,turn,elapsed_us")
        );
        let row = stdout
            .lines()
            .find(|line| line.starts_with("500000,"))
            .expect("基准行");
        assert_eq!(row.split(',').nth(1), Some(expected));
    }
    let conflict = game(&["--distilled-policy", POLICY, "--search-n", "64"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("--distilled-policy"));
}
