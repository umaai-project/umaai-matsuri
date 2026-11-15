# ADR-0003: MatsuriView 作为呈现层单一数据源与字段边界

- **状态**：已采纳
- **日期**：2026-11-15
- **关联**：`matsuri-core/src/game/view.rs`、`matsuri-cli/src/manual.rs`、`matsuri-ai/src/output.rs`、[ARCHITECTURE.md](../../ARCHITECTURE.md)、[online_wiring.md](../plan/online_wiring.md)

## 背景（Context）

手动核对面板、在线输出与前端回显都需要消费局面状态；若各端各自拼接内部状态或将呈现文本、候选动作预览与建局配置混入同一快照结构，会导致状态口径分叉并绑死演进边界。

## 决策（Decision）

`MatsuriView`（由 `MatsuriGame::view()` 输出）作为呈现层与在线外壳的**唯一状态数据源**，显式标记为 `#[non_exhaustive]`；名称文本、动作候选/预览与建局配置不进入 `MatsuriView`，分别由呈现层查静态表、决策上下文与配置层承担。

## 影响（Consequences）

- 保持回合快照纯粹、只读、可序列化，前后端与手动面板共享同一事实源。
- 呈现层在需要展示名称或候选信息时，必须自行结合静态表或决策上下文消费，不能要求 `MatsuriView` 承载展示文本。
