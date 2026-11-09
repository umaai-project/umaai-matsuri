//! 数据更新：`ai_data` 与 `ura_data` 两个独立频道。
//!
//! 源是编译期常量；清单走引擎 `umaai-update` 的 `update.json` 协议（逐文件 sha256、
//! 全量校验后才替换、失败回滚）。`ai_data` 只允许三张基础表落到 `data/`，
//! `events.json` / `constants.json` / 配置文件 / 剧本资产一律不参与；
//! `ura_data` 只接受 `.br` 根文件并装到发布目录的 `.portable/`。

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use umaai_update::{
    Channel, ChannelCheck, ChannelUpdate, ChannelUpdateOutcome, CheckOutcome, HttpTransport,
    Options, Progress, Source, check, update,
};

use super::{Paint, Paths, Result, fail};

/// 更新源基址（与旧更新工具一致；程序频道共用）
pub(super) const UPDATE_BASE: &str = "https://cdn2.viktorlab.cn/uma";
/// 防盗链请求头（程序频道共用）
pub(super) const UPDATE_REFERER: &str = "https://viktorlab.cn";
/// `ai_data` 允许落地的三张基础表
const AI_DATA_FILES: [&str; 3] = ["umaDB.json", "cardDB.json", "text_data_dict.json"];
/// 小黑板数据扩展名
const URA_DATA_EXT: &str = ".br";

/// 执行一次检查、确认与更新；返回后由菜单统一 `pause`
pub(super) fn run(paths: &Paths, paint: &Paint) -> Result<()> {
    let source = Source::new(UPDATE_BASE)?.with_header("Referer", UPDATE_REFERER);
    let channels = channels(paths)?;
    let transport = HttpTransport::new();

    println!();
    println!("提示：更新前请退出在线模式与小黑板，避免文件占用导致替换失败。");
    println!("{}", paint.bold("检查数据更新…"));
    let report = check(&source, &channels, &transport);
    if !report_checked(&report.channels, paint) {
        return Err(fail("存在频道清单不符合约定或检查失败，未做任何更新。"));
    }
    if report.pending() == 0 {
        println!("{}", paint.green("全部频道已是最新。"));
        return Ok(());
    }
    if !confirm()? {
        println!("已取消，未做任何更新。");
        return Ok(());
    }

    println!();
    println!(
        "{}",
        paint.bold("开始更新（临时目录 .umaai-update/，任一失败自动回滚）…")
    );
    let tty = io::stdout().is_terminal();
    let mut on_progress = |p: Progress<'_>| {
        let pct = p
            .done
            .saturating_mul(100)
            .checked_div(p.total)
            .unwrap_or(100) as u8;
        if tty {
            print!("\r  下载 {}/{} {pct:>3}%", p.channel, p.path);
            let _ = io::stdout().flush();
        } else if pct == 100 {
            println!("  下载 {}/{} 完成", p.channel, p.path);
        }
    };
    let report = update(
        &source,
        &channels,
        &transport,
        &Options::new(&paths.install_dir),
        &mut on_progress,
    );
    if tty {
        println!();
    }
    if !report_updated(&report.channels, paint) {
        return Err(fail("部分频道更新失败；临时目录已保留，排查后可重试。"));
    }
    println!("提示：重新进入在线模式 / 重启小黑板后，新数据才会被载入。");
    Ok(())
}

/// 频道声明：`ai_data` 门控 schema，`ura_data` 只做文件形态守卫
fn channels(paths: &Paths) -> Result<Vec<Channel>> {
    let schema = data_schema(&paths.install_dir)?;
    Ok(vec![
        Channel::data("ai_data", "AI 数据", paths.install_dir.join("data"))?.expect_schema(schema),
        Channel::data(
            "ura_data",
            "小黑板数据",
            paths.install_dir.join(".portable"),
        )?,
    ])
}

/// 从 `data/manifest.json` 读取数据 schema 版本（程序升级后无需改启动器常量）
fn data_schema(install_dir: &Path) -> Result<u32> {
    let path = install_dir.join("data").join("manifest.json");
    let text = fs::read_to_string(&path)
        .map_err(|err| fail(format!("读取 {} 失败：{err}", path.display())))?;
    let parsed: SchemaManifest = serde_json::from_str(&text)
        .map_err(|err| fail(format!("解析 {} 失败：{err}", path.display())))?;
    Ok(parsed.schema_version)
}

/// `manifest.json` 中本模块关心的字段
#[derive(serde::Deserialize)]
struct SchemaManifest {
    /// 数据 schema 版本
    schema_version: u32,
}

/// 打印检查结果；返回 `false` 表示有频道不可更新（清单守卫或网络失败）
fn report_checked(items: &[ChannelCheck], paint: &Paint) -> bool {
    let mut ok = true;
    for item in items {
        let outcome = match &item.result {
            Ok(outcome) => {
                if let Err(err) = guard(item.id.as_str(), outcome) {
                    println!("{} {}：{err}", paint.red("○"), item.name);
                    ok = false;
                    continue;
                }
                outcome
            }
            Err(err) => {
                println!("{} {}：检查失败：{err}", paint.red("○"), item.name);
                ok = false;
                continue;
            }
        };
        if outcome.up_to_date() {
            println!(
                "{} {}：已是最新（远端 {}）",
                paint.green("●"),
                item.name,
                outcome.version
            );
        } else {
            println!(
                "{} {}：{} 个文件待更新，共 {:.1} MB（远端 {}）",
                paint.yellow("●"),
                item.name,
                outcome.pending.len(),
                outcome.pending_bytes as f64 / 1024.0 / 1024.0,
                outcome.version
            );
        }
    }
    ok
}

/// 打印更新结果；返回 `false` 表示有频道失败
fn report_updated(items: &[ChannelUpdate], paint: &Paint) -> bool {
    let mut ok = true;
    for item in items {
        match &item.result {
            Ok(ChannelUpdateOutcome::UpToDate { version }) => {
                println!("{} {}：已是最新（{version}）", paint.green("●"), item.name);
            }
            Ok(ChannelUpdateOutcome::Updated {
                version,
                files,
                bytes,
            }) => {
                println!(
                    "{} {}：已更新 {} 个文件，{:.1} MB（{version}）",
                    paint.green("●"),
                    item.name,
                    files.len(),
                    *bytes as f64 / 1024.0 / 1024.0
                );
            }
            Err(err) => {
                println!("{} {}：更新失败：{err}", paint.red("○"), item.name);
                ok = false;
            }
        }
    }
    ok
}

/// 频道文件形态守卫：拒绝任何本仓不接受的路径
fn guard(id: &str, outcome: &CheckOutcome) -> Result<()> {
    match id {
        "ai_data" => guard_ai(outcome),
        "ura_data" => guard_ura(outcome),
        _ => Ok(()),
    }
}

/// `ai_data` 必须严格等于三张基础表，避免上游扩表覆盖剧本定制资产
fn guard_ai(outcome: &CheckOutcome) -> Result<()> {
    let mut got: Vec<&str> = outcome
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut want = AI_DATA_FILES.to_vec();
    got.sort_unstable();
    want.sort_unstable();
    if got != want {
        return Err(fail(format!(
            "清单与本地约定不一致，远端文件 {got:?}，只允许 {want:?}"
        )));
    }
    Ok(())
}

/// `ura_data` 只接受根目录下的 `.br` 文件
fn guard_ura(outcome: &CheckOutcome) -> Result<()> {
    for file in &outcome.files {
        let path = file.path.as_str();
        if path.contains('/') || !path.ends_with(URA_DATA_EXT) {
            return Err(fail(format!("清单包含非 .br 根文件：{path}")));
        }
    }
    Ok(())
}

/// 更新前确认；空行 / `y` 视为确认，EOF 或其它输入视为取消
pub(super) fn confirm() -> Result<bool> {
    print!("确认更新？[Y/n] ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(false);
    }
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use umaai_update::FileEntry;

    /// 构造一个指定文件集的检查结果
    fn outcome(paths: &[&str]) -> CheckOutcome {
        CheckOutcome {
            version: "v1".to_string(),
            files: paths
                .iter()
                .map(|path| FileEntry {
                    path: (*path).to_string(),
                    size: 1,
                    sha256: "0".repeat(64),
                })
                .collect(),
            pending: Vec::new(),
            pending_bytes: 0,
        }
    }

    /// `ai_data` 严格白名单：三张基础表通过，扩展或多出文件拒绝
    #[test]
    fn ai_data_guard_is_strict() {
        assert!(guard_ai(&outcome(&AI_DATA_FILES)).is_ok());
        assert!(guard_ai(&outcome(&["umaDB.json", "cardDB.json"])).is_err());
        assert!(
            guard_ai(&outcome(&[
                "umaDB.json",
                "cardDB.json",
                "text_data_dict.json",
                "events.json"
            ]))
            .is_err()
        );
        assert!(guard_ai(&outcome(&["ai_data/umaDB.json"])).is_err());
    }

    /// 小黑板频道只接受根目录 `.br`
    #[test]
    fn ura_data_guard_rejects_non_br() {
        assert!(guard_ura(&outcome(&["events_male.br", "names.br"])).is_ok());
        assert!(guard_ura(&outcome(&["config.yaml"])).is_err());
        assert!(guard_ura(&outcome(&["sub/events.br"])).is_err());
    }
}
