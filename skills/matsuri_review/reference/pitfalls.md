# 坑位（按踩过的顺序）

1. **`skillPt` 两个口径**：帧原值是「剩余」，`view.uma.skill_pt` 是「剩余 + 本局累计花掉」；学技能反推只能用前者。
2. **训练预览只在训练前帧有效**：`isAfterTraining` 帧的 `failureRate` 常全 0；B6 只用训练前帧。
3. **命令编码不可猜**：`101–106` 与训练位不是顺序对应（102 涨耐力也涨力量…）；要 anti-guess，用 `preview_training_layers` 与实际增量反查。
4. **同回合多帧**：同一回合可能有 `_2/_3`（训练屏 / 训练后 / 事件），反推按 `(turn, seq)` 时序，落位取回合末帧。
5. **事件对齐**：帧 `storyId`（如 830322002）与 actions 的 `event_id`（如 10002）不同源；同回合多条 `check_event` 时实际选择不可判，不要硬匹配。
6. **`choice_number` 是 1 基**（两选项给 2），映射候选下标要回退。
7. **1200 折半**：帧五维是真值；只有 `training` 预览是表示值，比较前用 `restore_displayed_gain` 还原。
8. **事件帧 / FromCheckEvent 帧**：`festa.trainLevelTotal` 会被整组置 0，B1 用结算前帧的 `act_level` 自判。
9. **AI 陪跑局**：`regret = 0` 是「重算与在线一致」，不是「打得好」；且不能当手动节奏证据。
10. **强制比赛回合**：候选只有 1 个 → 走手写 fallback、无搜索分，`eval` 会标不可判，不是工具坏。
11. **缓存**：`recompute/turn*.json` 带 `n/seed/version`；改 N 或升级版本会失效重算，不同 N 不可混比。
12. **追加稽古的 choices**：插件端没有 400015106 这个事件（AI 端补），actions 里查不到 `check_event` 属正常，不要当缺口。
