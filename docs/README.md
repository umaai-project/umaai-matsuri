# 骏大祭剧本仓文档索引

> 本目录记录**本剧本**（`matsuri` / scenarioId 15）的机制、术语、决策与进度。
> 通用方法（仓库形态 / 开发流程 / 性能优化）的权威版在引擎仓；开发期直接读引擎仓原文，
> 稳定期按 git tag 钉版本。

## 1. 本仓文档

| 文档 | 内容 | 何时更新 |
|---|---|---|
| [`plan/scenario_dev_plan.md`](plan/scenario_dev_plan.md) | 开发计划：阶段状态 + 决策记录 + 待决事项 | 阶段推进、口径拍板 |
| [`plan/intake.md`](plan/intake.md) | 接入包：情报位置、机制清单、数值清单、数据表清单、待确认问题 | S0 核对期间 |
| [`plan/stubs.md`](plan/stubs.md) | 数值占位登记（`STUB` / `TBD`）：位置、来源、替换条件、基线影响 | 出现占位时 |
| [`plan/deck_config.md`](plan/deck_config.md) | **卡组配置方案**：`[decks.*]` schema / `DeckParams` 分层 / default 与 variant / 深合并与匹配 / 迁移清单 | 卡组参数或配置段变更时 |
| [`plan/deck_param_scan.md`](plan/deck_param_scan.md) | **卡组参数扫描方案**：四套 build 的定卡（单卡控制变量 + 类型排名）、speed 换卡与重录基准、`fusion` / `advice` / 手写旋钮的扫参顺序、测试专用 `--matsuri-config` 入口 | 定卡 / 扫参推进、结果落档时 |
| [`plan/online_wiring.md`](plan/online_wiring.md) | **在线接线方案**：`MatsuriGame` 剧本专属状态盘点与还原来源分类（协议直读 / 插件收集 / 插件计算 / 本侧重算）、`thisTurn.json` 的 `matsuri` 段草案、两端改动清单与待核清单 `G1–G10` | 在线外壳（S8）推进、`G#` 有结论时 |
| [`plan/round_2026-11-09.md`](plan/round_2026-11-09.md) | **本轮修改与待办**：已改项 / 验证结果 / 已定位但未修改项 / 基线快照 / 后续顺序 | 每轮收尾 |
| [`plan/search_sh.md`](plan/search_sh.md) | **搜索分配方案（设计稿，未实施）**：UCB 不等样本终局倒挂、固定组 Successive Halving / Racing、单旋钮 `search_n=4096`、引擎/剧本落点与验收计划 | 方案评审 / 实施时 |
| [`plan/review_tool_plan_2026-11-13.md`](plan/review_tool_plan_2026-11-13.md) | **复盘工具开发计划（替代 11-06 方案）**：插件端局包归档与数据补充（actions 扩白名单 / meta / 时间戳）、剧本仓记录层与 `review` bin（`open` / 按需 `eval` / `curve` / `report`）、`pending` 契约与缓存、指标口径与检查项、技能与阶段验收；**引擎零改动** | 复盘推进、插件补数拍板 |
| [`plan/release_launcher.md`](plan/release_launcher.md) | **发布启动器（`matsuri-ai`）计划 + 实现记录**：完整版布局 / 菜单计划、§0 基础版差异、§0.1 数据更新（`ai_data` / `ura_data`）、§0.2 程序更新与 Authenticode 签名（`program` 频道整包 zip）；核心 CLI 不改；不做 GUI | 发布形态变更时 |
| [`mechanism.md`](mechanism.md) | 机制文档：公式 / 流程 / 实现落点 / 测试对应 | 机制实现或修正 |
| [`matsuri_memo_cn.md`](matsuri_memo_cn.md) | 中文机制备忘：按机制类型逐条记录（骨架 / 演目 / 排练 / 教诲 / 汇报会 / 合宿 / 比赛 / 事件）；**§2.2 为担当编排规则**（五步流水线 + 卡落位 / NPC 摊法 / 追加负责人 / 形态穷举）；**§3.5 为分身机制**（教诲 +1 人的落点与打法）；§14 为 **未决清单**（P0–P2 未定 13 条：`△` 3 / `□` 10，P0 已清空（U02 收口）；§14.8 另有 U41 / U44 / U45；已定条与已核实项移到 §14.4 / §14.5 备查） | 上游日文备忘或真机口径更新 |
| [`glossary.md`](glossary.md) | 术语与官方译名的单一事实源（词条表 §1–§8 + 译名来源 §9） | 术语 / 译名变动 |
| [`changelog.md`](changelog.md) | **口径与决定变更记录**：按日期升序的裁定日志 + 与旧实现 / 资料的分歧 | 每次口径拍板、基线重录 |
| [`tests_overview.md`](tests_overview.md) | 测试一览（单元 / 机制 / 冒烟 / 基线） | 测试增删 |
| [`perf_profiling.md`](perf_profiling.md) | 性能分析与优化：测量工具 / 热点拆解 / 改动清单 / 配对测量 / 复现命令 | 性能轮次、测量工具变化 |
| [`handoff.md`](handoff.md) | 交接提示词（上下文很长时） | 用户同意交接时 |

## 2. 引擎通用文档（权威版在引擎仓 `../umaai_next`）

| 文档 | 内容 |
|---|---|
| [`../../umaai_next/docs/scenario/README.md`](../../umaai_next/docs/scenario/README.md) | 接入指南：建仓 / 依赖 / 纪律 / 常见坑 |
| [`../../umaai_next/docs/scenario/repo_form.md`](../../umaai_next/docs/scenario/repo_form.md) | 剧本仓形态：crate 分工 / 数据配置 / 引擎钩子 / 文档 / 测试 / 分面 / 验收 |
| [`../../umaai_next/docs/scenario/dev_process.md`](../../umaai_next/docs/scenario/dev_process.md) | 开发流程：S0 机制核对 → S1–S9 → 扩展轨 / 性能优化轨 |
| [`../../umaai_next/docs/scenario/perf_plan.md`](../../umaai_next/docs/scenario/perf_plan.md) | 性能优化：工具选择 / 测量协议 / 手段 / 阶段 |
| [`../../umaai_next/docs/scenario/contracts.md`](../../umaai_next/docs/scenario/contracts.md) | 契约逐项清单（`Game` / `Person` / `ActionEnum` / `Trainer` / `Frontend` / 装配 / `StageId` / View） |
| [`../../umaai_next/docs/notes/matsuri_scenario_preview.md`](../../umaai_next/docs/notes/matsuri_scenario_preview.md) | 预告情报汇总（开发代号 `matsuri`；持续更新的未确认情报） |

- 本仓文档与引擎文档冲突时：**契约以引擎 `contracts.md` 为准**，本剧本术语以本仓 `glossary.md`、
  口径决定以本仓 `changelog.md` 为准。

## 3. 更新纪律

- 机制结论先落 `mechanism.md`，术语落 `glossary.md`，口径裁定落 `changelog.md`；三者不互相抄。
- 未确认的机制不写成结论（标 `❓`），数值占位进 `stubs.md`。
- 数值占位替换后重录基线，并在 `changelog.md` 的决定记录写明影响面。
- 文档不放绝对路径；链接用仓内相对路径或引擎文档占位路径。
