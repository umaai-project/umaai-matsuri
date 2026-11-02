//! 配对门禁判官（`matsuri-gate`）端到端冒烟
//!
//! 用合成 dump 验证三件事：
//! 1. 同一份 dump 自比 ⇒ Δ=0、判定「无显著差异」、退出码 0；
//! 2. B 更靠近真值 ⇒ 判定「显著更近」；
//! 3. 两臂样本号集合不一致 ⇒ 报错、退出码 2。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 一行样本：(样本号, 五维和, 五维分, PT)
type Row = (u64, i64, i64, i64);

/// 写一份合成 dump（`MC_DUMP` 的 JSONL 形状）
fn write_dump(dir: &Path, name: &str, rows: &[Row]) -> PathBuf {
    let body: Vec<String> = rows
        .iter()
        .map(|(i, sum, stat, pt)| {
            format!("{{\"i\":{i},\"sum\":{sum},\"stat\":{stat},\"pt\":{pt}}}")
        })
        .collect();
    let path = dir.join(name);
    std::fs::write(&path, body.join("\n")).expect("写 dump");
    path
}

/// 跑判官，返回 (退出码, stdout)
fn run_gate(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_matsuri-gate"))
        .args(args)
        .output()
        .expect("跑 matsuri-gate");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

/// 临时目录（按进程号隔离）
fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("matsuri-gate-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

#[test]
fn self_comparison_reports_no_significant_difference() {
    let dir = tmp_dir("self");
    let rows: Vec<Row> = (0..20).map(|i| (i, 500 + i as i64, 3000, 400)).collect();
    let a = write_dump(&dir, "a.jsonl", &rows);
    let b = write_dump(&dir, "b.jsonl", &rows);

    let (code, out) = run_gate(&[
        "--a",
        a.to_str().unwrap(),
        "--b",
        b.to_str().unwrap(),
        "--five",
        "20,20,20,20,20",
        "--pt",
        "400",
        "--stat-score",
        "3000",
    ]);
    assert_eq!(code, 0, "自比应正常出报告并退出 0");
    assert!(out.contains("无显著差异"), "自比应判无显著差异：\n{out}");
    assert!(out.contains("| Δ=B−A |"), "应出表格：\n{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closer_arm_is_reported_as_significantly_closer() {
    let dir = tmp_dir("closer");
    let a_rows: Vec<Row> = (0..20).map(|i| (i, 50, 1000, 100)).collect();
    let b_rows: Vec<Row> = (0..20).map(|i| (i, 90, 1900, 190)).collect();
    let a = write_dump(&dir, "a.jsonl", &a_rows);
    let b = write_dump(&dir, "b.jsonl", &b_rows);

    let (code, out) = run_gate(&[
        "--a",
        a.to_str().unwrap(),
        "--b",
        b.to_str().unwrap(),
        "--five",
        "20,20,20,20,20",
        "--pt",
        "200",
        "--stat-score",
        "2000",
    ]);
    assert_eq!(code, 0);
    assert!(
        out.contains("显著更近"),
        "B 更靠近真值应判显著更近：\n{out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mismatched_sample_sets_are_rejected() {
    let dir = tmp_dir("mismatch");
    let a_rows: Vec<Row> = (0..20).map(|i| (i, 50, 1000, 100)).collect();
    let b_rows: Vec<Row> = (0..10).map(|i| (i, 90, 1900, 190)).collect();
    let a = write_dump(&dir, "a.jsonl", &a_rows);
    let b = write_dump(&dir, "b.jsonl", &b_rows);

    let out = Command::new(env!("CARGO_BIN_EXE_matsuri-gate"))
        .args([
            "--a",
            a.to_str().unwrap(),
            "--b",
            b.to_str().unwrap(),
            "--five",
            "20,20,20,20,20",
        ])
        .output()
        .expect("跑 matsuri-gate");
    assert_eq!(out.status.code(), Some(2), "样本号集合不一致应报输入错误");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("不一致"), "错误信息要点明不一致：\n{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
