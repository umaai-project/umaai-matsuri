---
name: matsuri_review
description: 骏大祭（scenarioId 15）局包赛后复盘：事实层 open → 单点 eval → curve → report，按判据给结论、不编数。
---

# 骏大祭赛后回看

## 什么时候用

用户给出一局真机局包（`game{id}.zip` 或散帧目录），要「看看这局打得怎么样」。**不是**用来评价玩家水平、不做排名与社会比较。

## 工作流（按顺序，不要跳）

1. **定位局包**：默认 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/game{id}.zip`；散帧目录同路径。
2. **`review open <局包> [--out <目录>]`**：零重算，产出 `facts.json` 与 `brief.md`。**先读 `brief.md`**（一次 Read 就够，约 10KB）。
3. **按需 `review eval`**：只在需要「选择偏差」时用（信条：`pending` 不编数）。
   - 单点：`review eval <局包> --turn N [--seq k] [--n 1024]`
   - 全量：`review eval <局包> --all --n 256`（78 点约 1 分钟）
4. **`review curve`**：区间 T(n) 曲线与形态，只在需要看走势时跑。
5. **`review report [--narrative md]`**：合并 facts + 已缓存 eval/curve → `digest.json` + `report.html`（4 图）。

## 纪律

- **事实与叙事分开**：工具只出事实；结论只在对话里写。不把 `pending` 当已测。
- **选择偏差**用配对 95% 区间：`CI 跨零 = 同等优解`，只有显著才写「这里有一条更优路线」。
- **不做 AI 估分 vs 实际终局对照**，也不做在线/重算轨迹一致性——只作调试 hint。
- **AI 陪跑局（有 `ai-decision`）**：regret 大量为 0 只说明重算与在线一致，不能读成「没问题」；也不能当手动操作证据。
- 数据引用一律带回合号与数值，与 `digest.json` 一致；不为了气势编数字。

## 命令速查

    review open   <局包|目录> [--game id] [--out 目录] [--inherit-expect 400]
    review eval   <局包> --turn N [--seq k] [--n 1024] [--force]
    review eval   <局包> --all [--n 256]
    review infer  <局包> [--what advice,fusion,skill,kensan|all] [--turn N] [--json]
    review curve  <局包> [--from A] [--to B] [--n 1024]
    review report <局包> [--narrative narrative.md]

产物都在 `<out>/` 下：`facts.json / brief.md / infer.json / curve.json / digest.json / report.html / recompute/turn*.json`。

## 常见追问

- 「哪一回合最亏？」→ `eval --all` 里挑 `regret > ci95` 且 `ci95` 最小的点。
- 「教誨/転成是不是选错了？」→ `infer --what advice,fusion` + 对应窗口 `eval` 点。
- 「为什么终局分比期望低？」→ `curve` 末点 T(n) vs 终局分（`final_gap`），再看阴跌窗口。
- 「出行/演目节奏对不对？」→ `brief.md§8`（段回合、节点、卡位），不判快慢（无基线分位）。

## 不做

- 不给「水平评级」，不做跨玩家比较。
- 不重打插件、不合成不存在的字段（帧里没有名次/友情数量/事件数值）。
