# 架构概览（ARCHITECTURE）

本仓是「午式骏大祭（`matsuri`）」的 Rust 六 crate workspace，通过 Git Submodule `umaai_next/` 挂载引擎仓并按 `path` 依赖引用其稳定面。领域术语见 [GLOSSARY.md](GLOSSARY.md)，具体数值与机制规则见 [docs/mechanism.md](docs/mechanism.md)。

## 1. Crate 拓扑与依赖边界

```text
matsuri-launcher (bin: matsuri-ai) ──(子进程调度)──▶ matsuri-cli (bin: matsuri)
   └─ 仅依赖 umaai-update，不依赖本仓业务 crate

matsuri-cli   ──▶ matsuri-ai ──┐
matsuri-tools ──▶ matsuri-sim ─┼─▶ matsuri-core ──▶ umaai_next/ (umaai-core / umasim / umaai)
                  └─ matsuri-ai┘
```

- **单向依赖，禁止反向依赖**：`matsuri-core` 不得依赖业务 crate；`matsuri-sim`（离线模拟）与 `matsuri-ai`（在线决策）互不依赖且不得依赖 `matsuri-cli`；`matsuri-launcher` 保持零业务依赖。
- **不碰引擎仓**：接入与维护剧本时 `umaai_next/` 的 `git diff` 应为空；确需引擎新能力时回引擎仓按「带默认实现、非破坏」加钩子。
- **按需加依赖**：重依赖各归其位（如 `pprof` 仅在 `matsuri-tools` 的 `profile` feature 启用），未实际消费前不向 `Cargo.toml` 提前引入依赖或 feature。

## 2. 核心数据流

- **离线模拟**：`data/`（`MatsuriData` + `MatsuriConfig`）→ `MatsuriGame`（`Stage` 阶段机驱动）→ `MatsuriView` 只读快照 → `Trainer` / `MatsuriSearchTrainer` 决策 → `matsuri-sim`（基线跑局裁判）/ `matsuri-cli`（终端运行与 `manual` 核对面板）/ `matsuri-tools`（测卡、扫参、复盘、性能测量）。
- **在线决策**：小黑板帧文件（`urafile.rs` / `frame.rs`）→ `game/restore.rs` 局面还原 → `session.rs` / `online.rs` 决策循环 → `MatsuriSearchTrainer` → `output.rs` / `matsuri-cli::panel` 输出建议。

## 3. 各 Crate 与关键落点

### `crates/matsuri-core`（剧本机制、决策与核心装配）

- **入口与配置**：`lib.rs`（仅模块声明、常量与重导出，不含实现）、`data.rs`（剧本资产 schema）、`config.rs`（剧本配置段）、`deck_config.rs`（卡组档读取与合并）、`testing.rs`（测试数据根定位）。
- **局面与阶段（`src/game/`）**：
  - `mod.rs`：`MatsuriGame` 状态、演目槽/等级账本、`build_game` 建局入口与预览。
  - `stage.rs`：`Stage` 五阶段状态机（对引擎只投影显式赋号的 `StageId`）与事件队列 `run_event_queue`。
  - `traits.rs`：引擎契约 `impl`（`Game` / `TrainingValueSource` / `FlatSearchGame`）。
  - `action.rs`：`Action` 枚举、显示及动作结算入口。
  - `view.rs` / `diag.rs`：只读回合快照 `MatsuriView` 与 `diag!` 状态快照辅助。
  - **机制子域**：`distribute.rs`（训练分布与社交人头落位）、`train.rs`（常规训练与演目排练）、`advice.rs`（教诲传授/改派/转成与效果）、`festa.rs`（进度汇报会、合宿与午式骏大祭演出）、`event.rs`（剧本与友人事件）、`settle.rs`（回合末结算）、`person.rs` / `roster.rs` / `deck.rs`（角色、担当分配与卡组）、`restore.rs`（在线帧还原）。
- **AI 决策与估值**：
  - `trainer.rs`：手写训练员（守门链、打分表、教诲分配与 rollout 基策）。
  - `search.rs`：FlatSearch 搜索训练员 `MatsuriSearchTrainer`（训练/教诲/事件三路搜索与叶估）。
  - `value.rs`：训练员估值层（官方评分边际、`pt_favor` 与动作/事件折算；私有模块）。

### `crates/matsuri-sim`（离线模拟 lib）

- `lib.rs` / `baseline.rs`：提供基线跑局与判定原语（只出 lib，测卡与扫参入口放在 `matsuri-tools`）。

### `crates/matsuri-ai`（在线决策 lib）

- `urafile.rs` / `frame.rs`：小黑板帧文件监听与帧段解析。
- `session.rs` / `online.rs`：在线会话状态同步与决策循环。
- `output.rs` / `final_score.rs`：决策结果导出与终局评分汇总。

### `crates/matsuri-cli`（终端入口，bin `matsuri`）

- `main.rs`：命令行入口（`clap` 解析，`--help` / `--version`，切换 `matsuri` / `random` / `manual` / `online` 等模式）与静态 `DATA_ROOT` 注入。
- `manual.rs`：手动核对面板（消费 `MatsuriView` 纯呈现，用于机制人工核对）。
- `online.rs` / `panel.rs`：在线模式驱动与终端呈现面板。
- `explain.rs` / `ai_trace.rs` / `logging.rs`：决策解释、AI 轨迹与 `diag!` stderr 日志器。

### `crates/matsuri-tools`（外围离线工具）

- **工具 bin（`src/bin/`）**：`bench`（性能测量）、`sweep`（开局因子扫参）、`cardtest`（测卡与配对跑测）、`review`（局包复盘）、`rhythm`（演目 Lv 与友人节奏统计）、`audit_search`、`probe_fail`（CLI 参数手写解析）。
- **复盘模块（`src/review/`）**：`discover` / `facts` / `derive` / `curve` / `eval` / `infer` / `brief` / `report` / `tuning`，将在线/模拟局包解析为事实层并重算估值报告。

### `crates/matsuri-launcher`（发布启动器，bin `matsuri-ai`）

- `main.rs` / `startup.rs` / `program.rs` / `update.rs`：面向最终用户的菜单入口、通道自检、四频道数据/程序更新及子进程拉起 `matsuri`（CLI）。

## 4. 常见改动快速定位

| 改动诉求 | 优先查看的文件 |
|---|---|
| 演目槽、演目排练、训练收益与失败判定 | `matsuri-core/src/game/{mod.rs, train.rs}` |
| 教诲传授 / 改派 / 转成与数值效果 | `matsuri-core/src/game/advice.rs`、`matsuri-core/src/data.rs` |
| 合宿、进度汇报会、URA 回合与回合末结算 | `matsuri-core/src/game/{stage.rs, festa.rs, settle.rs}` |
| 担当分配、神驹 / 吾妻 / 理事长记者分布 | `matsuri-core/src/game/{person.rs, roster.rs, distribute.rs}` |
| 引擎契约接缝（`Game` / `FlatSearchGame`） | `matsuri-core/src/game/traits.rs` |
| 搜索逻辑、打分与叶节点估值 | `matsuri-core/src/{trainer.rs, search.rs, value.rs}` |
| 在线帧解析与局面还原偏差 | `matsuri-ai/src/{frame.rs, session.rs}`、`matsuri-core/src/game/restore.rs` |
| 手动核对面板或终端输出格式 | `matsuri-cli/src/{manual.rs, panel.rs, explain.rs}` |

## 5. 架构硬约束

- **契约 trait impl 不可拆文件**：同一个 trait（如 `Game`）的 `impl` 必须整体落在同一文件（`traits.rs`）。
- **`Deref` 底座不得与契约同名**：`BaseGame` 的公开方法不得与 `Game` 契约方法同名。
- **可见性分级**：crate 内跨模块调用标 `pub(crate)`；跨 crate 消费面标 `pub`。
- **数据与配置注入**：lib 侧不写死路径（`MatsuriData::load` / `MatsuriConfig::load*` / `Data::init` 的根由调用方注入）；bin 数据根为代码级常量（`matsuri-cli/src/main.rs` 的 `DATA_ROOT`），工作目录存在同名 `ai_config.toml` / `matsuri_config.toml` 时覆盖合并；未知键严格报错。
- **`StageId` 显式赋号**：对外投影枚举值只增不改，修改已有编号会导致旧种子失效。
- **呈现与决策分离**：`MatsuriView` 为纯只读值快照，不混入呈现文本或候选动作预览；`manual.rs` / 前端只消费 `MatsuriView` 与静态表。
- **诊断埋点统一走 `diag!`**：机制与训练员状态变化统一使用 `crate::diag!`（target = `diagnostic`；预览与搜索 rollout 静默），不以 `log::info!` 直打业务状态。
