# 赛后回看（复盘）工具开发计划 · matsuri

> 日期：2026-11-13 · 状态：实施中（`review open` 与插件数据面已落地；2026-11-14 局包路径改为扁平）
> 替代：[review_improvement_plan_2026-11-06.md](review_improvement_plan_2026-11-06.md)（**存档**，仅保留方法论与外部调研；本文修正其三个过时前提与两处概念）
> 范围：插件端（数据采集 / 局包归档）+ 剧本仓（记录层、复盘 bin、技能）；**引擎仓零改动**（仅两个可选增益，见 §9）
> 前置拍板（2026-11-13）：不改引擎；支持「开 AI」与「只开小黑板」两种模式的局；复盘工具**按需重算**，不在打开时全量重算。

---

## 0. 一页结论

1. **数据面由插件负责**：一局一包（帧 + 终局帧），局边界摘要内嵌在终局帧的 `review` 段。**不扩 actions**：教诲分配 / 转成 / 学技能由逐帧状态 diff 反推（见 §3.2）。散帧写入路径**不动**（AI 侧依赖它）。
2. **AI 侧记录是可选增强**：开 AI 时写 sidecar（候选分 / T(n) / 运气分）；不开 AI 时走**按需重算**。
3. **复盘工具分两层**：`open` 出事实（零重算）、`eval` 按需买估值（单点、带缓存）；`report` 只做合并与叙述注入。
4. **引擎硬依赖为 0**：所需 API 均已在稳定面；建议的 `ActionResult` 分位数与 `SearchOutput::paired_diff` 是增益，不是前提。
5. **明确不补**：比赛名次 / 用时 / 输给谁、事件选项数值效果、真机随机种子；教诲 / 转成 / 学技能的**动作请求**也不补（帧 diff 反推，见 §3.2）。

---

## 1. 与旧计划的关系

### 1.1 三个前提已过时（旧文档基于 2026-11-06 的现状）

| # | 旧文档说法 | 现状（2026-11-13 核实） |
|---|---|---|
| 1 | §5.0「消费端白名单尚未同步」 | **已过时**：引擎 commit `35ba2d5` 已把 `game{id}_finalScore.json` 列入帧白名单并解析 `actionSchemaVersion` / `actions`；只剩「无人消费」 |
| 2 | §4.1「`umaai_review` 可接 `umaai_next`」 | 引擎**无 zip / CSV / 落盘**（`open_items #15` 明确延后）；旧实现归档在 `Backup/umaai-rs-ref`，且 `SUPPORTED_SCENARIO_ID = 14`、认旧固定名 `finalScore.json` → **不能复用代码**，只借结构 |
| 3 | §5.6 / P2「同一随机流配对」 | 真机帧无种子、随机在服务端 → 反事实与 O/E/A 三线**只在模拟局成立**；真机只能给「期望 + 运气 + 遗憾」 |

### 1.2 两处概念修正

| 旧文档 | 问题 | 本文口径 |
|---|---|---|
| §5.6.2「参考轨迹 R vs 实际轨迹 A 双线」 | 真机无法逐回合测得「按玩家走法到终局的期望」 | 真机只有**一条** T(n)（局面期望终局分）+ 决策损失；两条线只在模拟局成立 |
| §5.6.3「分环节归因 = 消融重跑」 | 真机不能重跑 | 真机只能按**决策类型汇总 regret**（倾向性表述）；消融只用于模拟局 |

---

## 2. 数据来源矩阵

| 复盘需要 | 开 AI + 黑板 | 只开黑板 | 来源 | 必需性 |
|---|---|---|---|---|
| 逐回合帧（五维 / 体力 / 干劲 / 演目 / 教诲 / 合宿 / 人头 / 学时表） | ✅ | ✅ | 插件散帧 → 局包 | P0 |
| 玩家实际动作 | ✅ | ✅ | 终局帧 `actions[]` | P0 |
| 终局评分 / 档次 / `inheritGains` / `skillPtSpent` | ✅ | ✅ | 终局帧 | P0 |
| 教诲分配 / 转成 | ✅ | ✅ | 帧 `acts[].adviceId` / `fusionId` 逐帧 diff | P0（见 §3.2） |
| 学技能（只到「花了点」） | ✅ | ✅ | 帧 `skillPt` 下降 + 终局 `skillPtSpent` | P0（见 §3.2） |
| 局边界（start_turn / mid_entry / 帧数 / 时间） | ⚠️ 靠首帧推 | ⚠️ 同 | **写入终局帧 `review` 段** | P0（见 §3.1） |
| AI 候选分 / T(n) / 运气分 | ✅ | ❌ | AI sidecar（matsuri-cli `--record`） | P1（可重算替代） |
| AI 在场标记（开 AI / 只开黑板） | ✅ `ai-decision` | ❌（无标记） | 包内 `game{id}_ai-decision.json` | P0 |
| 技能学习明细（学了哪个） | ❌ | ❌ | **不补**（AI 只看点数） | — |
| 比赛名次 / 用时 / 输给谁 | ❌ | ❌ | **不补** | — |
| 事件选项数值效果 | ❌ | ❌ | **不补** | — |
| 真机随机种子 | ❌ | ❌ | 做不到 | — |

**两种模式的差别只在「AI 评估」这一列**；观测事实（帧 / 动作 / 终局）两种模式同源。这也是把数据采集下沉到插件的主要理由。

---

## 3. 插件端规划（**第一优先，P0 数据面**）

> 约束：插件改动须用户确认方案后再动代码；真机测试由用户手动进行；依赖闭包成套同步（见 §3.4）。
> 现状：散帧 `game{id}_turn{N}[_k].json` + `thisTurn.json` 由 `GameStatusOutput` 原子写；终局帧在正常终局一个出口写 `game{id}_finalScore.json`；`actions` 只采集 `exec_command` / `check_event` / `race_entry` 三类请求；教诲 / 转成 / 学技能**不扩 actions**，由帧 diff 反推（§3.2）。

### 3.1 局包归档

| 项 | 设计 |
|---|---|
| 目标 | 一局一包，复盘工具直接消费；不改散帧路径与文件名（AI `FileChannel` 依赖） |
| 触发 | 终局帧落盘之后，在同一次调用内打包（`GameStatusSend_GameEnd` 的终局出口）；同局只打一次（`FinalScoreWritten` 已保证一局一份） |
| 兜底 | 下一局 `start` 响应时若上局已见终局仍未打包则补包；**不要挂在 `load` 上**（换局时终局帧可能还没到）。进程退出补包仅作可选 |
| 位置 | `PluginData/SendGameStatusPlugin/game{id}.zip`（与帧同目录、**目录保持扁平**；不进 `ura_plugins` 更新集合，见 §3.4）。旧版 `records/game{id}.zip` 只读兼容：存在即视为已归档，新包不再写那里 |
| 原子性 | 写 `.partial` → `File.Move` 覆盖；失败重试并保留散帧不动 |
| 包内容 | 本局全部 `game{id}_turn*.json` + `game{id}_finalScore.json`；**不含** `thisTurn.json`；**不新增 meta / manifest 文件** |
| 局级摘要 | 写进 `finalScore.json` 的 `review` 子对象（见下），不单独成文件 |
| 缺帧 / 中途载入 | 不阻断打包；`mid_entry = start_turn > 0`（或首个 `isFullGame=false`）；`missing_turns` 由帧回合序列算差集 |
| 保留策略 | 默认保留最近 50 局（按 mtime），超出删最旧；开关默认**关**（先观察磁盘占用，见 §12-D1）。**插件只新增 zip、不删原散帧**，清理交给工具或用户 |
| 兼容 | **只支持新包**（含 `review` 段 + 新协议帧 `charaEffects`）。无 `review` 的旧包、旧协议帧（`illEffects`）一律不兼容，工具直接报错并提示用新版插件重录（2026-11-14 拍板） |

**为什么不在插件里画图**：T(n) / 候选分只在 AI 进程内存里（插件侧没有模拟器）。插件只做**数据归档**；图与报告归复盘工具。

**局级摘要内嵌在终局帧的 `review` 段**（2026-11-13 收敛：不再新增文件）

```json
"review": {
  "schema": 1,
  "startTurn": 0, "endTurn": 77, "midEntry": false,
  "frameCount": 131, "missingTurns": [],
  "firstCapturedAt": 1762... , "lastCapturedAt": 1762... ,
  "loads": [{"turn": 30, "at": 1762...}],
  "pluginVersion": "2026.11.10", "dataVersion": "20261110"
}
```

| 汇总字段 | 来源（终局时枚举本局帧目录即得） |
|---|---|
| `startTurn` / `endTurn` | 帧回合最小 / 最大 |
| `midEntry` | 首帧 `isFullGame`（或 `startTurn > 0`） |
| `frameCount` / `missingTurns` | 文件计数与回合差集 |
| `firstCapturedAt` / `lastCapturedAt` | 帧文件 mtime 的最小 / 最大 |
| `loads` | 进程内记的读档时间线（见 §3.2） |
| `pluginVersion` / `dataVersion` | 构建期常量 |

**为什么可以不再单独成文件**：帧文件名只有 `game{charaId}_turn{N}`，缺的是「局边界 / 完整性 / 版本」三类摘要，而这三类都能在**终局那一刻枚举目录**算出，塞回终局帧即可；zip 自身带 CRC32，整包损坏会在解压时报错，不需要额外 manifest 对账。

**兼容性已核实**：引擎 `FinalScoreFrame` 与 `matsuri-ai` 的解析都没有 `deny_unknown_fields`，新增 `review` 段会被旧链路直接忽略——旧 AI + 新插件、新 AI + 旧插件都不炸。

**必须接受的代价**：未正常终局的局没有 finalScore → 也就没有 `review` 段。工具把「无 finalScore」直接判为「未终局」，这类局只复盘到最后一帧（本来就给不了终局估分）。

**最小改动面（三个文件，零新状态）**

| 文件 | 改动 |
|---|---|
| `GameStatusOutput.cs` | 新增打包出口（`System.IO.Compression` 在 net10.0 基础框架内，无需加包）；`review` 段派生（终局时枚举本局帧：回合范围 / 计数 / 缺帧 / mtime） |
| `GameStatusSend_GameEnd.cs` | 写 finalScore 前补 `review` 段；终局出口调用打包；记录 `load` 时间线；**ActionLog 不动** |
| `Class1.cs` | 不新增请求注册；仅当后续要补 `actionPhase` 时再改 |


### 3.2 数据补充清单

**必须补（P0）**

| # | 补什么 | 依据 / 收益 | 落点 |
|---|---|---|---|
| 1 | 终局帧内嵌 `review` 段（局边界 / 完整性 / 版本） | 帧文件名不含局边界与完整性；终局时枚举目录即可派生（见 §3.1） | `GameStatusSend_GameEnd` + `GameStatusOutput` |
| 2 | `review.loads`（读档时间线：回合 + 时间） | 读档后同回合帧会以 `_N` 追加，不分段会把新旧时间线混成一局；`load` 响应已注册，只记一对（turn, at） | `GameStatusSend_GameEnd` |

**教诲 / 转成 / 学技能：不扩 actions，改由帧 diff 反推**

| 数据 | 反推规则 | 不纳入的过程信息（不影响结论） |
|---|---|---|
| 教诲落位 | 逐帧比对 `acts[].adviceId`（0 = 未分配），**首次出现新 id 的回合**记为该演目的生效回合；5 个演目即 5 条落位记录 | 提交时间、窗口内改选次数、手选 vs 自动设置——复盘判「最终落位是否偏离建议」，不判怎么点的 |
| 转成 | 逐帧比对 `acts[].fusionId`（0 / 601 起），首现回合记为转成回合；配合 `adviceId` 变化得「基础教诲 → 融合版」 | 提交顺序与中途改选——只判「最后融合成什么」 |
| 学技能 | 逐帧比对 `skillPt` 下降的回合；终局 `skillPtSpent` 给总额 | 学了**哪个**技能、性价比——AI 侧本来也只关注点数消耗 |

> **原则：效果可核、过程不评。** 复盘的对象是**生效结果**（落位 / 融合 / 点数），不是操作序列；所以反推不是权宜之计，而是与目标一致的选择——落位与融合由帧字段唯一确定（还原层同源），不存在「猜」的成分。
> 比对口径：教诲是**5 槽整体方案**（游戏一次提交 5 条），「是否偏离建议」按整套方案比，不按逐槽比；报告里这三类统一标「反推」，不标「实测」。

> 叙事约束：写「这局落在 X」而不是「你选了 X」；`auto_setting` 局不评「是否主动选择」，只评结果是否可行、是否偏离建议。

**建议补（P1，可与 P0 同批但可延后）**

| # | 补什么 | 理由 |
|---|---|---|
| 3 | `festa.events[]`（全量事件，见 `docs/plan/online_wiring.md` 已登记者） | 复盘想讲清「这回合弹了哪些事件」；不补则只有 `keyEvents` 的 story_id |
| 4 | 继承上限增量 `inheritLimitGains` 逐帧 | 新协议帧已带；旧 `illEffects` 帧不做兼容（直接不支持），继承判据只用新帧 |
| 5 | `summer_camp_remain_turn_num` 递减语义 | 合宿判据依赖它（11-06 §7.2 待核） |
| 6 | `actionPhase`（`before`/`after`） | 收掉 `isAfterTraining` 粘连（11-08 §7）；复盘按阶段对齐决策点会更稳，但可用 `seq` 兜底 |
| 7 | `festa.advices` 全表（advice → fusion）+ `assistRemain` 次数 + `training[].gauges` 预测 | 帧里只发挂在该演目上的那一个`adviceId` 与 `assistSupportLeft(0/1)`，复盘要多算继承/教诲才需要；名称仍取 master 表 |

**不补（拍板）**

- 比赛名次 / 用时 / 输给谁（协议无，且不做「输在哪」的判据）。
- 事件选项的数值效果（引擎事件表可查定义，不回放随机结果）。
- 真机随机种子（做不到）。
- 技能「实际生效」结果（只有 `skillPtSpent`；连请求侧 `skill_id` 也不补，AI 只关注点数）。
- 教诲分配 / 转成 / 学技能的**动作请求**（由帧 diff 反推，见上表）。
- 汇报会的选择（请求只有 `current_turn`，本就没有选择）。

**名称字典不补进帧**：帧里只有 `adviceId` / `fusionId` / `actType`，名称与效果在 FestaScenarioAnalyzer 的 `Data/master-data.json`，由工具侧读表。

### 3.3 插件 ↔ 工具契约

1. 工具优先读 `game{id}.zip`（含旧版 `records/game{id}.zip`）；无包则读同目录散帧（兼容老局与人工 zip）。
2. `review.schema` 单调递增；工具按 `review.schema` 分支，未知字段忽略；`review` 缺失 / 旧协议帧 → **直接报错**，不做降级推断。
3. 包内条目用相对路径、不带外层目录（与参考实现一致，便于整包解到临时目录）。
4. `actions[].turn` 为 **0 基**（= `current_turn - 1`），工具侧统一用 0 基。
5. 工具侧不依赖插件内部类型（只读 JSON），插件侧不感知 AI 是否存在。

### 3.4 发布与同步

| 项 | 结论 |
|---|---|
| 分发 | `ura_plugins` 频道：`Plugins/*.zip` + `.portable/version.json`；源仓 UmaSimData；`PluginData/` 运行时数据**不纳入**更新集合 |
| 依赖闭包 | `SendGameStatusPlugin.zip` 不含依赖 DLL，按 manifest 从同目录加载；**5 个包必须成套同版**（EventLogger / Festa / Ramen / Legend / Pioneer） |
| 本次涉及 | 只有 `SendGameStatusPlugin`（打包 + 终局帧 `review` 段 + `loads` 时间线），**不动 ActionLog、不新增请求注册**；若后续补 `actionPhase` 或 `festa.events[]` 再评估 |
| 注意 | 终局帧改名后「旧引擎消费端不进包」，本仓复盘工具**不经过**旧引擎 record，直接读插件输出 |

### 3.5 插件端验收

1. 新协议真机局（`game6324` 起）：包内回合帧 + finalScore 齐备，zip 可正常解压；`review` 段字段自洽（frameCount / 回合范围 / 缺帧 / 时间）。
2. 反推核对：由帧 diff 得到的教诲落位（5 条）、转成（T23/T47）、技能点下降回合，与游戏内实际一致（肉眼对账即可，不作自动化验收）。
3. 中途载入局 `review.midEntry = true`；读档重开局 `review.loads` 非空且工具能分段；缺帧局 `missingTurns` 非空；下一局 `start` 兜底补包成功。
4. 未终局的局按「无 finalScore」判未终局；旧包/旧协议帧**不要求可读**（见 §11）。

### 3.6 AI 在场标记（2026-11-14 新增）

目的只有一个：**区分「开 AI 养的局」与「只开黑板自己养的局」**，过程信息一律不写。

| 项 | 结论 |
|---|---|
| 写入方 | `matsuri-cli online`（只在真机模式；`--replay` 不写） |
| 触发 | T77 帧到达时写一次（`turn == max_turn`，不要求是决策帧）；同局只写一份 |
| 文件 | `PluginData/SendGameStatusPlugin/game{id}_ai-decision.json`（与帧同目录、扁平） |
| 不完整局 | 不写（AI 中途退出 → 无文件） |
| 写入失败 | `log::debug` 静默，不影响决策 |
| 插件处理 | `PackGame` 收进 zip；`BuildReview` 读成 `review.ai`；`CleanupFrames` 打包后删除 |

内容（只放最终状态）：

```jsonc
{
  "schema": 1,
  "game": 6324,
  "turn": 77,          // 触发帧回合
  "firstTurn": 0,      // AI 本局首次决策回合（>0 = 中途才开）
  "lastTurn": 76,      // 最后一次决策回合（估分对应局面）
  "expected": 86554,   // 官方口径期望终局分
  "rank": "LG8",
  "mode": "search",    // hand / search
  "writtenAt": 1794632000000
}
```

判定：文件存在 = 「开过 AI 且陪到局终」；不存在 = 「无 AI 标记」（老包、只开黑板、中途退出都落这里，不写“没开 AI”）。
`expected`/`rank` 只作展示/调试 hint，**不做 AI 估分与实际终局的对照结论**（见 §6 不做项）。

---

## 4. 剧本仓：记录层（AI sidecar，P1 前提）

| 项 | 设计 |
|---|---|
| 入口 | `matsuri-cli online --record <dir>`；缺省读 `ai_config [record]`（`enabled` / `root`，当前 `false` / `logs`） |
| 落点 | `records/game{id}/ai.jsonl` + `ai-meta.json`（与插件局包同 id，工具侧合并） |
| 行字段 | `turn / seq / kind(train·advice·event·race) / stage / chosen_idx / candidates[{desc,score,stdev,n,five,pt}] / t_n_raw / t_n_display / turn_delta / total_luck / expected_score / expected_rank / frame_digest / action_attempted` |
| meta | `seed`、卡组构成与档名、`luck_turn_bonus`、版本、配置指纹、`mode(online·hand)`、`search_n` |
| 纪律 | `matsuri-ai` 保持「只出 lib、不落盘」；写盘在 `matsuri-cli`。教诲帧不推进 T(n)（沿用 `tests/advice_boundary.rs` 口径） |
| 现状缺口 | `output.rs::json_payload` 只写 `score` 列表，`stdev/n/five/pt` 丢弃 → 记录层需要独立 DTO，不动小黑板契约 |

---

## 5. 复盘工具（`matsuri-tools` 新 bin `review`）

### 5.1 子命令与按需重算

```
review open   <局目录|局包.zip> [--game id]  # 只读帧+actions+终局 → facts.json / brief.md（pending 标好）
review eval   --turn N [--seq k] [--n 1024|8192]   # 单点：候选表 + regret + CI + T(n)，落缓存
review curve  [--from A --to B] [--n 1024]         # 区间/全量 T(n) 与累计差（粗算档）
review report [--narrative md]              # 合并 facts + 已缓存 eval/curve → digest / report.html
```

### 5.2 `pending` 契约（skill 顺滑的前提）

| 标记 | 含义 | skill 动作 |
|---|---|---|
| `measured` | 帧 + `actions` 直接可得 | 直接写 |
| `online` | AI sidecar（T(n)、候选分、运气） | 直接写，标「AI 侧实测」 |
| `recomputed(N=…)` | 事后重算，带 N | 可写，标「估算」 |
| `pending` | 能算未算，带 `turn/seq/kind/est_cost` | 由 skill 决定是否 `eval`，或降级表述 |
| `unavailable` | 数据本身没有（名次、用时、技能生效、随机流） | 不写结论，放口径备注 |

### 5.3 缓存与一致性

- 路径 `<game>/recompute/turn{NN}_{seq}.json`，按定位键索引。
- 头部带 `seed + 配置指纹 + 代码版本 + N`；任一不符即失效并标 `stale`。
- 不同 N **不可混比**；曲线要么统一 N，要么标「混合 N」。
- 幂等：同 seed + 同配置 + 同 N 逐位一致（在线回放 438 行 / 284 行全等已验证）。

### 5.4 成本

`est ≈ 候选数 × N × 1.3 ms`：N=1024 约 1–10 s/点、N=8192 约 10–80 s/点；全量 78 回合 N=1024 在 24 线程下约 25–40 s。**先粗后精**：全曲线 1024，极值点 8192。

### 5.5 模块

| 模块 | 职责 |
|---|---|
| `pack` | 局包 / 散帧解析（局 → turn → 文件名排序）、meta 降级推理 |
| `replay` | 每帧还原（复用 `matsuri-ai::frame` + `matsuri-core::restore`）→ 状态行 |
| `score` | `calc_score` + `score_parts`（7 分量）+ 档次 + 五维显示值 |
| `trajectory` | T(n)、本回合 Δ、累计差、形态（step / ramp / sawtooth）、伪信号标记 |
| `decisions` | 实际动作解码、候选命中、regret + CI、同路率 |
| `attribution` | 分环节：普通训练 / 稽古 / 休息 / 出行 / 比赛 / 事件 / 教诲 / 转成 / 合宿 / URA |
| `checks` | 检查项（§7） |
| `inherit` / `profile` | 继承贡献与偏差；逐年训练画像（普通训练 vs 稽古分列） |
| `report` | `brief.md`（一次 Read 主路径）/ `digest.json` / `report.html`（4 图 + `narrative` 注入） |

---

## 6. 指标口径（matsuri 版）

| 参考实现指标 | matsuri 处理 | 说明 |
|---|---|---|
| `T(n)` / `turn_delta` / `total_luck_end` | **保留** | 引擎 `LuckScorer` 已在线使用，`bonus = 40`（2026-11-11 重标定） |
| 执行偏离（状态差推断） | **替换** | 训练 / 休息 / 出行 / 比赛 / 事件选项读 `actions`（`command_id` / `choice_number`）比对候选表；教诲 / 转成 / 技能走帧 diff 反推；推断链只在无 actions 的老包降级 |
| `alt_candidate` | **保留口径** | 有编号 = 换了候选，不必然不听建议 |
| 一致率 | **改名「同路率」**，不作评分 | 沿用教育心理学约束 |
| 继承贡献 / 偏差 | **保留，口径重定** | 终局 `inheritGains` + 上限增量；参考实现「Σextra_count + 126」不适用 |
| 分身彩圈 | **删除** | 无同构；改「演目闪彩担当 +2」的观测与「吃到与否」 |
| 超级拉面 / RMJ / 友人次数 / 自选赛期限 | **删除** | 拉面专属；换「合宿 / URA 期（全稽古）/ 教诲窗口」 |
| 年界 / 地区选择 / 旧包假跳 | **换标记集** | 教诲窗口（T2/11/23/35/47/59）、转成（T23/T47）、汇报会（T23/47/67）、合宿、URA（72–77）、学技能 |
| 训练画像 | **保留 + 扩展** | 普通训练与稽古分列；演目 Lv / gauge 单独成行 |
| 五维显示值 | **保留** | > 1200 部分减半（`DISPLAY_HALF_FROM_STATUS`） |
| 理论上限 / O-E-A / 反事实 | **仅模拟局** | 真机无随机流 |

**逐回合分解（真机唯一可落地的部分）**

```
regret_n = score(best) − score(chosen)      ← 决策贡献（带配对 CI）
luck_n   = ΔT(n) − regret_n                 ← 运气实现
累计校验：Σluck + Σregret ≈ 终局分 − T(0)
```

**形态纪律**：step 有明确地址；ramp 是上游拖累；sawtooth **不标失误**。

**不做项（2026-11-14 拍板）**：不做「AI 估分 vs 实际终局」的对照结论，也不做「在线轨迹 vs 重算轨迹」的一致性验收——AI 估分本身带误差，这类一致性只是程序调试 hint，不影响决策质量；AI 标记里的 `expected` 仅作展示。

---

## 7. 检查项（候选，四要素：判据 / 取数 / 输出 / 状态）

> 口径按 2026-11-14 用户逐条拍板更新；级别：**gate**（门禁，必须报）/ **warn**（警告）/ **info**（陈述）/ **collect**（先收集数据，不下结论）。

| # | 检查项 | 判据 | 取数 | 级别 | 状态 |
|---|---|---|---|---|---|
| B1 | 汇报会未大成功 | 该回合全演目 Lv 未达要求（T23/47/67；资产要求 5 个演目达 Lv3/5/6） | 帧回合末 `acts[].level`（先用 Lv 自判，不看 `resultType`） | **gate** | 可做；正常养未成功是小概率，但保留门禁 |
| B2 | 教诲落位 vs 建议 | 先比「实际落位」与「建议落位」的**同局面估分差**（配对 CI）：跨零 → 同等优解、不算问题；显著 → 再结合终局五维 / PT 分项解释（可能评分低但 PT 或某维更高） | 帧 `advice_slots` 反推 + 重算两者期望分 | **collect** | 复杂，先收集数据（见下） |
| B3 | 転成选择 vs 预设 | 同 B2：实际融合 vs AI 预设/候选的估分差 + CI；显著时同样看五维/PT | 帧 `advice_fused` + `fusion` 预设 | **collect** | 同上，先收集数据 |
| B4 | **合宿延长** | 列出本局所有 `camp.extended = true` 的回合（不是“缺席”）；附每年延长回合数 | 帧 `camp.extended` / `camp.counted` | info | 可做 |
| B5 | 必赛未跑（退赛） | 目标赛回合不在 `raceHistory` → 该赛未跑；**帧无名次，输赢不可判**（不写“未赢”） | 帧 `raceHistory` / `isRacing` + `umaDB.races` | **暂缓** | 2026-11-14 用户：**不查 B5**（基期对账也停）；保留条目备后续 |
| B6 | 体力线硬练 | 回合末 `vital < 32` 且当回合无 0% 失败训练（守门④） | 帧 `vital` + 训练表 | **gate** | 可做（门禁检查点） |
| B7 | 干劲掉落未恢复 | 掉落后 3 回合无出行/休息且未回升 | 帧 `motivation` 序列 | **gate** | 可做（门禁检查点） |
| — | 演目槽满未稽古 / 神驹缺席 / URA 决赛输 | — | — | — | **不检测**（2026-11-14 拍板） |

**判据总纲：选择偏差（B2/B3/C1/C2 共用）**

对任何可枚举候选的决策点（训练 / 休息 / 出行 / 比赛 / 事件选项 / 教诲 / 転成）：

1. 取「实际选择」与「首选候选」的**同局面期望分**（同一随机流配对 rollout），得 `Δ = E(best) − E(actual)` 与配对 95% CI；
2. **CI 跨零 → 视为同等优解，可接受**（玩家命中 2/3 选不算问题）；
3. 显著 → 才算“选择偏差”，再进一步分析：选的是第几选、损失多少、是否有五维 / PT / 资源路线上的取舍解释（可能评分低但 PT 或某维更高）；
4. 报告措辞：不显著只陈述“与实际选择差距在噪声内”，显著才写“这里有一条更优路线”。

C 组（决策类）按同一套判定：

| # | 检查项 | 判据 | 取数 | 级别 | 状态 |
|---|---|---|---|---|---|
| C1 | 训练/休息/出行/比赛选择偏差 | 实际 vs 一选的估分差 + 配对 CI | `actions` + 候选表 | collect | 需 `eval` |
| C2 | 事件选项选择偏差 | 同上（事件候选用事件表效果） | `actions.choice_number` + 事件候选 | collect | 需 `eval` |
| C3 | 训练失败 | 建议训练 + 五维零增长 + 体力有消耗 | 前后帧差 + 训练表 | **gate** | 可做（门禁检查点） |
| C4 | 关键资源 | 吾妻出行 5 段 / 助力支撑剩余（只陈述是否用满） | 帧 `friend.out_used` / `assist_support_left` | **gate** | 可做（门禁检查点） |
| C5 | 干劲 | 同 B7，合并 | — | **gate** | 合并 |

E7（自选赛打满时机，info）：逐区间列出「required 场数 / 实际完成场数 / 打满回合」，用于看是否拖到最后才补赛；数据取 `umaDB` 的 freeRaces 区间 + 帧 `race` 回合序列。

**B2 / B3 的数据收集（先只收集、不判定）**

每个教诲窗口（T2/11/23/35/47/59）与転成点（T23/T47）各记一行：

```json
{
  "turn": 2, "kind": "advice",
  "actual": [[act, adviceId], ...5],
  "suggested": [[act, adviceId], ...5],
  "actualExpected": 86210.0, "suggestedExpected": 86554.0,
  "delta": 344.0, "ci95": 260.0,
  "fiveExpected": [[...], [...]], "ptExpected": [...],
  "finalFive": [...], "finalPt": 11000
}
```

累计若干局后再定「显著」阈值（优先用配对 CI 跨零判定）。收集阶段报告只写「落位与建议不同 / 估分差 X±Y」，不下“失误”结论。

### 7.1 剧本专属判据（2026-11-14 增补）

**（1）演目等级增长**

- 按卡位分组：未带卡位（该训练位无支援卡，如 speed_build 根位）只判**结算达标**（T23/47/67 = Lv3/5/6）；带卡位才判**快慢**。
- 取数：逐帧 `acts[].level/gauge`；首次到达 Lv2…Lv10 的回合；稽古成功次数 = Lv 增量 − 合宿结算 3 次（15/39/63 各 +1）。
- 快慢标准：由 §7.3 基线程序给出「到达 Lv k 的回合」分位，真机对自身历史分位比较；收集期只陈述曲线与节点。
- 随机性：稽古失败不升 Lv，慢不等于选择差。

**（2）友人出行**

- 两条 warn（2026-11-14 拍板，均不判“损失”，措辞用“注意”）：
  - B 第一年段数：T23 前已走段数 < 4 →「第一年只走了 N/5 段」；
  - C 截止线：T36 前未走满 5 段 →「T36 前只走完 N/5 段」（合宿 36–39 / 60–63 与 URA 会挤压可用回合）。
- 解锁时点**不作判据**（2026-11-14 取消）：解锁是首次点击后的羁绊概率判定，随机性大；只记录「解锁回合 / 各段回合 / 走满回合」作陈述。
- 收益陈述（info）：每段 +1 最低演目 Lv，最多 +5；不折算分数。
- 取数：`friend_stage / friend_outgoingUsed`（0/1/2 = UnClicked / BeforeUnlock / AfterUnlock）或 `friend.out_used[5]`；完成回合 = 5 段全 true。
- 基线触发率（2048×50）：T36 前走满 speed 手写 50/50、搜索 45/50，wisdom 手写 50/50、搜索 37/50；T23 前 ≥4 段 手写 44–47/50、搜索 28–34/50。

**（3）运气波动**

- `swings`：ΔT 分解为 `score_parts` 七分量（五维 + PT + skill）+ 状态差分；标签 `机制 / 局面 / 操作 / 混合`。
- 判定优先级：机制结算 → `regret` 的 CI（`eval` 后）→ 状态驱动启发式（属性/PT 由随机判定驱动=局面；体力/干劲/技能点由选择驱动=操作）。
- 阴跌段：只陈述「5 回合窗口累计 ΔT < −300」的时段与对应状态变化，归因到机制 / 随机 / 选择三类启发式。
- **不做属性 gap 与“超/未达预期”**（2026-11-14 用户：每维每回合累计增量分位无法获得，不作为判据）。

**（4）继承质量**

- 已知口径：模拟约 **400 点/次**（两次 ≈ 800），真机常达不到 → 继承回合 `T(n)` 下移属**正常波动**，不判操作。
- **不做分位判定**（2026-11-14 用户：inheritGains 分布无法获得）：只出实际值（两次各自总量 + 各维明细 + 上限增量）与可配置标准（默认 400/次）的对照陈述。
- **该标准做成代码可配置**（2026-11-14 用户：后续可能调整）：不硬编码判据，放在 review 侧 tuning（默认 400/次，可被 CLI 参数覆盖）；不改 `matsuri_config.toml`（避免污染游戏配置与内置默认锁）。
- 输出 `inherit[]`：`turn(30/54) / total / expectedStandard / delta / gains / limitGains`（不做分位与 verdict）。

**（5）追加稽古**

- **以触发事件为准**：`actions.check_event.event_id = 400015106`（name 追加の稽古；choice1 = 加练 −5 体/智+20/羁绊+5，choice2 = 休息 +5 体）。
- 位置：`kensan_act`（研鑽演目）+ `kensan_rate`。**未転成 701 时 `kensan_act = None`，此时不会触发追加稽古**，所以位置直接取该值，不必回看上一回合训练（2026-11-14 更正）。
- 执行：选 1 后下一帧该演目是否真的走稽古（`practice[act]` 且 `ActTrain`）。
- 输出 `kensan.triggers[]` + `total/add/rest/executed/wasted` + `byAct/byPhase`；倾向跨局聚合。

**（6）状态资源（切者/小切、心情盾、幸运体质）**

- 取数：`isQieZhe / isXiaoQie / PositiveThinkingCount / LuckyCount / isRefreshMind / isIll / motivation`。
- **小切**：局外固定（`[pre] qie`），口径系数 PT×1.04，**不算运气**；**切者**：随机事件，**好运（大幅）**，PT×1.1；切者清小切。
- 心情盾 / 幸运体质：计数与事件对齐——事件发生且计数 −1 = 生效；计数未变 = 该事件不被挡（标“机制待核”，不判失效）；无事件 = 未派上用场。
- 输出 `status: { qie, mindShield, lucky }`，info 级。

### 7.2 统计点清单

**逐局（`facts.json`）**：数据健康 4 条；机制 B1/B4/B6/B7（B5 暂缓）；决策 C3/C4 + E7；剧本（1）曲线/节点/稽古次数/卡位、（2）出行状态、（3）swings 分解、（4）继承实际值、（5）追加稽古明细、（6）状态资源。

**基线（`baseline` 程序）**：演目节点 Lv 与到达回合分位、友人解锁/完成回合分位与通过率、追加稽古触发频率分布。**不采**每维增量分位与继承分布（2026-11-14 用户：无法获得，不作判据）。

**跨局聚合**：选择偏差分布（B2/B3/C1/C2 的 Δ±CI）、追加稽古按位置/时机倾向、同卡组近 N 局自我参照。

**实现优先级**：P0 纯帧（健康 + B1/B4/B6/B7 + C3/C4 + E7 + （1）（2）（4）（5）（6）实际值 + （3）swings 分解）→ P1 基线程序（Lv/出行分位与快慢）→ P2 `eval`（regret + CI，B2/B3/C1/C2）→ P3 跨局聚合。

### 7.3 基线测量程序

- 新 bin `baseline`（`matsuri-tools`）：`--games N --seed --uma --deck-profile --trainer --out`；
- 先抽 `matsuri-sim` 的 pub `play()`（带逐回合回调），供 bench / baseline / P2 反事实共用；
- 输出 `benchmark/baselines/acts_friend_<deck>_<uma>.json`（分位表 + 条件头：seed 块 / 局数 / 训练员 / 卡组 / 版本），stdout 一张汇总表；
- 条件纪律：同卡组 / 同马娘 / 同训练员才可比；主基线用手写档 1000–3000 局，搜索档单独 20–50 局作对照。

---

## 8. 技能（`.trae/skills/matsuri_review/`）

| 文件 | 内容 |
|---|---|
| `SKILL.md` | 定位局包 → `review open` → 一次 Read `brief.md` → 按需 `eval` → `report`；**不派子代理**（brief 齐备时） |
| `reference/metrics_glossary.md` | T(n) / luck / regret + CI / 形态 / 同路率 / 显示值 / 演目与教诲口径 |
| `reference/pitfalls.md` | 跨语言取数坑沿用；机制坑换 matsuri（教诲帧不推进 T、同回合多决策、`bonus = 40`、真机无种子、`pending` 不可当事实） |
| `reference/persona.md` | 人设待拍板；缺省平实口吻 |
| `reference/probe_prompts.md` | 只在 brief 缺失时启用（字段名换 matsuri） |
| `reference/uma_specific.md` | 依赖具体马娘赛程 / 对手的判据，由模型层判读 |

工作流纪律：**先 `open` 拿事实，再决定要不要 `eval`**；报告里 `pending` 不许编数；`recomputed` 与 `online` 分开标注。

---

## 9. 引擎改动（最小集）

| 项 | 必要性 | 内容 |
|---|---|---|
| 硬依赖 | **0** | 强制动作样本 rollout、CRN、有序样本、`LuckScorer`、`actions` 解析均在稳定面 |
| E1（建议） | 增益 | `ActionResult` 增 `quantile(p)` / `se()` / 只读 `samples()`：区间更稳，长尾不被正态近似低估 |
| E2（建议） | 增益 | `SearchOutput::paired_diff(i,j)`（仅 `record_ordered_rollouts` 开启时）：配对差比独立近似**更少假阴性** |
| E3（建议） | 文档 | `docs/scenario/` 增「复盘数据契约」；更正 handoff §7.4 与 open_items #15 现状 |
| 不做 | — | 引擎内 zip / CSV / 归档；`max_depth` / 叶估；复现真机随机流 |

---

## 10. 阶段与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| **P0-A 插件数据面** | 局包归档 + 终局帧 `review` 段（局边界/完整性/时间）+ `loads` 时间线 + AI 标记收录（**不扩 actions**） | §3.5 四条；`game6324` 起的新协议真机复核 |
| **P0-B 工具事实层** | `review open`：pack / replay / score / decisions 命中 / brief + digest | 用 `game6324.zip` 跑通；160 条 actions 全落候选表；同输入两次逐位一致 |
| **P1-A 轨迹与形态** | 累计差曲线、step/ramp/sawtooth、伪信号标记、4 图报告 | 曲线终值 = 实际终局 − T(0)；标记回合不计损失；sawtooth 不标失误 |
| **P1-B 估值与归因** | `eval`（regret + CI）、缓存、分环节归因（决策类型汇总） | CI 跨零一律「不可判定」；重算幂等；`pending` 不编数 |
| **P1-C 技能** | `matsuri_review` 六个文件 | 用新协议局走一遍 open→eval→report，产出 narrative 并注入 HTML |
| **P2（仅模拟局）** | `matsuri-sim` 提 pub `play()`（合并三份私有副本）+ 动作复演（E 线）+ 同 seed 多局 O 线 | O ≥ E；反事实给 3–5 回合主变线；不拼接局部最优 |

> 依赖顺序：P0-A 与 P0-B 可并行（P0-B 先用 `6324` 的 zip 验证；无 zip 时读散帧）；P1-B 依赖 P0-B 的 facts。
> **不做**（2026-11-14 拍板）：AI 估分与实际终局的对照、在线轨迹与重算轨迹的一致性——AI 估分本身有误差，这类一致性只作程序调试 hint，不进功能与验收。

---

## 11. 风险与边界

1. **统计功效**：单局 78 个决策点，单局结论近似噪声；跨局聚合才算手法，报告不写「水平 / 评级」。
2. **真机不可复现**：不给上限、不做同随机流反事实；一切 `recomputed` 标估算。
3. **插件闭包**：5 包成套；只升一个包会「调用不存在的方法」。
4. **只支持新协议**：帧字段一律按 `charaEffects` 后的契约解析；旧 `illEffects` 帧与无 `review` 段的老包不做兼容（直接报错）。
5. **数据隐私**：局包含角色 id / 时间；只在本机留存，不入库、不外传。
6. **未终局**：无 finalScore 的局判未终局；无 `ai-decision` 的局判「无 AI 标记」，不写“没开 AI”。
7. **口径混用**：`online` 与 `recomputed` 不混比；不同 N 不混比。
8. **插件进程状态**：`TurnSequence` / `FinalScoreWritten` / `ActionLog` 全在内存，插件重启后同回合序号从 1 重来、actions 只剩后半局；复盘按 `frame_count` 与 `missing_turns` 判残缺。
9. **写盘路径**：`PluginData/SendGameStatusPlugin` 相对进程 CWD，宿主换工作目录时数据会落到别处；工具侧找不到包时提示而不是静默。
10. **不可信帧**：`attach_advice` / `gain_skills` / `check_point` 等走 `FromCheckEvent`（`source = event`），`trainLevelTotal` 会被整组置 0——复盘读训练等级时要避开这些帧。
11. **无跨进程锁**：插件与工具同时操作同目录时只靠 `.partial` + 原子改名与「包已存在不覆盖」约定；工具只读、不移动散帧。

---

## 12. 待拍板

| # | 问题 | 建议 |
|---|---|---|
| D1 | 局包保留策略（默认 50 局 / 不删） | 先不删，观察磁盘占用后再定 |
| D2 | 是否补 `festa.events[]` | 建议补（成本低，事件叙事要用） |
| D3 | ~~技能明细来源~~ | **已定（2026-11-13）：不补**，AI 只看点数消耗 |
| D4 | 人设口吻（吾妻 / 巫女 / 平实） | 平实起步，persona 作为可删覆盖层 |
| D5 | `records/` 是否加进剧本仓 `.gitignore` 与新目录约定 | 是，按 AGENTS「logs 只放运行期日志」的原意单列 |
| D6 | P2 是否本轮做 | 建议只做模拟局，且排在 P1-D 之后 |

---

## 13. P0 文件级拆解

> 前提：本节只列改动点与验收，不含代码。插件侧在用户确认方案后再动（插件仓纪律）；工具侧可先开工（只用现有散帧，不依赖插件改动）。

### 13.1 P0-A 插件（2 个文件，零新增文件、零 ActionLog 改动）

| 文件 | 改动点 | 验收 |
|---|---|---|
| `GameStatusOutput.cs` | ① 新增 `BuildReview(long charaId)`：正则 `^game(\d+)_turn(\d+)(?:_\d+)?\.json$` 枚举本局帧 → `startTurn/endTurn/frameCount/missingTurns`；mtime 取 `firstCapturedAt/lastCapturedAt`；`midEntry` 读首帧 `baseGame.isFullGame`（缺省 false）② `WriteFinalScore` 增加 `review` 参数并挂进 payload ③ 新增 `PackGame(long charaId, Review review)`：`ZipArchive` 逐条加入本局帧 + finalScore（不含 `thisTurn.json`），条目不套外层目录，`.partial` → `File.Move` ④ 打包失败只记日志，不动散帧 | 6316 / 6317 同构局：zip 可解压、条目数与 `review.frameCount` 一致 |
| `GameStatusSend_GameEnd.cs` | ① `load` 响应回调追加 `loads.Add((currentTurn, unixMs))`，按局清空 ② 正常终局写 finalScore 前取 `review`（含 `loads`）③ 写成功后调 `PackGame`，受 `FinalScoreWritten` 去重 ④ 下一局 `start` 时若上局缺 zip 则补包（可选） | 中途载入局 `midEntry=true`、`loads` 非空；无终局局不产包 |

**不动**：`Class1.cs`、`ActionLog`、帧写入路径与文件名、插件依赖（`System.IO.Compression` 在 net10.0 基础框架内）。
**兼容**：payload 序列化走 `JsonConvert`（`NullValueHandling.Ignore`），旧 AI 解析忽略 `review`。

### 13.2 P0-B 工具（新 bin `review` + 模块，落在 `matsuri-tools`）

| 文件 | 内容 |
|---|---|
| `crates/matsuri-tools/Cargo.toml` | 新增 `[[bin]] review`；依赖加 `matsuri-ai.workspace = true`、`serde.workspace = true`、`serde_json.workspace = true`（工作区已备；按纪律用 `cargo add` 而非手改） |
| `src/bin/review.rs` | 子命令 `open`（首个）/`eval`/`curve`/`report`；参数沿用仓内 `std::env::args` 风格（与 `bench` 一致，不引 clap） |
| `src/review/mod.rs` | 模块声明 + `ReviewError` + 共用类型（`Source`：`Measured/Online/Recomputed/Pending/Unavailable`） |
| `src/review/pack.rs` | 输入解析：`game{id}.zip`（含旧 `records/` 位置）优先，否则散帧目录；局 → turn → 文件名排序；读 `finalScore.review` 段；无 `review` 段直接报错 |
| `src/review/replay.rs` | `ParsedFrame::parse` → `to_base_game` → `MatsuriGame::restore` → 回合状态行（五维 / 体力 / 干劲 / 演目 / 教诲 / 合宿 / 比赛 / 技能点） |
| `src/review/score.rs` | `calc_score` / `score_parts` / `rank_name` / 五维显示值（`DISPLAY_HALF_FROM_STATUS`） |
| `src/review/facts.rs` | `facts.json`：逐回合状态 + 终局 + 每项 `source` 标记（`pending` 带 `turn/seq/kind/est_cost`） |
| `src/review/checks.rs` | 首批 6 条检查项（合宿 / 必赛 / 体力 / 干劲 / 演目槽 / URA 与技能点） |
| `src/review/brief.rs` | `brief.md` 渲染：先 §0 口径 / §1 总体 / §5 其他波动 / §7 检查项，其余留占位 |
| 依赖复用 | `matsuri-ai` 已导出 `ParsedFrame` / `parse_final_score` / `ReplaySource`；`MatsuriGame::restore` 已 pub |

**验收（不需要插件改动即可做）**：对 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin` 下的 6316 / 6317 散帧跑 `review open`，产出 `facts.json` + `brief.md`；161 条 actions 全部落到候选表或标「反推/候选外」；终局评分与在线 `final` 行一致；同输入两次输出逐字节一致。

### 13.3 P0-C 记录层（`matsuri-cli`，可与 P0-B 并行）

| 文件 | 内容 |
|---|---|
| `crates/matsuri-cli/src/record.rs`（新） | `RecordRow` 序列化 + `ai.jsonl` 写入 + `ai-meta.json`（seed / 卡组 / `luck_turn_bonus` / 版本 / 配置指纹 / `mode`） |
| `crates/matsuri-cli/src/online.rs` | `--record <dir>` 开关；每个 `Decision` 输出后写一行；换局按 `epoch` 切目录 |
| `crates/matsuri-ai/src/output.rs` | 新增 `RecordRow` DTO（不动现有 `json_payload` 的 10 字段契约） |

**验收**：`matsuri online --replay <dir> --record <out>` 在 `benchmark/_local/replay_*` 上产出 `ai.jsonl`，行数与决策数一致、`t_n` 与终端运气行一致。

### 13.4 执行顺序与阻塞

1. **P0-B 先开工**（不依赖插件确认），第一可验收点是 6316/6317 的 `open`。
2. **P0-A 等用户确认**插件方案后改 2 个文件，由用户真机跑 1 局验包。
3. P0-C 与 P0-B 并行；P1-B 依赖 P0-B 的 `facts`。
4. 阻塞项：D5 的 `records/` 目录约定（默认按建议值先落，若改只需换常量）、D1 保留策略（默认不删）。

**落地状态（2026-11-13 首切片）**：`review open` 已实现并在真机 6316 散帧上跑通——131 帧 / T0–T77 / 终局 81361（LG8）/ 155 条 actions / 0 解析失败；两次运行 `facts.json` 与 `brief.md` 逐字节一致。当前输出 `facts.json`（逐回合事实 + 终局 + 检查项 + pending）与 `brief.md`（§0/§1/§5/§7 + pending）。待补：zip 输入、`eval` / `curve` / `report`、`--game` 选择（多局混放）、末帧体力上限字段。

**落地状态（2026-11-14 第二切片）**：`review open` 支持目录与局包 zip（`zip` 依赖已登记）；`--game` 可多局混放时选局；旧协议帧（`illEffects`）直接报错。真机 `game6324.zip`：132 帧 / T0–T77 / 终局 77932（LG3）/ 160 条 actions / 0 解析失败；两次输出 hash 一致。`game6323` 散帧（无终局）正确判「未终局」并给 `no_final` 警告。

**落地状态（2026-11-14 第三切片）**：新增 `crates/matsuri-tools/src/review/derive.rs`（帧 diff 反推），事实层扩到 P0 机制门禁。
- 反推：教诲落位（同回合多帧按回合末折叠，避免一次分配拆成两轮）、転成、学技能（用帧原值 `skillPt`，不是回加 `skillPtSpent` 的口径）、追加稽古（`storyId = 400015106`；插件未采集该事件的 `check_event`，选择标不可判并陈述同回合训练命令）、终局 `actions` 归类、训练回合与帧预览**按命令 id 精确匹配**（命令 id 与 `baseGame.training` 键同源，6324 57/57）、继承逐次。
- 判据：数据健康 4 条（无终局 / 缺回合 / 解析失败 / 终局帧无 `review` 段）+ B1 / B4 / B6 / B7 / C3 / C4。B1 在「局没打到该回合」时不判（6323 只到 T15 不再误报 T23/47/67）；C4 只在正常终局判。
- 回归：`game6324.zip`（132 帧）终局 77932 / 训练 57 次（稽古 37、普通 20、失败 0）/ 追加稽古触发 15 次 / 检查项仅「末帧仍有 1 次助力支撑未用」；`game6323` 散帧（26 帧、无终局）只报 `no_final`。
- 待补：B2 / B3 / C1 / C2 需 `eval`（regret + CI）；E7 与剧本专属 (1)(2)(3)(4)(5)(6) 为下一刀；`review/tuning.rs` 与 `review infer` 子命令未开工。

**落地状态（2026-11-14 第六切片）**：曲线 / 报告 / 技能文件。
- `review curve <局> [--from A] [--to B] [--n] [--force]`：复用 eval 逐点（带缓存），输出 `curve.json`（points + summary：首末 T(n)、最大单步跌幅、形态 step/ramp/sawtooth/flat、终局 − 末点 T(n)）。
- `review report <局> [--narrative md]`：合并 facts + 已缓存 eval/curve → `digest.json` + `report.html`（4 张内嵌 SVG：分数轨迹 / 五维 / eval 点 best-chosen / 决策构成；`narrative.md` 注入叙事区，转义后展示）。
- 技能文件落在 `skills/matsuri_review/`：`SKILL.md` + `reference/{metrics_glossary,pitfalls,persona,probe_prompts,uma_specific}.md`（工作流、口径、坑位、平实口吻、追问模板、马娘相关）。
- 实测 `game6324`：`eval --all`（N=64）78 点 → 显著 5 / 跨零 47 / 不可判 26；`curve --from 20 --to 40` 21 点、形态 sawtooth、终局 − 末点 −814；`report` 22 KB / 4 图。

**落地状态（2026-11-14 第五切片）**：事实层补齐剧本专属判据，并补 `infer` 与 `eval --all`。
- `review/tuning.rs`：`Tuning { inherit_expect: 400.0 }`（模拟口径约 400/次），`open --inherit-expect` 覆盖；不进 `matsuri_config`。判据 (4) 只出实际值 + 标准对照（`script.inherit`），不做 verdict。
- `facts.script` 新增：演目节点 / 局末 Lv / 稽古次数 / 卡位（判据 1）、出行段回合与 T23/T36 两条 warn（判据 2）、5 回合阴跌窗口（判据 3）、继承对照（判据 4）、状态资源（判据 6）、自选赛进度（E7，info）。
- `review infer <局> [--what advice,fusion,skill,kensan|all] [--turn N] [--json]`：与 `open` 共用事实层，只写 `infer.json` + 摘要。
- `review eval --all`：本局每个有决策面的回合逐点重算，汇总「显著 / 跨零 / 不可判」。
- 实测 `game6324`：演目1 节点 T7→Lv10、演目4 无卡位只到 Lv6；出行 5/5（T10/11/13/19/22，走满 T22）；继承 T30 216（Δ−184）、T54 150（Δ−250）；状态 小切 / 盾 1 / 幸运 1；自选赛 T12–T26 1/1（达标 T23）；`eval --all` 78 点 → 显著 9 / 跨零 43 / 不可判 26（不可判主要是强制比赛回合走 fallback 与多事件回合）。

**落地状态（2026-11-14 第四切片）**：新增 `review eval`（单点重算）与 rollout 健康度检测。
- 入口：`review eval <局目录|zip> --turn N [--seq k] [--n 1024] [--force]`。只从目标回合**单帧**还原（`OnlineSession::replay_frame`），搜索自己 rollout 到终局；种子与在线同口径（`(master, turn, Probe)`），结果写 `<out>/recompute/turn{NN}_{seq}.json`（候选表 / chosen / best / regret / 配对 CI / T(n) / rollout 健康度）并打一行摘要。
- 实际选择反推：训练命令精确映射（`5101–5105` → 稽古、`301`/`command_type 3` → 休息、`race_entry` → 比赛、`101–106` → 用训练预览与实际帧增量反查训练位）；教诲按回合**末帧**槽位匹配候选（授权帧还没落位）；事件仅当该回合**恰好一条** `check_event` 时判，否则标不可判（帧 storyId 与请求 event_id 不同源）。
- 配对 CI：`record_ordered_rollouts` + `MatsuriSearchTrainer::last_ordered()`，逐 rollout 序号求 best−chosen 的差再算 95% 区间（CRN 下比独立近似少假阴性）。
- rollout 健康度：`FlatSearch::with_probe` 取 planned/succeeded/failed，`MatsuriTerminal` 新增 `career_failed` 维度（自选赛未达标是合法终局、不是 Err），`last_health()` 汇总；在线模式只在 **stderr** 告警（不动小黑板 JSON 契约），eval 落盘。
- 验证（`game6324`，N=256）：T2 教诲 regret 11±150（跨零）、T12 训练 1203±453（显著）、T16 训练 389±433（跨零）、T11 事件因多条 `check_event` 标不可判并给备注；四点 rollout 均 `planned = succeeded`、`career_failed = 0`；同参数二次运行命中缓存（18 ms）。
- 待补：`--all` 批量跑 pending 点；advice 匹配未计 `enforce_advice_constraints` 的重排（实测 T2 命中，失败时留空不猜）；`curve` / `report` / `tuning` / `infer` 仍待做。
