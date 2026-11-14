# 内部 Stage 与对外 StageId 单向投影

内部回合状态机采用细粒度的 `Stage`（`Begin → Distribute → Train → AfterTrain → End`）驱动引擎与剧本逻辑，对外则通过 `TurnPlan` 标志并单向投影为引擎契约 `StageId`（0–4，序号只增不改）。这样既允许剧本内部随时拆分子阶段，又不破坏引擎搜索、`View` 与在线协议对 `StageId` 编号稳定性的假设。
