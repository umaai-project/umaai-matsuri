//! 数据更新：`ai_data` / `ura_data` / `ura_plugins` 三个独立频道。
//!
//! 源是编译期常量；清单走引擎 `umaai-update` 的 `update.json` 协议（逐文件 sha256、
//! 全量校验后才替换、失败回滚）。三个频道互不重叠：`ai_data` 只落 `data/` 三张基础表，
//! `ura_data` 只落 `.portable/` 根下的 `.br`，`ura_plugins` 只落 `.portable/Plugins/*.zip`
//! 与 `.portable/version.json`（`PluginData/` 是运行时产物，永不纳入）。

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use umaai_update::{
    Channel, ChannelCheck, ChannelUpdate, ChannelUpdateOutcome, CheckOutcome, HttpTransport,
    Options, Progress, Source, check, update,
};

use super::{PORTABLE_DIR, Paint, Paths, Result, fail};

/// 更新源基址（与旧更新工具一致；三个频道共用）
pub(super) const UPDATE_BASE: &str = "https://cdn2.viktorlab.cn/uma";
/// 防盗链请求头（三频道共用）
pub(super) const UPDATE_REFERER: &str = "https://viktorlab.cn";
/// `ai_data` 允许落地的三张基础表
const AI_DATA_FILES: [&str; 3] = ["umaDB.json", "cardDB.json", "text_data_dict.json"];
/// 小黑板数据扩展名（频道装到 `.portable/` 根）
const URA_DATA_EXT: &str = ".br";
/// 小黑板插件在 `.portable` 下的目录名
const PLUGIN_DIR: &str = "Plugins";
/// 插件条目的路径前缀（`Plugins/`）
const PLUGIN_PREFIX: &str = "Plugins/";
/// 小黑板插件扩展名（URA 的插件包）
const PLUGIN_EXT: &str = ".zip";
/// 插件频道落在 `.portable` 根的版本标记（随包下发，本地只读）
pub(super) const VERSION_FILE: &str = "version.json";

/// 数据侧频道：各自独立清单、独立目标、独立守卫，也各自是菜单入口
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DataChannel {
    /// AI 数据：`data/` 三张基础表，schema 由本地 `manifest.json` 门控
    AiData,
    /// 小黑板数据：`.portable/` 根下的 `.br`
    UraData,
    /// 小黑板插件：`.portable/Plugins/*.zip` 与 `.portable/version.json`
    Plugins,
}

impl DataChannel {
    /// 频道 id（同时是远端目录名）
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::AiData => "ai_data",
            Self::UraData => "ura_data",
            Self::Plugins => "ura_plugins",
        }
    }

    /// 显示名（头部一行与菜单后缀共用）
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::AiData => "AI 数据",
            Self::UraData => "小黑板数据",
            Self::Plugins => "小黑板插件",
        }
    }

    /// 本地频道声明（装到哪个目录 + 是否门控 schema）
    pub(super) fn channel(self, install_dir: &Path) -> Result<Channel> {
        let dir = match self {
            Self::AiData => install_dir.join("data"),
            Self::UraData | Self::Plugins => install_dir.join(PORTABLE_DIR),
        };
        let channel = Channel::data(self.id(), self.name(), dir)?;
        match self {
            Self::AiData => Ok(channel.expect_schema(data_schema(install_dir)?)),
            Self::UraData | Self::Plugins => Ok(channel),
        }
    }

    /// 频道目录是否在位（不在位就不参与自检，避免「目录缺失 = 全部待更新」的假警报）
    pub(super) fn ready(self, install_dir: &Path) -> bool {
        install_dir.join(self.subdir()).is_dir()
    }

    /// 频道在安装根下的相对目录
    fn subdir(self) -> &'static str {
        match self {
            Self::AiData => "data",
            Self::UraData | Self::Plugins => PORTABLE_DIR,
        }
    }

    /// 文件形态守卫：拒绝任何本仓不接受的路径
    pub(super) fn guard(self, outcome: &CheckOutcome) -> Result<()> {
        match self {
            Self::AiData => guard_ai(outcome),
            Self::UraData => guard_ura(outcome),
            Self::Plugins => guard_plugins(outcome),
        }
    }
}

/// 更新源：基址常量 + 防盗链 `Referer`（数据与程序频道共用，启动自检也走它）
pub(super) fn source() -> Result<Source> {
    Ok(Source::new(UPDATE_BASE)?.with_header("Referer", UPDATE_REFERER))
}

/// 执行单个频道的检查、确认与更新；返回后由菜单统一 `pause`
pub(super) fn run(paths: &Paths, paint: &Paint, scope: DataChannel) -> Result<()> {
    let source = source()?;
    let channels = [scope.channel(&paths.install_dir)?];
    let transport = HttpTransport::new();

    println!();
    println!("提示：更新前请退出在线模式与小黑板，避免文件占用导致替换失败。");
    println!("{}", paint.bold(&format!("检查更新：{}", scope.name())));
    let report = check(&source, &channels, &transport);
    if !report_checked(scope, &report.channels, paint) {
        return Err(fail("频道清单不符合约定或检查失败，未做任何更新。"));
    }
    if report.pending() == 0 {
        println!("{}", paint.green("已是最新。"));
        return Ok(());
    }
    if !confirm()? {
        println!("已取消，未做任何更新。");
        return Ok(());
    }

    println!();
    println!(
        "{}",
        paint.bold("开始更新（临时目录 .umaai-update/，失败自动回滚）…")
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
        return Err(fail("更新失败；临时目录已保留，排查后可重试。"));
    }
    println!("提示：重新进入在线模式 / 重启小黑板后，新数据才会被载入。");
    Ok(())
}

/// 从 `data/manifest.json` 读取数据 schema 版本（程序升级后无需改启动器常量）
pub(super) fn data_schema(install_dir: &Path) -> Result<u32> {
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

/// 打印检查结果；返回 `false` 表示该频道不可更新（清单守卫或网络失败）
fn report_checked(scope: DataChannel, items: &[ChannelCheck], paint: &Paint) -> bool {
    let Some(item) = items.first() else {
        println!("{} {}：检查结果为空", paint.red("○"), scope.name());
        return false;
    };
    let outcome = match &item.result {
        Ok(outcome) => {
            if let Err(err) = scope.guard(outcome) {
                println!("{} {}：{err}", paint.red("○"), scope.name());
                return false;
            }
            outcome
        }
        Err(err) => {
            println!("{} {}：检查失败：{err}", paint.red("○"), scope.name());
            return false;
        }
    };
    if outcome.up_to_date() {
        println!(
            "{} {}：已是最新（远端 {}）",
            paint.green("●"),
            scope.name(),
            outcome.version
        );
    } else {
        println!(
            "{} {}：{} 个文件待更新，共 {:.1} MB（远端 {}）",
            paint.yellow("●"),
            scope.name(),
            outcome.pending.len(),
            outcome.pending_bytes as f64 / 1024.0 / 1024.0,
            outcome.version
        );
    }
    true
}

/// 打印更新结果；返回 `false` 表示失败
fn report_updated(items: &[ChannelUpdate], paint: &Paint) -> bool {
    let Some(item) = items.first() else {
        println!("{} 更新结果为空", paint.red("○"));
        return false;
    };
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
            return false;
        }
    }
    true
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

/// `ura_plugins` 只接受 `Plugins/` 下的单层 `.zip` 与根 `version.json`
///
/// `PluginData/`（运行时帧与 native 缓存）与 `.br` 数据永不纳入本频道。
fn guard_plugins(outcome: &CheckOutcome) -> Result<()> {
    for file in &outcome.files {
        let path = file.path.as_str();
        if path == VERSION_FILE {
            continue;
        }
        let ok = path.strip_prefix(PLUGIN_PREFIX).is_some_and(|rest| {
            !rest.starts_with('.')
                && rest.len() > PLUGIN_EXT.len()
                && !rest.contains('/')
                && rest.ends_with(PLUGIN_EXT)
        });
        if !ok {
            return Err(fail(format!(
                "清单包含非法条目：{path}（只允许 {PLUGIN_DIR}/*{PLUGIN_EXT} 与 {VERSION_FILE}）"
            )));
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

    /// 测试临时目录：放测试二进制旁边（target 内可写），沙箱下系统临时目录不可写
    fn temp_base() -> std::path::PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .map_or_else(std::env::temp_dir, |p| p.join("tmp-tests"))
    }
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
        let ai = DataChannel::AiData;
        assert!(ai.guard(&outcome(&AI_DATA_FILES)).is_ok());
        assert!(ai.guard(&outcome(&["umaDB.json", "cardDB.json"])).is_err());
        assert!(
            ai.guard(&outcome(&[
                "umaDB.json",
                "cardDB.json",
                "text_data_dict.json",
                "events.json"
            ]))
            .is_err()
        );
        assert!(ai.guard(&outcome(&["ai_data/umaDB.json"])).is_err());
    }

    /// 小黑板数据频道只接受根目录 `.br`
    #[test]
    fn ura_data_guard_rejects_non_br() {
        let ura = DataChannel::UraData;
        assert!(ura.guard(&outcome(&["events_male.br", "names.br"])).is_ok());
        assert!(ura.guard(&outcome(&["config.yaml"])).is_err());
        assert!(ura.guard(&outcome(&["sub/events.br"])).is_err());
    }

    /// 插件频道只接受 `Plugins/*.zip` 与根 `version.json`；运行时目录与 .br 一律拒绝
    #[test]
    fn plugins_guard_scope() {
        let plugins = DataChannel::Plugins;
        assert!(
            plugins
                .guard(&outcome(&[
                    "version.json",
                    "Plugins/SendGameStatusPlugin.zip",
                    "Plugins/SkillTipsResponseAnalyzer.zip"
                ]))
                .is_ok()
        );
        for bad in [
            "Plugins/PluginData/game1.json",
            "PluginData/SendGameStatusPlugin/thisTurn.json",
            "Plugins/sub/deep.zip",
            "Plugins/readme.txt",
            "names.br",
            "Plugins/.zip",
            "Plugins\\DMMPlugin.zip",
            "Plugins/../version.json",
        ] {
            assert!(plugins.guard(&outcome(&[bad])).is_err(), "{bad} 应被拒绝");
        }
    }

    /// 频道 id 与显示名一一对应（菜单 / 远端目录 / 显示三处不许漂）
    #[test]
    fn channel_ids_and_names() {
        assert_eq!(DataChannel::AiData.id(), "ai_data");
        assert_eq!(DataChannel::UraData.id(), "ura_data");
        assert_eq!(DataChannel::Plugins.id(), "ura_plugins");
        assert_eq!(DataChannel::Plugins.name(), "小黑板插件");
        assert_eq!(DataChannel::UraData.name(), "小黑板数据");
    }

    /// 插件频道离线安装：只落 `Plugins/*.zip` 与 `version.json`，运行时目录一字不动
    #[test]
    fn plugins_channel_install_offline() {
        use umaai_update::{Options, Source, TestTransport, sha256_hex, update};

        let dir = temp_base().join(format!("matsuri-plugin-inst-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let plugins = dir.join(PORTABLE_DIR).join("Plugins");
        let runtime = dir
            .join(PORTABLE_DIR)
            .join("PluginData")
            .join("SendGameStatusPlugin");
        fs::create_dir_all(&plugins).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(plugins.join("old.zip"), b"old").unwrap();
        fs::write(runtime.join("thisTurn.json"), b"{}").unwrap();

        let marker = br#"{"version":"20261111"}"#;
        let body = b"new-zip";
        let manifest = serde_json::json!({
            "protocol": 1,
            "version": "20261111",
            "files": [
                {"path": "version.json", "size": marker.len(), "sha256": sha256_hex(marker)},
                {"path": "Plugins/new.zip", "size": body.len(), "sha256": sha256_hex(body)},
            ],
        });
        let base = "https://cdn2.viktorlab.cn/uma";
        let transport = TestTransport::new()
            .serve_str(
                format!("{base}/ura_plugins/update.json"),
                &manifest.to_string(),
            )
            .serve(format!("{base}/ura_plugins/version.json"), marker.to_vec())
            .serve(format!("{base}/ura_plugins/Plugins/new.zip"), body.to_vec());
        let source = Source::new(base).unwrap();
        let channel = DataChannel::Plugins.channel(&dir).unwrap();

        let report = update(
            &source,
            std::slice::from_ref(&channel),
            &transport,
            &Options::new(&dir),
            &mut |_| {},
        );
        assert!(report.all_ok(), "{report:?}");
        assert_eq!(fs::read(plugins.join("new.zip")).unwrap(), body);
        assert_eq!(
            fs::read(dir.join(PORTABLE_DIR).join("version.json")).unwrap(),
            marker,
            "版本标记随包落地"
        );
        assert_eq!(
            fs::read(runtime.join("thisTurn.json")).unwrap(),
            b"{}",
            "运行时目录不得被触碰"
        );
        assert!(
            plugins.join("old.zip").is_file(),
            "逐文件协议不表达删除，旧插件仍在（已知缺口）"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
