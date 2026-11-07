//! 发布启动器 `matsuri-ai`：菜单、子进程装配、小黑板通道只读体检。
//!
//! 只做三件事：解析工作目录与游戏根、按选项启动 `bin/matsuri.exe`、收尾停留。
//! 不改核心 CLI、不写任何文件、不落日志。
//!
//! 两条数据路径勿混：`data/` 由核心 CLI 的 `DATA_ROOT="data"` 按子进程 cwd 解析；
//! 真机帧由 `--root` 指向的游戏根拼 `.portable/PluginData/SendGameStatusPlugin`。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

/// 游戏根下的 `.portable` 目录名（与引擎 `channel.rs` 约定一致）
const PORTABLE_DIR: &str = ".portable";
/// 插件输出目录的中间层
const PLUGIN_PARENT: &str = "PluginData";
/// 小黑板插件目录名
const PLUGIN_NAME: &str = "SendGameStatusPlugin";
/// 回合帧固定名
const FRAME_THIS_TURN: &str = "thisTurn.json";
/// 终局帧后缀：`game{id}_finalScore.json`
const FINAL_SCORE_SUFFIX: &str = "_finalScore.json";
/// 通道「近期活跃」的判定窗口（秒）
const FRESH_SECS: u64 = 120;

/// 本 crate 的错误类型（零依赖，不引第三方 error 库）
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// 启动器解析出的全部路径（固定结构，避免裸元组）
struct Paths {
    /// 安装根：exe 所在目录，也是子进程 cwd（`data/` 在此）
    install_dir: PathBuf,
    /// 启动时的当前目录
    launch_cwd: PathBuf,
    /// 核心 CLI 可执行文件
    core_exe: PathBuf,
    /// `--root` 指向的游戏根，找不到为 `None`
    game_root: Option<PathBuf>,
}

/// Windows 控制台 UTF-8 设置（非 Windows 为空实现，保 Linux CI 四门）
#[cfg(windows)]
mod console {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCP(code: u32) -> i32;
        fn SetConsoleOutputCP(code: u32) -> i32;
    }

    /// 把输入 / 输出代码页切到 UTF-8，避免中文乱码
    pub fn setup() {
        const CP_UTF8: u32 = 65001;
        // SAFETY: 两个 Win32 调用只改控制台代码页，无指针与生命周期约束。
        unsafe {
            let _ = SetConsoleCP(CP_UTF8);
            let _ = SetConsoleOutputCP(CP_UTF8);
        }
    }
}

/// 非 Windows 不需要设置代码页
#[cfg(not(windows))]
mod console {
    /// 空实现
    pub fn setup() {}
}

fn main() -> ExitCode {
    console::setup();
    let paths = match resolve_paths() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("启动失败：{err}");
            pause("按回车关闭…");
            return ExitCode::FAILURE;
        }
    };
    menu_loop(&paths)
}

/// 解析安装根、启动时 cwd、核心 exe 与游戏根
fn resolve_paths() -> Result<Paths> {
    let exe = std::env::current_exe()?;
    let install_dir = exe
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "取不到启动器所在目录"))?;
    let launch_cwd = std::env::current_dir()?;
    let core_exe = install_dir.join("bin").join(platform_exe("matsuri"));
    let game_root = find_game_root(&launch_cwd, &install_dir);
    Ok(Paths {
        install_dir,
        launch_cwd,
        core_exe,
        game_root,
    })
}

/// 可执行文件文件名（Windows 带 `.exe`）
fn platform_exe(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// 小黑板插件目录：`<root>/.portable/PluginData/SendGameStatusPlugin`
fn plugin_dir(root: &Path) -> PathBuf {
    root.join(PORTABLE_DIR)
        .join(PLUGIN_PARENT)
        .join(PLUGIN_NAME)
}

/// 游戏根解析：启动时 cwd 优先，其次安装根；两处都没有返回 `None`
fn find_game_root(launch_cwd: &Path, install_dir: &Path) -> Option<PathBuf> {
    [launch_cwd, install_dir]
        .into_iter()
        .find(|root| plugin_dir(root).is_dir())
        .map(Path::to_path_buf)
}

/// 核心 CLI 版本（`--version`）；取不到时显示「未知」
fn core_version(paths: &Paths) -> String {
    match Command::new(&paths.core_exe)
        .current_dir(&paths.install_dir)
        .arg("--version")
        .output()
    {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                "未知".to_string()
            } else {
                text
            }
        }
        _ => "未知".to_string(),
    }
}

/// 主菜单循环：四行只读信息 + 在线 / 体检 / 退出
fn menu_loop(paths: &Paths) -> ExitCode {
    let launcher_ver = option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0");
    let core_ver = core_version(paths);
    let root_text = match &paths.game_root {
        Some(root) => root.display().to_string(),
        None => "未找到（需要含 .portable 的目录）".to_string(),
    };
    let channel_ok = paths
        .game_root
        .as_deref()
        .is_some_and(|root| plugin_dir(root).is_dir());
    loop {
        println!();
        println!("骏大祭 AI");
        println!(
            "  工作目录  {}  （子进程 cwd，data/ 在此）",
            paths.install_dir.display()
        );
        println!(
            "  启动时目录  {}  （游戏根默认取它）",
            paths.launch_cwd.display()
        );
        println!("  游戏根    {root_text}  （--root）");
        println!(
            "  小黑板    {}",
            if channel_ok {
                "插件目录已就绪"
            } else {
                "插件目录未找到"
            }
        );
        println!("  版本      启动器 {launcher_ver} / {core_ver}");
        println!();
        println!("  1. 在线模式（默认，回车）");
        println!("  2. 检查小黑板连接");
        println!("  0. 退出");
        print!("请选择：");
        let _ = io::stdout().flush();
        let mut line = String::new();
        // EOF（0 字节）必须和「用户按回车」区分开：管道 / 重定向时不能当默认在线，否则空转。
        match io::stdin().read_line(&mut line) {
            Ok(0) => return ExitCode::SUCCESS,
            Ok(_) => {}
            Err(_) => return ExitCode::FAILURE,
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "" | "1" => {
                if let Err(err) = run_online(paths) {
                    eprintln!("在线模式：{err}");
                }
                pause("按回车返回菜单…");
            }
            "2" => {
                if let Err(err) = check_channel(paths) {
                    eprintln!("体检：{err}");
                }
                pause("按回车返回菜单…");
            }
            "0" | "q" => return ExitCode::SUCCESS,
            _ => println!("无此选项，请输入 1 / 2 / 0。"),
        }
    }
}

/// 在线模式：装配 `bin\matsuri.exe online --root <game_root>`，cwd 必须是安装根
fn run_online(paths: &Paths) -> Result<()> {
    let root = paths.game_root.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "未找到小黑板目录：请在含 .portable 的游戏根目录下启动，或检查插件是否安装",
        )
    })?;
    if !paths.core_exe.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "核心程序不存在：{}（请检查发布包完整性）",
                paths.core_exe.display()
            ),
        )
        .into());
    }
    println!();
    println!(
        "启动：{} online --root {}",
        paths.core_exe.display(),
        root.display()
    );
    println!("（长驻监听真机帧；Ctrl+C 结束。搜索预算仍由 [search].search_n 决定）");
    let status = Command::new(&paths.core_exe)
        .current_dir(&paths.install_dir)
        .arg("online")
        .arg("--root")
        .arg(root)
        .status()?;
    match status.code() {
        Some(code) => println!("核心程序退出码：{code}"),
        None => println!("核心程序被信号终止"),
    }
    if !status.success() {
        println!("非正常退出；请确认游戏根目录与插件目录是否正确。");
    }
    Ok(())
}

/// 通道体检：只读检查插件目录与白名单帧新鲜度，不启动子进程、不写文件
fn check_channel(paths: &Paths) -> Result<()> {
    let root = paths
        .game_root
        .as_deref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "未找到游戏根目录，无法体检"))?;
    let dir = plugin_dir(root);
    println!();
    println!("插件目录：{}", dir.display());
    if !dir.is_dir() {
        println!("结论：目录缺失 —— 检查游戏根目录与转发插件是否安装并产生过帧。");
        return Ok(());
    }
    let mut frames: Vec<(String, SystemTime)> = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_frame_name(&name) {
            frames.push((name, entry.metadata()?.modified()?));
        }
    }
    if frames.is_empty() {
        println!("结论：目录存在但没有白名单帧 —— 可能还没开始育成，或插件尚未写出。");
        println!("（白名单：thisTurn.json 与 game{{id}}_finalScore.json）");
        return Ok(());
    }
    frames.sort_by(|a, b| b.1.cmp(&a.1));
    let (name, latest) = frames
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "帧列表为空"))?;
    let age = match SystemTime::now().duration_since(*latest) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    };
    println!("白名单帧数：{}", frames.len());
    println!("最近一帧：{name}（{} 秒前）", age.as_secs());
    if age.as_secs() <= FRESH_SECS {
        println!("结论：最近 {FRESH_SECS} 秒内有写入，通道近期活跃。");
    } else {
        println!("结论：超过 {FRESH_SECS} 秒没有新帧 —— 可能未在育成中，或插件已停止输出。");
    }
    println!("说明：文件时间只证明「曾被写过」，不代表插件此刻在线；停在育成界面时可能误报。");
    Ok(())
}

/// 是否属于引擎实时通道白名单（口径见引擎 `channel.rs:98-114`）
fn is_frame_name(name: &str) -> bool {
    if name == FRAME_THIS_TURN {
        return true;
    }
    let Some(id) = name
        .strip_prefix("game")
        .and_then(|rest| rest.strip_suffix(FINAL_SCORE_SUFFIX))
    else {
        return false;
    };
    !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
}

/// 停留窗口：打印提示并读一行（不调用外部 shell）
fn pause(prompt: &str) {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}
