# 会话交接

> 面向接续：只写当前状态、未决项与下一步，细节引用其他文档与 commit。

## 1. 当前目标

回到 **Hint 公式修正**：把模拟的 hint 产出与折算对齐真机。H1/H2 已定、H3 已实施，H5（每卡上限）与 H6（继承 hint）未动；事件侧统计已完成、EventLogger 与模拟常量均已对齐（`card_event` 0.31、`no_event_turns` 补 4 回合）。

## 2. 已完成

- **H1**：`hint_attr = 0.167` 是后期标定值，文档原 25% 已改为 16.7%（两份 `mechanism.md`）。
- **H2**：`hint_level = min(5, 1 + 卡面 hintLevel)` 与游戏一致，不改。
- **H3 已实施（未提交）**：`distribute_hints` 改为按训练位判定——**全亮** = 该位排练 + 编成 5 种卡（不含 Lv）；**全结算** = 再加该演目 `all_hint`（Lv≥4），每个训练位独立。手写一局 H 365→359、评分 84333→84227；T29=136 仍高于真机 80，主体不在 H3。
- **一局 hint 分解**：训练 `#101` 284、事件 8002/8003 45、友人 8、初始 21 → 359；第一年 +70 = 训练 44 + 事件 26（8002/8003 21 + 友人 5）。
- **真机基准**：6330 H=383（非继承 233 + 继承 150）、6334 H=370（非继承 259 + 继承 111）；11 局 H 368–442。模拟 20 局（H3 后）H 295–358（均值 332），无继承。
- **事件统计（49 局）**：每卡独立 3 段、特殊卡（30244/30258/30270/30300/30324）1 段；打满率 65.3%、Y1 6.16、`card_event` p=0.31（可触发 48 回合）、`no_event_turns` 需补 0-based 1/2/12/68。
- **事件常量已落地（2026-11-20）**：`data/constants.json` `card_event` 0.35→0.31、`no_event_turns` 补 0-based 1/2/12/68；20 局手写档（seed 73200）配对 N 14.30→12.90、Y1 6.75→5.10、8003 4.55→3.50；对照真机 49 局 meanN 12.73、Y1 6.16。
- **H5/H6 已实施（2026-11-20）**：每卡上限 = 5 × 技能数（`data/card_skills.json`；友人卡池 0 不发技能灵感）；继承 `[pre.inherit]` 7 项（折扣等级 70/次，`cover = 0.5` 容量近似，`mode = bucket|flat`，`scale = 1.0` 预设）；20 局终局 H 均值 335→442（继承两跳 4–139/次）。
- **分桶量化（2026-11-20）**：`tmp_probe_hint_bucket` 按「每卡技能池 × 每技能封顶 5、只随机未满级」重放灵感事件流——a-normal 20 局损耗 5.8%、6330 卡组 10 局 13.0%，前 30 回合 ≤6%；封顶不足以解释合宿段差异，暂不改主模型。
- **EventLogger 已改并提交**（`f5497ae`）：`ScenarioRules.cs`（15 用 0.31、排除 1-based 2/3/13/69）、`SeriesStepCount`、`card_event.json` v2（runs/runId/finished）、finish 端点、`IsEventBreaking` 移植。

## 3. 未决 / 阻塞

1. **继承分配待真机校准**：`cover = 0.5`、折扣等级 70/次是首版假设；20 局终局 H 442 高于真机 11 局 368–442 的均值，且非继承部分仍偏高（模拟 335 vs 6330 非继承 233）。
2. **饱和模型**：用户决定不做技能级分发，改拟合（加继承 +125，`hint_pt_rate` 6.5→5.75）尚待确认。
3. **模拟连续事件仍是全局计数**（8001/8002/8003 各 5 次、合计上限 15/局），与实机每卡独立 3 段（特殊卡 1 段）不符；8003 段受「后段不得多于前段」约束被 Y1 压缩（新常量后 4.55→3.50）。
4. **H3 改动未提交**：`distribute.rs`、`train.rs`、两份 `mechanism.md` 仍在工作树。

## 4. 下一步

1. 确认后提交 H3。
2. 连续事件改为**每卡独立 3 段 + 特殊卡 1 段**，概率 0.31，对齐 EventLogger 口径。
3. 继承分配校准：对 6330 卡组跑 20 局，对比 T30/T54 跳变与终局 H；必要时调 `cover` / 折扣等级。
4. 饱和模型：继承落地后重估 `hint_pt_rate`（原拟合 5.75 待 A/B 验证）。

## 5. 关键入口

| 用途 | 位置 |
|---|---|
| 总分/折算公式 | `umaai_next/crates/umasim/src/game/uma.rs`（`total_pt` / `score_parts`） |
| hint 发放 | `crates/matsuri-core/src/game/distribute.rs`、`train.rs`（`after_train` / `do_hint_with_bonus`） |
| 常量 | `data/constants.json`（`base_hint_rate=7.5`、`hint_attr=0.167`、`max_hint_per_card=60`、`pt_score_rate=1.88`、`hint_pt_rate=6.5`） |
| 事件数据 | `data/events.json`（8002/8003）、`data/scenario_matsuri.json`（`act_effects`、`advices[301]`） |
| 机制文档 | `docs/mechanism.md`、`umaai_next/docs/mechanism.md` |
| 实机事件数据 | `Matsuri_AI/.portable/PluginData/EventLoggerPlugin/card_event.json`（旧版 `events.json` 停写） |
