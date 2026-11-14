# brief 缺字段时的追问

`brief.md` 已经覆盖：覆盖情况、终局、教诲/転成/技能、追加稽古、训练、状态波动、合宿、检查项、剧本专属。

缺字段时的追问模板（先问自己「帧里有没有」）：

| 缺什么 | 先查 | 追问 |
|---|---|---|
| 比赛名次 / 输赢 | 帧无 `rank` | 别问，直接标 `unavailable` |
| 友情 / 彩圈 / 大成功数 | 帧无计数 | 用 `train[].failureRate` 预览代替，标「超出预览」 |
| 学技能明细（学了哪个） | 只有 `skillPtSpent` | 只报点数，不报技能名 |
| 随机种子 | 真机不发 | 不做同随机流反事实 |
| AI 候选分 | 需 `eval` | 跑 `review eval --turn N`，标 `recomputed` |
| 某回合是否教誨窗口 | `brief` 的 `pending` 列表 / `facts.turns[].advice_window` | 直接读 facts，别猜 |

原则：`pending` 不许编数；`recomputed` 必须带 N；缺数据就写缺数据。
