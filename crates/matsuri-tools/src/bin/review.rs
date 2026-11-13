//! 复盘工具入口：`review open <局目录> [--out <目录>] [--data <数据根>]`
//!
//! 事实层只读帧与终局帧；`eval` / `curve` / `report` 后续按需补。

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map_or("", String::as_str);
    if command != "open" {
        println!("用法：review open <局目录> [--out <目录>] [--data <数据根>]");
        return Ok(());
    }
    let input = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("缺少局目录：review open <局目录>"))?;
    let input = PathBuf::from(input);
    let mut out: Option<PathBuf> = None;
    let mut data_root: Option<PathBuf> = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--data" => {
                data_root = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    let dir = matsuri_tools::review::open(matsuri_tools::review::OpenArgs {
        input: &input,
        out: out.as_deref(),
        data_root: data_root.as_deref(),
    })?;
    println!("{}", dir.display());
    Ok(())
}
