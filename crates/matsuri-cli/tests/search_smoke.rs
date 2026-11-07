//! 搜索档冒烟：小预算跑满整局，确认 FlatSearch 接线与摘要日志产出

use std::process::{Command, Stdio};

#[test]
fn search_mode_runs_full_game() {
    let root = matsuri_core::testing::repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_matsuri"))
        .arg("matsuri")
        .env("MATSURI_SEARCH_N", "1")
        // 生产默认日志级别是 `warn`，本用例要校验搜索摘要日志 → 临时提到 info
        .env("MATSURI_LOG_LEVEL", "info")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("启动 matsuri 搜索档");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "搜索档应跑完整局；stderr 尾部：\n{}",
        stderr.lines().rev().take(20).collect::<Vec<_>>().join("\n")
    );
    assert!(
        stderr.contains("【搜索】"),
        "搜索档应输出决策摘要；stderr 前 600 字：\n{}",
        stderr.chars().take(600).collect::<String>()
    );
    assert!(
        stdout.contains("评分"),
        "搜索档应输出终局评分；stdout 尾部：\n{}",
        stdout.lines().rev().take(20).collect::<Vec<_>>().join("\n")
    );
}
