//! 手动档冒烟：管道输入走完整局，确认面板 / 事件回显 / 终局块都能产出
//!
//! 手动档不引交互依赖，用标准输入喂「全 0」（第一个动作 / 第一项选项）即可跑满 78 回合。

use std::io::Write;
use std::process::{Command, Stdio};

/// 一局所需决策数上限（动作 + 事件选择；远小于 512）
const DECISIONS: usize = 512;

#[test]
fn manual_mode_runs_full_game_with_scripted_input() {
    let root = matsuri_core::testing::repo_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_matsuri"))
        .arg("manual")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 matsuri manual");
    {
        let stdin = child.stdin.as_mut().expect("stdin 可写");
        write!(stdin, "{}", "0\n".repeat(DECISIONS)).expect("写入输入");
    }
    let output = child.wait_with_output().expect("等待手动档结束");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "手动档应跑完整局；stderr:\n{stderr}"
    );
    for marker in [
        "【事件】",
        "【数值】",
        "【演目】",
        "【分布】",
        "【动作】",
        "【目标赛】",
        "评分",
    ] {
        assert!(
            stdout.contains(marker),
            "手动面板应包含 `{marker}`（stdout 长度 {}）",
            stdout.len()
        );
    }
    // 事件回显覆盖 S6 关键链
    for event in ["马娘登场", "求むるは、新たな書", "追加の稽古"] {
        assert!(
            stdout.contains(event) || stderr.contains(event),
            "手动档应回显事件 `{event}`"
        );
    }
}
