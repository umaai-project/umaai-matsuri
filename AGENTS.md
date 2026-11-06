# umaai-matsuri 项目规则

## 对话规则与工作规范

1. **语言**：思考和回答使用中文，语气简洁、专业。
2. **避免信息过载**：需要分析大量代码 / 数据并提取结论时，使用子代理。
3. **方案选择**：涉及不在上下文中的领域知识或需要选型时，停下来给出建议交用户选择，不直接生成代码。
4. **提案前审视**：提案前从「做减法」的角度重新考虑一次。
5. **编码前确认方案**：每一阶段的任务实际开始编码前，先总结当前方案并向用户确认，确认不再调整后再开始实际编码。

## 项目上下文

- 本仓是 **骏大祭剧本仓**：官方名「朋、史解き ―午式駿大祭―」（駿大祭編），
  `scenarioId = 15`；开发代号 **`matsuri`**，参考库别名 `shuntaisai`（mee-umasim）。
- 引擎仓为并列目录 `../umaai_next`，本仓以 path / git 依赖引用它的 `umaai-core` / `umasim` / `umaai`；
  **不 fork 引擎、不在引擎仓加剧本 crate**。
- 引擎契约：`../umaai_next/docs/scenario/contracts.md`；接入纪律：
  `../umaai_next/docs/scenario/README.md`；仓库形态 / 开发流程 / 性能优化：
  `../umaai_next/docs/scenario/{repo_form,dev_process,perf_plan}.md`。
- 成熟参考实现：`../umaai-onsen`（温泉剧本仓，workspace 五 crate 形态来源）。
- 本仓文档索引见 `docs/README.md`，含开发计划、接入包、stub 登记、机制、术语、测试与交接。

## 文档规则

以下文档随代码同步维护，在提交准备阶段统一更新：

1. **README 状态**：结构 / 验收命令 / 数据说明的现状变化，不写修改过程。
2. **接入包**（`docs/plan/intake.md`）：情报位置、机制 / 数值 / 数据表清单、待确认问题；
   S0 核对期间持续更新，结论分流到机制、术语与 stub。
3. **开发计划**（`docs/plan/scenario_dev_plan.md`）：阶段状态、决策记录、待决事项。
4. **数值占位**（`docs/plan/stubs.md`）：`STUB` / `TBD` 的位置、来源、替换条件与基线影响。
5. **机制 / 术语 / 测试**：`docs/mechanism.md`、`docs/glossary.md`、`docs/tests_overview.md`
   分别随机制改动、口径决定、测试增删更新。
6. **handoff**（`docs/handoff.md`）：上下文很长时，经用户同意后写交接提示词。

## 代码结构

**workspace 五 crate，按终端业务形态划分。** `matsuri-core` 是宿主稳定面（机制 + 训练员 +
装配）；`matsuri-sim`（离线模拟）与 `matsuri-ai`（在线决策）是两条业务线 lib；终端入口由
`matsuri-cli` 承载；`matsuri-tools` 是外围工具面。业务 crate 之间不允许反向依赖
（sim / ai 不得依赖 cli）。重依赖各归其位，**在对应业务真正用上前不要往 `Cargo.toml` 加依赖或 feature**。

| crate / 文件 | 职责 |
|---|---|
| `crates/matsuri-core/src/lib.rs` | 模块声明 + 对外重导出 + 常量（**不含实现**） |
| `crates/matsuri-core/src/game/mod.rs` | 局面 `MatsuriGame` + 资源账本 / 预览 + `build_game` |
| `crates/matsuri-core/src/game/view.rs` | 回合信息快照 `MatsuriView`（分节：turn / resources / uma / board / schedule / friend / camp）+ `view()` |
| `crates/matsuri-core/src/game/diag.rs` | 诊断辅助（关键状态快照 / 变化摘要，供 `diag!` 埋点；只读） |
| `crates/matsuri-core/src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）+ 事件队列 `run_event_queue` |
| `crates/matsuri-core/src/game/action.rs` | `Action` + `Display` + `ActionEnum` + 动作结算入口 |
| `crates/matsuri-core/src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`） |
| `crates/matsuri-core/src/{data,config}.rs` | 剧本资产 schema / 剧本配置段 |
| `crates/matsuri-core/src/trainer.rs` | 手写训练员（守门链 + 打分表 + 事件选项 + 教诲分配价值矩阵；rollout 基策） |
| `crates/matsuri-core/src/search.rs` | FlatSearch 搜索训练员 `MatsuriSearchTrainer`（Train / 教诲 / 事件三路 + 事件队列续跑 + 叶估接口） |
| `crates/matsuri-core/src/value.rs` | 训练员估值层（官方评分边际 + pt_favor + 动作 / 事件 / 教诲分配折算；私有模块） |
| `crates/matsuri-core/src/testing.rs` | 测试定位件（仓根 / 数据根） |
| `crates/matsuri-sim` | 离线模拟：基线裁判（跑局与判定原语）；只出 lib。测量入口不放这里 |
| `crates/matsuri-ai` | 在线决策：帧段解析 / Session 接线 / 决策循环；只出 lib |
| `crates/matsuri-cli` | 终端入口：bin `matsuri`（切 `matsuri` / `random` / `manual`） |
| `crates/matsuri-cli/src/manual.rs` | 手动核对面板（人工核对机制）：状态取 `MatsuriView`（纯呈现）+ 育成配置 + 彩色训练明细表 + 剧本资源 / 教诲分配结果 + 动作槽增量预览 + Inquire 动作 / 事件选择 |
| `crates/matsuri-cli/src/logging.rs` | stderr 日志器：消费 `[output] log_level`，承接 `diag!` 记录 |
| `crates/matsuri-tools` | 外围工具（一个子面一个 bin）：`bench` / `sweep` / `cardtest`（测卡）/ `audit_search` / `probe_fail`；复盘待补 |

- **契约 trait impl 不可拆文件**：一个 trait 的 `impl` 必须整体落在同一文件。
- **可见性**：crate 内跨模块调用标 `pub(crate)`；跨 crate 消费面标 `pub`。
- **阶段自持**：内部强类型 `Stage`，对外只投影 `StageId`（显式赋号，改号 = 旧种子失效）。
- **不碰引擎**：接入新剧本时引擎仓 `git diff` 为空；确需新能力时回引擎仓按「带默认实现、非破坏」加钩子。
- **不预置剧本形态**：阶段 / 事件 / 训练 / 比赛 / 资源等强依赖剧本的实现，先在
  `docs/plan/scenario_dev_plan.md` 列为「待确认项」，S0 机制核对后再自建。
- **诊断日志**：机制 / 训练员的状态变化埋点统一走 `crate::diag!`（引擎宏，target = `diagnostic`；
  预览与搜索 rollout 静默），不要用 `log::info!` 直打业务状态。

## 数据与配置

- **统一数据目录 `data/`（剧本仓唯一发布库，拷仓即跑）**：剧本资产 `scenario_matsuri.json` +
  剧本配置默认层 `matsuri_config.toml`；引擎仓 `gamedata/` 的六张静态表与 `ai_config.toml` 已拷入。
- bin 的数据根是**代码级常量**（`crates/matsuri-cli/src/main.rs` 的 `DATA_ROOT`，跑局从仓根发起）。
  引擎覆盖层约定保留：工作目录放同名 `ai_config.toml` / `matsuri_config.toml` 存在才合并。
- lib 侧不写死路径：`MatsuriData::load` / `MatsuriConfig::load*` / `Data::init` 的根由调用方注入。
- 配置层未知键**严格报错**；剧本专属配置段归本仓，不进引擎配置。

## Rust 编码规范

### 错误处理
1. `Result` 优先；可用 `Result` 的场合不用 `unwrap`。
2. 取 `Option` 内部值时优先 `ok_or_else` 转 `Result`；越界报错，不静默取末档。
3. 仅测试 / 临时代码允许 `unwrap` / `panic`。

### 代码结构
1. **新类型原则**：固定结构的数据建新类型，避免裸 Tuple。
2. **文档化注释**：新增函数 / 结构体须有 rustdoc 注释，函数注释 ≤10 行、文件头 ≤15 行；
   注释必须对应代码现状，历史修改写 `README.md` 状态行或剧本仓文档。
3. 变量命名简短；长词可用缩写。
4. **`Deref` 底座不得与契约同名**（`BaseGame` 的公开方法不得与 `Game` 契约方法同名）。

## 测试规范

1. **前期验证**：机制是否成立以**对话核对为主**（`docs/plan/intake.md` 的确认清单），
   自动化测试随业务落地逐步补齐，不用测试代替机制确认。
2. **分层**：单元测试（内联，私有细节 / 纯函数）用 `assert`；机制集成测试、基线测试与冒烟测试
   在业务落地后补到对应 crate 的 `tests/`。
3. 测试进程的 cwd 在各 crate 目录：数据根 / 基线路径统一经 `matsuri_core::testing` 取仓根。
   引擎 `umaai-core::testing` 的 `env!` 固化的是引擎仓路径，**跨仓不可用**。
4. **Release 档**跑 binary；`cargo test` 统一加 `--offline`。

## 工具使用

- 日常 `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace --release` 加 `--offline`。
- 需要改 registry（`cargo add` / `fetch`）时**先请求提权**。
- **同一 target 目录一次只跑一个 cargo**（并发会互等锁，表现为卡死）。
- 从剧本仓根执行 cargo 命令；不要再套 `--manifest-path`。
- 用户确认整理提交之前，不要执行`cargo fmt`或`cargo clippy`，因为这两个会修改代码，导致需要重新读取全部代码。

## Git 操作

1. **不自动 `commit` / `push`**：先确认用户已完成全部调整，再进入提交准备。
2. **提交准备**：同步更新 `README.md` 状态与剧本仓文档，由用户确认后再提交。
3. **提交范围**：涵盖工作树下由 Agent 与用户做的全部修改。
4. 提交说明需点明**默认行为是否等价**（引擎侧）；剧本侧数值位移须在剧本仓文档记口径分歧。
5. 遵循 Git 安全协议，避免破坏性操作。
