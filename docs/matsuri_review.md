# 骏大祭局包赛后复盘（matsuri_review）

> 真机局包（`game{id}.zip` 或散帧目录，默认位于 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/`）赛后事实复盘指南。Agent 化入口见 [`skills/matsuri_review/`](../skills/matsuri_review/SKILL.md)（工作流、口径速查、坑位、命令与追问路由）。协议边界见 [`online_protocol.md`](online_protocol.md)，剧本机制见 [`mechanism.md`](mechanism.md)，实现与内部反推细节见 [`crates/matsuri-tools/src/review/`](../crates/matsuri-tools/src/review/)。

## 1. 工作流

入口：`cargo run --release -p matsuri-tools --bin review -- <子命令>`（无参打印完整参数）。产物均落 `<out>/`：

1. **`open <局包|目录>`（必选，零重算）**：产出 `facts.json` 与 `brief.md`；先读 `brief.md`（约 10KB）掌握全貌。
2. **`eval <局包> [--swap <动作名>]` / `infer <局包>`（按需）**：仅在排查选择偏差或教诲/技能反推时运行（单点 `--turn N` 默认 `--n 1024`；全量 `--all` 建议 `--n 256`，逐决策面各算一点）。**同一回合有多个决策面（事件 / 训练 / 教诲）时单点必须带 `--seq`**：不给会直接报错并列出各面（如 `seq=0 事件 / seq=2 训练 / seq=3 教诲`），不再静默取第一帧。`--swap` 指定替代动作，另出「替代 − 实际」的终局差。若实际动作不在搜索候选（教诲 120 排列剪枝到 16、训练未映射），`regret` 标不可判——这是剪枝所致，可用 `whatif` 直接强制该动作做对照。
3. **`whatif <局包> --turn N --swap <动作名>[,<动作名>…]`（按需）**：从该回合局面分叉一次，对比「历史实际（反推失败则策略默认）」与各替代动作之后 `--horizon` 回合的走向；固定手写训练员、默认 `--k 100`（秒级），同时看动作频数与各维度均值差。`--horizon 0` 跑满终局，另给终局分（K 组均值）。
4. **`curve <局包>`（按需）**：查看区间 `T(n)` 走势与 `final_gap`。
5. **`report <局包>`（汇总）**：合并 `facts` 与已缓存结果生成 `digest.json` 与 `report.html`。

## 2. 复盘纪律与口径

- **表述规范**：只基于 `brief.md`、`facts.json`、`digest.json` 给结论（正向先行，带回合号与数值，结尾落一个下一局可执行动作）；不对玩家水平评级或做跨玩家比较。
- **内置标准（不暴露）**：好局线 / 非常优秀线 / 继承预期（306/次）都是内部判据——过线时给正面表述，**未过则不提**；继承只说「低于 / 高于预期」，不写标准值与差值；技能点未花（局末预结算）不作判据；五维一律显示值（超 1200 减半）。
- **统计与状态**：选择偏差 `regret = best.score − chosen.score` 按配对 95% CI 判优劣（`CI 跨零 = 同等优解`）；`recomputed` 须标明 `N` 且不同 `N` 不混比；`pending` 项先跑 `eval` 再下结论。
- **反事实口径**：`whatif` 只分叉一次（根动作不同），之后各支交给同一策略继续；同组各支共用 `master`，T+k 回合的随机骨架一致。基线默认取**历史实际动作**（帧 `actions` 反推；反推失败降级为策略默认并在报告标 `actual_source=default`）；`--swap` 可逗号分隔多个替代；**不支持事件回合**（根动作是选项而非 `Action`）；同回合多帧时**教诲帧优先**（`special + playing_state=51`），其次训练帧；只有 1 个候选的回合（强制比赛 / 本番）直接提示不可 whatif。`--horizon` 内两支动作无分歧即「影响被吸收」。判读：动作频率 SE≈√(p(1−p)/K)（K=100、p=0.5 时 ≈5pt），两比例差超过 10pt 才算「下游行为变了」；均值差按 SE=σ/√K 判（K=100 时体力≈1.5、五维总和≈15、评分≈150），超过 2SE 才值得说。维度：动作分布、体力、干劲、五维属性、五维分、演目 Lv 总和、技能点、PT、官方评分（五维分+PT=评分可对账），另给五维分维。whatif 固定手写训练员，不做搜索口径；想看「AI 最优继续」的终局差用 `eval --swap`。
- **命中判据**：`abs(regret) ≤ ci95` 即「命中」——实际选择与同局面最优的差在随机噪声内，不展开、不写「失误」；显著（`abs(regret) > ci95`）才分析第几选、损失与五维 / PT 取舍。`--swap` 的 `Δ = 替代 − 实际`，同样按配对 CI 判命中。
- **对比边界**：不做「AI 估分 vs 实际终局」或「在线/重算轨迹一致性」对比；AI 陪跑局中 `regret = 0` 仅说明重算与在线一致；继承仅对照 `400/次` 标准，不下 verdict。
- **帧 diff 反推**：结论须标「反推」；教诲落位取回合末帧、转成取 `fusionId` 首现、学技能用帧原值 `skillPt` 下降（区别于含已花累计的 `view.uma.skill_pt`）、追加排练取 `storyId = 400015106`。

## 3. 追问路由与正常边界

| 场景 | 处理方式 |
|---|---|
| 哪回合最亏 | 取 `eval --all` 中 `regret > ci95` 且 `ci95` 最小的点（按 `turn` + `seq` 定位到决策面） |
| 如果换一手会怎样 | 终局差：`eval --turn N --swap <动作名>`，看 `Δ ± ci95`；走向：`whatif --turn N --swap <动作名> --k 100`（`--horizon 0` 跑满时另给终局分），看下游动作分布与首个分叉回合。动作名歧义或未命中时报错会列出候选表，按表重选 |
| 教诲 / 转成是否选错 | 跑 `infer --what advice,fusion` 并结合对应窗口 `eval` |
| 终局分低于期望 | 看 `curve` 末点 `T(n)` 与终局分差值（`final_gap`）及阴跌窗口 |
| 出行 / 演目节奏 | 读 `brief.md` §8 陈述节点，无基线分位故不判快慢 |
| 缺失字段（名次 / 友情数 / 技能名 / 种子） | 直接标 `unavailable` 或仅报已有点数（`skillPtSpent`）；教诲窗口查 `facts.turns[].advice_window` |
| 正常工具边界（非故障） | 强制比赛单候选走 fallback、`eval` 标不可判；同回合多条 `check_event` 不可判；训练后帧 `failureRate` 全 0；`400015106` 无 `check_event` |
