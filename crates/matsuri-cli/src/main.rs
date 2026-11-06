//! 终端前端 bin `matsuri`：装配一局骏大祭剧本并打印
//!
//! 本 crate（`matsuri-cli`）是终端前端业务：跑局入口（`matsuri` / `random` / `manual` 训练员）；
//! 在线决策外壳按需再加 bin。后续实现仓的入口按宿主需要自定（终端 / Web / Android 各自装配自己的
//! bin / lib）；本 bin 只是主线自测外壳，复用 `umaai-cli` 的呈现件（样例面，pub 面不承诺稳定）。
//!
//! 用法：`cargo run --release --bin matsuri -- [训练员]`
//! （缺省 `matsuri`；可切 `random`——引擎随机基线、`manual`——机制核对面板（人工逐决策）；
//! 数据根是代码级配置 [`DATA_ROOT`]，不暴露在命令行）

mod explain;
mod logging;
mod manual;
mod online;
mod panel;

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriSearchTrainer, MatsuriTrainer, build_game};
use umaai::Config;
use umaai::runner::Runner;
use umaai_cli::frontend::{PrintFrontend, PrintOptions, Verbosity};
use umaai_cli::render::term;
use umaai_core::Data;
use umasim::trainer::RandomTrainer;

/// 统一数据根（代码级配置）：引擎静态数据拷贝 + 剧本资产 + 双配置默认层
///
/// 建仓时把引擎 `gamedata/` 的六张静态表与 `ai_config.toml` 拷入 `data/`，之后拷仓即跑。
/// 引擎覆盖层约定保留：工作目录放同名 `ai_config.toml` / `matsuri_config.toml` 存在才合并。
pub(crate) const DATA_ROOT: &str = "data";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 顶层分流：`--version` / `online` 子命令 / 离线跑局（原有用法不变）
fn dispatch(argv: &[String]) -> Result<()> {
    if argv.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("matsuri {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    match argv.first().map(String::as_str) {
        Some("online") => online::run(&argv[1..]),
        // 兼容原有用法：`matsuri [训练员]`（缺省 matsuri）
        _ => run(),
    }
}

/// 读取可选环境变量并解析：未设置返回 `None`，非法值报错（不静默回退）
fn env_parse<T>(name: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(raw) => Ok(Some(
            raw.trim()
                .parse()
                .map_err(|err| anyhow!("{name} 非法（{raw}）：{err}"))?,
        )),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow!("读取 {name} 失败：{err}")),
    }
}

/// 装配一局并跑到底（数据根为代码级配置；命令行只选训练员）
fn run() -> Result<()> {
    // 训练员：matsuri（样例策略，默认）/ random（引擎随机基线）/ manual（机制核对面板）
    let trainer_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "matsuri".to_string());

    // explain：手写档每回合打印与 manual 共用面板 + 候选打分表（默认关；不改任何数值）
    let explain = std::env::args().any(|arg| arg == "--explain")
        || std::env::var("MATSURI_EXPLAIN").is_ok_and(|value| value != "0");

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    // 诊断日志：级别来自 `[output] log_level`（默认 info；`off` 关闭，batch / 基线可静音）
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(DATA_ROOT)?;
    let data = Arc::new(MatsuriData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;

    let consts = &Data::get()?.consts;
    let mut out = std::io::stdout();
    // 手动档由面板自持逐回合 / 逐事件信息（Quiet 只出终局块），避免引擎行与面板交错
    let opts = PrintOptions {
        verbosity: if trainer_name == "manual" || explain {
            Verbosity::Quiet
        } else {
            Verbosity::Normal
        },
        width: term::resolve_width(None),
        color: false,
    };
    let mut fe = PrintFrontend::new(&mut out, opts, consts);
    let game = build_game(&cfg, data)?.with_config(matsuri_cfg.clone());
    // 编成警告（禁止项已在建局时报错）：提示宿主 / 玩家
    for warning in &game.deck_warnings {
        eprintln!("⚠ 编成警告：{warning}");
    }
    // 规则主种子由 Runner 注入（`cfg.sim.seed`）：引擎两轴与剧本策略轴都按它派生
    let mut runner = Runner::new(&cfg, &mut fe)?;
    match trainer_name.as_str() {
        "matsuri" => {
            // 搜索预算归剧本配置 `[search].search_n`（每候选 rollout 次数；默认 8192，0 = 手写档）；
            // 引擎 `[search]` 只提供内核参数（UCB / 激进度 / max_depth）。
            let mut search = cfg.search.clone();
            let search_n =
                env_parse::<usize>("MATSURI_SEARCH_N")?.unwrap_or(matsuri_cfg.search.search_n);
            matsuri_cfg.search.search_n = search_n;
            // 激进度：内核定档 50 偏高尾；可扫参，缺省沿用引擎配置层
            if let Some(radical) = env_parse::<f64>("MATSURI_SEARCH_RADICAL")? {
                search.radical_factor_max = radical;
            }
            if search_n > 0 {
                // 搜索档暂不支持 explain 面板（面板与候选表绑定手写打分同源）
                if explain {
                    return Err(anyhow!(
                        "搜索档暂不支持 --explain（search_n={search_n} > 0）"
                    ));
                }
                let trainer = MatsuriSearchTrainer::new(matsuri_cfg, search)?;
                runner.run_game(game, &trainer, None)?;
            } else {
                let trainer = MatsuriTrainer::new(matsuri_cfg);
                if explain {
                    let trainer = explain::ExplainTrainer::new(trainer);
                    runner.run_game(game, &trainer, None)?;
                } else {
                    runner.run_game(game, &trainer, None)?;
                }
            }
        }
        "random" => {
            runner.run_game(game, &RandomTrainer, None)?;
        }
        "manual" => {
            // 跑局前打印一次育成配置（育成马娘 / 卡组 / 继承配置）
            print!("{}", manual::newgame_config(&game));
            runner.run_game(game, &manual::ManualTrainer::new(), None)?;
        }
        other => {
            return Err(anyhow!(
                "未知训练员: {other}（可选 matsuri / random / manual）"
            ));
        }
    }
    Ok(())
}
