# 交接提示词：M2 MCTS 搜索

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-04）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next` 未改。
- **git**：本轮提交 =「注释 / 冗余整理 + M2 前置修复」；提交前工作树干净。
- **验收**：workspace **209 测例**（148 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；
  `cargo fmt --all --check` / `check` / `clippy -D warnings` / `test` 四门绿；`cargo doc` 0 warning。
- **基线**：100101 种子 61444 冒烟 **81553 / LG8、PT 11490**（本轮修复后逐位不变）；
  100 局 61444–61543 均 **78124.5** / PT 11052.2（修复前录制，S9 重录）；102102 均 76406.6。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`）；
  `data/ai_config.toml` `[search] search_n = 0`（默认关，env / 该键开启）。
- **本轮已修**：根动作随机流（H1）、`search_n` 被覆盖（H2）、`flags_value` 符号与双计、
  友人出行半态、融合非原子、`AdviceAssignment` 越界、面板窗口 / 友人事件数据源。
- **明确不修**：手动档教诲下标 0（手动档仅测试用）；合宿 `vital≤32` 的选训策略留下一步；低优先级项。

## 2. 下一步：M2 MCTS（唯一主线）

目标：把手写训练员的动作决策接入引擎 `FlatSearch`；搜索档可开关，手写档保持默认与基线。

现状（可用的接线）：

- `MatsuriGame: FlatSearchGame`（`game/traits.rs`）：`RolloutTrainer = MatsuriTrainer`、
  `fork_for_rollout`、`apply_root_action`（已改走自持策略流）、`search_score` = 官方评分。
- `MatsuriTrainer::with_search` / `search_advice`（`trainer.rs`）：**只在教诲分配 120 候选**用 MC；
  rollout 基策 = 同一配置的手写训练员且关搜索；搜索 RNG 按 `(master, turn)` 派生；`DiagGuard` 静默。
- CLI：`MATSURI_SEARCH_N` / `_TOPK` / `_RADICAL`；默认 0 关。目前没有搜索测试。

动工前先做（本轮 review 余项）：

1. **补搜索基线测试**：固定种子 + `search_n>0`，断言同种子两次逐位一致、记录耗时 / 终局分；
   再加「`search_n=0` 与手写逐位一致」守门。
2. **拍板排序目标函数**：`search_score` 现在是官方评分（忽略 `pt_favor`），而手写剪枝用
   `pt_favor_rate` 加权分；扩到普通动作前必须统一口径（官方评分或加权分）。
3. 搜索范围先小步：普通训练 5 选 1 → 再扩到全部动作；`AssignAdvice` 不进普通搜索。
4. 决策点约 70 个，成本 ≈ 决策点 × 候选 × `search_n` 终局 rollout；先用 N=64/128 测时。
5. rollout 内的教诲 / 事件决策点不得再开搜索（防递归；现有 `MatsuriTrainer::new` 已保证）。
6. 同一局面重复搜索逐位一致；分支只经 `fork_for_rollout`；`diag!` 用 `DiagGuard` 静默。

验收：`search_n=0` 零差 → 固定种子可复现 → 100 局配对 vs 手写基线（换新种子块 63700 起）→ 四门。

## 3. 关键落点 / 命令 / 纪律

- 搜索：`crates/matsuri-core/src/game/traits.rs`、`trainer.rs`、`crates/matsuri-cli/src/main.rs`；
  引擎契约 `../umaai_next/crates/umasim/src/search/searchable.rs`。
- 机制：`game/{mod,train,event,settle,stage,action}.rs`；估值 `value.rs`；配置 `config.rs` + `data/matsuri_config.toml`。
- 命令（仓根；`--offline`；同一 target 一次只跑一个 cargo）：
  `cargo fmt --all --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` /
  `cargo test --workspace --release --offline` / `cargo run --release --offline --bin matsuri -- matsuri [--explain]`。
- 不碰引擎；覆盖层只放临时目录 / `logs/`；调参一律换全新种子块。
- 种子块已用：61444–61543（基线）、61544–61643、62200–62299、62800–63099、63100–63699；下一轮 **63700 起**。
- 已否杠杆（不要重复）：白给属性 / `pt_favor_rate=16`、根性稽古软旋钮、单演目 focus、`weights.hint`、
  `advice.hint`、普通训练等级进度价值、`kensan.scale=2`、动机 clamp、助力支撑专项、PT 饱和阈值。

## 4. 文档

- 机制 / 口径：`docs/mechanism.md`、`docs/glossary.md`（失败率乘算 / deyilv 现算 / 无引擎特殊事件结构 / 非大成功暂不处理）。
- 计划 / 测试 / 交接：`docs/plan/scenario_dev_plan.md`、`docs/tests_overview.md`、`docs/handoff.md`。
- README 只保留现状与基本信息，不堆更新日志。
