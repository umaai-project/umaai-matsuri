//! 发布启动器 `matsuri-ai`：菜单、子进程装配、小黑板启动、通道只读体检、更新子菜单与启动自检。
//!
//! 只做七件事：解析工作目录与游戏根、按选项启动 `bin/matsuri.exe`、
//! 另开控制台窗口拉起同目录小黑板（URA）、只读体检、按频道更新（AI 数据 / 小黑板数据 /
//! 小黑板插件 / 程序本体，收在更新子菜单里）、启动时只读自检四份清单（只显示结论）。
//! 不改核心 CLI、不落日志；数据只写 `data/` 与 `.portable/`，程序更新只写程序本体与程序资产。
//!
//! 两条数据路径勿混：`data/` 由核心 CLI 的 `DATA_ROOT="data"` 按子进程 cwd 解析；
//! 真机帧由 `--root` 指向的游戏根拼 `.portable/PluginData/SendGameStatusPlugin`。
//!
//! 小黑板是 Terminal.Gui 常驻 TUI（stdin / stdout 被重定向即报错退出），
//! 必须用 `CREATE_NEW_CONSOLE` 独立窗口启动，否则菜单与在线模式互相占屏。

mod program;
mod startup;
mod update;

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

use inquire::Select;

/// 主菜单项：TTY 下由 inquire 渲染，非 TTY 下按 `key()` 编号打印
const ITEMS: [Item; 5] = [
    Item::Online,
    Item::Blackboard,
    Item::Check,
    Item::Updates,
    Item::Quit,
];

/// 更新子菜单项（各项只跑自己的频道）
const UPDATE_ITEMS: [UpdateItem; 6] = [
    UpdateItem::Check,
    UpdateItem::AiData,
    UpdateItem::UraData,
    UpdateItem::Plugins,
    UpdateItem::Program,
    UpdateItem::Back,
];

/// 一份可渲染的菜单项：`key()` 供非 TTY 编号输入，`label()` 按自检结论追加后缀
trait MenuEntry: Copy {
    /// 非 TTY 兜底编号（末项为 0）
    fn key(self) -> &'static str;

    /// 菜单显示文本
    fn label(self, up: &startup::Status) -> String;
}

/// 主菜单选项（枚举化，避免标签 / 索引 / 编号三处各写一遍而失步）
#[derive(Clone, Copy)]
enum Item {
    /// 在线模式：长驻监听真机帧
    Online,
    /// 另开窗口拉起同目录小黑板
    Blackboard,
    /// 小黑板通道只读体检
    Check,
    /// 进入更新子菜单
    Updates,
    /// 退出启动器
    Quit,
}

impl Item {
    /// 菜单基础文本（不含自检后缀）
    fn base(self, up: &startup::Status) -> String {
        match self {
            Item::Online => "在线模式（默认，回车）".to_string(),
            Item::Blackboard => "启动小黑板".to_string(),
            Item::Check => "检查小黑板连接".to_string(),
            Item::Updates => match up.pending_count() {
                0 => "更新与插件（联网）".to_string(),
                count => format!("更新与插件（联网） · {count} 项可更新"),
            },
            Item::Quit => "退出".to_string(),
        }
    }
}

impl MenuEntry for Item {
    fn key(self) -> &'static str {
        match self {
            Item::Online => "1",
            Item::Blackboard => "2",
            Item::Check => "3",
            Item::Updates => "4",
            Item::Quit => "0",
        }
    }

    fn label(self, up: &startup::Status) -> String {
        self.base(up)
    }
}

/// 更新子菜单选项：每个更新入口只跑自己的频道
#[derive(Clone, Copy)]
enum UpdateItem {
    /// 重跑一次只读自检，刷新头部与后缀
    Check,
    /// `ai_data`
    AiData,
    /// `ura_data`（`.portable/` 根下的 `.br`）
    UraData,
    /// `ura_plugins`（`.portable/Plugins/*.zip`）
    Plugins,
    /// `program`（整包更新程序本体）
    Program,
    /// 回主菜单
    Back,
}

impl UpdateItem {
    /// 菜单基础文本
    fn base(self) -> &'static str {
        match self {
            UpdateItem::Check => "检查更新（只读，刷新状态）",
            UpdateItem::AiData => "更新 AI 数据",
            UpdateItem::UraData => "更新小黑板数据",
            UpdateItem::Plugins => "更新小黑板插件",
            UpdateItem::Program => "更新程序本体",
            UpdateItem::Back => "返回主菜单",
        }
    }

    /// 该入口对应的频道是否待更新
    fn pending(self, up: &startup::Status) -> bool {
        match self {
            UpdateItem::AiData => up.ai_data_pending(),
            UpdateItem::UraData => up.ura_data_pending(),
            UpdateItem::Plugins => up.plugins_pending(),
            UpdateItem::Program => up.program_pending(),
            UpdateItem::Check | UpdateItem::Back => false,
        }
    }

    /// 该入口对应的数据频道（检查项与返回项没有）
    fn channel(self) -> Option<update::DataChannel> {
        match self {
            UpdateItem::AiData => Some(update::DataChannel::AiData),
            UpdateItem::UraData => Some(update::DataChannel::UraData),
            UpdateItem::Plugins => Some(update::DataChannel::Plugins),
            UpdateItem::Check | UpdateItem::Program | UpdateItem::Back => None,
        }
    }
}

impl MenuEntry for UpdateItem {
    fn key(self) -> &'static str {
        match self {
            UpdateItem::Check => "1",
            UpdateItem::AiData => "2",
            UpdateItem::UraData => "3",
            UpdateItem::Plugins => "4",
            UpdateItem::Program => "5",
            UpdateItem::Back => "0",
        }
    }

    fn label(self, up: &startup::Status) -> String {
        if self.pending(up) {
            format!("{} · 有更新", self.base())
        } else {
            self.base().to_string()
        }
    }
}

/// 头部信息：标题分隔线长度（列）
const SEPARATOR_COLS: usize = 46;
/// 头部信息：标签列显示宽度（4 个全角字 + 2 列间隔）
const LABEL_COLS: usize = 10;
/// 小黑板可执行文件主名（与 `matsuri-ai.exe` 同目录，扩展名由 `platform_exe` 补）
const BLACKBOARD_EXE: &str = "UmamusumeResponseAnalyzer";

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

/// 本 crate 的错误类型（不引第三方 error 库）
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// 以文本构造启动器错误（用于无底层 IO 错误的语义失败）
fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

/// 启动器解析出的全部路径（固定结构，避免裸元组）
struct Paths {
    /// 安装根：exe 所在目录，也是子进程 cwd（`data/` 在此）
    install_dir: PathBuf,
    /// 核心 CLI 可执行文件
    core_exe: PathBuf,
    /// 小黑板可执行文件（同目录，缺失时启动项报错）
    ura_exe: PathBuf,
    /// `--root` 指向的游戏根，找不到为 `None`
    game_root: Option<PathBuf>,
}

/// Windows 控制台 UTF-8 代码页 / 窗口标题 / VT 转义 / 清屏（非 Windows 走空实现，保 Linux CI 四门）
#[cfg(windows)]
mod console {
    /// 控制台字符坐标
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Coord {
        x: i16,
        y: i16,
    }

    /// 控制台矩形（仅用于取缓冲区信息）
    #[repr(C)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }

    /// `CONSOLE_SCREEN_BUFFER_INFO`
    #[repr(C)]
    struct ScreenBufferInfo {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        window: SmallRect,
        max_window: Coord,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCP(code: u32) -> i32;
        fn SetConsoleOutputCP(code: u32) -> i32;
        fn SetConsoleTitleW(title: *const u16) -> i32;
        fn GetStdHandle(std: u32) -> isize;
        fn GetConsoleMode(handle: isize, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: isize, mode: u32) -> i32;
        fn GetConsoleScreenBufferInfo(handle: isize, info: *mut ScreenBufferInfo) -> i32;
        fn FillConsoleOutputCharacterW(
            handle: isize,
            ch: u16,
            len: u32,
            at: Coord,
            written: *mut u32,
        ) -> i32;
        fn SetConsoleCursorPosition(handle: isize, at: Coord) -> i32;
    }

    /// 标准输出句柄编号（`(DWORD)-11`）
    const STD_OUTPUT_HANDLE: u32 = 0u32.wrapping_sub(11);
    /// 启用 VT 转义序列（Windows 10 起支持）
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    /// 切 UTF-8 代码页并尝试开启 VT 上色；返回 VT 是否可用
    pub fn setup() -> bool {
        const CP_UTF8: u32 = 65001;
        // SAFETY: 三个调用只改控制台自身状态，无指针与生命周期约束。
        unsafe {
            let _ = SetConsoleCP(CP_UTF8);
            let _ = SetConsoleOutputCP(CP_UTF8);
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0u32;
            GetConsoleMode(handle, &mut mode) != 0
                && SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }

    /// 设置控制台窗口标题，避免标题栏显示成 exe 路径
    pub fn set_title(title: &str) {
        let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` 以 0 结尾且在本调用期间有效。
        unsafe {
            let _ = SetConsoleTitleW(wide.as_ptr());
        }
    }

    /// 用空格填满整个缓冲区并把光标归位（VT 不可用时的清屏兜底）
    pub fn clear() {
        // SAFETY: 缓冲区信息来自系统调用；填充长度取自该信息，坐标为原点。
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut info: ScreenBufferInfo = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
                return;
            }
            let cells = u32::from(info.size.x.max(0) as u16) * u32::from(info.size.y.max(0) as u16);
            let origin = Coord { x: 0, y: 0 };
            let mut written = 0u32;
            let _ =
                FillConsoleOutputCharacterW(handle, u16::from(b' '), cells, origin, &mut written);
            let _ = SetConsoleCursorPosition(handle, origin);
        }
    }
}

/// 非 Windows：标题为空实现，上色只在真终端里开（避免重定向输出混入转义）
#[cfg(not(windows))]
mod console {
    use std::io::IsTerminal;

    /// 空实现；VT 能力按「是否真终端」判定
    pub fn setup() -> bool {
        std::io::stdout().is_terminal()
    }

    /// 空实现
    pub fn set_title(_title: &str) {}

    /// 真终端里用 VT 清屏；重定向输出时不动，避免脚本日志混入转义
    pub fn clear() {
        if std::io::stdout().is_terminal() {
            print!("\u{1b}[2J\u{1b}[H");
        }
    }
}

/// 终端上色：VT 不可用时全部退化为原文，老 conhost 不出转义残留
struct Paint {
    /// 是否允许写 SGR 序列
    vt: bool,
}

impl Paint {
    /// 按控制台能力构造
    fn new(vt: bool) -> Self {
        Self { vt }
    }

    /// 用 SGR 代码包裹文本；不可用时原样返回
    fn with(&self, sgr: &str, text: &str) -> String {
        if self.vt {
            format!("\u{1b}[{sgr}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }

    /// 标题粗体
    fn bold(&self, text: &str) -> String {
        self.with("1", text)
    }

    /// 分隔线等弱化元素
    fn dim(&self, text: &str) -> String {
        self.with("2", text)
    }

    /// 就绪状态点
    fn green(&self, text: &str) -> String {
        self.with("32", text)
    }

    /// 缺失 / 失败状态点
    fn red(&self, text: &str) -> String {
        self.with("31", text)
    }

    /// 半就绪状态点（程序在位但插件目录缺失等）
    fn yellow(&self, text: &str) -> String {
        self.with("33", text)
    }

    /// 清屏：VT 可用走转义序列，否则交给控制台兜底
    fn clear(&self) {
        if self.vt {
            print!("\u{1b}[2J\u{1b}[H");
            let _ = io::stdout().flush();
        } else {
            console::clear();
        }
    }
}

/// 启动器入口：控制台能力准备 → 路径解析 → 菜单循环
fn main() -> ExitCode {
    let paint = Paint::new(console::setup());
    let launcher_ver = option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0");
    console::set_title(&format!("骏大祭 AI v{launcher_ver}"));

    let paths = match resolve_paths() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("启动失败：{err}");
            pause("按回车关闭…");
            return ExitCode::FAILURE;
        }
    };
    program::startup(&paths.install_dir);
    let mut update = startup::run(&paths, &paint);
    menu_loop(&paths, &paint, &mut update)
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
    let ura_exe = install_dir.join(platform_exe(BLACKBOARD_EXE));
    let game_root = find_game_root(&launch_cwd, &install_dir);
    Ok(Paths {
        install_dir,
        core_exe,
        ura_exe,
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

/// 主菜单循环：头部只读信息 + 在线 / 小黑板 / 体检 / 更新子菜单 / 退出
fn menu_loop(paths: &Paths, paint: &Paint, up: &mut startup::Status) -> ExitCode {
    let core_ver = core_version(paths);
    let mut first = true;
    loop {
        // 每次操作回到菜单前清屏，避免上一轮输出堆叠
        if !first {
            paint.clear();
        }
        first = false;
        print_header(paths, paint, &core_ver, up);
        let Some(item) = choose("请选择", &ITEMS, up) else {
            return ExitCode::SUCCESS;
        };
        match item {
            Item::Online => {
                if let Err(err) = run_online(paths) {
                    eprintln!("在线模式：{err}");
                }
                pause("按回车返回菜单…");
            }
            Item::Blackboard => {
                if let Err(err) = run_blackboard(paths) {
                    eprintln!("小黑板：{err}");
                    pause("按回车返回菜单…");
                }
            }
            Item::Check => {
                if let Err(err) = check_channel(paths) {
                    eprintln!("体检：{err}");
                }
                pause("按回车返回菜单…");
            }
            Item::Updates => update_menu(paths, paint, up),
            Item::Quit => return ExitCode::SUCCESS,
        }
    }
}

/// 更新子菜单：每项只跑自己的频道；动作返回后重跑自检刷新头部与后缀
fn update_menu(paths: &Paths, paint: &Paint, up: &mut startup::Status) {
    let mut first = true;
    loop {
        if !first {
            paint.clear();
        }
        first = false;
        print_update_header(paint, up);
        let Some(item) = choose("更新", &UPDATE_ITEMS, up) else {
            return;
        };
        match item {
            UpdateItem::Back => return,
            // 只读检查：不打印、不确认，直接重跑自检刷新状态
            UpdateItem::Check => {
                *up = startup::run(paths, paint);
                continue;
            }
            UpdateItem::Program => {
                if let Err(err) = program::run(paths, paint) {
                    eprintln!("更新程序：{err}");
                }
            }
            _ => {
                let Some(channel) = item.channel() else {
                    continue;
                };
                if let Err(err) = update::run(paths, paint, channel) {
                    eprintln!("更新{}：{err}", channel.name());
                }
            }
        }
        pause("按回车返回更新菜单…");
        // 动作后重跑自检：让头部的「更新」行与菜单后缀反映最新状态
        *up = startup::run(paths, paint);
    }
}

/// 更新子菜单头：标题 + 分隔线 + 当前自检结论
fn print_update_header(paint: &Paint, up: &startup::Status) {
    println!();
    println!("{}", paint.bold("更新与插件"));
    println!("{}", paint.dim(&"─".repeat(SEPARATOR_COLS)));
    info_row("状态", &up.summary(paint));
    println!();
}

/// 头部信息：标题 + 分隔线 + 五行只读状态（标签列按显示宽度对齐）
fn print_header(paths: &Paths, paint: &Paint, core_ver: &str, up: &startup::Status) {
    let launcher_ver = option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0");
    let ura_ok = paths.ura_exe.is_file();
    let plugin_ok = paths
        .game_root
        .as_deref()
        .is_some_and(|root| plugin_dir(root).is_dir());
    println!();
    println!(
        "{}",
        paint.bold(&format!("骏大祭 AI · 駿大祭編 · v{launcher_ver}"))
    );
    println!("{}", paint.dim(&"─".repeat(SEPARATOR_COLS)));
    info_row("安装目录", &paths.install_dir.display().to_string());
    info_row(
        "游戏根",
        &match &paths.game_root {
            Some(root) => root.display().to_string(),
            None => paint.red("未找到（需要含 .portable 的游戏根）"),
        },
    );
    let (dot, text) = match (ura_ok, plugin_ok) {
        (true, true) => (paint.green("●"), "程序在位 · 插件目录就绪"),
        (true, false) => (paint.yellow("●"), "程序在位 · 插件目录未找到"),
        (false, true) => (paint.red("○"), "程序缺失 · 插件目录就绪"),
        (false, false) => (paint.red("○"), "程序缺失 · 插件目录未找到"),
    };
    info_row("小黑板", &format!("{dot} {text}"));
    let core_exe_name = paths
        .core_exe
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    info_row(
        "核心程序",
        &if paths.core_exe.is_file() {
            format!("{} bin/{core_exe_name} · {core_ver}", paint.green("●"))
        } else {
            format!("{} bin/{core_exe_name} 缺失", paint.red("○"))
        },
    );
    info_row("更新", &up.summary(paint));
    println!();
}

/// 头部信息行：标签按显示宽度补齐到 `LABEL_COLS` 列
fn info_row(label: &str, value: &str) {
    let pad = LABEL_COLS.saturating_sub(display_width(label));
    println!("{label}{}{value}", " ".repeat(pad));
}

/// 终端显示宽度：全角 / 宽字符按 2 列计，只用于头部标签对齐
fn display_width(text: &str) -> usize {
    text.chars().map(|ch| if is_wide(ch) { 2 } else { 1 }).sum()
}

/// 是否宽字符（CJK / 假名 / 全角标点 / 常用 emoji）
fn is_wide(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1F64F
            | 0x1F900..=0x1F9FF
            | 0x20000..=0x3FFFD
    )
}

/// 读取一次菜单选择；`None` 表示结束程序 / 返回上级
fn choose<T: MenuEntry>(title: &str, items: &[T], up: &startup::Status) -> Option<T> {
    if !io::stdin().is_terminal() {
        return choose_plain(items, up);
    }
    let labels: Vec<String> = items.iter().map(|item| item.label(up)).collect();
    match Select::new(title, labels).prompt() {
        Ok(choice) => items.iter().copied().find(|item| item.label(up) == choice),
        // 取消（ESC / Ctrl+C）、非 TTY 或其它 inquire 错误一律退出，不空转。
        Err(_) => None,
    }
}

/// 非 TTY（管道 / 重定向）兜底：编号输入，供脚本调用与自动化验证
fn choose_plain<T: MenuEntry>(items: &[T], up: &startup::Status) -> Option<T> {
    loop {
        for item in items {
            println!("  {}. {}", item.key(), item.label(up));
        }
        print!("请选择：");
        let _ = io::stdout().flush();
        let mut line = String::new();
        // EOF（0 字节）必须和「用户按回车」区分开，否则管道场景会无限循环。
        match io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        let answer = line.trim().to_ascii_lowercase();
        if answer.is_empty() {
            return items.first().copied();
        }
        if answer == "q" {
            return None;
        }
        if let Some(item) = items.iter().copied().find(|item| item.key() == answer) {
            return Some(item);
        }
        let keys: Vec<&str> = items.iter().map(|item| item.key()).collect();
        println!("无此选项，请输入 {}。", keys.join(" / "));
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

/// 小黑板：同目录 URA 另开控制台窗口拉起，不等待（它要与在线模式同时常驻）
fn run_blackboard(paths: &Paths) -> Result<()> {
    let exe = &paths.ura_exe;
    if !exe.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "小黑板程序不存在：{}（发布包应与 matsuri-ai.exe 放在同一目录）",
                exe.display()
            ),
        )
        .into());
    }
    spawn_detached(exe, &paths.install_dir)?;
    println!();
    println!("已在新窗口启动小黑板：{}", exe.display());
    println!("  1. 在该窗口点「启动」，确认监听地址与端口（默认 4693）");
    println!("  2. 回到本窗口选「在线模式」，再打开游戏");
    println!("  3. 结束小黑板用 Ctrl+C");
    println!("（工作目录沿用小黑板自身设置：exe 旁有 .portable 即便携模式）");
    Ok(())
}

/// 独立控制台启动：TUI 抢占终端，且父进程不能等它退出
#[cfg(windows)]
fn spawn_detached(exe: &Path, dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    /// 新开一个控制台窗口（不继承本进程的 stdin / stdout）
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    Command::new(exe)
        .current_dir(dir)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(|_| ())
}

/// 非 Windows 无控制台窗口概念，只能普通启动（不等待）
#[cfg(not(windows))]
fn spawn_detached(exe: &Path, dir: &Path) -> io::Result<()> {
    Command::new(exe).current_dir(dir).spawn().map(|_| ())
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
    frames.sort_by_key(|frame| std::cmp::Reverse(frame.1));
    let (name, latest) = frames
        .first()
        .ok_or_else(|| io::Error::other("帧列表为空"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 主菜单第 4 项按待更新数追加后缀，其余项不受影响
    #[test]
    fn main_menu_labels() {
        let none = startup::Status::for_test([false; 4]);
        assert_eq!(Item::Updates.label(&none), "更新与插件（联网）");
        let some = startup::Status::for_test([true, false, false, true]);
        assert_eq!(
            Item::Updates.label(&some),
            "更新与插件（联网） · 2 项可更新"
        );
        assert_eq!(Item::Online.label(&some), "在线模式（默认，回车）");
        assert_eq!(Item::Quit.label(&some), "退出");
    }

    /// 更新子菜单只在对应频道待更新时追加后缀
    #[test]
    fn update_menu_labels() {
        let some = startup::Status::for_test([true, false, true, false]);
        assert_eq!(UpdateItem::Program.label(&some), "更新程序本体 · 有更新");
        assert_eq!(UpdateItem::UraData.label(&some), "更新小黑板数据 · 有更新");
        assert_eq!(UpdateItem::AiData.label(&some), "更新 AI 数据");
        assert_eq!(UpdateItem::Plugins.label(&some), "更新小黑板插件");
        assert_eq!(UpdateItem::Check.label(&some), "检查更新（只读，刷新状态）");
        assert_eq!(UpdateItem::Back.label(&some), "返回主菜单");
    }

    /// 编号：主菜单 1–4 / 0，更新子菜单 1–5 / 0（非 TTY 脚本依赖它）
    #[test]
    fn menu_keys() {
        let keys: Vec<&str> = ITEMS.iter().map(|item| item.key()).collect();
        assert_eq!(keys, ["1", "2", "3", "4", "0"]);
        let keys: Vec<&str> = UPDATE_ITEMS.iter().map(|item| item.key()).collect();
        assert_eq!(keys, ["1", "2", "3", "4", "5", "0"]);
    }
}
