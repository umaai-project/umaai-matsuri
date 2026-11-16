# ADR-0010: do_hint 固有效果上下文边界

- **状态**：已采纳
- **日期**：2026-11-16
- **关联**：`umasim::BaseGame::do_hint`、`SupportCard::calc_threshold_effect` / `calc_training_effect`、`umaai_next` 仓、[mechanism.md](../../mechanism.md)

## 背景（Context）

灵感次数由支援卡固有效果的词条 33（Hint 数量提升）决定，读取点在 `BaseGame::do_hint`（训练后生成灵感事件时）。该方法的上下文只有 `deck` / `persons` / `distribution`，**不是 `Game`**，拿不到 `is_shining_at` / `shining_count`，也拿不到剧本对这两个方法的覆写。

现有卡池中词条 33 只出现在 type 1 / 2（羁绊阈值型）；type 13（闪彩型）当前只加 `ganJing` / `xunLian` / `vitalCostDrop`。因此「只算阈值固有」在现有数据下是精确的，但这是一个依赖卡池的能力边界。

## 决策（Decision）

1. `do_hint` 只读 `calc_threshold_effect`：面板 + type 0 / 1·2 / 20 / 21，**不感知闪彩、训练位等动态固有**。
2. `calc_training_effect` 仍是完整入口：阈值固有 + type 13 动态叠加；训练数值 / 彩圈按它读取。
3. 禁止在 `BaseGame::do_hint` 内重新引入 `effect` 缓存，或伪造 `shining` 状态来凑完整效果。
4. 若未来动态固有（如 type 13）携带词条 33，必须先把完整效果 / 次数从 `Game` 层传入 `do_hint`（或把灵感生成上移到 `Game` 层），并先明确「闪彩」按训练结算前还是灵感生成后判定。

## 影响（Consequences）

- 当前行为精确且无缓存；灵感次数与闪彩解耦，`do_hint` 只依赖羁绊 / 编成，逻辑简单。
- 未来要支持「闪彩时 Hint 数量 +1」时，需要调整 `default_after_train` / `do_hint` 的调用链；在此之前，数据层应保持「动态固有不含词条 33」。
- 建议补一条数据守门测试：遍历 `cardDB.json`，断言 type 13 卡不含词条 33。卡池一旦变化即失败，强制回到本 ADR 重新决策。
