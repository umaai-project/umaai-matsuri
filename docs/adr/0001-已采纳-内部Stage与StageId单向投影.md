# ADR-0001: 内部 Stage 与对外 StageId 单向投影

- **状态**：已采纳
- **日期**：2026-11-15
- **关联**：`matsuri-core/src/game/{stage.rs, traits.rs}`、[ARCHITECTURE.md](../../ARCHITECTURE.md)

## 背景（Context）

引擎搜索、`View` 与在线协议依赖稳定的 `StageId` 编号假设；同时剧本内部需要细粒度的回合状态机来驱动阶段流转与演进，若直接将内部状态暴露为引擎契约枚举，后续拆分子阶段会破坏跨仓契约与旧种子稳定性。

## 决策（Decision）

内部回合状态机采用细粒度 `Stage`（`Begin → Distribute → Train → AfterTrain → End`）驱动引擎与剧本逻辑；对外则通过 `TurnPlan` 标志并**单向投影**为显式赋号的引擎契约 `StageId`（0–4）。已有 `StageId` 编号只增不改。

## 影响（Consequences）

- 允许剧本内部自由调整或拆分子阶段，而不影响引擎搜索与在线协议的契约边界。
- 修改或复用已有 `StageId` 编号会导致旧种子与契约失效，因此新增阶段只能追加编号。
