//! 终端前端 bin `matsuri`：装配一局骏大祭剧本并打印
//!
//! 本 crate（`matsuri-cli`）是终端前端业务：跑局入口（`matsuri` / `random` / `manual` 训练员）；
//! 在线决策外壳按需再加 bin。后续实现仓的入口按宿主需要自定（终端 / Web / Android 各自装配自己的
//! bin / lib）；本 bin 只是主线自测外壳，复用 `umaai-cli` 的呈现件（样例面，pub 面不承诺稳定）。
//!
//! 用法：`cargo run --release --bin matsuri -- [训练员]`
//! （缺省 `matsuri`；可切 `random`——引擎随机基线、`manual`——无颜色人工选择；
//! 数据根是代码级配置 [`DATA_ROOT`]，不暴露在命令行）

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
    // 训练员：matsuri（样例策略，默认）/ random（引擎随机基线）
    let trainer_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "matsuri".to_string());

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    Data::init(DATA_ROOT)?;
    let data = Arc::new(MatsuriData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let matsuri_cfg =
        MatsuriConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;

    let consts = &Data::get()?.consts;
    let mut out = std::io::stdout();
    let opts = PrintOptions {
        verbosity: Verbosity::Normal,
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
            let trainer = MatsuriTrainer::new(matsuri_cfg);
            runner.run_game(game, &trainer, None)?;
        }
        "random" => {
            runner.run_game(game, &RandomTrainer, None)?;
        }
        "manual" => {
            runner.run_game(game, &manual::ManualTrainer, None)?;
        }
        other => {
            return Err(anyhow!(
                "未知训练员: {other}（可选 matsuri / random / manual）"
            ));
        }
    }
    Ok(())
}
