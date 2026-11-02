//! 手动档冒烟：管道输入走完整局，确认面板 / 事件回显 / 终局块都能产出
//!
//! 手动档动作选择在交互终端用 Inquire、非 TTY 退回编号输入；此处用标准输入喂「全 0」
//! （第一个动作 / 第一项选项）即跑满 78 回合。

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
        "【育成配置】",
        "卡组",
        "继承",
        "【事件】",
        "【数值】",
        "【教诲分配】",
        "【剧本资源】",
        "流镝(速)",
        "剧本机制自 T2",
        "未分配",
        "【训练】",
        "教诲 → 転成",
        "[NPC]",
        "【动作】",
        "【目标赛】",
        "【演目槽】",
        "←101 牵绊",
        "可触发追加稽古",
        "配色：",
        "评分",
    ] {
        assert!(
            stdout.contains(marker),
            "手动面板应包含 `{marker}`（stdout 长度 {}）",
            stdout.len()
        );
    }
    // 非合宿回合不再输出合宿状态行（只在合宿区显示）
    assert!(
        !stdout.contains("非合宿回合"),
        "非合宿回合不应再输出合宿状态（stdout 长度 {}）",
        stdout.len()
    );
    // 管道（非 TTY）输出应自动关色：颜色只在实际终端生效
    assert!(
        !stdout.contains('\x1b'),
        "非 TTY 输出不应含 ANSI 颜色序列（stdout 长度 {}）",
        stdout.len()
    );
    // 追加稽古改标列标题：动作后缀「追稽✓」已移除
    assert!(
        !stdout.contains("追稽✓"),
        "动作后缀不应再出现追稽✓（stdout 长度 {}）",
        stdout.len()
    );
    // 事件回显覆盖 S6 关键链
    for event in [
        "马娘登场",
        "求むるは、新たな書",
        "駿大祭合宿の申し出",
        "追加の稽古",
    ] {
        assert!(
            stdout.contains(event) || stderr.contains(event),
            "手动档应回显事件 `{event}`"
        );
    }
}
