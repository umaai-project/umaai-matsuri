# 口径速查（matsuri）

## 分数

- **T(n)**：某回合局面的期望终局分（官方口径，搜索加权均分扣掉 PT 放大）。`eval` 的 `t_n`。
- **score**：逐帧 `search_score(1.0)` = `skill + pt + 五维得分`；swings 用它分解。
- **显示值**：五维 >1200 部分减半（呈现用）；帧 `fiveStatus` 是真值，不折半；只有 `baseGame.training` 预览是折半表示值。

## 决策

- **regret** = `best.score − chosen.score`（加权分口径）。
- **配对 CI**：逐 rollout 序号求 `best_k − chosen_k`，`mean ± t·sd/√n`；跨零 = 同等优解。
- **chosen 反推**：`5101–5105` 稽古、`301`/`command_type 3` 休息、`race_entry` 比赛、`101–106` 用训练预览与实际增量反查训练位。

## 反推（帧 diff，标「反推」不标实测）

- 教诲落位：按回合**末帧**槽位首现新 id；転成：`fusionId` 首现；学技能：帧原值 `skillPt` 下降；追加稽古：帧 `storyId = 400015106`。

## 判据（facts.checks）

- **gate**：B1 発表会未大成功、B6 体力线、B7 干劲掉落、C3 训练失败、C4 关键资源。
- **warn**：数据健康（无终局 / 缺帧 / 解析失败 / 无 `review` 段）、出行 T23 前 <4 段、T36 前未走满。
- **info**：合宿延长、E7 自选赛未打满、助力支撑剩余。
- 判据 (1) 演目：带卡位才判快慢（需基线分位，当前无 → 只陈述节点）；(3) 阴跌窗口只陈述；(4) 继承只对照标准 400/次；(6) 状态资源不判失效。

## 状态标记

- `pending`：能算未算（要 `eval`）；`recomputed`：事后重算（标 N）；`online`：在线记录（本仓暂未接）。
- 不同 N 不混比；`recomputed` 与 `online` 分开标。
