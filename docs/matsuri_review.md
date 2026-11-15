# 骏大祭局包赛后复盘（matsuri_review）

> 真机局包（`game{id}.zip` 或散帧目录，默认位于 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/`）赛后事实复盘指南。协议边界见 [`online_protocol.md`](online_protocol.md)，剧本机制见 [`mechanism.md`](mechanism.md)，实现与内部反推细节见 [`crates/matsuri-tools/src/review/`](../crates/matsuri-tools/src/review/)。

## 1. 工作流

入口：`cargo run --release -p matsuri-tools --bin review -- <子命令>`（无参打印完整参数）。产物均落 `<out>/`：

1. **`open <局包|目录>`（必选，零重算）**：产出 `facts.json` 与 `brief.md`；先读 `brief.md`（约 10KB）掌握全貌。
2. **`eval <局包>` / `infer <局包>`（按需）**：仅在排查选择偏差或教诲/技能反推时运行（单点 `--turn N` 默认 `--n 1024`；全量 `--all` 建议 `--n 256`）。
3. **`curve <局包>`（按需）**：查看区间 `T(n)` 走势与 `final_gap`。
4. **`report <局包>`（汇总）**：合并 `facts` 与已缓存结果生成 `digest.json` 与 `report.html`。

## 2. 复盘纪律与口径

- **表述规范**：只基于 `brief.md`、`facts.json`、`digest.json` 给结论（正向先行，带回合号与数值，结尾落一个下一局可执行动作）；不对玩家水平评级或做跨玩家比较。
- **统计与状态**：选择偏差 `regret = best.score − chosen.score` 按配对 95% CI 判优劣（`CI 跨零 = 同等优解`）；`recomputed` 须标明 `N` 且不同 `N` 不混比；`pending` 项先跑 `eval` 再下结论。
- **对比边界**：不做「AI 估分 vs 实际终局」或「在线/重算轨迹一致性」对比；AI 陪跑局中 `regret = 0` 仅说明重算与在线一致；继承仅对照 `400/次` 标准，不下 verdict。
- **帧 diff 反推**：结论须标「反推」；教诲落位取回合末帧、转成取 `fusionId` 首现、学技能用帧原值 `skillPt` 下降（区别于含已花累计的 `view.uma.skill_pt`）、追加排练取 `storyId = 400015106`。

## 3. 追问路由与正常边界

| 场景 | 处理方式 |
|---|---|
| 哪回合最亏 | 取 `eval --all` 中 `regret > ci95` 且 `ci95` 最小的点 |
| 教诲 / 转成是否选错 | 跑 `infer --what advice,fusion` 并结合对应窗口 `eval` |
| 终局分低于期望 | 看 `curve` 末点 `T(n)` 与终局分差值（`final_gap`）及阴跌窗口 |
| 出行 / 演目节奏 | 读 `brief.md` §8 陈述节点，无基线分位故不判快慢 |
| 缺失字段（名次 / 友情数 / 技能名 / 种子） | 直接标 `unavailable` 或仅报已有点数（`skillPtSpent`）；教诲窗口查 `facts.turns[].advice_window` |
| 正常工具边界（非故障） | 强制比赛单候选走 fallback、`eval` 标不可判；同回合多条 `check_event` 不可判；训练后帧 `failureRate` 全 0；`400015106` 无 `check_event` |
