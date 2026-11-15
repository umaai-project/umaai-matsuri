# 在线帧协议与还原契约（`matsuri` / `scenarioId = 15`）

> 本文档记录小黑板（`SendGameStatusPlugin`，仓内无 C# 源码）与 `matsuri-ai` 之间的跨进程外部协议边界与特殊约定。
> 无状态还原、同内容幂等与容错原则见 [`adr/已采纳/0006-已采纳-在线无状态帧还原与协议容错契约.md`](adr/已采纳/0006-已采纳-在线无状态帧还原与协议容错契约.md)。
> Rust 解析结构见 [`crates/matsuri-core/src/game/festa.rs`](../crates/matsuri-core/src/game/festa.rs)（`FestaState` / `FestaAct`）、[`crates/matsuri-ai/src/frame.rs`](../crates/matsuri-ai/src/frame.rs) 与 [`umaai_next/crates/umaai/src/protocol/base.rs`](../umaai_next/crates/umaai/src/protocol/base.rs)。

---

## 1. 顶层信封与插件写出时机

- **顶层结构**：`{ "festa": { ... }, "baseGame": { ... } }`，其中 `baseGame.scenarioId = 15`；`baseGame` 混用 camelCase 与 snake_case，反序列化须按原名映射且**不加 `deny_unknown_fields`**。
- **写出端点**：精确端点 `load` / `start` / `exec_command` 及 8 个正则端点（`attach_advice|check_event|check_point|continue|festa_live|gain_skills|race_entry|race_out`）。
- **静默跳过**：`change_short_cut` / `finish_claw_crane` 不写；`islegal == false`（仅插件侧门禁）、`festa == null` 或单选项/待选中间态不写。
- **同回合多帧**：同回合可能写出多帧（`_2` / `_3` 后缀）。

---

## 2. 跨端协议特殊约定与约束

| 项目 | 协议约定 / 约束 |
|---|---|
| **`ReviseOver1200` 折半** | **`baseGame.training`（`lower + upper`）为表示值（≥1200 及跨 1200 部分已折半）**；`baseGame.fiveStatus` / `fiveStatusLimit` **未折半** |
| **`baseGame.turn` / `raceHistory`** | `turn` 为 **0 基**（0–77）；`raceHistory` 为已比赛回合列表（**含自选赛、含输赢**） |
| **`isAfterTraining` 粘连** | 真机存在状态粘连，不可单独判阶段，亦不可用于教诲搜索根阶段 |
| **`festa.persons[].targetId`** | 不可用 `targetId - 1` 算卡位（开新局友人占 1，中途加载友人不进表）；T0/T1 会下发 `origin == "guest"` 且 `targetId < 1000`（`101–115`）的占位项 |
| **`trainLevelTotal == 0`** | `0` 表示本帧无有效训练等级（合宿 `command` 帧及 `load` / `event` / `special` 帧整组为 0） |
| **`charaEffects` 病种序** | `1–6` 按日文文本库分类 142 顺序（`1 熬夜 / 2 懒惰 / 3 皮肤 / 4 变胖 / 5 偏头痛 / 6 不擅长`），勿按 `Ill` 位序推 |
| **指令 ID 非顺序映射** | 训练指令为 `101 速 / 105 耐 / 102 力 / 103 根 / 106 智`；稽古 `5101–5105`、転成 `601–605` 均为顺序 |
| **跨端表滞后降级** | 插件卡表滞后时 `persons[i].charaId == 0`；事件表未收录时仅发 `storyId` 而不发 `story` |
| **真机静态常量** | `maxActLevel` 恒 10、`needGauge` 恒 5、`lockedTrainingId` 恒 `-1`；`assistSupportLeft`（状态 47）仅表示有无，剩余次数由事件/消耗口径推算 |

---

## 3. 还原、路由与选档边界

- **缺段门禁与单点容错**：缺 `festa` 段直接拒绝为非骏大祭帧；未知与零值严格区分；越界或无法映射项单点丢弃并记 `diag!`，不整帧失败（实现见 `crates/matsuri-core/src/game/restore.rs`）。
- **路由与阶段定档**（实现见 `crates/matsuri-ai/src/{online.rs,session.rs}`）：
  - `stale_snapshot` 仅拦截「`training` 预览非零且 `personDistribution` 全 `-1`」的矛盾快照。
  - `source == "special"` 仅当 `playing_state == 51` 且命中 `plan.advice_window` 时走 `Route::Advice(window)`，其余跳过；`source == "event"` 若本地事件表查不到 `storyId` 则直接跳过、**绝不回落训练**。
  - 入口阶段：有决策面（人头落位或预览非零）或 `!isAfterTraining` 落 `Train`，否则落 `AfterTrain`。
- **在线选档**：按帧内 `baseGame.cardId` 构成自动匹配 `data/matsuri_config.toml` 的 `[decks.*]`，构成变化时重建训练员。

---

## 4. `--verify-training` 与 G13 铁律

- **G13 铁律**：`baseGame.training` **仅用于 `--verify-training` 离线/在线验算**，严禁参与 `restore` 与 rollout 决策。
- **验算口径**：合宿期 `training` 仍带普通训练占位假值且跨 1200 轴已按 `ReviseOver1200` 折半，比对前须结合未折半的 `fiveStatus` 经 `restore_displayed_gain` 还原；事件/结算帧全 0 预览跳过比对。
