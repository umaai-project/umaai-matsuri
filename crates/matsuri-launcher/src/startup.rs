//! 启动自检：只读比对四份清单（程序 / AI 数据 / 小黑板数据 / 小黑板插件）。
//!
//! 只做「检查」这一段：并发拉清单 → 逐频道判定 → 汇总成头部一行，
//! 不下载、不写盘、不弹询问、任何失败都不阻断启动。
//! 判定复用 [`super::update`] / [`super::program`] 的清单解析、白名单守卫与本地摘要比对，
//! 与更新子菜单同源，避免「自检说有更新、点进去却拒绝更新」。
//! 顺序口径：程序升级可能抬数据 schema，实际更新仍应先程序后数据（见发布计划 §0.4）。

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::thread::{Scope, ScopedJoinHandle};
use std::time::Duration;

use umaai_update::{Channel, HttpTransport, Source, Transport, check};

use super::update::{self, DataChannel};
use super::{PORTABLE_DIR, Paint, Paths, program};

/// 自检连接超时：与菜单更新的默认连接超时同档，坏链路下最多等这么久
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 自检整体超时：单请求上限，四条并发后即首屏总预算
const TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
/// 环境变量：置任意值即关闭启动自检
const DISABLE_ENV: &str = "MATSURI_NO_UPDATE_CHECK";
/// 字节到 MB 的换算基数
const MB: f64 = 1024.0 * 1024.0;
/// 失败原因在头部一行的最大字符数
const DETAIL_CHARS: usize = 28;

/// 单频道自检结论
#[derive(Debug)]
enum ChannelState {
    /// 已是最新（版本）
    Latest(String),
    /// 有更新（远端版本 / 待下载字节）
    Pending { version: String, bytes: u64 },
    /// 未参与本次自检（原因）
    Skipped(&'static str),
    /// 检查失败（原因）
    Failed(String),
}

impl ChannelState {
    /// 是否待更新
    fn pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }
}

/// 一次启动自检的汇总（四个频道各一条结论）
#[derive(Debug)]
pub(super) struct Status {
    /// `program` 频道
    program: ChannelState,
    /// `ai_data` 频道
    ai_data: ChannelState,
    /// `ura_data` 频道
    ura_data: ChannelState,
    /// `ura_plugins` 频道
    plugins: ChannelState,
}

impl Status {
    /// 全部频道跳过（非交互 / 已关闭），`reason` 为展示原因
    fn skipped(reason: &'static str) -> Self {
        Self {
            program: ChannelState::Skipped(reason),
            ai_data: ChannelState::Skipped(reason),
            ura_data: ChannelState::Skipped(reason),
            plugins: ChannelState::Skipped(reason),
        }
    }

    /// 全部频道记为失败（仅用于源常量不可用这类整体故障）
    fn failed(reason: &str) -> Self {
        let state = || ChannelState::Failed(reason.to_string());
        Self {
            program: state(),
            ai_data: state(),
            ura_data: state(),
            plugins: state(),
        }
    }

    /// 四频道都跳过时返回展示原因
    fn skip_reason(&self) -> Option<&'static str> {
        match (&self.program, &self.ai_data, &self.ura_data, &self.plugins) {
            (
                ChannelState::Skipped(reason),
                ChannelState::Skipped(_),
                ChannelState::Skipped(_),
                ChannelState::Skipped(_),
            ) => Some(reason),
            _ => None,
        }
    }

    /// 程序频道是否有更新（更新子菜单后缀用）
    pub(super) fn program_pending(&self) -> bool {
        self.program.pending()
    }

    /// AI 数据是否有更新
    pub(super) fn ai_data_pending(&self) -> bool {
        self.ai_data.pending()
    }

    /// 小黑板数据是否有更新
    pub(super) fn ura_data_pending(&self) -> bool {
        self.ura_data.pending()
    }

    /// 小黑板插件是否有更新
    pub(super) fn plugins_pending(&self) -> bool {
        self.plugins.pending()
    }

    /// 待更新频道数（主菜单「更新与插件」后缀用）
    pub(super) fn pending_count(&self) -> usize {
        [&self.program, &self.ai_data, &self.ura_data, &self.plugins]
            .iter()
            .filter(|state| state.pending())
            .count()
    }

    /// 测试用：按「程序 / AI 数据 / 小黑板数据 / 小黑板插件」是否待更新造一份结论
    #[cfg(test)]
    pub(super) fn for_test(pending: [bool; 4]) -> Self {
        let state = |pending: bool| match pending {
            true => ChannelState::Pending {
                version: "v".to_string(),
                bytes: 0,
            },
            false => ChannelState::Latest("v".to_string()),
        };
        Self {
            program: state(pending[0]),
            ai_data: state(pending[1]),
            ura_data: state(pending[2]),
            plugins: state(pending[3]),
        }
    }

    /// 头部「更新」行文本：一行汇总四频道，附状态点染色
    pub(super) fn summary(&self, paint: &Paint) -> String {
        if let Some(reason) = self.skip_reason() {
            return paint.dim(&format!("－ 已跳过（{reason}）"));
        }
        let items = [
            ("程序", &self.program),
            (DataChannel::AiData.name(), &self.ai_data),
            (DataChannel::UraData.name(), &self.ura_data),
            (DataChannel::Plugins.name(), &self.plugins),
        ];
        let mut pending = Vec::new();
        let mut latest = Vec::new();
        let mut failures = Vec::new();
        for (name, state) in items {
            match state {
                ChannelState::Pending { version, bytes } => pending.push(format!(
                    "{name} {}（{:.1} MB）",
                    short(version),
                    *bytes as f64 / MB
                )),
                ChannelState::Latest(version) => {
                    latest.push(format!("{name} {}", short(version)));
                }
                ChannelState::Skipped(reason) => latest.push(format!("{name} {reason}")),
                ChannelState::Failed(detail) => failures.push(format!("{name} {}", clip(detail))),
            }
        }
        if !pending.is_empty() {
            let mut text = format!(
                "{} {} 项可更新：{}",
                paint.green("●"),
                pending.len(),
                pending.join("、")
            );
            if !failures.is_empty() {
                text.push_str(&format!("；{} 项检查失败", failures.len()));
            }
            return text;
        }
        if let Some(first) = failures.first() {
            return if latest.is_empty() {
                format!("{} 检查失败：{first}（可点 4 / 5 重试）", paint.red("○"))
            } else {
                format!(
                    "{} 部分检查失败：{}",
                    paint.yellow("●"),
                    failures.join("；")
                )
            };
        }
        if latest.is_empty() {
            return paint.dim("－ 无可检查频道");
        }
        format!("{} 已是最新（{}）", paint.green("●"), latest.join(" · "))
    }
}

/// 启动时跑一次只读自检；`paint` 用于清掉「检查中」提示行
pub(super) fn run(paths: &Paths, paint: &Paint) -> Status {
    if let Some(reason) = skip_reason() {
        return Status::skipped(reason);
    }
    let tty = io::stdout().is_terminal();
    if tty {
        print!("检查更新…");
        let _ = io::stdout().flush();
    }
    let status = inspect(paths);
    if tty {
        paint.clear();
    }
    status
}

/// 跳过自检的原因（非交互运行或显式关闭）
fn skip_reason() -> Option<&'static str> {
    if std::env::var_os(DISABLE_ENV).is_some() {
        return Some(DISABLE_ENV);
    }
    if !io::stdin().is_terminal() {
        return Some("非交互运行");
    }
    None
}

/// 四频道并发检查（总预算即单请求上限），任一失败只影响自己那一行
fn inspect(paths: &Paths) -> Status {
    let Ok(source) = update::source() else {
        return Status::failed("更新源常量不可用");
    };
    let transport = HttpTransport::with_timeouts(CONNECT_TIMEOUT, TOTAL_TIMEOUT);
    let install = paths.install_dir.clone();

    std::thread::scope(|threads| {
        let program = {
            let (src, transport, dir) = (source.clone(), transport.clone(), install.clone());
            threads.spawn(move || check_program(&dir, &src, &transport))
        };
        let ai = spawn_data(threads, DataChannel::AiData, &install, &source, &transport);
        let ura = spawn_data(threads, DataChannel::UraData, &install, &source, &transport);
        let plugins = spawn_data(threads, DataChannel::Plugins, &install, &source, &transport);
        Status {
            program: join(program),
            ai_data: settle(ai),
            ura_data: settle(ura),
            // 已是最新时优先显示本地版本标记（`.portable/version.json`），离线也读得到
            plugins: match settle(plugins) {
                ChannelState::Latest(remote) => {
                    ChannelState::Latest(installed_plugins(&install).unwrap_or(remote))
                }
                other => other,
            },
        }
    })
}

/// 一个数据频道的自检任务：目录不在位 / 声明失败直接给结论，否则起线程
enum Task<'scope> {
    /// 频道目录不在位，不参与自检
    Absent,
    /// 频道声明失败（本地配置读不出等）
    Failed(String),
    /// 线程在跑
    Running(ScopedJoinHandle<'scope, ChannelState>),
}

/// 起一个数据频道的自检线程（目录不在位时不发起任何请求）
fn spawn_data<'scope>(
    threads: &'scope Scope<'scope, '_>,
    scope: DataChannel,
    install: &Path,
    source: &Source,
    transport: &HttpTransport,
) -> Task<'scope> {
    if !scope.ready(install) {
        return Task::Absent;
    }
    let channel = match scope.channel(install) {
        Ok(channel) => channel,
        Err(err) => return Task::Failed(err.to_string()),
    };
    let (src, transport) = (source.clone(), transport.clone());
    Task::Running(threads.spawn(move || check_data(scope, channel, &src, &transport)))
}

/// 收线：目录不在位记「跳过」，声明失败与线程 panic 记「失败」
fn settle(task: Task<'_>) -> ChannelState {
    match task {
        Task::Absent => ChannelState::Skipped("目录未就位"),
        Task::Failed(err) => ChannelState::Failed(err),
        Task::Running(handle) => join(handle),
    }
}

/// 收线：线程 panic 只让本频道失败，不影响其他频道
fn join(handle: ScopedJoinHandle<'_, ChannelState>) -> ChannelState {
    handle
        .join()
        .unwrap_or_else(|_| ChannelState::Failed("自检线程异常退出".to_string()))
}

/// 程序频道：拉清单 + 本地逐文件比对（不下载整包、不验签）
fn check_program(install: &Path, source: &Source, transport: &dyn Transport) -> ChannelState {
    let manifest = match program::fetch_manifest(source, transport) {
        Ok(manifest) => manifest,
        Err(err) => return ChannelState::Failed(err.to_string()),
    };
    let pending = program::pending_files(install, &manifest);
    if pending.is_empty() {
        return ChannelState::Latest(program::version_label(&manifest));
    }
    // 程序频道按整包下载量报体积（逐文件之和是解压后的大小，会明显高估）
    ChannelState::Pending {
        version: program::version_label(&manifest),
        bytes: program::package_size(&manifest),
    }
}

/// 数据频道：`check` + 与更新菜单同一套形态守卫
fn check_data(
    scope: DataChannel,
    channel: Channel,
    source: &Source,
    transport: &dyn Transport,
) -> ChannelState {
    let report = check(source, std::slice::from_ref(&channel), transport);
    let Some(item) = report.channels.first() else {
        return ChannelState::Failed("检查结果为空".to_string());
    };
    let outcome = match &item.result {
        Ok(outcome) => outcome,
        // 清单 404 = 该频道还没发布（新频道首次上线的正常中间态），记跳过而不是失败
        Err(err) if missing_channel(err) => return ChannelState::Skipped("频道未上线"),
        Err(err) => return ChannelState::Failed(err.to_string()),
    };
    if let Err(err) = scope.guard(outcome) {
        return ChannelState::Failed(err.to_string());
    }
    if outcome.pending.is_empty() {
        return ChannelState::Latest(outcome.version.clone());
    }
    ChannelState::Pending {
        version: outcome.version.clone(),
        bytes: outcome.pending_bytes,
    }
}

/// 清单 404 判定：远端频道目录还没有 `update.json`（首次发布前属正常态）
fn missing_channel(err: &umaai_update::UpdateError) -> bool {
    matches!(err, umaai_update::UpdateError::Http { detail, .. } if detail.contains("404"))
}

/// 读本地插件版本标记（`.portable/version.json`，只读；缺失或损坏返回 `None`）
fn installed_plugins(install: &Path) -> Option<String> {
    /// 版本标记里本模块关心的字段
    #[derive(serde::Deserialize)]
    struct Local {
        /// 已安装版本
        version: String,
    }
    let path = install.join(PORTABLE_DIR).join(update::VERSION_FILE);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Local>(&text)
        .ok()
        .map(|local| local.version)
}

/// 版本串显示用裁剪：时间戳型版本（`2026-11-10 12:35:32.912264`）只留到秒
fn short(version: &str) -> String {
    match version.find('.') {
        Some(dot) if dot >= 19 => version[..dot].to_string(),
        _ => version.to_string(),
    }
}

/// 失败原因裁剪到头部一行能容纳的长度
fn clip(detail: &str) -> String {
    let mut out: String = detail.chars().take(DETAIL_CHARS).collect();
    if detail.chars().count() > DETAIL_CHARS {
        out.push('…');
    }
    out
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

    /// 造一份四频道结论
    fn status(
        program: ChannelState,
        ai: ChannelState,
        ura: ChannelState,
        plugins: ChannelState,
    ) -> Status {
        Status {
            program,
            ai_data: ai,
            ura_data: ura,
            plugins,
        }
    }

    /// 以「已是最新」造一个状态串
    fn latest(version: &str) -> ChannelState {
        ChannelState::Latest(version.to_string())
    }

    /// 时间戳型版本只留到秒，非时间戳原样返回
    #[test]
    fn short_trims_micros() {
        assert_eq!(short("2026-11-10 12:35:32.912264"), "2026-11-10 12:35:32");
        assert_eq!(short("0.1.2"), "0.1.2");
        assert_eq!(short("2026-11-10"), "2026-11-10");
    }

    /// 全部最新：一行汇总四个版本（插件显示本地版本标记）
    #[test]
    fn summary_all_latest() {
        let paint = Paint::new(false);
        let all = status(
            latest("0.1.2"),
            latest("2026-11-10 12:35:32.912264"),
            latest("2026-11-10 13:41:58"),
            latest("20261111"),
        );
        let text = all.summary(&paint);
        assert!(text.contains("已是最新"), "{text}");
        assert!(text.contains("程序 0.1.2"), "{text}");
        assert!(text.contains("小黑板插件 20261111"), "{text}");
        assert_eq!(all.pending_count(), 0);
    }

    /// 有更新：列出条数与体量，并按频道置菜单后缀
    #[test]
    fn summary_pending_sets_menu_flags() {
        let paint = Paint::new(false);
        let mixed = status(
            ChannelState::Pending {
                version: "0.1.3".to_string(),
                bytes: 3 * 1024 * 1024,
            },
            latest("2026-11-11 09:00:00"),
            latest("2026-11-10 13:41:58"),
            ChannelState::Pending {
                version: "20261112".to_string(),
                bytes: 24 * 1024 * 1024,
            },
        );
        let text = mixed.summary(&paint);
        assert!(text.contains("2 项可更新"), "{text}");
        assert!(text.contains("程序 0.1.3"), "{text}");
        assert!(text.contains("小黑板插件 20261112"), "{text}");
        assert!(mixed.program_pending() && mixed.plugins_pending());
        assert!(!mixed.ai_data_pending() && !mixed.ura_data_pending());
        assert_eq!(mixed.pending_count(), 2);
    }

    /// 全部失败：红点 + 首条原因；不全失败时降为黄点
    #[test]
    fn summary_failures() {
        let paint = Paint::new(false);
        let dead = status(
            ChannelState::Failed("连接超时".to_string()),
            ChannelState::Failed("连接超时".to_string()),
            ChannelState::Failed("连接超时".to_string()),
            ChannelState::Failed("连接超时".to_string()),
        );
        assert!(dead.summary(&paint).contains("检查失败"));
        let mixed = status(
            latest("0.1.2"),
            ChannelState::Failed("连接超时".to_string()),
            latest("2026-11-10 13:41:58"),
            ChannelState::Skipped("目录未就位"),
        );
        let text = mixed.summary(&paint);
        assert!(text.contains("部分检查失败"), "{text}");
        assert!(text.contains("AI 数据 连接超时"), "{text}");
        assert_eq!(mixed.pending_count(), 0);
        // 「未就位」只在没有失败项时并入「已是最新」一行，避免一行里堆两种语义
        let absent = status(
            latest("0.1.2"),
            latest("2026-11-10 12:35:32"),
            ChannelState::Skipped("目录未就位"),
            ChannelState::Skipped("目录未就位"),
        );
        let text = absent.summary(&paint);
        assert!(text.contains("已是最新"), "{text}");
        assert!(text.contains("小黑板插件 目录未就位"), "{text}");
    }

    /// 跳过：显示原因，不报「检查失败」
    #[test]
    fn summary_skipped() {
        let paint = Paint::new(false);
        let text = Status::skipped("非交互运行").summary(&paint);
        assert!(text.contains("已跳过"), "{text}");
        assert!(text.contains("非交互运行"), "{text}");
        assert!(!text.contains("检查失败"), "{text}");
    }

    /// 失败原因按字符裁剪，避免头部一行被协议错误撑爆
    #[test]
    fn clip_limits_width() {
        let long = "连接失败：https://cdn2.viktorlab.cn/uma/program/update.json 被拒绝";
        let clipped = clip(long);
        assert!(clipped.chars().count() <= DETAIL_CHARS + 1, "{clipped}");
        assert!(clipped.ends_with('…'), "{clipped}");
        assert_eq!(clip("短"), "短");
    }

    /// 插件频道离线往返：本地一致 → 已是最新；改一个字节 → 待更新
    #[test]
    fn plugins_channel_offline_roundtrip() {
        use umaai_update::{Source, TestTransport, sha256_hex};

        let dir = temp_base().join(format!("matsuri-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = dir.join(PORTABLE_DIR).join("Plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let body = b"zip-bytes";
        std::fs::write(plugins.join("A.zip"), body).unwrap();
        let marker = br#"{"version":"20261111"}"#;
        std::fs::write(dir.join(PORTABLE_DIR).join("version.json"), marker).unwrap();

        let manifest = serde_json::json!({
            "protocol": 1,
            "version": "20261111",
            "files": [
                {"path": "Plugins/A.zip", "size": body.len(), "sha256": sha256_hex(body)},
                {"path": "version.json", "size": marker.len(), "sha256": sha256_hex(marker)},
            ],
        });
        let transport = TestTransport::new().serve_str(
            "https://cdn2.viktorlab.cn/uma/ura_plugins/update.json",
            &manifest.to_string(),
        );
        let source = Source::new("https://cdn2.viktorlab.cn/uma").unwrap();
        let channel = DataChannel::Plugins.channel(&dir).unwrap();

        let state = check_data(DataChannel::Plugins, channel.clone(), &source, &transport);
        assert!(matches!(state, ChannelState::Latest(_)), "{state:?}");

        std::fs::write(plugins.join("A.zip"), b"other").unwrap();
        let state = check_data(DataChannel::Plugins, channel, &source, &transport);
        assert!(matches!(state, ChannelState::Pending { .. }), "{state:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 联网冒烟：指向真实安装目录（`MATSURI_SMOKE_INSTALL`），打印头部一行与逐频道结论
    #[test]
    #[ignore = "联网冒烟：需 MATSURI_SMOKE_INSTALL 指向真实安装目录与可用网络"]
    fn smoke_live_inspect() {
        let install =
            std::path::PathBuf::from(std::env::var("MATSURI_SMOKE_INSTALL").expect("需要安装目录"));
        let paths = crate::Paths {
            core_exe: install.join("bin").join(crate::platform_exe("matsuri")),
            ura_exe: install.join(crate::platform_exe(crate::BLACKBOARD_EXE)),
            install_dir: install,
            game_root: None,
        };
        let status = inspect(&paths);
        println!("{status:?}");
        println!("更新  {}", status.summary(&Paint::new(false)));
    }
}
