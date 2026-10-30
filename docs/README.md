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
| [`mechanism.md`](mechanism.md) | 机制文档：公式 / 流程 / 实现落点 / 测试对应 | 机制实现或修正 |
| [`matsuri_memo_cn.md`](matsuri_memo_cn.md) | 中文机制备忘：按机制类型逐条记录（骨架 / 演目 / 排练 / 教诲 / 汇报会 / 合宿 / 比赛 / 事件）；**§2.2 为担当编排规则**（五步流水线 + 卡落位 / NPC 摊法 / 追加负责人 / 形态穷举）；**§3.5 为分身机制**（教诲 +1 人的落点与打法）；§14 为 **未决清单**（只列还没定的 19 条：`△` 2 / `□` 17，P0 仅 U02；已定 21 条 + 其他已核实项移到 §14.4 / §14.5 备查） | 上游日文备忘或真机口径更新 |
| [`glossary.md`](glossary.md) | 术语单一事实源 + 口径与用语决定记录 | 每次口径决定 |
| [`tests_overview.md`](tests_overview.md) | 测试一览（单元 / 机制 / 冒烟 / 基线） | 测试增删 |
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

- 本仓文档与引擎文档冲突时：**契约以引擎 `contracts.md` 为准**，本剧本口径以本仓 `glossary.md` 为准。

## 3. 更新纪律

- 机制结论先落 `mechanism.md`，术语与口径落 `glossary.md`；两者不互相抄。
- 未确认的机制不写成结论（标 `❓`），数值占位进 `stubs.md`。
- 数值占位替换后重录基线，并在 `glossary.md` 的决定记录写明影响面。
- 文档不放绝对路径；链接用仓内相对路径或引擎文档占位路径。
