# 交接提示词：搜索档（FlatSearch）调优

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-04）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next` 未改。
- **验收**：workspace **218 测例**（153 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 +
  3 CLI 冒烟 + 3 搜索集成，另 1 个 1024 手动 `#[ignore]`）；`cargo test --workspace --release --offline` 绿。
- **手写基线**：100101 种子 61444 冒烟 **81553 / LG8、PT 11490**；
  100 局 61444–61543 均 **78124.5** / PT 11052.2（S9 重录）。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`）；
  `[search].search_n = 0`（默认手写；搜索档用 env / 覆盖层显式开启）。
- **搜索档已落**：`MatsuriSearchTrainer` 包引擎 `FlatSearch`（扁平 MC + UCB + CRN），
  三类作用面 `train` / `advice` / `event` 可开关；`search_n` 是**每候选** rollout 次数。
  教诲 120 候选剪枝 top-16；事件用 dummy token + `search_with`，应用当前选项后从
  `event_queue` 下一个事件继续；排序 `search_score(pt_favor_rate)`，失败分支按实际分；
  `LeafEvaluator` 接口已预留，生产 `max_depth=0`。
- **单局验证**（seed 61444，默认档）：1024 档 **83326 / LG11、PT 11602**（90.8 s）；
  2048 档 **83519 / LG11、PT 11901**（180 s）。单局冒烟，不是基线；同种子两次逐位一致。
- **理想测试**（seed 61444，106301 狄杜斯 + 小切 + 心情盾×2）：
  - 手写：**85384 / LG14、PT 11808**（0.03 s）。
  - 全作用面 4096、`radical=50`：**83366 / LG11、PT 11173**（373.85 s）。
  - 全作用面 4096、`radical=1.0` / `stdev=2500`：**83422 / LG11、PT 11339**（345.02 s）。
  - Train-only 1024：`radical=1.0` **84209**；+advice **83176**；+event **83048**。
  - **Train-only 1024、`radical=0.0`（mean）：86711 / LG17、PT 12446**；
    Train-only 4096、`radical=0.0`：**86955 / LG17、PT 12625**（95.27 s）。
  - 结论：`rank_ratio^radical` 在 `radical=1.0` 仍偏高尾，`0.0` 明显更好；
    该局 advice/event 搜索有副作用；全部单 seed，待多 seed 复验。
- **普通马娘对照**（seed 61444，100101 + 小切 + 心情盾×2，Train-only、`radical=0.0`）：
  手写 **82807 / LG10、PT 11622**（0.03 s）；1024 **85095 / LG14、PT 11993**（37.78 s）；
  4096 **85184 / LG14、PT 12333**（105.38 s）。**4096 比手写 +2377 分 / LG +4**，
  明显高于狄杜斯的 +1571（86955 vs 85384）——狄杜斯铁心后五维接近上限，搜索能拿的
  面板分边际更小，符合预期。
- **随机流**：搜索与 ramen 一致，`FlatSearch` 每次恰好从调用方决策流取一个
  `next_u64` 派生 rollout 种子；局面规则流由 `fork_for_rollout` 按 rollout 种子独立派生，
  搜索不读 / 不改真实局面的规则流。`search_consumes_one_decision_draw` 守门。

## 2. 下一步：搜索档调优（唯一主线）

1. **单局调优**（不进 CI，先 1024/2048）：
   - `event` 默认开：核对事件队列续跑与多事件 stage 的行为；
   - `advice_topk=16`：扫 8 / 16 / 24；
   - `radical_factor_max`：引擎默认 50 偏高尾；理想测试 `0.0` 明显优于 `1.0`，
     扫 0.0 / 0.5 / 1.0；
   - `use_ucb=false` 与 true 的预算 / 方差对比（`search_n` 是每候选上限）。
2. **机制完全实现后**再跑 **8192**（每候选）：先 1 局测时，再小批量
   （建议新种子块 **63800 起**；63700–63799 已被 Workbench 主仓基线占用）。
3. 为正再扩批并决定默认档；默认档仍保持 `search_n=0` 手写，S9 只录一次基线。

验收：`search_n=0` 零差（已有测试）→ 固定种子可复现（已有测试）→ 单局 1024/2048 记录 →
8192 小批量 → 四门。

### 2.1 搜索性能剖面与优化（引擎 `docs/scenario/perf_plan.md`）

用户指定的下一步：按引擎 perf 计划做 **O0–O4**，不改决策语义、先测量后优化。
同时先用多 seed 复验理想测试的「`radical=0.0` train-only 更好」：
优先拆 train-only / +advice / +event，`radical_factor_max = 0.0 / 0.5 / 1.0`，
`expected_search_stdev` 只影响 UCB 探索、不改排序，先固定 2500。

- **O0 基线锁定**：固定构建（workspace Release，不改 opt / LTO / codegen）、工具链、数据、
  `matsuri_config [search]`、种子、线程数；记录 `search_n=1024` 单局耗时（当前约 90 s）
  与 `SearchProbe` 的候选数 / planned / succeeded / elapsed。
- **O1 剖面定位**：优先 microbench（rollout 单回合 / 训练估值 / 事件结算 / clone 成本），
  再用整局 A/B；pprof 只作高频线索，**不用 flamegraph**。重点看
  `MatsuriSearchTrainer::search_train/advice/event`、`MatsuriGame::clone`、
  `value.rs::Model::new`、`preview_training`、`event_queue` 克隆、`FlatSearch` 调度。
- **O2 单点改造**：一次只改一个点；保持 RNG 调用次数 / 顺序、遍历顺序、浮点运算顺序。
  候选方向：`prune` 的 `select_action` 重复打分、事件搜索的队列 / 局面 clone、rollout
  诊断文本关闭（已有 `DiagGuard`）、训练预览缓存、`search_group_size` / 线程数。
- **O3 回归验收**：同种子 A/B，排除 `elapsed*` 后结果逐位一致；固定根 `SearchProbe`
  候选数 / 统计位模式 / 最优动作一致；四门 + 1024 复现测试。
- **O4 固化**：保留微基准 / 探针入口与命令；测量 CSV / 补丁放 `logs/`，不进仓；
  数值位移按 glossary 记口径并重录基线。

## 3. 关键落点 / 命令 / 纪律

- 搜索：`crates/matsuri-core/src/search.rs`（`MatsuriSearchTrainer` / `LeafEvaluator`）、
  `game/traits.rs`（`search_score` 的 `pt_favor_rate` 口径）、`game/stage.rs`
  （`run_event_queue` 事件队列）、`config.rs` + `data/matsuri_config.toml` `[search]`、
  `crates/matsuri-cli/src/main.rs`（`MATSURI_SEARCH_N`）。
- 引擎契约：`../umaai_next/crates/umasim/src/search/{flat_search,searchable}.rs`；
  不碰引擎。
- 机制：`game/{mod,train,event,settle,stage,action}.rs`；估值 `value.rs`。
- 命令（仓根；`--offline`；同一 target 一次只跑一个 cargo）：
  - 单局 1024：`MATSURI_SEARCH_N=1024 ./target/release/matsuri matsuri`
  - 单局 2048：`MATSURI_SEARCH_N=2048 ./target/release/matsuri matsuri`
  - 1024 复现（约 3–4 min）：`cargo test --release --offline -p matsuri-core --test search -- --ignored`
  - 四门：`cargo fmt --all --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` /
    `cargo test --workspace --release --offline` / `cargo doc --workspace --no-deps --offline`
- 不碰引擎；覆盖层只放临时目录 / `logs/`；调参一律换全新种子块。
- 种子块已用：61444–61543（基线）、61544–61643、62200–62299、62800–63099、63100–63699、
  63700–63799（Workbench 主仓基线）；下一轮 **63800 起**。
- 已否杠杆（不要重复）：白给属性 / `pt_favor_rate=16`、根性稽古软旋钮、单演目 focus、`weights.hint`、
  `advice.hint`、普通训练等级进度价值、`kensan.scale=2`、动机 clamp、助力支撑专项、PT 饱和阈值。
- 搜索档注意：`--explain` 与 `search_n>0` 互斥；`max_depth>0` 会在构造时拒绝（叶估仅接口）。

## 4. 文档

- 机制 / 口径：`docs/mechanism.md` §6.6、`docs/glossary.md`（搜索档口径行）。
- 计划 / 测试 / 交接：`docs/plan/scenario_dev_plan.md`、`docs/tests_overview.md`、`docs/handoff.md`。
- README 只保留现状与基本信息，不堆更新日志。
