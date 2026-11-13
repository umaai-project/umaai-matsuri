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
| `matsuri-launcher` | Windows 发布启动器 `matsuri-ai`（菜单 / 子进程装配 / 通道体检 / 启动自检更新；交互选择用 `inquire`，不依赖业务 crate） |

- 引擎以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；不 fork 引擎、不在引擎仓加剧本 crate。
- 数据根为 `data/`：引擎静态表 + `scenario_matsuri.json` + 默认层 `matsuri_config.toml` / `ai_config.toml`。
- 工作目录同名配置可作覆盖层；`[decks.*]` 只由引擎 / 工具使用，玩家侧不覆盖。
- **基准 / 测卡数据归 `benchmark/`**：基线、卡组夹具与结论 md 入库，试验产物放 `benchmark/_local/`（不入库）；
  `logs/` 只放运行期日志，禁止放测试 / 测卡 / 扫参 / 性能结果数据。分工见
  [`benchmark/README.md`](benchmark/README.md)，规则见 `AGENTS.md` §输出目录约定。

## 当前状态

- **机制闭环**：阶段机 / 人头分布 / 训练两层公式 / 演目稽古 / 教诲与転成 / 発表会·本番·合宿·URA / 事件链 / 手写训练员 / 诊断日志均已接入。
  **变胖（`Ill::FAT`）**：训练 / 排练速度分量归零；每回合 15%（暂定）自解 + 治病 / 合宿休息 /
  「追加訓練」选项 1 定点治；线上病种由帧 `charaEffects`（全量状态 ID，按文本库 142 表挑 1–6）精确还原。
- **卡组配置**：`data/matsuri_config.toml` 分 `[decks.*]`；`[decks.default]` 为完整默认档（GA 冠军 `g0269`，21101），
  其他档深合并；未匹配 comp 走 default；`variant = true` 仅显式选择。
  非默认档 `stamina_build`（12101）/ `wisdom_build`（11102）为 GA 独立调参的完整档，按构成自动触发（在线按帧内实际卡组）；
  guts 20111 未建档，回落 default。
- **搜索**：`[search].search_n` 为每候选 rollout 数（默认 **8192**，`0` = 手写档）；
  `[search].threads` 固定 rayon 全局池线程数（默认 **0** = 自动，显式 `RAYON_NUM_THREADS` 优先），
  在在线 / 离线入口首次搜索前应用；Train / 教诲 / 事件分开关；
  教诲候选按 801 硬约束去重后再剪枝 / 搜索。PT 项权重 `pt_score_rate = 1.88`（2026-11-13 标定；
  `data/constants.json`，改动会位移搜索 / 估值与基线）。
- **在线决策**：`matsuri online` 支持真机帧监听与回放；输出局面面板、运气分、融合固定决策、Top 5 候选、
  教诲完整映射与 `AdvicePreset` 差异；**同回合每张新帧都重新决策**（只有内容完全相同的重复帧按摘要跳过），
  面板在搜索前打「AI计算中…」；在线档用精简面板（不显示演目分配表 / 配色图例 / 追加稽古说明，
  手动核对与 `explain` 仍有这些区块）；**人类面板统一不显示【变化】/【演目Lv】/【演目槽】/
  【目标赛】/【自选赛】**（2026-11-13 精简）；还原名单按名额合并：帧只发在场者，未登场神驹按
  批次（`avail_turn`）保留、名单总数守恒；搜索侧跳过 End 阶段的重复教诲处理，
  教诲搜索根阶段按窗口表定档（0/1/3/5→Begin、2/4→End），不读帧 `isAfterTraining`（插件侧粘连）。
  **运气行**带「期望 E （±S，档）」：`E` 按回合计入 `luck_turn_bonus` 显示分投影（`E₀ + (77−t)×b`），
  档次与 JSON 同步按投影后的 E 重算 / 输出，`S`（单局终局 σ）不变；
  **跳过帧**只显示黄色跳过行，不再重复打印帧提示；
  **事件帧**按插件透传的 `storyId` 查本地事件表（`scenario_events` 优先 + 引擎五表）还原，
  本仓数据覆盖帧数据、帧只提供选项文本；决策前回显事件与逐选项效果，查不到则跳过（不给训练建议）。
- **CLI**：`matsuri` / `matsuri online` 已迁移 clap，提供 `--help` / `--version`；`matsuri-tools` 仍用手写解析。
- **发布**：Windows 启动器已实现（`crates/matsuri-launcher`，bin `matsuri-ai`）：
  菜单为在线模式（默认）/ 启动小黑板 / 通道体检 / 更新与插件（联网，子菜单）/ 退出；
  小黑板取同目录 `UmamusumeResponseAnalyzer.exe`，用 `CREATE_NEW_CONSOLE` 另开窗口拉起
  （URA 是 Terminal.Gui TUI，与在线模式需同时常驻）。
  更新子菜单为检查更新（只读刷新）/ 更新 AI 数据 / 更新小黑板数据 / 更新小黑板插件 / 更新程序本体 / 返回。
  数据侧按三个频道：`ai_data`（`data/` 三张基础表）、`ura_data`（`.portable/` 根下的 `.br`）、
  `ura_plugins`（`.portable/Plugins/*.zip` + `.portable/version.json`），
  走引擎 `umaai-update` 的 `update.json` 协议（逐文件 sha256、失败回滚）；`ai_data` 严格白名单，
  不覆盖 `events.json` / `constants.json` / 配置 / 剧本资产；`ura_plugins` 不收 `.br`，
  `.portable/PluginData/`（运行时帧与 native 缓存）永不纳入。三个数据频道的清单统一由
  UmaSimData 的 `make_version.py` 生成：`ai_data` / `ura_data` 用显式 `filelist`，
  `ura_plugins` 用 `glob = ["Plugins/*.zip"]` 展开并写回 `version.toml`，同时生成版本标记
  `ura_plugins/version.json`（内容 `{channel, version}`，只由目录 mtime 决定，重跑不制造假更新）。
  更新程序走 `program` 频道整包 zip（`matsuri-ai.exe` + `bin/matsuri.exe` + 6 个程序资产，
  约 3.3 MB），校验逐文件 sha256 与两份 exe 的 Authenticode 指纹后备份替换，启动器有变化时自重启；
  **不含小黑板本体与 `.portable/`**。发布侧 `scripts/package_launcher.py --sign --program`
  先签名再生成 `dist/program/`（证书与口令只走环境变量）。
  **启动自检**：进菜单前并发只读比对 `program` / `ai_data` / `ura_data` / `ura_plugins` 四份清单
  （连接 5 s、单请求 8 s，正常网络约 1.6 s），结论只显示不改行为——头部第 5 行「更新」给状态点与版本，
  「更新与插件」与子菜单各项按对应频道追加「· 有更新」/「· N 项可更新」；不下载、不弹询问、
  不写状态文件，失败只降级显示且不阻断启动，频道清单 404 记「未上线」；
  非交互运行或 `MATSURI_NO_UPDATE_CHECK` 时整段跳过（见 `docs/plan/release_launcher.md` §0.4）。
  头部为标题 + 分隔线 + 五行只读信息（安装目录 / 游戏根 / 小黑板 / 核心程序 / 更新，标签按显示宽度对齐、
  状态点按文件在位染色）；每次操作返回主菜单前清屏（输出被重定向时不动，避免污染脚本日志）；
  不写配置、不落日志。
  发布包为 `matsuri-ai.exe` + `bin/matsuri.exe` + `data/`，打包与附加资源见
  `scripts/package_launcher.py`、`dist/extra/`；完整版计划与差异见
  [`docs/plan/release_launcher.md`](docs/plan/release_launcher.md) §0。
- **测卡 / 扫参**：`cardtest` 支持 `--matsuri-config` 显式完整配置入口；单卡定卡、教诲落位、
  五维系数、机制旋钮四轮扫描均已完成（结果见 `benchmark/_local/deckselect` / `benchmark/_local/advscan` /
  `benchmark/_local/statusscan` / `benchmark/_local/knobscan`）；`cardtest` 汇总带友人出行列（`friend_mean` / `friend_full`）。
  手写训练员三档（21101 / 12101 / 11102）由 CMA-ES 联合调参（`scripts/ga_trainer.py`，需 numpy），
  再经搜索档 N=8192 配对验收（`scripts/search_check.py`）后定档。
  方案与结果见 [`docs/plan/deck_param_scan.md`](docs/plan/deck_param_scan.md) §9–§10。
- **复盘（事实层）**：`matsuri-tools` 新增 `review` bin：`review open <帧目录>` 读局帧 → `facts.json` /
  `brief.md`（逐回合事实、终局、检查项、`pending` 待重算项）；**不跑搜索、不做估值**（`eval` / `curve` /
  `report` 按需补）。真机 `game6316` 已跑通（131 帧 / T0–T77 / 终局 81361（LG8）/ 0 解析失败），
  两次输出逐字节一致。数据面与口径见
  [`docs/plan/review_tool_plan_2026-11-13.md`](docs/plan/review_tool_plan_2026-11-13.md)。

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

# 测卡 / 扫参（手写档；实验输出落 `benchmark/_local/`）
./target/release/cardtest pool --top 12
./target/release/cardtest build --comp all --pick measured --with-default --out benchmark/decks/measured.csv
./target/release/cardtest run --decks benchmark/decks/measured.csv --ref default_21101 --games 3000 --seed 82000
./target/release/cardtest run --decks benchmark/_local/scan/candidates.csv \
  --matsuri-config benchmark/_local/scan/scan.toml --ref ref_stamina --games 5000 --seed 87000

# 复盘（事实层；输入为含 game{id}_turn*.json + finalScore 的局目录）
./target/release/review open <帧目录> --out <输出目录>
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

当前：workspace **全绿**（2026-11-13 复查）+ 4 手动 `#[ignore]`；clippy `-D warnings` 全绿（2026-11-13 复查）；基线 / 性能复现见
[`docs/tests_overview.md`](docs/tests_overview.md) 与 [`docs/perf_profiling.md`](docs/perf_profiling.md)。
clippy 只在 Windows 侧跑；CI 也是 `-D warnings` 硬门禁。

基线快照（2026-11-13，`pt_score_rate` 2.0 → 1.88 后重录）：手写 `a-normal` 82808.8 / `b-ideal` 87411.2，
搜索 N=8192 `a-normal` 86554.8 / `b-ideal` 90067.6 —— **MCTS 比手写高 +2656 ~ +3746 分**（详见
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
| [`docs/plan/review_tool_plan_2026-11-13.md`](docs/plan/review_tool_plan_2026-11-13.md) | 复盘工具计划：局包 / 数据补充 / `review` 子命令 / 指标口径 |
| [`docs/mechanism.md`](docs/mechanism.md) | 机制公式 / 流程 / 实现落点 |
| [`docs/glossary.md`](docs/glossary.md) | 术语与官方译名 |
| [`docs/changelog.md`](docs/changelog.md) | 口径与决定变更记录 |
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

## 许可证

本仓以 **CC BY-NC-SA 4.0**（署名–非商业性使用–相同方式共享）发布，全文见 [`LICENSE`](LICENSE)。
Windows 发布包由 `scripts/package_launcher.py` 随附同一份许可证文件。
