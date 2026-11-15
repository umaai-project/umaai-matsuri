# ADR-0008: FlatSearch 搜索范式与无叶估基策

- **状态**：已采纳
- **日期**：2026-11-05
- **关联**：`matsuri-ai`、[mechanism.md](../mechanism.md)、[search_sh.md](../plan/search_sh.md)

## 背景（Context）

需要为剧本提供在线/离线搜索决策能力。若从零自建 MCTS 树或依赖深层叶子估值网络，开发与调试成本高、且树内递归搜索容易造成算力爆炸；同时，高尾部激进加权（`radical_factor > 0`）会扭曲正常期望排序。

## 决策（Decision）

1. **采用引擎 `FlatSearch` + 手写基策**：`MatsuriSearchTrainer` 封装引擎 `FlatSearch<MatsuriGame>`，rollout 基策固定为手写 `MatsuriTrainer`（无搜索，防递归），暂不自建 MCTS 树。
2. **算术均值排序与无叶估**：设置 `radical_factor_max = 0`（取算术均值，不做高尾加权）；仅预留 `LeafEvaluator` 接口，生产固定 `max_depth = 0`。
3. **动作空间控制与预算基线**：融合（転成）走配置预设不进搜索树；教诲 120 候选先由手写估值剪枝 top-16 再搜索；默认生产预算定为每候选 `search_n = 8192`（实测 8192 与 12288 的 95% 最小可分差落入噪声内），引擎 `ActionResult` 按样本存储以消除直方图截断。

## 影响（Consequences）

- 搜索逻辑与手写策略严格解耦，既保留了手写策略可单独 `--explain` 的可解释性，又显著降低了单局方差（a-normal σ 降至手写约 1/2）。
- 若未来引入 `LeafEvaluator` 或改用 Successive Halving（见 `search_sh.md`），需新增/取代本 ADR 并重录搜索基线。
