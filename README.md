# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓。

- **开发代号**：`matsuri`（别名：mee-umasim 参考库用 `shuntaisai`，本仓统一用 `matsuri`）
- **形态**：workspace 五 crate（与 `umaai-onsen` 同构）——`matsuri-core`（机制 + 训练员 + 装配，
  宿主稳定面）/ `matsuri-sim`（模拟器与基线裁判）/ `matsuri-ai`（在线决策核心，骨架）/
  `matsuri-cli`（终端入口与手动训练员）/ `matsuri-tools`（测卡 / 复盘，骨架）。
- **依赖**：开发期以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；稳定期换
  git tag。不 fork 引擎、不在引擎仓加剧本 crate。
- **数据**：引擎静态数据已拷入 `data/`；剧本资产 `scenario_matsuri.json` 与配置默认层
  `matsuri_config.toml` 由本仓自持，根目录由宿主注入。

## 当前状态

- **S1 建仓完成**：五 crate 改名、依赖路径、引擎静态数据、CI 与文档骨架已落地。
- **S0 机制与用语文档基本收口（2026-10-31 / 2026-11-01 memo 复核）**：`single_mode_15_*` 11 张表与
  `single_mode_training_effect` 已解码；彩圈 +2、gauge 归属、合宿延长 / Lv+1、U18 延长回合、U33 合宿外出、
  友人分身、稽古基础值、**训练值两层口径（主 / 副属性拆分）**、目标赛 +4、`check_point_effect 204`、
  10 月前半 turn 66、排练失败「完全无效」、牵绊教诲 +3 平摊等均已定为口径。
- **S2 数据层完成（2026-11-01）**：`data/scenario_matsuri.json` 16 个顶层键（`master.mdb` 11 张表 +
  `single_mode_training_effect` + memo 口径，映射见 [`docs/plan/intake.md`](docs/plan/intake.md) §4.1）
  + `data/matsuri_config.toml`（融合预设）+ `data.rs` / `config.rs` 重写 + `tests/data.rs` L2；
  四档验收绿（35 测例）。已接入 `umaai-next` 新基础：`data/events.json` 同步赛后奖励与 URA 决赛事件
  （目标赛 `race_career` 覆写 5/72，非目标赛随机 1 项，比赛乘算统一 floor）。演目 / gauge / 教え / 合宿等
  机制在 S3+ 落地，剩余 TBD 见 [`docs/plan/stubs.md`](docs/plan/stubs.md)。
- **S3 局面与阶段机（2026-11-01）**：一回合五阶段 `Begin → Distribute → Train → AfterTrain → End`
  （`StageId` 0/3/1/4/2，只增不改）；回合类型 / 边界（强制比赛、合宿窗口与延长、発表会 / 本番、
  教诲窗口、URA 收尾）由 `TurnPlan` 表达；`CampState` 做合宿计数与顺延；演目 / 教诲账本
  （`ActLedger` / `AdviceLedger`）+ `MatsuriView` 全量快照；建局门禁=仅 SSR 吾妻；凯斯奇迹 T65
  强制休息按临时记录落地（后续引擎「马娘特殊事件记录」改造后迁移）。四档绿（34 内联 + 11 L2 +
  5 机制 = 50 测例）。**参考卡组**换为：速·待兼诗歌剧 / 速·乐透心 / 耐·樱花千代王 /
  力·摩耶重炮 / 智·爱慕律动 / 友·SSR 吾妻（满凸）。演目 / gauge / 教え / 合宿结算等机制在 S4+ 落地。
- **S4 前置：剧本人头与名单（2026-11-01）**：新增 `MatsuriPerson`（引擎 `BasePerson` 委托 +
  担当 / 来源 / 可用回合）与 `PersonOrigin`（与引擎 `PersonType` 分工：前者剧本标签、后者引擎语义）；
  建局完成「卡 → 理事长 / 记者 → 编成外神驹（第 1 批 5+余额 1、第 2 批 5）」名单与担当分配
  （link 钉位 / 整位 2 张 / 外派链 / 余额）；**编成检测**双层：禁止项（缺 SSR 吾妻 / 多友人 /
  非 SSR 吾妻友人 / 通用规则）拒绝建局，警告项（link 类型 / 同类 ≥3 / 外派 / 类型 <5 种 /
  人物数极差 ≥2）随 `MatsuriView` 下发。
  四档绿（38 内联 + 11 L2 + 13 机制 = 62 测例）。

## 结构

| crate / 文件 | 职责 |
|---|---|
| `crates/matsuri-core/src/lib.rs` | 模块声明 + 对外重导出 + `MAX_TURN` |
| `crates/matsuri-core/src/game/mod.rs` | 局面 `MatsuriGame`（含 `ActLedger` / `AdviceLedger` 账本）+ 资源快照 `MatsuriView` + `build_game`（含友人卡门禁） |
| `crates/matsuri-core/src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）+ 回合结构 `TurnPlan` + 合宿进度 `CampState` |
| `crates/matsuri-core/src/game/person.rs` | 剧本人头 `MatsuriPerson` + 来源 `PersonOrigin`（与引擎 `PersonType` 分工见文档） |
| `crates/matsuri-core/src/game/roster.rs` | 建局人头名单 + 担当分配（link 钉位 / 外派链 / NPC 余额） |
| `crates/matsuri-core/src/game/deck.rs` | 编成检测：禁止项 / 警告项（`DeckError` / `DeckWarning` / `DeckReport`） |
| `crates/matsuri-core/src/game/action.rs` | `Action` + `Display` + `ActionEnum` |
| `crates/matsuri-core/src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`） |
| `crates/matsuri-core/src/data.rs` | 剧本资产 schema / 载入 / 校验（`scenario_matsuri.json` 16 节） |
| `crates/matsuri-core/src/config.rs` | 剧本配置段（融合 / 転成预设 + 覆盖层） |
| `crates/matsuri-core/src/trainer.rs` | 样例训练员 `MatsuriTrainer` |
| `crates/matsuri-cli/src/main.rs` | 终端前端：bin `matsuri`（跑局） |
| `crates/matsuri-cli/src/manual.rs` | 无颜色手动训练员（机制核对） |
| `crates/matsuri-sim` | 模拟器与基线裁判（骨架） |
| `crates/matsuri-ai` | 在线决策核心（骨架） |
| `crates/matsuri-tools` | 外围工具（骨架） |
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
| [`docs/handoff.md`](docs/handoff.md) | 交接提示词 |

## 运行与验收

```powershell
# 数据根是代码级常量 data/（crates/matsuri-cli/src/main.rs 的 DATA_ROOT）
cargo run --release --bin matsuri --            # 样例训练员
cargo run --release --bin matsuri -- random     # 引擎随机基线（对照用）
cargo run --release --bin matsuri -- manual     # 无颜色人工选择（机制核对用）

cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
```

- `cargo check` / `clippy` / `test` 通过（**不改引擎任何一行**）。
- 接入新剧本时引擎仓 `git diff` 为空——确需新能力时回引擎仓按「带默认实现、非破坏」加钩子。
- 三条随机轴（局面 / 事件 / 策略）+ 决策流只喂 `Trainer`；阶段自持、对外只投影 `StageId`；
  同一 `master` 下两次运行 / 两支分支逐位一致。
- 剧本资源给前端走 `MatsuriView`（形态 A 五条纪律），不进契约 trait。

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
