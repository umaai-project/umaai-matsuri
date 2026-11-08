# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓。

## 形态

workspace 六 crate：

| crate | 职责 |
|---|---|
| `matsuri-core` | 剧本机制、手写训练员、FlatSearch 搜索训练员、局面装配（宿主稳定面） |
| `matsuri-sim` | 离线模拟与基线裁判 |
| `matsuri-ai` | 在线帧解析、一帧一决策会话、路由与输出 DTO |
| `matsuri-cli` | 终端入口：离线跑局、在线帧 / 回放、手动核对、面板与日志 |
| `matsuri-tools` | 外围测量：`bench` / `sweep` / `cardtest` 与诊断工具 |
| `matsuri-launcher` | Windows 发布启动器 `matsuri-ai`（菜单 / 子进程装配 / 通道体检；交互选择用 `inquire`，不依赖业务 crate） |

- 引擎以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；不 fork 引擎、不在引擎仓加剧本 crate。
- 数据根为 `data/`：引擎静态表 + `scenario_matsuri.json` + 默认层 `matsuri_config.toml` / `ai_config.toml`。
- 工作目录同名配置可作覆盖层；`[decks.*]` 只由引擎 / 工具使用，玩家侧不覆盖。

## 当前状态

- **机制闭环**：阶段机 / 人头分布 / 训练两层公式 / 演目稽古 / 教诲与転成 / 発表会·本番·合宿·URA / 事件链 / 手写训练员 / 诊断日志均已接入。
  **变胖（`Ill::FAT`）**：训练 / 排练速度分量归零；每回合 15%（暂定）自解 + 治病 / 合宿休息 /
  「追加訓練」选项 1 定点治；线上病种由帧 `charaEffects`（全量状态 ID，按文本库 142 表挑 1–6）精确还原。
- **卡组配置**：`data/matsuri_config.toml` 分 `[decks.*]`；`[decks.default]` 为完整默认档（冠军档 `ga-r2`），
  其他档深合并；未匹配 comp 走 default；`variant = true` 仅显式选择。
  当前只保留 `wisdom` 一套非默认档（教诲落位四窗口 + `status_weights` 均已定档）；
  `stamina / guts` 于 2026-11-07 退役，备份与加回步骤见 `logs/deckselect/retired_2026-11-07/`。
- **搜索**：`[search].search_n` 为每候选 rollout 数（默认 **8192**，`0` = 手写档）；Train / 教诲 / 事件分开关；
  教诲候选按 801 硬约束去重后再剪枝 / 搜索。
- **在线决策**：`matsuri online` 支持真机帧监听与回放；输出局面面板、运气分、融合固定决策、Top 5 候选、
  教诲完整映射与 `AdvicePreset` 差异；**同回合每张新帧都重新决策**（只有内容完全相同的重复帧按摘要跳过），
  面板在搜索前打「AI计算中…」；在线档用精简面板（不显示演目分配表 / 配色图例 / 追加稽古说明，
  手动核对与 `explain` 仍是完整档）；搜索侧跳过 End 阶段的重复教诲处理。
  **运气行**带「期望 E （±S，档）」：`E` 按回合计入 `luck_turn_bonus` 显示分投影（`E₀ + (77−t)×b`），
  档次与 JSON 仍是官方口径，`S` = 单局终局 σ；**跳过帧**只显示黄色跳过行，不再重复打印帧提示；
  **事件帧**按插件透传的 `storyId` 查本地事件表（`scenario_events` 优先 + 引擎五表）还原，
  本仓数据覆盖帧数据、帧只提供选项文本；决策前回显事件与逐选项效果，查不到则跳过（不给训练建议）。
- **CLI**：`matsuri` / `matsuri online` 已迁移 clap，提供 `--help` / `--version`；`matsuri-tools` 仍用手写解析。
- **发布**：Windows 启动器已实现（`crates/matsuri-launcher`，bin `matsuri-ai`）：
  菜单为在线模式（默认）/ 启动小黑板 / 通道体检 / 退出；小黑板取同目录 `UmamusumeResponseAnalyzer.exe`，
  用 `CREATE_NEW_CONSOLE` 另开窗口拉起（URA 是 Terminal.Gui TUI，与在线模式需同时常驻）。
  头部为标题 + 分隔线 + 四行只读信息（安装目录 / 游戏根 / 小黑板 / 核心程序，标签按显示宽度对齐、
  状态点按文件在位染色）；不写配置、不落日志。
  发布包为 `matsuri-ai.exe` + `bin/matsuri.exe` + `data/`，打包与附加资源见
  `scripts/package_launcher.py`、`dist/extra/`；完整版计划与差异见
  [`docs/plan/release_launcher.md`](docs/plan/release_launcher.md) §0。
- **测卡 / 扫参**：`cardtest` 支持 `--matsuri-config` 显式完整配置入口；单卡定卡、教诲落位、
  五维系数、机制旋钮四轮扫描均已完成（结果见 `logs/deckselect` / `logs/advscan` /
  `logs/statusscan` / `logs/knobscan`）。仅 wisdom 的五维系数定档；stamina / guts 因构成性失分
  且参数调不动已退役，等构成搜索后再加入；`fusion` 全枚举与 T47/T59 教诲窗口补扫待做。
  方案与结果见 [`docs/plan/deck_param_scan.md`](docs/plan/deck_param_scan.md) §9。

## 快速开始

```powershell
# 离线跑局（默认搜索档；手写档加 $env:MATSURI_SEARCH_N=0）
cargo run --release --bin matsuri --
cargo run --release --bin matsuri -- random
cargo run --release --bin matsuri -- manual
cargo run --release --bin matsuri -- matsuri --explain   # 仅手写档

# 在线：真机帧监听 / 回放
cargo run --release --bin matsuri -- online --root <游戏根目录>
cargo run --release --bin matsuri -- online --replay <帧目录> --trainer search --json

# 参数帮助
cargo run --release --bin matsuri -- --help
cargo run --release --bin matsuri -- online --help

# 测卡 / 扫参（手写档；实验输出落 logs/）
./target/release/cardtest pool --top 12
./target/release/cardtest build --comp all --pick measured --with-default --out decks/measured.csv
./target/release/cardtest run --decks decks/measured.csv --ref default_21101 --games 3000 --seed 82000
./target/release/cardtest run --decks logs/scan/candidates.csv \
  --matsuri-config logs/scan/scan.toml --ref ref_stamina --games 5000 --seed 87000
```

主要环境变量：`MATSURI_SEARCH_N`、`MATSURI_SEARCH_RADICAL`、`MATSURI_EXPLAIN`、`MATSURI_ROOT`。
默认测试马娘 105902；局外状态默认不注入，理想测试档用 `bench --qie xiaoqie --shield 2` 覆盖。

## 验收

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo test --workspace --release --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
```

当前：workspace **278 通过 + 2 手动 `#[ignore]`**；clippy `-D warnings` 全绿（2026-11-08 清零）；基线 / 性能复现见
[`docs/tests_overview.md`](docs/tests_overview.md) 与 [`docs/perf_profiling.md`](docs/perf_profiling.md)。
clippy 只在 Windows 侧跑；CI 也是 `-D warnings` 硬门禁。

基线快照（2026-11-08，`bench baseline` 两档四项全绿）：手写 `a-normal` 80701.2 / `b-ideal` 86004.3，
搜索 N=8192 `a-normal` 86392.4 / `b-ideal` 90455.4 —— **MCTS 比手写高 +4451 ~ +5691 分**（详见
[`docs/tests_overview.md`](docs/tests_overview.md) §4.2）。

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/README.md`](docs/README.md) | 文档索引 |
| [`docs/plan/scenario_dev_plan.md`](docs/plan/scenario_dev_plan.md) | 阶段、决策记录、待决事项 |
| [`docs/plan/deck_config.md`](docs/plan/deck_config.md) | `[decks.*]` 配置 schema / 分层 / 合并与迁移 |
| [`docs/plan/deck_param_scan.md`](docs/plan/deck_param_scan.md) | 四套 build 定卡 / 扫参顺序 / `--matsuri-config` 入口 |
| [`docs/plan/online_wiring.md`](docs/plan/online_wiring.md) | 在线接线路由 / 还原口径 |
| [`docs/plan/release_launcher.md`](docs/plan/release_launcher.md) | Windows 发布启动器 `matsuri-ai` 计划 |
| [`docs/mechanism.md`](docs/mechanism.md) | 机制公式 / 流程 / 实现落点 |
| [`docs/glossary.md`](docs/glossary.md) | 术语与口径决定 |
| [`docs/tests_overview.md`](docs/tests_overview.md) | 测试一览 |
| [`docs/perf_profiling.md`](docs/perf_profiling.md) | 性能测量与复现 |

## 依赖

```toml
# 开发期：并列目录
umaai-core = { path = "../umaai_next/crates/umaai-core" }
umasim     = { path = "../umaai_next/crates/umasim" }
umaai      = { path = "../umaai_next/crates/umaai" }
umaai-cli  = { path = "../umaai_next/crates/umaai-cli" }

# 稳定期：同一 repo / tag
# umaai = { git = "<engine repo>", tag = "engine-v0.1.0" }
```
