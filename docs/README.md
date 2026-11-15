# 骏大祭剧本仓文档索引

> 本目录记录本剧本（`matsuri` / scenarioId `15`）的机制、术语、计划、决策与验证；通用接入规范见 `umaai_next/docs/scenario/`。

## 1. 本仓文档

| 文档 | 职责 | 触发条件（何时读/改） |
|---|---|---|
| [`mechanism.md`](mechanism.md) | 机制公式、流程、实现落点与测试对应 | 核对或修改剧本机制与估值口径时 |
| [`matsuri_memo_cn.md`](matsuri_memo_cn.md) | 上游中文机制备忘与未决问题清单 | 查验或更新原始机制情报与未决项时 |
| [`../GLOSSARY.md`](../GLOSSARY.md) | 领域术语与官方译名的单一事实源 | 新增、修正术语或译名时 |
| [`adr/`](adr/) | 架构决策记录 | 评估、做出或推翻架构决策时 |
| [`changelog.md`](changelog.md) | 按日期升序的口径裁定与历史分歧日志 | 每次口径拍板或重录基线时 |
| [`tests_overview.md`](tests_overview.md) | 分 crate 测试面、基线快照与验收说明 | 增删测试、重录基线或排查验收时 |
| [`perf_profiling.md`](perf_profiling.md) | 性能测量工具、热点拆解与复现命令 | 性能分析或优化轮次时 |
| [`plan/scenario_dev_plan.md`](plan/scenario_dev_plan.md) | 阶段状态、决策记录与待决事项 | 推进阶段或查阅全局进度时 |
| [`plan/intake.md`](plan/intake.md) | S0 接入情报、数据表字典与待确认清单 | 核对数据表索引或原始接入口径时 |
| [`plan/stubs.md`](plan/stubs.md) | 数值占位（`STUB` / `TBD`）登记与替换条件 | 新增、排查或替换占位参数时 |
| [`plan/deck_config.md`](plan/deck_config.md) | `[decks.*]` 配置 schema、分层合并与匹配规则 | 调整卡组配置结构或自动选档时 |
| [`plan/deck_param_scan.md`](plan/deck_param_scan.md) | 四套 build 定卡、扫参顺序与调参入口 | 推进定卡、扫参或落档调参结论时 |
| [`plan/online_wiring.md`](plan/online_wiring.md) | 在线帧还原口径、协议路由与两端改动清单 | 修改在线监听、帧解析或面板契约时 |
| [`plan/release_launcher.md`](plan/release_launcher.md) | 发布启动器（`matsuri-ai`）计划与实现边界 | 变更启动器菜单、更新频道或打包发布时 |
| [`plan/review_tool_plan_2026-11-13.md`](plan/review_tool_plan_2026-11-13.md) | 复盘工具与局包协议设计及指标口径 | 修改 `review` 子命令或局包字段时 |
| [`../skills/matsuri_review/SKILL.md`](../skills/matsuri_review/SKILL.md) | 局包复盘执行规范与报告工作流 | 执行或调整复盘技能流程时 |
| [`plan/search_sh.md`](plan/search_sh.md) | 搜索预算分配（Successive Halving）设计稿 | 评审或实施搜索分配改造时 |
| [`plan/round_2026-11-09.md`](plan/round_2026-11-09.md) | 历史轮次修改与待办快照（归档参考） | 追溯 2026-11-09 轮次上下文时 |
| [`handoff.md`](handoff.md) | 长会话交接文档 | 需交接给下一位 Agent 时 |

## 2. 引擎通用文档（`umaai_next/`）

| 文档 | 内容 |
|---|---|
| [`../umaai_next/docs/scenario/README.md`](../umaai_next/docs/scenario/README.md) | 接入指南：建仓 / 依赖 / 纪律 / 常见坑 |
| [`../umaai_next/docs/scenario/repo_form.md`](../umaai_next/docs/scenario/repo_form.md) | 剧本仓形态：crate 分工 / 数据配置 / 引擎钩子 / 文档 / 测试 / 分面 / 验收 |
| [`../umaai_next/docs/scenario/dev_process.md`](../umaai_next/docs/scenario/dev_process.md) | 开发流程：S0 机制核对 → S1–S9 → 扩展轨 / 性能优化轨 |
| [`../umaai_next/docs/scenario/perf_plan.md`](../umaai_next/docs/scenario/perf_plan.md) | 性能优化：工具选择 / 测量协议 / 手段 / 阶段 |
| [`../umaai_next/docs/scenario/contracts.md`](../umaai_next/docs/scenario/contracts.md) | 契约逐项清单（`Game` / `Person` / `ActionEnum` / `Trainer` / `Frontend` / 装配 / `StageId` / View） |
| [`../umaai_next/docs/notes/matsuri_scenario_preview.md`](../umaai_next/docs/notes/matsuri_scenario_preview.md) | 预告情报汇总（开发代号 `matsuri`；持续更新的未确认情报） |

冲突优先级：契约以引擎 [`contracts.md`](../umaai_next/docs/scenario/contracts.md) 为准，领域术语以 [`../GLOSSARY.md`](../GLOSSARY.md) 为准，口径裁定以 [`changelog.md`](changelog.md) 为准。

## 3. 写入边界

- **单一归属**：机制结论写 `mechanism.md`，口径裁定写 `changelog.md`，架构决策写 `adr/`（规范见 [`agents/domain.md`](agents/domain.md)），未确认机制标 `❓`，数值占位记入 `plan/stubs.md`，不互相抄录。
- **`changelog.md` 只追加**：不删改历史行；被取代的结论在原「决定」列加注修正去向，并填写「基线影响」。
- **相对路径**：文档内链接使用仓内相对路径，不写本机绝对路径。
