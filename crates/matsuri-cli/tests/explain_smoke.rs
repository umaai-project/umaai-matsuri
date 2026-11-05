//! explain 冒烟：`--explain` 跑完整局，确认与 manual 共用的面板 + 候选打分 + 决策理由产出
//!
//! explain 模式只打印、不改决策；stdout 应为「面板 × 每回合 + 终局块」，stderr 为诊断日志。

use std::process::{Command, Stdio};

#[test]
fn explain_mode_runs_full_game_with_panels() {
    let root = matsuri_core::testing::repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_matsuri"))
        .args(["matsuri", "--explain"])
        // explain 面板只与手写打分同源；默认档已开搜索，这里显式关掉
        .env("MATSURI_SEARCH_N", "0")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("启动 matsuri --explain");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "explain 档应跑完整局；stderr:\n{stderr}"
    );
    for marker in [
        "【动作】",
        "【候选打分】",
        "【决策理由】",
        "【教诲分配",
        "【剧本资源】",
        "T0",
        "评分",
    ] {
        assert!(
            stdout.contains(marker),
            "缺少 {marker}；stdout 前 600 字：\n{}",
            stdout.chars().take(600).collect::<String>()
        );
    }
    // 78 个动作面板 + 6 个教诲分配窗口都应有候选表
    assert!(
        stdout.matches("【候选打分】").count() > 80,
        "面板应逐回合带候选表"
    );
    // 教诲分配窗口 6 次
    assert_eq!(stdout.matches("教诲分配（T").count(), 6);
}
