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

use clap::{Parser, Subcommand};
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

/// 顶层 CLI
#[derive(Debug, Parser)]
#[command(
    name = "matsuri",
    version,
    about = "骏大祭剧本训练员（离线跑局 / 在线帧决策）",
    propagate_version = true,
    subcommand_precedence_over_arg = true
)]
struct Cli {
    /// 训练员：matsuri（默认） / random / manual
    #[arg(value_name = "TRAINER", value_parser = ["matsuri", "random", "manual"])]
    trainer: Option<String>,
    /// 显式卡组档（[decks.*] 名字；离线扫参用）；缺省按实际卡组 comp 匹配
    #[arg(long, value_name = "NAME")]
    deck_profile: Option<String>,
    /// 手写档每回合打印面板 + 候选打分表（搜索档不支持）
    #[arg(long)]
    explain: bool,
    /// 子命令：online 真机帧 / 回放逐帧决策
    #[command(subcommand)]
    command: Option<Command>,
}

/// 子命令
#[derive(Debug, Subcommand)]
enum Command {
    /// 真机帧 / 回放逐帧决策
    #[command(
        after_help = "配置与环境：默认层 data/ai_config.toml + data/matsuri_config.toml，\
                            工作目录同名覆盖层；MATSURI_SEARCH_N / MATSURI_SEARCH_RADICAL 可临时覆盖搜索预算与激进度。"
    )]
    Online(online::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Online(args)) => online::run(args),
        None => run(cli.trainer.as_deref(), cli.deck_profile, cli.explain),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 读取可选环境变量并解析：未设置返回 `None`，非法值报错（不静默回退）
pub(crate) fn env_parse<T>(name: &str) -> Result<Option<T>>
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

/// `[search].threads` → rayon 全局池；须在首次搜索前调用（进程内只能初始化一次）
///
/// `0` = 自动（逻辑核数）；显式设了 `RAYON_NUM_THREADS` 时以环境变量为准。
/// 返回是否实际设置了全局池。
pub(crate) fn apply_search_threads(threads: usize) -> bool {
    if threads == 0 || std::env::var_os("RAYON_NUM_THREADS").is_some() {
        return false;
    }
    if let Err(err) = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
    {
        eprintln!("警告：搜索线程池初始化失败（{err}），沿用默认线程数");
        return false;
    }
    true
}

/// 装配一局并跑到底（数据根为代码级配置；命令行只选训练员）
fn run(trainer: Option<&str>, deck_profile: Option<String>, explain_flag: bool) -> Result<()> {
    let trainer_name = trainer.unwrap_or("matsuri");
    // explain：手写档每回合打印与 manual 共用面板 + 候选打分表（默认关；不改任何数值）
    let explain = explain_flag || std::env::var("MATSURI_EXPLAIN").is_ok_and(|value| value != "0");

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    // 诊断日志：级别来自 `[output] log_level`（默认 info；`off` 关闭，batch / 基线可静音）
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(DATA_ROOT)?;
    let data = Arc::new(MatsuriData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let mut matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;
    apply_search_threads(matsuri_cfg.search.threads);
    let comp = matsuri_core::deck_comp(&cfg.sim.cards).context("卡组构成解析失败")?;
    matsuri_cfg = matsuri_cfg.for_deck(comp);
    if let Some(name) = &deck_profile {
        matsuri_cfg = matsuri_cfg.for_profile(name)?;
    }

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
    match trainer_name {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `threads = 0` = 自动：不初始化 rayon 全局池
    #[test]
    fn auto_threads_keeps_rayon_default() {
        assert!(!apply_search_threads(0));
    }
}
