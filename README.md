# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓。

- **开发代号**：`matsuri`（别名：mee-umasim 参考库用 `shuntaisai`，本仓统一用 `matsuri`）
- **形态**：workspace 五 crate（与 `umaai-onsen` 同构）——`matsuri-core`（机制 + 训练员 + 装配，
  宿主稳定面）/ `matsuri-sim`（模拟器与基线裁判）/ `matsuri-ai`（在线决策核心，骨架）/
  `matsuri-cli`（终端入口与手动训练员）/ `matsuri-tools`（测量与外围工具）。
- **依赖**：开发期以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；稳定期换
  git tag。不 fork 引擎、不在引擎仓加剧本 crate。
- **数据**：引擎静态数据已拷入 `data/`；剧本资产 `scenario_matsuri.json` 与配置默认层
  `matsuri_config.toml` 由本仓自持，根目录由宿主注入。

## 现状

- **机制闭环**：五阶段机 + `TurnPlan` + `MatsuriView` 完整回合快照；动作全集（训练 / 演目稽古 /
  比赛 / 休息 / 出行 / 友人出行 / 治病）；人头名单与每回合分布；训练两层公式（下层引擎 +
  上层剧本）；演目 gauge / Lv / 稽古两档；教诲（10 种效果 + 転成 + 120 排列分配）；発表会 / 本番 /
  合宿 / URA / 终局奖励；事件链（固定 / 随机 / 合宿抑制 / 友人链 / 追加稽古 / 变胖）；手写训练员
  （守门链 + 打分表 + 事件选项 + 教诲分配择优）；诊断日志 `diag!`。
- **搜索**：`FlatSearchGame` 集成层已接（`MatsuriSearchTrainer`，引擎扁平 MC + UCB + CRN）；
  `matsuri_config [search].search_n` = 每候选 rollout 次数（**默认 8192**；0 = 手写档），
  `train` / `advice` / `event` 分开关；教诲 120 候选剪枝到 top-16；
  事件搜索在 Begin 内部结算时会补跑该阶段尾部（T2 教诲授予与首分配），
  叶子估值器仅预留接口，生产 `max_depth=0`。
  搜索排序口径 `radical_factor_max=0`（算术均值，不做高分尾部加权）；
  `search_group_size=1024`（每候选样本下限），`expected_search_stdev=3300`。
- **搜索单局验证**（默认马娘 105902、seed 61444、24 线程、`radical_factor_max=0`）：
  N=1024 **85309 / PT 12427**（3.7 s）；N=8192 **86290 / PT 12733**（17.1 s）；
  N=12288 **85458 / PT 12317**（23.8 s）。单局冒烟，不是基线；单 seed 下 N 不单调（搜索噪声）。
- **搜索理想测试**（单 seed，Train-only、`radical=0.0`，旧预设口径）：普通 100101 + 小切 / 盾×2
  手写 **82807 / LG10** → 4096 **85184 / LG14（+2377）**；狄杜斯 106301 → **85384 → 86955**
  （+1571，铁心后五维近上限，边际更小）。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`，`search_n=8192`）；
  默认测试马娘 **105902**（目白多伯，2026-11-05 全马娘扫测定档）；手写档用 `MATSURI_SEARCH_N=0`
  或工作目录覆盖层关闭。局外状态默认不注入（`[pre] qie = "none"`、`positive_thinking = 0`）；理想测试档用
  `bench --qie xiaoqie --shield 2` 覆盖，`--uma` 可换马娘。
- **基线**（seeds 73200 起，2026-11-05 录；两档分开，不与任何旧数字混比）：

| 档 | 手写 100 局 | 搜索 10 局（N=8192） |
|---|---|---|
| 普通马娘 105902，无 buff | **80291.3** / PT 11262.5（sd 2635） | **84639.2** / PT 12247.4（sd 1526，22.8 s/局） |
| 狄杜斯 106302，小切 + 盾 2 | **84219.9** / PT 11890.8（sd 2566） | **87646.6** / PT 12923.4（sd 491，21.2 s/局） |

  搜索档 10 局的均分标准误约 ±483 / ±155；跑满 100 局可把标准误压到 ±150 / ±50。
  默认开局预设为 `extra_count = [10,10,120,140,0,100]`（100101 扫参结果，见
  [docs/mechanism.md](docs/mechanism.md) §5.6，未按新默认马娘重扫）。
- **验收**：workspace **225 通过 + 2 手动档**（1024 整局复现、教诲微基准，均 `#[ignore]`）；
  `fmt --check` / `check` / `clippy -D warnings` / `test` 四门绿。
- **性能**：四轮 O0–O4 优化，配对测量与复现见 [`docs/perf_profiling.md`](docs/perf_profiling.md)：
  第一轮（预览零克隆 / 教诲查表 / 原因惰性化）整局 N=1024 55.7 s → 24.7 s；第二轮
  （诊断载荷惰性化 / rollout 静默事件明细 / 事件队列去深拷贝 / 参与教诲去重）同轮配对
  手写整局 −27%、串行 rollout −26%、整局 N=256 −20%、N=1024 −14%，单次 rollout 分配
  4080 → 2085 次；第三轮（引擎侧事件 / 训练后处理 / 选项结算诊断载荷门控）分配
  2085 → 1017 次，串行 rollout −13%、手写整局 −15%、整局 N=256 −4.2%、N=1024 −7.3%；
  第四轮（诊断 guard 冗余 store 假共享）并行倍率约 4× → 12.6×（24T flat 每 rollout
  33.4–35.2 → 11.3–11.7 μs），整局 N=256 6.00 → 1.81 s、N=1024 13.40 → 4.13 s（24T）。
  四轮均决策与终局逐位一致。
- **下一步**：两档基线补到 100 局搜索（seeds 73200–73299）；之后转入评分口径与事件质量，
  不再加搜索预算（分辨率实测已饱和）。调参一律换全新种子块（73200 起已用）。
## 结构

| crate / 文件 | 职责 |
|---|---|
| `crates/matsuri-core/src/lib.rs` | 模块声明 + 对外重导出 + `MAX_TURN` |
| `crates/matsuri-core/src/game/mod.rs` | 局面 `MatsuriGame` + `ActLedger` / `AdviceLedger` 账本 + 演目 gauge / Lv / 発表会 buff 派生 + 训练预览 `TrainingPreview` / 动作资源预览 `ResourcePreview` + `build_game`（含友人卡门禁） |
| `crates/matsuri-core/src/game/view.rs` | 回合信息快照 `MatsuriView`（分节：turn / resources / uma / board / schedule / friend / camp + 编成警告 / 事件记录）+ `view()` |
| `crates/matsuri-core/src/game/diag.rs` | 诊断辅助：关键状态快照 / 变化摘要 / 回合类型摘要（供 `diag!` 埋点，只读） |
| `crates/matsuri-core/src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）+ 回合结构 `TurnPlan` + 合宿进度 `CampState` + 事件队列 `run_event_queue`（事件搜索续跑） |
| `crates/matsuri-core/src/game/person.rs` | 剧本人头 `MatsuriPerson` + 来源 `PersonOrigin`（与引擎 `PersonType` 分工见文档） |
| `crates/matsuri-core/src/game/roster.rs` | 建局人头名单 + 担当分配（link 钉位 / 外派链 / NPC 余额） |
| `crates/matsuri-core/src/game/deck.rs` | 编成检测：禁止项 / 警告项（`DeckError` / `DeckWarning` / `DeckReport`） |
| `crates/matsuri-core/src/game/distribute.rs` | 人头分布（亮起锁担当 / 不在率 / 吾妻补位 / 分身）+ 得意率 / 灵感分配；**理事长·记者不落稽古位（全稽古回合不出现）；吾妻有稽古必补未满稽古位；每训练位至多 1 名友人 / 团队 / 理事长 / 记者** |
| `crates/matsuri-core/src/game/event.rs` | 事件管线：回合开始事件链（固定 / 随机分布 / 合宿抑制 / 友人解锁）/ 友人链事件扩展 / 追加稽古事件 / 疲劳回复与变胖 / turn 66 hint 记账 |
| `crates/matsuri-core/src/game/train.rs` | 训练结算：下层引擎公式 + 上层剧本乘区 / 封顶 + 失败率 + 训练后处理（羁绊 / 灵感 / 加练 / 追加稽古事件 / 友人点击 / 变胖速度归零） |
| `crates/matsuri-core/src/game/advice.rs` | 教诲：槽效果聚合（按绑定演目 Lv × 参与人数）/ 融合 / 分配接口 |
| `crates/matsuri-core/src/game/settle.rs` | 回合末结算：発表会 / 本番 / 合宿 +1 / 教诲窗口 / 育成终了 |
| `crates/matsuri-core/src/game/action.rs` | `Action` + `Display` + `ActionEnum` + 比赛 / 休息 / 出行 / 治病结算入口（含演目槽入账） |
| `crates/matsuri-core/src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`，含分布 / 得意率 / 训练值 / 失败率 / 终局覆写） |
| `crates/matsuri-core/src/data.rs` | 剧本资产 schema / 载入 / 校验（`scenario_matsuri.json`：`scenario_events` / `event_extras`；稽古两档 / 规则常量） |
| `crates/matsuri-core/src/config.rs` | 剧本配置段（融合预设 / `[pre]` 育成前状态 / `[search]` 作用面与预算 / `[trainer]` 旋钮 + 覆盖层） |
| `crates/matsuri-core/src/trainer.rs` | 手写训练员 `MatsuriTrainer`（守门链 + 打分表 + 教诲分配价值矩阵 + 事件选项） |
| `crates/matsuri-core/src/search.rs` | FlatSearch 搜索训练员 `MatsuriSearchTrainer`（Train / 教诲 / 事件三路 + 事件队列续跑 + 叶估接口） |
| `crates/matsuri-cli/src/main.rs` | 终端前端：bin `matsuri`（跑局；`matsuri` / `random` / `manual` 训练员选择；`--explain` / `MATSURI_EXPLAIN` 开手写档决策解释） |
| `crates/matsuri-cli/src/panel.rs` | 手写档 / 手动档**共用回合面板** `BoardPanel`：状态全部从 `MatsuriView` 取（纯呈现）；育成配置 + 彩色训练明细表（人物着色：彩圈 / 助力黄 / Hint 叹号 / 羁绊；掷中时含研鑽担当的列标题加紫「追加稽古」行）+ 剧本资源与教诲分配结果 + 合宿·友人链·赛程 + 五维与上限显示压缩（>1200 减半）+ 动作槽增量 / 助力支撑标记 + 演目槽变化行 + 事件名亮蓝；`render(..., scores)` 可追加候选打分表 |
| `crates/matsuri-cli/src/explain.rs` | 手写档决策解释包装（`--explain`）：复用 `panel.rs` 面板，面板后追加候选打分表（`explain_action` 同源）与决策理由；教诲分配 top-10、事件选项期望值；**只打印不改决策** |
| `crates/matsuri-cli/src/manual.rs` | 手动核对面板（面板渲染在 `panel.rs`）：逐决策人机交互（Inquire / 非 TTY 编号输入），教诲分配只支持「保持现状」；事件选项组回显 |
| `crates/matsuri-cli/src/logging.rs` | stderr 日志器：消费 `[output] log_level`，承接引擎 / 剧本 `diag!` 记录（默认 `info`、`off` 静默） |
| `crates/matsuri-sim` | 基线裁判（`baseline` 模块：JSON 解析 / 自检 / 判定）；测量函数不放这里 |
| `crates/matsuri-ai` | 在线决策核心（骨架） |
| `crates/matsuri-tools` | 外围工具与**全部测量入口**：`bench`（整局 / 决策分段 / rollout 分布 / 固定根 / 阶段拆分 / 分配计数 / 平铺并行对照 / 剖面）、`sweep`（开局预设 / 全马娘扫测）、`baseline`（按 `baselines/` 复跑并判定）、`audit_search` / `probe_fail`（固定根分辨率审计 / rollout 失败定位）；测卡 / 复盘待补 |
| `data/` | 引擎静态表 + 剧本资产 `scenario_matsuri.json` + 双配置默认层 |
| `docs/` | 本仓文档骨架 |

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/README.md`](docs/README.md) | 本仓文档索引与引擎文档入口 |
| [`docs/plan/scenario_dev_plan.md`](docs/plan/scenario_dev_plan.md) | 开发计划：阶段状态 / 决策记录 / 待决事项 |
| [`docs/plan/intake.md`](docs/plan/intake.md) | 接入包：情报位置 / 机制 / 数值 / 数据表 / 待确认 |
| [`docs/plan/stubs.md`](docs/plan/stubs.md) | 数值占位登记（`STUB` / `TBD`） |
| [`docs/mechanism.md`](docs/mechanism.md) | 机制文档（公式 / 流程 / 实现落点） |
| [`docs/glossary.md`](docs/glossary.md) | 术语单一事实源 + 口径决定记录 |
| [`docs/tests_overview.md`](docs/tests_overview.md) | 测试一览 |
| [`docs/perf_profiling.md`](docs/perf_profiling.md) | 性能分析与优化：工具 / 热点 / 改动 / 配对测量 / 复现 |
| [`docs/handoff.md`](docs/handoff.md) | 交接提示词 |

## 运行与验收

```powershell
# 数据根是代码级常量 data/（crates/matsuri-cli/src/main.rs 的 DATA_ROOT）
cargo run --release --bin matsuri --            # 默认搜索档（search_n=8192；单局约 21 s / 24 线程）
$env:MATSURI_SEARCH_N=0; cargo run --release --bin matsuri --   # 手写档
cargo run --release --bin matsuri -- random     # 引擎随机基线（对照用）
cargo run --release --bin matsuri -- manual     # 手动核对面板（动作 Inquire；Esc 退出，非 TTY 退回编号输入）
$env:MATSURI_SEARCH_N=0; cargo run --release --bin matsuri -- matsuri --explain   # 决策解释（仅手写档）
# 诊断日志级别见 data/ai_config.toml [output] log_level（默认 info；工作目录覆盖层可设 off 静默）

cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
```

- `cargo check` / `clippy` / `test` 通过（**不改引擎任何一行**）。
- 接入新剧本时引擎仓 `git diff` 为空——确需新能力时回引擎仓按「带默认实现、非破坏」加钩子。
- 三条随机轴（局面 / 事件 / 策略）+ 决策流只喂 `Trainer`；阶段自持、对外只投影 `StageId`；
  同一 `master` 下两次运行 / 两支分支逐位一致。
- 回合信息给前端 / 在线外壳走 `MatsuriView`（**完整回合快照**，分节；名称文本由呈现层按静态表解析），
  不进契约 trait。
- 诊断日志统一走 `diag!`（target = `diagnostic`）：级别由 `[output] log_level` 控制（默认 `info`；
  `off` 静默），批量 / 基线入口用 `DiagGuard::suppress()`；工作目录放 `ai_config.toml` 覆盖层即可调级别。

## 依赖三档

```toml
# 1. 开发期：并列目录（当前写法）
umaai-core = { path = "../umaai_next/crates/umaai-core" }
umasim     = { path = "../umaai_next/crates/umasim" }
umaai      = { path = "../umaai_next/crates/umaai" }
umaai-cli  = { path = "../umaai_next/crates/umaai-cli" }   # 仅终端入口需要

# 2. 稳定期：同一 repo、同一 tag
# umaai = { git = "<engine repo>", tag = "engine-v0.1.0" }

# 3. 下游（Web / Android）：自喂帧、无文件监听，并剪掉并行后端
# umaai = { git = "<engine repo>", tag = "engine-v0.1.0", default-features = false }
```
