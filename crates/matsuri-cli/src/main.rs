//! 终端前端 bin `matsuri`：装配一局骏大祭剧本并打印
//!
//! 本 crate（`matsuri-cli`）是终端前端业务：跑局入口（`matsuri` / `random` / `manual` 训练员）；
//! 在线决策外壳按需再加 bin。后续实现仓的入口按宿主需要自定（终端 / Web / Android 各自装配自己的
//! bin / lib）；本 bin 只是主线自测外壳，复用 `umaai-cli` 的呈现件（样例面，pub 面不承诺稳定）。
//!
//! 用法：`cargo run --release --bin matsuri -- [训练员]`
//! （缺省 `matsuri`；可切 `random`——引擎随机基线、`manual`——机制核对面板（人工逐决策）；
//! 数据根是代码级配置 [`DATA_ROOT`]，不暴露在命令行）

mod logging;
mod manual;

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use matsuri_core::{MatsuriConfig, MatsuriData, MatsuriTrainer, build_game};
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
const DATA_ROOT: &str = "data";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 装配一局并跑到底（数据根为代码级配置；命令行只选训练员）
fn run() -> Result<()> {
    // 训练员：matsuri（样例策略，默认）/ random（引擎随机基线）/ manual（机制核对面板）
    let trainer_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "matsuri".to_string());

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    // 诊断日志：级别来自 `[output] log_level`（默认 info；`off` 关闭，batch / 基线可静音）
    logging::init_logger(&cfg.output.log_level).context("初始化日志器失败")?;
    Data::init(DATA_ROOT)?;
    let data = Arc::new(MatsuriData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;

    let consts = &Data::get()?.consts;
    let mut out = std::io::stdout();
    // 手动档由面板自持逐回合 / 逐事件信息（Quiet 只出终局块），避免引擎行与面板交错
    let opts = PrintOptions {
        verbosity: if trainer_name == "manual" {
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
            // 蒙特卡洛（引擎 FlatSearch）由环境变量显式开启，默认关：
            // 搜索成本 = 决策点数 × 剪枝宽度 × search_n 局终局 rollout，
            // 开着跑 100 局矩阵不现实，只在验证 / 蒸馏时打开。
            let search_n: usize = std::env::var("MATSURI_SEARCH_N")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            let topk: usize = std::env::var("MATSURI_SEARCH_TOPK")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(4);
            let mut search = cfg.search.clone();
            search.search_n = search_n;
            // 激进度：内核定档 50 会把排序压到 rollout 分数的高尾，教诲只有 6 个决策点、
            // 每点几十万次 rollout 的噪声下等于放大方差；可扫参，缺省沿用配置层
            if let Ok(v) = std::env::var("MATSURI_SEARCH_RADICAL")
                && let Ok(radical) = v.trim().parse::<f64>()
            {
                search.radical_factor_max = radical;
            }
            let trainer = MatsuriTrainer::new(matsuri_cfg).with_search(search, topk);
            runner.run_game(game, &trainer, None)?;
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
