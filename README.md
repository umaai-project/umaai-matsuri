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
  力·摩耶重炮 / 智·爱慕律动 / 友·SSR 吾妻（满凸）。演目 / gauge / 教え / 発表会 / 合宿结算等机制在 S5+ 落地。
- **S4 前置：剧本人头与名单（2026-11-01）**：新增 `MatsuriPerson`（引擎 `BasePerson` 委托 +
  担当 / 来源 / 可用回合）与 `PersonOrigin`（与引擎 `PersonType` 分工：前者剧本标签、后者引擎语义）；
  建局完成「卡 → 理事长 / 记者 → 编成外神驹（第 1 批 5+余额 1、第 2 批 5）」名单与担当分配
  （link 钉位 / 整位 2 张 / 外派链 / 余额）；**编成检测**双层：禁止项（缺 SSR 吾妻 / 多友人 /
  非 SSR 吾妻友人 / 通用规则）拒绝建局，警告项（link 类型 / 同类 ≥3 / 外派 / 类型 <5 种 /
  人物数极差 ≥2）随 `MatsuriView` 下发。
  四档绿（38 内联 + 11 L2 + 13 机制 = 62 测例）。
- **S4 动作与结算（2026-11-01）**：`Action` 全集（普通训练 / 演目稽古 `ActTrain` / 比赛 / 休息 /
  普通出行 / 友人出行 / 治病）+ 按回合类型的动作集（强制比赛 / 强制休息 / 合宿全稽古 / 日常与 URA 收尾）；
  单动作结算：目标赛 / URA / 自选比赛走 `push_race_event`（比赛加成含発表会永久 buff，floor 在引擎）、
  普通训练与演目稽古下层公式（稽古切 `training.keiko` 表）+ 失败判定、普通 / 合宿版休息、普通出行、
  吾妻出行 5 段（每段最低演目 Lv+1 + memo 五段事件）；`AfterTrain` 统一结算 `unresolved_events`、
  `apply_event` 补事件羁绊归属、`Distribute` 过渡清零（真实分布留 S5）。四档绿
  （39 内联 + 12 L2 + 22 机制 = 73 测例）；整局冒烟（样例训练员）7690 分 / B、390 阶段。
  演目 Lv / gauge / 発表会 / 训练上层加成 / 事件链留 S5 / S6。
- **S5 机制（2026-11-01）**：机制闭环补齐——①**人头分布**重写（亮起锁担当 / 神驹不在率 50 /
  吾妻排练补位 / 協力・転成・URA 分身 / 每位置 ≤5 / 得意率 + 発表会基线）；②**演目 gauge** 入账 /
  消费（训练参与按担当归集、闪彩 +2；目标赛 +4；休息 / 出行 / 非目标赛 +1；稽古成功清槽 + Lv+1，
  合宿稽古不升不清；三则落点 + 满槽跳过）；③**稽古两档**基础值（`training.keiko.classic` 恢复，
  turn 24 起切档）与**演目 Lv 效果**（全基础 +10 / 友情 / SP 排练 / 灵感 / SP·基础上限）；④**教诲系统**
  （10 种效果 × 绑定演目 Lv × 参与人数叠加、絆 +3 平摊、助力支撑、研鑽追加稽古、不屈、両道、
  turn 2/11/23/35/47/59 窗口、turn 23/47 転成融合、120 排列接口留 S7）；⑤**训练两层公式**（下层 =
  引擎默认公式 + 上层 = 发布会 / 教え / 演目 Lv / 稽古 +10 / 获得量乘区，主 / 副 / SP 拆分 + floor +
  上昇量上限）；⑥**発表会 / 本番 / 合宿 +1 / URA 训练 / 育成终了**结算（结果按实际演目 Lv 判定，
  buff / 一次性奖励 / 訓練 Lv+1 / 本番演出 / 全演目 +1 / 终局奖励）；⑦`MatsuriView` 补 S5 资源字段。
  四档绿（**81 内联 + 13 L2 + 25 机制 = 119 测例**）；整局冒烟（样例训练员）**34368 分 / UE9、
  PT 6128、390 阶段**（random 基线 19516 分 / SS+、PT 3985）。
- **S6 事件管线（2026-11-01）**：回合开始事件链落地——①**固定事件**（引擎通用 `story_events` +
  剧本固定 `scenario_events`：`400015105` 教え見直し turn 11/35/59、`400015106` 追加の稽古代码触发）；
  ②**随机分布**（支援卡连续事件 / 马娘事件 / 掉心情，分布与数值按引擎 `constants.json` / `events.json`
  原始值；支援卡事件候选只取可用支援卡人头，友人 / 团队不计入）；③**合宿抑制**合并为单一判定
  （第 1 年合宿允许、第 2/3 年常规与延长段抑制随机事件；非合宿回合沿用引擎 `no_event_turns` 基线，
  STUB）；④**友人链**（吾妻 `809052xxx`：首次点击 → 首次事件 / 之后 40% 训练后事件 / 出行解锁上·下
  → 疲劳回复 flag / 出行 5 段真实卡面事件 / Classic 正月 / 结局；友人事件叠卡面事件效果加成）；
  ⑤**追加稽古事件化**（训练员二选一：上档 = 直前训练 +20 / SP+20 / 体力−5 / 理事长羁绊 +5 /
  最低演目 gauge+1 / 解一个 debuff；下档 +5 / 体力+5；删除 50/50 STUB）；⑥**变胖状态**
  （大胃王事件 33% 附加：训练 / 排练速度增量归零，回合开始 20% 概率解除、治病 / 合宿版休息解除）；
  ⑦turn 66 进化技能只记 `hint_level + 1`；⑧接引擎通用「回合后」事件（turn 48 抽签 / turn 77 育成结束）
  与继承特殊结算。四档绿（**95 内联 + 15 L2 + 27 机制 = 137 测例**）；整局冒烟（样例训练员）
  **44946 分 / UC6、PT 7496、390 阶段**（S6 事件收益接入后的预期位移；S9 前不建基线）。
  策略训练员 / 搜索（S7/M2）、技能灵感与粉丝不建模（TBD）留后续。
- **S8 手动核对面板（2026-11-01，先行落地 + 四轮显示调整；S8 其余待做）**：`manual` 档从「摘要 + 编号输入」
  升级为机制核对面板——**开局打印育成配置**（育成马娘 / 卡组含突破与稀有度 / 继承：蓝因子 + 剧本因子 +
  开局继承）；每次动作决策前打印回合结构 / 五维（含引擎上限）/ 体力干劲 / 状态 / **剧本资源表**（演目
  2 字短名 · 槽 x/y · Lv · 教诲→転成 · 担当人物；発表会结果与永久 buff / 助力支撑 / 追加稽古着色）/
  **彩色训练明细表**（每训练一列，列头 = 训练名 Lv，排练时列头 = 演目名 Lv；格内列出该位置全部人物：
  短名、未满 100 的羁绊、Hint 亮黄叹号；人物按类型着色：友人绿 / 彩圈卡亮绿 / 普通支援卡白 / NPC 灰）/
  合宿（仅合宿回合显示计数）/ 友人链（状态·出行段·下一段事件）/ 目标赛与自选赛窗口 / 动作候选（训练成功
  收益预览 + 失败率 + **演目槽增量**，**Inquire 交互选择，非 TTY 退回编号输入**）；事件选择用 **Inquire**，
  事件名亮蓝色，选项组列概率 / 结果档 / 到账数值 / ExtraStatus / 状态增删；每次提示附「自上次以来新结算
  事件」（含 id / 名称 / 选项 / 实际到账）、「上一步后数值变化」（含 **演目 Lv 上升**行）与**教诲分配结果**
  （首次传授 / 転成 / 改派·保持；结果行后紧跟刷新后的剧本资源表）；状态行仅显示已有状态。颜色与表格复用
  `umaai-cli` 呈现原语（非 TTY / `NO_COLOR` 自动退化纯文本）。核心侧新增 `EventRecord` 事件记录（容量 32，
  `MatsuriView.recent_events` 同步）、`preview_training` 与 `preview_action_resources` / `ResourcePreview`
  （克隆局面只读预览，不耗随机流、不改局面）；`[嘉宾]` 统一改 `[NPC]` 且用角色名前两字。
  **剧本机制自 turn 2 起：0–1 回合无担当分配，训练 / 休息 / 出行 / 事件不获取演目槽**
  （`MatsuriGame::scenario_started` 闸门），面板在 0–1 回合标「未分配 / 机制自 T2 开始」；**教诲首次授予
  以 turn 2 Begin（NPC / 友人加入后立即）为准，效果从 T2 的分布 / 训练起生效**（End 的窗口 0 不再重复授予）。
  手动档引擎 `PrintFrontend` 切 `Quiet`，面板为唯一信息源；CLI 冒烟（管道输入跑满整局 + 非 TTY 无 ANSI +
  教诲 / 资源 / 短名 / 无合宿误报断言）。四档绿（**100 内联 + 15 L2 + 27 机制 + 5 CLI 内联 + 1 CLI 冒烟
  = 148 测例**）；样例训练员冒烟 **40899 分 / UC、PT 6982**（T2 闸门 + T2 教诲生效后的位移；S9 前不建基线）。
  S7 训练员 / S8 其余（View 字段补全 / 在线外壳 / 选择器）仍待做。

## 结构

| crate / 文件 | 职责 |
|---|---|
| `crates/matsuri-core/src/lib.rs` | 模块声明 + 对外重导出 + `MAX_TURN` |
| `crates/matsuri-core/src/game/mod.rs` | 局面 `MatsuriGame` + `ActLedger` / `AdviceLedger` 账本 + 演目 gauge / Lv / 发布日期 buff 派生 + 事件记录 `EventRecord` / 训练预览 `TrainingPreview` / 动作资源预览 `ResourcePreview` + `MatsuriView` + `build_game`（含友人卡门禁） |
| `crates/matsuri-core/src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）+ 回合结构 `TurnPlan` + 合宿进度 `CampState` |
| `crates/matsuri-core/src/game/person.rs` | 剧本人头 `MatsuriPerson` + 来源 `PersonOrigin`（与引擎 `PersonType` 分工见文档） |
| `crates/matsuri-core/src/game/roster.rs` | 建局人头名单 + 担当分配（link 钉位 / 外派链 / NPC 余额） |
| `crates/matsuri-core/src/game/deck.rs` | 编成检测：禁止项 / 警告项（`DeckError` / `DeckWarning` / `DeckReport`） |
| `crates/matsuri-core/src/game/distribute.rs` | 人头分布（亮起锁担当 / 不在率 / 吾妻补位 / 分身）+ 得意率 / 灵感分配 |
| `crates/matsuri-core/src/game/event.rs` | 事件管线：回合开始事件链（固定 / 随机分布 / 合宿抑制 / 友人解锁）/ 友人链事件扩展 / 追加稽古事件 / 疲劳回复与变胖 / turn 66 hint 记账 |
| `crates/matsuri-core/src/game/train.rs` | 训练结算：下层引擎公式 + 上层剧本乘区 / 封顶 + 失败率 + 训练后处理（羁绊 / 灵感 / 加练 / 追加稽古事件 / 友人点击 / 变胖速度归零） |
| `crates/matsuri-core/src/game/advice.rs` | 教诲：槽效果聚合（按绑定演目 Lv × 参与人数）/ 融合 / 分配接口 |
| `crates/matsuri-core/src/game/settle.rs` | 回合末结算：発表会 / 本番 / 合宿 +1 / 教诲窗口 / 育成终了 |
| `crates/matsuri-core/src/game/action.rs` | `Action` + `Display` + `ActionEnum` + 比赛 / 休息 / 出行 / 治病结算入口（含演目槽入账） |
| `crates/matsuri-core/src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`，含分布 / 得意率 / 训练值 / 失败率 / 终局覆写） |
| `crates/matsuri-core/src/data.rs` | 剧本资产 schema / 载入 / 校验（`scenario_matsuri.json` 18 节：+ `scenario_events` / `event_extras`；稽古两档 / S5·S6 规则常量） |
| `crates/matsuri-core/src/config.rs` | 剧本配置段（融合 / 転成预设 + 覆盖层） |
| `crates/matsuri-core/src/trainer.rs` | 样例训练员 `MatsuriTrainer` |
| `crates/matsuri-cli/src/main.rs` | 终端前端：bin `matsuri`（跑局；`matsuri` / `random` / `manual` 训练员选择） |
| `crates/matsuri-cli/src/manual.rs` | 手动核对面板（S8）：育成配置 + 彩色训练明细表（人物着色 / Hint 叹号 / 羁绊）+ 剧本资源与教诲分配结果 + 合宿（仅合宿回合）·友人链·赛程 + 动作槽增量预览 + Inquire 动作 / 事件选择 |
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
cargo run --release --bin matsuri -- manual     # 手动核对面板（动作 Inquire；Esc 退出，非 TTY 退回编号输入）

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
