# 骏大祭剧本仓文档索引

> 本目录记录本剧本（`matsuri` / scenarioId `15`）的机制、计划、决策与验证；通用接入规范见 `umaai_next/docs/scenario/`。

## 1. 本仓文档

| 文档 | 职责 | 触发条件（何时读/改） |
|---|---|---|
| [`mechanism.md`](mechanism.md) | 当前生效的机制公式、流程、实现落点与测试对应（含 `[decks.*]` 与外部评分参考） | 核对或修改剧本机制与估值口径时 |
| [`online_protocol.md`](online_protocol.md) | 在线帧协议（`festa` / `baseGame`）、还原规则与 `--verify-training` 验算口径 | 修改在线监听、帧解析或面板契约时 |
| [`release.md`](release.md) | 发布启动器（`matsuri-ai`）目录布局、四频道更新与签名发版流程 | 变更启动器菜单、更新频道或打包发布时 |
| [`matsuri_memo_cn.md`](matsuri_memo_cn.md) | 上游中文机制原始备忘 | 核对上游一手情报时 |
| [`adr/`](adr/) | 架构与核心算法范式决策记录（规范见 [`agents/domain.md`](agents/domain.md)） | 探索架构边界、评估或推翻架构/算法范式决策时 |
| [`changelog.md`](changelog.md) | 按日期升序的历史裁定与基线位移流水账（非日常查阅入口） | 仅在拍板新口径/重录基线时追加，或需追溯历史裁定原因时按关键词检索 |
| [`tests_overview.md`](tests_overview.md) | 分 crate 测试面、基线快照与验收说明 | 增删测试、重录基线或排查验收时 |
| [`perf_profiling.md`](perf_profiling.md) | 性能测量工具、热点拆解与复现命令 | 性能分析或优化轮次时 |
| [`todo.md`](todo.md) | 跨阶段收拢的真实未完成功能待办清单 | 认领待办或规划下一轮开发时 |
| [`intake.md`](intake.md) | S0 `master.mdb` 数据表字典与资产提取映射 | 核对数据表结构或重导出资产时 |
| [`stubs.md`](stubs.md) | 数值占位（`STUB` / `TBD`）与待真机核对项登记 | 新增、排查或替换占位参数时 |
| [`search_sh.md`](search_sh.md) | 搜索预算分配（Successive Halving）设计稿 | 评审或实施搜索分配改造时 |
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

## 3. 文档总则

- **真值优先级**：[`contracts.md`](../umaai_next/docs/scenario/contracts.md) > [`../GLOSSARY.md`](../GLOSSARY.md) > [`adr/`](adr/) > [`mechanism.md`](mechanism.md)。
- **单一事实源**：按 §1 职责分工落盘，不互相抄录；未确认机制标 `❓`。
- **相对路径**：文档内链接统一使用仓内相对路径，不写本机绝对路径。
