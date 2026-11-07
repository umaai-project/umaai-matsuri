//! 极简 stderr 日志器：消费引擎配置 `[output] log_level`
//!
//! 引擎与剧本的诊断日志（`diag!`，target = `diagnostic`）经 `log` 门面出来后落到这里；
//! 不装日志器时这些记录被静默丢弃（`log` 门面语义）。级别放在共享状态上，重复初始化
//! （测试里多次调用）不报错且即时生效。

use std::sync::Mutex;

use anyhow::{Result, bail};

/// 生效的日志级别（`enabled` 每次读取；重复装日志器后新级别也会作用到已装实例）
static LEVEL: Mutex<log::LevelFilter> = Mutex::new(log::LevelFilter::Off);

/// 当前生效的日志级别
fn level() -> log::LevelFilter {
    *LEVEL.lock().unwrap_or_else(|err| err.into_inner())
}

/// 极简 stderr 日志器（一行一条：`[INFO ] 文本`）
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, meta: &log::Metadata<'_>) -> bool {
        meta.level() <= level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{:<5}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// 按级别名装日志器（级别名非法即报错，与配置层严格口径一致）
///
/// 环境变量 `MATSURI_LOG_LEVEL` 覆盖配置值——生产默认 `warn`，现场排障时临时提到 `info`。
pub fn init_logger(level_name: &str) -> Result<()> {
    let level_name = std::env::var("MATSURI_LOG_LEVEL").unwrap_or_else(|_| level_name.to_string());
    let filter = match level_name.to_ascii_lowercase().as_str() {
        "off" => log::LevelFilter::Off,
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        other => bail!(
            "output.log_level 非法 \"{other}\"（可用: off / error / warn / info / debug / trace）"
        ),
    };
    *LEVEL.lock().unwrap_or_else(|err| err.into_inner()) = filter;
    log::set_max_level(filter);
    if filter == log::LevelFilter::Off {
        return Ok(());
    }
    // 已装过（重复调用）不是错误；级别走静态，重复装也能同步
    let _ = log::set_boxed_logger(Box::new(StderrLogger));
    Ok(())
}
