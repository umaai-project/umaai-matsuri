# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓。

## 形态

workspace 五 crate：

| crate | 职责 |
|---|---|
| `matsuri-core` | 剧本机制、手写训练员、FlatSearch 搜索训练员、局面装配（宿主稳定面） |
| `matsuri-sim` | 离线模拟与基线裁判 |
| `matsuri-ai` | 在线帧解析、一帧一决策会话、路由与输出 DTO |
| `matsuri-cli` | 终端入口：离线跑局、在线帧 / 回放、手动核对、面板与日志 |
| `matsuri-tools` | 外围测量：`bench` / `sweep` / `cardtest` 与诊断工具 |

- 引擎以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；不 fork 引擎、不在引擎仓加剧本 crate。
- 数据根为 `data/`：引擎静态表 + `scenario_matsuri.json` + 默认层 `matsuri_config.toml` / `ai_config.toml`。
- 工作目录同名配置可作覆盖层；`[decks.*]` 只由引擎 / 工具使用，玩家侧不覆盖。

## 当前状态

- **机制闭环**：阶段机 / 人头分布 / 训练两层公式 / 演目稽古 / 教诲与転成 / 発表会·本番·合宿·URA / 事件链 / 手写训练员 / 诊断日志均已接入。
- **卡组配置**：`data/matsuri_config.toml` 分 `[decks.*]`；`[decks.default]` 为完整默认档（冠军档 `ga-r2`），
  其他档深合并；未匹配 comp 走 default；`variant = true` 仅显式选择。
- **搜索**：`[search].search_n` 为每候选 rollout 数（默认 **8192**，`0` = 手写档）；Train / 教诲 / 事件分开关；
  教诲候选按 801 硬约束去重后再剪枝 / 搜索。
- **在线决策**：`matsuri online` 支持真机帧监听与回放；输出局面面板、运气分、融合固定决策、Top 5 候选、
  教诲完整映射与 `AdvicePreset` 差异；在线教诲路由按窗口去重，搜索侧跳过 End 阶段的重复教诲处理。
- **CLI**：`matsuri` / `matsuri online` 已迁移 clap，提供 `--help` / `--version`；`matsuri-tools` 仍用手写解析。
- **发布**：Windows 启动器 `matsuri-ai` 方案已定（在线优先、`bin/` 放核心二进制、`data/config.toml`），
  见 [`docs/plan/release_launcher.md`](docs/plan/release_launcher.md)；尚未实现。
- **测卡 / 扫参**：`cardtest` 支持 `--matsuri-config` 显式完整配置入口；四套 build 的定卡与扫参方案见
  [`docs/plan/deck_param_scan.md`](docs/plan/deck_param_scan.md)。

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
```

当前：workspace **264 通过 + 2 手动 `#[ignore]`**；基线 / 性能复现见
[`docs/tests_overview.md`](docs/tests_overview.md) 与 [`docs/perf_profiling.md`](docs/perf_profiling.md)。
clippy 只在 Windows 侧跑。

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
