//! 测试支撑（不承诺稳定 API）
//!
//! workspace 化后测试进程的 cwd 在各 crate 目录，数据根按「crate 清单目录上溯两级＝仓根」
//! 定位。注意引擎 `umaai-core::testing` 的 `env!` 固化的是**引擎仓**路径，跨仓不可用；
//! 本模块在仓内成员间通用（成员都在 `crates/` 下一层，任一成员上溯两级都得到同一仓根）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use umaai_core::Data;
use umasim::game::Game;

use crate::{MatsuriData, MatsuriGame};

/// 仓根目录（任一成员 crate 的 `CARGO_MANIFEST_DIR` 上溯两级）
pub fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("crate 目录上溯两级即仓根（成员都在 crates/ 下一层）")
}

/// 统一数据目录（`<仓根>/data`：引擎静态数据 + 剧本资产 + 双配置默认层）
pub fn data_dir() -> PathBuf {
    repo_root().join("data")
}

/// 用仓内数据 / 配置建一局（**内联机制测试**用：固定主种子 42）
///
/// 引擎全局数据 `Data::init` 幂等（同数据根直接复用），因此可在多个测试里重复调用。
pub fn sample_game() -> MatsuriGame {
    let root = data_dir();
    Data::init(&root).expect("引擎静态数据初始化失败");
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").expect("载入引擎配置失败");
    sample_game_with_uma(cfg.sim.uma)
}

/// 用指定马娘建一局（自选比赛义务区间等按马娘差异的机制测试用；固定主种子 42）
pub fn sample_game_with_uma(uma_id: u32) -> MatsuriGame {
    let root = data_dir();
    Data::init(&root).expect("引擎静态数据初始化失败");
    let cfg = umaai::Config::load_with_overlay_file(&root, ".").expect("载入引擎配置失败");
    let data = Arc::new(MatsuriData::load(&root).expect("载入剧本数据失败"));
    let mut game = MatsuriGame::newgame(
        uma_id,
        &cfg.sim.cards,
        cfg.inherit(),
        cfg.race_grades().expect("赛程等级表加载失败"),
        data,
    )
    .expect("建局失败");
    game.set_rule_master(42);
    game
}

/// 内联测试用：仓内资产（一次载入）
pub fn sample_data() -> Arc<MatsuriData> {
    let root = data_dir();
    Data::init(&root).expect("引擎静态数据初始化失败");
    Arc::new(MatsuriData::load(&root).expect("载入剧本数据失败"))
}
