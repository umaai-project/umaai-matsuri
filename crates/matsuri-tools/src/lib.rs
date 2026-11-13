//! # 剧本外围工具（`matsuri-tools`）
//!
//! 一个子面一个 bin：`bench` / `sweep` / `cardtest` / `review`。
//!
//! - **测卡**：引擎 `umaai-tools` 的 `DeckSpec` / `deck` / `arms` 内核 +
//!   `matsuri-sim` 的测量函数；
//! - **复盘**：读局包 → 还原 → 事实层（本模块 `review`），估值按需重算。
//!
//! 形态：单 bin 多子命令；依赖与 bin 在实现时登记。
pub mod review;
