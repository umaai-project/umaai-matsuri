# 领域文档（Domain Docs）

本文件规定 Agent 在探索代码库、使用领域术语及记录架构决策时的读写规范。

## 探索前先阅读以下文档

- 仓库根目录的 **`GLOSSARY.md`**
- **`docs/adr/`**：阅读与即将改动区域相关的 ADR（架构决策记录）。若 `docs/adr/` 尚无对应记录，静默继续即可。

## 目录结构

本仓采用单上下文仓库布局：

```text
/
├── GLOSSARY.md
├── docs/adr/
│   └── 0001-<status>-<decision-title>.md
└── crates/
```

## 严格使用术语表词汇

当输出内容涉及领域概念（如 Issue 标题、重构提案、假设、测试命名）时，应使用 `GLOSSARY.md` 中定义的标准术语，不要擅自替换为术语表明确避免的同义词。

如果所需的领域概念尚未收录在术语表中，通常意味着两种情况之一：要么正在引入项目未使用的自造词（需重新斟酌），要么确实存在术语缺口（记录下来以便后续补充至 `GLOSSARY.md`）。

## 显式提示 ADR 冲突

如果输出方案与现有 `Accepted` ADR 相矛盾，必须显式指出，切勿静默覆盖：

> _与 ADR-0001（<决策简述>）存在冲突，但值得重新讨论的原因是……_

## 编写与更新 ADR

做出架构决策（或用户拍板替代既有方案）后，**必须**完成以下两步：

1. **按模板新建 ADR**：在 `docs/adr/` 下创建 `NNNN-<status>-<slug>.md`（序号四位递增，状态全小写：`proposed` / `accepted` / `deprecated` / `superseded`）。
2. **同步更新被取代的旧 ADR**：若新决策推翻或取代既有 ADR，必须修改旧 ADR 正文的 `状态` 行（如 `Superseded by [ADR-XXXX](XXXX-accepted-<slug>.md)`），将旧文件重命名为 `NNNN-superseded-<slug>.md`（废弃则改为 `deprecated`），并同步更新仓内引用该旧路径的链接。

### 模板

```markdown
# ADR-NNNN: <决策标题>

- **状态**：Accepted
- **日期**：YYYY-MM-DD
- **关联**：<涉及的 crate / 模块 / 文档>

## Context

说明触发决策的问题背景、约束条件与可选边界。

## Decision

明确选定的方案、不变量与禁止事项。

## Consequences

记录该决策带来的收益、代价、后续约束或演进限制。
```
