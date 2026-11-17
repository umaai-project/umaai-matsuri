# 坑位（matsuri 复盘）

1. **多帧 = 多决策面，不是重复记录**。同一回合常见三帧（事件 / 训练 / 教诲）；`eval` 单点在多面时
   不带 `--seq` 会直接报错并列出 `seq` 与面名。静默取第一帧会把「教诲回合」评成「事件回合」。
2. **教诲帧是 `special + playing_state=51`**。T35 的例子：`turn35` 事件、`turn35_2` 训练、
   `turn35_3` 教诲。没有这帧的局（如某些 AI 直接分配的局）教诲 whatif 不可用，属采集缺失。
3. **教诲帧跨训练前后**。`turn35_2`（`isAfterTraining=false`）→ `turn35_3`（`true`）之间是训练收益
   （速 +218、技能点 +105、演目 Lv+1），不是「教诲让期望跳了」。判教诲效果要看同点换排列的 Δ。
4. **`score` 不是期望分**。`brief` / `facts` 的 `score` 是当前局面评分（万级）；期望终局分是
   `eval.t_n`（八万级）。混用会得出「T35 评分跳了 1.5k = 运气」的错误结论。
5. **`eval` 的「不可判」多数是剪枝**。教诲 120 排列剪到 16，实际排列不在候选里就算不出 regret；
   这不是反推失败，用 `whatif` 强制该动作即可对照。
6. **`skillPt` 双口径**：`view.uma.skill_pt` = 帧原值 + `skillPtSpent`（累计获得）；
   学技能反推要用 `ParsedFrame.status.skill_pt` 的**原值下降**。
7. **训练预览只在训练前帧有效**：`isAfterTraining=true` 的帧 `failureRate` 全 0，B6 只能用同回合训练前帧。
8. **事件帧的训练等级是残值**：`source=event` 时 `trainLevelTotal` 被整组置 0；B1 用结算前帧的
   `act_level` 自判，不读 `resultType`。
9. **`whatif` 的边界**：事件回合不支持；只有 1 个候选的回合（强制比赛 / 本番）直接拒绝；
   教诲动作名用槽位映射描述（`速←助力 …`），与 `eval` 候选同口径。
10. **`whatif` 的基线是「历史实际」**，反推失败才降级策略默认（`actual_source=default`）；
    非 AI 局里「策略默认」不代表玩家当时的选择。
11. **缓存按 `turn{NN}_{seq}` 区分**：`recompute/` 下同回合可能多文件；不同 `N` 不混比，
    `--force` 才重算。
12. **命令行中文与日志编码**：PowerShell 的 `*>` 重定向是 UTF-16LE，直接读会乱码；
    用 `Get-Content -Encoding Unicode` 或在脚本里按 BOM 解码。
13. **验证前先重建 skill 二进制**：改过 review 代码后要重跑构建脚本更新 `bin/review.exe`，
    否则跑的是旧版本（旧 schema / 旧行为）。
14. **局包与散帧**：`discover` 只认 `game{id}_turn{N}[_{k}].json` 与 `finalScore`，包内其它条目静默忽略。
