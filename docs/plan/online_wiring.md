# 在线接线方案：Festa 帧 → `MatsuriGame` 还原

> 状态：**E1 / E2 / M1 / M2 / M3 已实施**（2026-11-06），M4（78 帧决策一致率 + 真机闭环）待做。
> 实施中改正的口径见 §9，**以 §9 为准**（§2–§5 保留冻结时的原始判断）。
>
> 本文回答三件事：**①** `SendGameStatusPlugin` 写出的 `thisTurn.json` 里 Festa 剧本段长什么样；
> **②** `matsuri` 侧怎样用这份 JSON + 引擎通用段还原出一个**可继续决策**的 `MatsuriGame`；
> **③** 在线档的 lib/CLI 分层、引擎改动面与验收口径。
>
> 依据：真机 game6310 帧（`Matsuri_Ai_Test/.portable/PluginData/SendGameStatusPlugin/` 下的
> `thisTurn.json`、`game6310_turn12/40.json`，中途 `load` 开局）与插件源码
> `URA_Plugins/SendGameStatusPlugin/{GameStatusSend_Festa.cs,GameStatusSend_Base.cs,Analyzers/FestaAnalyzers.cs,GameStatusOutput.cs}`。

---

## 0. 目标与非目标

| 目标 | 说明 |
|---|---|
| 一帧一决策 | 在线循环每读一帧重建一次局面，因此还原必须是**无状态函数**：`帧 + 本地静态数据 → MatsuriGame` |
| 状态够用 | 还原后的局面要能直接 `list_actions` / 预览训练值 / 结算动作，与真机同回合的离线模拟**逐项一致** |
| 不重放 | 不做「从第 0 回合重跑到当前回合」；随机分布 / 事件结果一律取真机值 |
| 非目标 | 不回写决策给小黑板（现协议无回传通道）；不复原呈现层（`event_log` / `View` 可由还原态重算） |

**关键约束（决定整套方案形态）**：`Session::step_turn`（`umaai_next/crates/umaai/src/session.rs:208-214`）
内部已完成 `parse_base_status → to_base_game → (build)(base) → run_stage`，`build` 只拿得到 `BaseGame`，
拿不到帧原文与 `source / playing_state`；它的路由是 `source == "event" ? 事件 : 训练` 的二分，
覆盖不了骏大祭的 `special` 中间态（`G10`）、同回合重复帧（`G7`）与 `trainLevelTotal == 0`（`G12`）。

⇒ **在线档不走 `Session` 编排**，由 `matsuri-ai` 用引擎的 pub 零件自写循环（§5.2）：
`FileChannel → parse_base_status → to_base_game → parse_festa → restore → 决策`。
引擎只改「帧通道」与「终局帧」两处（§5.3），`session.rs` / `contracts.md` 不动，旧调用点零改动。

```
真机 ──► 小黑板(Host) ──► SendGameStatusPlugin ──► .portable/PluginData/SendGameStatusPlugin/thisTurn.json
                                    ▲                        │
                     EventLogger + FestaScenarioState 快照     ▼
                                                    matsuri-ai 在线循环
                                                              │
                                        FileChannel::next_frame → RawFrame
                                        parse_base_status → to_base_game → BaseGame（通用段，引擎）
                                        parse_festa      → FestaState（剧本段，matsuri-ai）
                                        MatsuriGame::restore(base, festa, data, cfg)
                                                              │
                                                          决策 / 呈现 / 记录（不回溯真机）
```

---

## 1. 两侧现状

### 1.1 引擎侧（`umaai_next`）

| 能力 | 位置 | 对本剧本的含义 |
|---|---|---|
| 帧通道 | `crates/umaai/src/protocol/channel.rs`（**E1 改**） | `thisTurn.json` + `game{id}_finalScore.json`；旧固定名 `finalScore.json` 不再识别 |
| 通用段解析 | `protocol/base.rs::parse_base_status` | 容忍未知键；`BaseStatus` 公开 `source / playing_state / turn / scenario_id / single_mode_chara_id`，`to_base_game` 只投影成 `BaseGame` |
| 底座还原 | `BaseStatus::to_base_game` | `turn / uma / deck / friend / distribution / train_level_count / events / story` 全部落 `BaseGame`，`FrameOutcome.notes` 可带走 |
| 在线编排 | ~~`Session::step_turn`~~ → `matsuri-ai` 自写（§5.2） | `Session` 的 `build` 只拿 `BaseGame`、路由只有 event/train 二分，满足不了 G7/G10/G12；只复用零件，不接 `Session` |
| 终局帧 | `protocol/final_score.rs`（**E2 改**） | 三道闸 + `actionSchemaVersion / actions`；只进 DTO，不进决策链路 |

`BaseParams`（宿主注入）里与剧本相关的两项：`friend_chara_id`（吾妻）与 `friend_first_event`。

> **G14 已关闭**：E1 起引擎只认 `game{id}_finalScore.json`（按文件名模式匹配，不再认固定名）。
> 真机 `game6310_finalScore.json` 带 `single_mode_chara_id=6310`，三道闸走帧内 id 即可。
> 本方案只依赖 `thisTurn.json`，终局帧仅用于 M4 对照。

### 1.2 小黑板侧（`URA_Plugins`）

| 现成件 | 位置 | 提供的能力 |
|---|---|---|
| Festa 帧构造 | `GameStatusSend_Festa.cs` | `FestaFrame`（9 字段）+ `baseGame` 通用段 |
| 帧时机与分发 | `Analyzers/FestaAnalyzers.cs` | 4 类响应 + 8 端点正则；`ResolveSource` 过滤中间态；`ApplyLevelBaseline` 处理等级缺失 |
| 协议投影 | `deps/FestaScenarioAnalyzer/FestaProjection.cs`、`FestaModels.cs` | `FestaTurn` 快照：演目 / 教诲 / 训练位 / 参与者 / 检查点 / 状态 45·47 |
| 逐回合记录 | `EventLoggerPlugin`（`EventLoggerRoundSnapshot`） | `NormalTrainingCounts`（`trainLevelCount` 来源）、`AllEvents`、`KeyEvents`、`RaceHistory`、`SkillPtSpent` 等 |
| 帧写出 | `GameStatusOutput.cs` | 原子写 + 同回合序号；`thisTurn.json` 与 `game{id}_turn{n}[_{k}].json` 双写 |
| 终局帧 | `GameStatusSend_GameEnd.cs` | `game{id}_finalScore.json`，顶层含 `actionSchemaVersion` + `actions`（玩家实际动作，不进决策链路） |

帧构造复用 `FestaScenarioAnalyzer` 快照：`FestaTurn` 是跨插件公开只读 DTO，两个插件版本成对发布。
快照与当前帧的 `CharaId` / `Turn` 不一致时 `baseGame.islegal = false`，整帧不发。

---

## 2. `MatsuriGame` 剧本专属状态盘点

`MatsuriGame`（`crates/matsuri-core/src/game/mod.rs`）除 `base: BaseGame` 外共 24 个字段，
下表按语义合并同类项，另附 3 个 `BaseGame` 里本剧本依赖但通用段还原不完整的字段。
按「还原来源」分五类（**A** 协议直读 / **B** 小黑板端收集 / **C** 小黑板端换算 /
**D** matsuri 侧重算 / **E** 不进帧）：

| # | 字段 | 语义 | 类 | 还原依据 |
|---|---|---|---|---|
| 1 | `persons` | 人头 + 担当演目 + 来源 + 可用回合 | **A+C** | `festa.persons[]` 给 `targetId/actType/origin`；卡人头由 `baseGame.cardId` 推导；`avail_turn` / `needs_great_success` 按静态规则 + `check_points` 反推（`G6`） |
| 2 | `stage` | 阶段机 | **E** | 还原入口固定为 **`Train`**（决策点）；不跑 `Begin`（见 §3.1） |
| 3 | `strategy` | 策略随机流 | **E** | 按会话主种子重新派生（`set_rule_master` 已由 Session 注入） |
| 4 | `data` / `keiko_table` / `keiko_classic_table` | 静态资产 | **D** | 宿主载入 `scenario_matsuri.json` |
| 5 | `keiko_settling` | 稽古结算上下文（瞬时） | **E** | 还原时 `false` |
| 6 | `acts.{gauge, level}` | 演目槽 / 演目 Lv | **A** | `festa.acts[].{gauge, level}`，数组按 `actType 1..5` 排序；`G2` |
| 7 | `acts` 上限 | Lv 上限 / 槽上限 | **A** | `festa.maxActLevel` / `festa.acts[].needGauge` |
| 8 | `advice.{slots, fused}` | 5 槽基础教诲 + 転成 id | **A** | `festa.acts[].{adviceId, fusionId}`，口径见 §3.4（`G1`） |
| 9 | `advice.acquired` | 教诲是否已授予 | **A** | `turn ≥ 2` 或 `acts[].adviceId` 非全 0 |
| 10 | `camp.{year, counted}` | 合宿进度 | **A** | `festa.campRemain` + 静态窗口表反推（§3.2） |
| 11 | `plan` | 回合结构 | **D** | `derive_plan()` 用 `turn` + 已还原的 `camp` + 静态赛程重算 |
| 12 | `forced_rest` | 强制休息 | **D** | 纯函数 `is_forced_rest_turn(chara_id, turn)`（临时硬编码表：凯斯奇迹 T65） |
| 13 | `career_failed` | 自选比赛义务未达标 | **D** | `base.uma.free_races` + `race_history` 自算 |
| 14 | `fatigue_recovery` | 疲劳回复一次性 flag | **A** | `festa.fatigueRecovery`（状态 45）；消费时点等价性见 `G8` |
| 15 | `check_points[3]` | 発表会 / 本番结果 | **A** | `festa.checkPoints[].{scheduleId, resultType}`，int 原值，枚举映射 `G3` |
| 16 | `finals_done` | 本番是否已结算 | **A** | 由 `check_points[2]` 与 `turn` 派生 |
| 17 | `deck_warnings` | 编成警告 | **D** | `inspect_deck`（建局静态重算） |
| 18 | `event_log` | 最近事件记录（呈现用） | **E** | `baseGame.keyEvents` 仅关键族；展示层需要全量事件时由插件补字段 |
| 19 | `event_queue` / `event_cursor` | 本阶段待结算事件队列 | **A** | 通用段 `story` 已装进 `base.unresolved_events`；决策点帧该队列为空 |
| 20 | `assist_support_left` | 助力支撑剩余次数 | **C** | `festa.assistSupportLeft` 给 0/1 有无；次数由 matsuri 累计或插件补历史（§3.7） |
| 21 | `kensan_pending` | 本回合追加稽古是否掷中 | **A** | `festa.kensan`（bool[5] 逐训练位），引擎取任一为真 |
| 22 | `train_level_bonus` | 発表会大成功「全训练 Lv+1」累计 | **C** | `festa.trainLevelTotal` − (`base.train_level_count / 4 + 1`)，下限 0；`trainLevelTotal[i]==0` 见 §3.6 |
| — | `base.train_level_count` | 训练设施点击数（4 次 +1） | **B** | `baseGame.trainLevelCount`（`EventLogger.Current.NormalTrainingCounts`） |
| — | `base.distribution` | 本回合人头分布 | **A** | `baseGame.personDistribution`（int[5][5]，`-1` 空位） |
| — | `base.events` | 已发生事件计数 | **A** | `baseGame.keyEvents`；只覆盖关键族（§3.5） |

**小结**：全部 `G#` 已关闭，**不再需要插件补字段**（§7）——
`avail_turn` / `needs_great_success` 由 `check_points` + 静态规则还原（`G6` 已用 T2 / T47 真机闭环）；
`assist_support_left` 按「有则 1 回合」处理（`G4`，误差方向已知）；
`trainLevelTotal == 0` 按三条规则分辨（`G12`）；编成外神驹的 `targetId` 就是 `chara_id`，
身份不敏感、只需人数结构一致（`G11`）。其余要么已在帧里、要么 matsuri 自己能算、要么不需要。

---

## 3. 逐项口径

### 3.1 入口阶段：为什么不能从 `Begin` 进

`stage_body(Stage::Begin)` 有副作用：`derive_plan` / `forced_rest` / `recover_fatigue` /
`roll_fat_recovery` / `generate_events`（抽随机事件）/ `acquire_advice` / `roll_kensan`。
真机跑到决策点时这些已经发生过，再从 `Begin` 跑一遍会重复抽事件、重复掷追加稽古。

⇒ 还原提供**显式恢复入口**（`MatsuriGame::restore(base, frame, data, config)`），四步：

1. **复用会话已建好的 `BaseGame`**：`uma / deck / inherit / turn / flags / friend /
   distribution / events / train_level_count` 都由通用段还原，重建一次只会丢信息；
2. 复用 `newgame` 的静态装配（`data` / `keiko` 两张表 / `config` / `inspect_deck` → `deck_warnings` /
   `init_persons`），把 `base` 原样接上；
3. **覆盖**剧本字段：`persons`、`acts`、`advice`、`camp`、`check_points`、
   `finals_done`、`assist_support_left`、`kensan_pending`、`train_level_bonus`、`fatigue_recovery`；
4. `plan = derive_plan()`、`forced_rest = is_forced_rest_turn(chara_id, turn)`、
   `career_failed` 按赛程自算、`stage = Stage::Train`。

`init_persons` 一定会跑（它决定 `deck_warnings` 与卡的默认担当），随后被第 3 步整段替换；这一点要写进
`restore` 的注释。

**门禁沿用建局口径**：装配阶段会跑 `inspect_deck`，编成不支持（例如没带 SSR 吾妻）时直接报错。
真机遇到这种局 → 还原失败 → 宿主决定是否提示「本剧本暂不支持该编成」；不要为了能跑而放宽门禁。

### 3.2 合宿：`campRemain` 足够，不需要历史

窗口表固定（0 基）：`12–15 / 36–39 / 60–63`，每年 4 回合；只有**训练类回合**计入 `counted`，
比赛 / 强制休息不计 → `counted` 可能落后于「窗口内已过回合数」，这才是延长段的来源。

`festa.campRemain`（= `festa_info.summer_camp_remain_turn_num`）是服务端下发的**存储值**：

```
counted = 窗口长度(4) − campRemain
year    = max{ i | window[i].start ≤ turn 且 campRemain > 0 }
campRemain == 0 ⇒ year = None（本次合宿已在上一回合 End 结束）
```

真机 T12 见 `campRemain=4`、T40 见 `1`、终盘见 `0`，与窗口表自洽。
延长段（`turn > window.end` 且 `remain > 0`）自然落入 `year = 该窗口年次`、`camp_extended = true`。
**唯一风险**：`remain` 在延长段是否继续减 1（`G9`）。

### 3.3 人头与担当（G11 / G6 已闭环）

**正确性判据：身份不敏感、人数精确。** 离线 `init_roster` 是「五步流水线 + 序取随机马娘」，
真机神驹名单必然不同；但名字 / 具体担当个体不影响训练值——只要**每演目担当人数**与
**每训练位人数、origin 构成**一致即可。

| 必须精确 | 可以不同 |
|---|---|
| 6 张卡：`baseGame.cardId` + `baseGame.persons[i].charaId` | guest 的 `chara_id`（用 `targetId` 或离线随机占位，只需唯一、可解名） |
| 每演目担当人数（卡侧 `card_acts` + 第 1 批 6 + 第 2 批 5） | 具体哪个 guest 落在哪个演目 |
| 每训练位人数与 origin 构成 | guest 的显示名 |
| 理事长 / 记者 / 吾妻的存在与羁绊 | — |

**编号空间（78 帧真机统计）**：`personDistribution` 只出现 `0–7` 与 `>=1000`，**从未出现 100–999**：

```
0–5     → 卡位 1–6（含友人卡）
6       → 理事长（festa.persons targetId = 102）
7       → 记者（targetId = 103）
>=1000  → 编成外神驹，targetId 即 chara_id（= game_id/100 的 4 位口径）
101–115 → 协议侧占位/NPC，actType = 0、从不出现在分布 → restore 直接丢弃
```

- 真机 T77 的 guest `1009/1012/1024/1029/1030` 均能按 `umaDB[chara_id*100+1]` 解出名字；
  `scenario_matsuri.json::acts[].npc_chara_id`（1009/1096/1024/1029/1128）也是同一空间。
- **但真机名单 ≠ 静态配置**（act2 配置 1096、真机 1012；act5 配置 1128、真机 1030）
  ⇒ 必须用帧内 `targetId` 覆盖离线 `init_roster` 的随机结果。
- `festa.persons` 只发 `actType > 0` 的成员；`actType` 缺失的 guest 不参与担当，
  但仍占一个人头 → 建为 `act = None` 的占位，仅计人数、不给羁绊。
- 卡的担当取 `festa.persons[]` 中 `origin == "card"` 的 `actType`（`targetId` 1–6）。

**批次与可用回合（G6，game6309 / game6310 两局一致）**：

| 批次 | 加入时点 | 真机证据 | matsuri 口径 |
|---|---|---|---|
| 第 1 批 6 名 | **T1 末尾** | T0/T1 帧 0 名；**T2** 帧首次出现 6 名 | `avail_turn = 2`、`needs_great_success = false` |
| 第 2 批 5 名 | **T47 末尾** | T47 `ps=49` 仍 6 名；T47 `ps=51` 变 11 名，同帧 `checkPoints` 新增 `{scheduleId:2, resultType:2}` | `avail_turn = 48`、`needs_great_success = true`（`check_points[1]` 大成功） |

`check_points[scheduleId - 1] = resultType`（帧的 `scheduleId` 是 **1 基**），`resultType == 2` = 大成功。

### 3.4 演目与教诲

- `gauge` / `level`：`festa.acts[].{gauge, level}` 直读，数组按 `actType 1..5` 排序；
  `level` 为 **1 基**（真机 T12 见 1、终盘见 10 = `maxActLevel`），`G2` 抓开局帧确认。
- `needGauge`：`festa.acts[].needGauge`，槽上限协议直给（真机恒 5）；
  `outingGainLevel` = `friend_outing_gain_act_level`。
- **教诲 id 口径**：`FestaProjection.cs:135-136` 中
  `adviceId = act_info_array[].advice_id`、
  `fusionId = advice_info_array[advice_id].fusion_advice_id`。真机值：

  | 帧 | 样例值 | 语义 |
  |---|---|---|
  | T12 | `adviceId=501, fusionId=0`；`301/0`；`101/0` | 未転成：基础教诲 101–501，`fusionId=0` |
  | T40 | `adviceId=601, fusionId=101`；`501/0` | 転成后 `adviceId` 为目标 601–1001，`fusionId` 指回原基础 101–501 |
  | 终盘 | `adviceId=801, fusionId=501`；`601/101`；`1001/…` | 同上 |

  **G1 已关闭**：`data/scenario_matsuri.json` 的 `advices` 已是全集
  `{101,201,301,401,501,601,701,801,901,1001}`（每条 10 档 `levels`），
  `fusion` 是 `{101..501}` 的固定升级效果。接线规则：

  ```
  fusionId == 0 → slots[act] = adviceId,  fused[act] = 0      // 未転成
  fusionId != 0 → slots[act] = fusionId,  fused[act] = adviceId // 已転成
  ```

  与 `game/advice.rs` 的 `[601,0,0,0,0]` 测试夹具同义。`fused` 取错会连带影响
  助力支撑 / 特化主属性 / 兼修副属性，仍是 `restore` 的第一优先核对项。

### 3.5 事件历史与 `fatigue_recovery`

`forced_rest` 由纯函数重算（§2 #12）。其余两项：

| 用途 | 帧字段 | 说明 |
|---|---|---|
| `fatigue_recovery` | `festa.fatigueRecovery` | 状态 45 有无；真机 T12 `true`、T40 / 终盘 `false`。协议 flag 与引擎一次性 flag 的消费时点等价性见 `G8` |
| `event_log` | `baseGame.keyEvents` | 友人首次点击 + 支援卡连续事件两族；展示层需要全量事件时由插件补 `festa.events[]` |

> `baseGame.keyEvents` 只覆盖关键族，`raceHistory` 只记取胜回合（0 基）。
> 按事件 id 判定的逻辑需要全量事件时，先补帧字段。
> 事件额外效果已体现在 gauge / 五维 / 体力里，不需要还原。

### 3.6 训练等级：两笔账

离线口径：`level = train_level_count / 4 + 1 + train_level_bonus`（夹 `normal_train_level_max`）。
帧已把两笔账分开：

```
baseGame.trainLevelCount[i] = EventLogger.Current.NormalTrainingCounts[i]
                              （成功普通训练次数，排除稽古 5101–5105、排除训练失败）
festa.trainLevelTotal[i]    = training_level_info_array[].level（含発表会 +1）
train_level_bonus[i]        = max(0, total − (click / 4 + 1))
```

真机 T40：`trainLevelCount=[3,1,0,1,2]`、`trainLevelTotal=[3,2,2,2,3]`。

**G12 已关闭：三条规则，绝不写 0。**

| 帧值 | 判定 | 处理 |
|---|---|---|
| `trainLevelTotal[i] > 0` | 有效等级 | 直接更新；`bonus = max(0, total − (click / 4 + 1))` |
| `== 0 && kensan[i]` | 稽古位（防御分支） | 沿用上一帧基准 |
| `== 0 && !kensan[i]` | 整帧无效（`source ∈ {load, event, special}`） | 整组沿用上一帧基准，标 `level_unknown[i]` |

**真机依据**：78 帧统计中 `0` 只出现在 `load / event / special` 的**整组**帧
（T0 load、T2 special、T3 event 全 0）；43 个含 `kensan=true` 的 `command` 帧
`trainLevelTotal` **全部 > 0**（T25 `[2,2,2,2,2]` + kensan `[T,F,F,F,T]`、
T40 `[2,3,2,2,3]` + kensan `[F,T,F,T,F]`）。即「单点 0 + 稽古」在现有真机未出现，
规则 2 仅作防御；`kensan` 只用于标注稽古位，不参与等级判定。

稽古 5101–5105 的 `level` 恒为 1、是稽古自身等级，不参与训练位等级计算。

### 3.7 助力支撑与追加稽古

| 量 | 帧字段 | 说明 |
|---|---|---|
| `assist_support_left` | `festa.assistSupportLeft`（0/1，`FestaTurn.AssistSupportActive` = 状态 47 有无） | **G4 口径**：有 → `assist_support_left = 1`；下一帧仍有则保持 1；消失 → 0。真实上限 3 次，改为逐帧刷新后上界是「状态持续回合数」，方向为**高估**，已接受 |
| `kensan_pending` | `festa.kensan`（bool[5]，来自 `FestaTraining.UpperTraining` = `is_upper_training_event_occurred`） | 引擎取任一为真；真机 T40 `[false,false,false,false,true]`、终盘 `[true,false,false,false,true]` |

两者都直接决定训练值，「拿不到就不显示」在面板可接受、**在决策链路上不可接受**（少 5% 训练效果会改变排序）。
`G4` 已按「有则 1 回合」定档（误差方向高估，用户接受）；`kensan` 的逐位分辨见 §3.6（`G12`）。

### 3.8 `baseGame.training`：训练预览的协议真值

`GameStatusSend_Festa.cs::ApplyTraining` 遍历 `home_info.command_info_array`，只收
`command_type == 1 && is_enable == 1` 的指令，按**指令 id** 写入字典：

```jsonc
"training": {
  "5101": { "failureRate": 0,
            "lower": [50, 0, 22, 0, 0, 66, -19],
            "upper": [57, 10, 13, 10, 10, 55, 0] }
}
```

- `lower` 取 `home_info.command_info_array`（游戏下层），`upper` 取 `festa_data_set.command_info_array`（剧本上层），
  逐项相加即总增量；`failureRate` 为协议原值。
- 两个数组都是 **7 项定长**：速 / 耐 / 力 / 根 / 智 / 技能点 / 体力（负数为消耗）；**未做 1200 衰减**。
- key 原样保留剧本专属 id：稽古 `5101–5105`、转成 `601–605`、基础 `101–106`；指令 id → 训练位的映射由 matsuri 负责。
- 不可训练回合为空对象 `{}`。该字段目前仅骏大祭填充，其余剧本为 `null` 不输出。

**G13 已定：`training` 只用于验算，不参与状态恢复、也不参与 rollout。**
`restore` 不读该字段；`train_level` / `training_value` 全部由 `stage + persons + bonus` 重算。
验算只在 `matsuri-ai` 的 `--verify-training` 模式里做：把 `preview_training` 的 `lower + upper`
与帧值逐项比对、输出偏差表；默认在线运行不读、不打印，避免「用真值喂决策」。

---

## 4. `thisTurn.json` 的 Festa 帧格式

> 以下字段名、类型、量纲以插件真机输出为准，可直接照此写 `serde` 结构体。

### 4.1 顶层与写出时机

顶层两段（序列化顺序 `festa` → `baseGame`，JSON 对象无序）：

```jsonc
{ "festa": { /* 剧本段，§4.2 */ }, "baseGame": { /* 通用段，§4.3 */ } }
```

- `scenarioId = 15` 在 **`baseGame.scenarioId`**；段名固定 **`festa`**。
- 序列化用 `NullValueHandling.Ignore`；`AnalyzeFesta` 在 `islegal == false || festa == null` 时直接不写帧，
  所以正常帧都带 `festa`。
- **写出时机**（`Analyzers/FestaAnalyzers.cs`）：注册 `load` / `start` / `exec_command` 三类精确端点 +
  `check_event` 类型配 8 端点正则（`attach_advice|check_event|check_point|continue|festa_live|
  gain_skills|race_entry|race_out`）；`change_short_cut` / `finish_claw_crane` 不带 `home_info`，不注册。
  要求 `home_info.command_info_array.Length >= 5` 且 `EventLoggerPlugin` / `FestaScenarioAnalyzer` 均可用；
  `ResolveSource(unchecked_event_array)` 返回 `null` 的中间态 / 待选事件帧不写。
- 同回合可能有多帧（文件名 `_2` / `_3` 后缀，真机 game6310 有 `turn14_2.json` 等），
  在线循环按 `G7` 去重；`baseGame.source` 取值 `load / command / event / special`。
- 快照错帧（`snapshot.CharaId/Turn` 与当前响应不一致）→ `baseGame.islegal=false`，整帧不写。

### 4.2 `festa` 段字段表（`FestaFrame`，9 个字段）

| 字段 | 类型 | 协议来源 | 语义 / 注意 |
|---|---|---|---|
| `acts` | array | `festa_data_set.act_info_array`（按 `act_type` 升序） | 5 个演目，元素见下表 |
| `maxActLevel` | int | `festa_info.max_act_level` | 演目 Lv 上限（真机 10） |
| `campRemain` | int | `festa_info.summer_camp_remain_turn_num` | 合宿剩余（存储值，§3.2） |
| `assistSupportLeft` | int `0\|1` | 状态 47 有无 | 有无标记；剩余次数另算（§3.7） |
| `fatigueRecovery` | bool | 状态 45 有无 | 协议直读（§3.5） |
| `checkPoints` | array | `check_point_result_array` | `{scheduleId:int, resultType:int}` 原值；真机晚期 3 项全为 `resultType=2` |
| `trainLevelTotal` | int[5] | `training_level_info_array[].level` + 快照基础项 | `0` = 本帧无有效值（§3.6） |
| `kensan` | bool[5] | `FestaTraining.UpperTraining` | 逐训练位追加稽古标记，引擎取 OR |
| `persons` | array | `festa_data_set.evaluation_info_array`（仅 `actType>0`） | `{targetId:int, actType:int, origin:string}`；`origin ∈ card/friend/guest/yayoi/reporter` |

`acts[]` 元素（`FestaActFrame`）：

| 字段 | 类型 | 协议来源 | 语义 |
|---|---|---|---|
| `actType` | int 1–5 | `act_type` | 与训练位 0–4 同序 |
| `gauge` | int | `act_gauge` | 当前演目槽 |
| `level` | int | `level` | 演目 Lv（1 基，`G2`） |
| `needGauge` | int | `need_act_gauge` | 升级所需槽（真机恒 5） |
| `adviceId` | int | `advice_id` | 当前教诲 id（未転成 101–501；転成后 601–1001，§3.4） |
| `fusionId` | int | `advice_info_array[advice_id].fusion_advice_id` | 未転成 0；転成后指回原基础 101–501 |
| `outingGainLevel` | int | `friend_outing_gain_act_level` | 友人出行演目 Lv 增益 |

### 4.3 `baseGame` 段与本剧本相关的字段

`baseGame` 共 46 个键（`GameStatusSend_Base<T>`，`T = PersonBase`），camelCase 与 snake_case 混用，按原名读：

| 字段 | 类型 | 说明 |
|---|---|---|
| `scenarioId` | int | 15 |
| `turn` | int | **0 基**（0–77） |
| `vital` / `maxVital` / `motivation` | int | 体力 / 上限 / 干劲 1–5 |
| `fiveStatus` / `fiveStatusLimit` | int[5] | 已 `ReviseOver1200` |
| `skillPt` / `skillScore` / `totalHints` / `skillPtSpent` | int | 技能点 / 已买技能分 / Hint 总等级 / 累计花费 |
| `trainLevelCount` | int[5] | 成功普通训练次数（历史点击数，§3.6） |
| `training` | `map<int, {failureRate, lower[7], upper[7]}>` | 训练预览协议真值（§3.8），仅骏大祭 |
| `cardId` | int[6] | 支援卡 id |
| `persons` | array[6] | `{personType, charaId, friendship, isHint, cardRecord}`；`charaId` 为真实角色 id |
| `personDistribution` | int[5][5] | 每训练位最多 5 人，`-1` 空位；编号空间见 §3.3 |
| `lockedTrainingId` | int | 骏大祭恒 `-1` |
| `friend_stage` / `friend_outgoingUsed` | int | 友人阶段 0/1/2、已出行段数 |
| `friendship_noncard_yayoi` / `friendship_noncard_reporter` | int | 非卡理事长 / 记者羁绊 |
| `playing_state` | int | 1 = 可决策；非 1 为重复回合（真机 T40 为 1） |
| `source` | string | `load / command / event / special` |
| `isFullGame` | bool | 中途加载为 `false`（game6310），此时前半局历史不完整 |
| `isAfterTraining` / `islegal` | bool | 本回合是否已执行指令 / 帧是否合法 |
| `raceHistory` | int[] | 取胜回合（0 基） |
| `keyEvents` | int[] | 关键族 `story_id`（§3.5） |
| `inheritGains` / `inheritLimitGains` | int[] / int[][] | 继承属性增量 / 五维上限增量（协议原值） |
| 状态标志 | bool/int | `isQieZhe` `isXiaoQie` `isAiJiao` `PositiveThinkingCount` `isRefreshMind` `isIll` `LuckyCount` `isRacing` `ptScoreRate` `failureRateBias` `zhongMaBlueCount` `saihou` 等 |
| `illEffects` | int[] | **生病病种**（游戏 `chara_effect_id` 原值，**文本库分类 142** 口径：1 熬夜 / 2 懒惰成性 / 3 皮肤粗糙 / 4 变胖 / 5 偏头痛 / 6 不擅长练习）。`isIll` 只是「有没有病」的布尔；本字段 2026-11-08 新增，旧插件不给 → 退化 `Ill::UNKNOWN` |
| `storyId` | int? | **待选事件 id**（`evt.story_id` 原值，取事件串里**第一个**多选项事件）。出现条件 = 本帧 `unchecked_event_array` 有 `choice_array.Length > 1` 的事件；单选项自动事件不发帧。效果表命中时另有 `story`，**未收录时只有本字段**，选项数据由接收端自持 |

> **病种还原（`G15`，2026-11-08）**：引擎 `BaseStatus::to_flags` 只有布尔 → `Ill::UNKNOWN`，`Ill::FAT`（变胖）
> 永远认不出来（训练速度分量不归零 → 高估速度训练）。插件补 `illEffects` 后由 `matsuri-ai::frame` 读出、
> `RestoreInput.ill_effects` 传给 `restore.rs::ill_from_effects`。
> 映射**按 142 表**（`Matsuri_AI/data/text_data_dict.json` 分类 142）：
> `1 熬夜 → NIGHT` / `2 懒惰成性 → LAZY` / `3 皮肤粗糙 → SKIN` / `4 变胖 → FAT` /
> `5 偏头痛 → HEADACHE` / `6 不擅长练习 → BAD_TRAINER`。
> **不要按 `Ill` 的位序推**（142 是日文 UI 列表顺序，位序是 变胖/皮肤/头痛/摸鱼/熬夜/不擅长），
> 也**不要**和事件数据里的 `add_flags.ill` 混用——那里是位集（`1` = 变胖）。
> 缺字段时**不覆盖**引擎的 `UNKNOWN`，行为与旧版一致（守门① 走治病）。

> **事件帧（`G16`，2026-11-08）**：插件只透传 `storyId`，接收端按 id 查本地事件表
> （本仓 `scenario_events` → 引擎 `story` / `uma` / `card` / `friend` / `system` 五表）
> 还原 `EventData` 并注入 `unresolved_events`（**本仓数据优先**：引擎按 `story` 建过的同 id 事件会被覆盖，
> 帧只提供选项文本；选项数不一致按本仓并打帧提示）；**查不到时路由直接跳过**，绝不回落训练建议
> （事件帧的 `training` / 分布 / 等级都是残值）。剧本关键事件里，追加稽古走 `scenario_events`
> （`400015106`）、友人出行第三段走 `friend_events`（`809052013`），均已覆盖。

### 4.4 真机片段（`game6310_turn40.json`，值已核对）

```jsonc
{
  "festa": {
    "acts": [
      { "actType": 1, "gauge": 5, "level": 6, "needGauge": 5,
        "adviceId": 601, "fusionId": 101, "outingGainLevel": 0 }
      // … 共 5 项，actType 1..5
    ],
    "maxActLevel": 10,
    "campRemain": 1,
    "assistSupportLeft": 1,
    "fatigueRecovery": false,
    "checkPoints": [ { "scheduleId": 1, "resultType": 2 } ],
    "trainLevelTotal": [3, 2, 2, 2, 3],
    "kensan": [false, false, false, false, true],
    "persons": [
      { "targetId": 1, "actType": 1, "origin": "card" },
      { "targetId": 1009, "actType": 1, "origin": "guest" }
      // …
    ]
  },
  "baseGame": {
    "scenarioId": 15, "turn": 40, "vital": 77, "maxVital": 108, "motivation": 5,
    "playing_state": 1, "source": "command", "isFullGame": false,
    "trainLevelCount": [3, 1, 0, 1, 2],
    "fiveStatus": [1382, 739, 902, 733, 727],
    "fiveStatusLimit": [3288, 2504, 2516, 2238, 2414],
    "cardId": [303233, 302424, 302831, 303044, 302894, 303114],
    "personDistribution": [[0,1,5,1009,-1],[3,1096,-1,-1,-1],[2,1024,-1,-1,-1],
                           [1029,1133,-1,-1,-1],[4,5,1128,-1,-1]],
    "lockedTrainingId": -1,
    "keyEvents": [830242001, 830283001, 830283002, 830311002, 830311003, 830289001],
    "raceHistory": [11, 23, 38],
    "training": {
      "5101": { "failureRate": 0, "lower": [50,0,22,0,0,66,-19],
                "upper": [57,10,13,10,10,55,0] }
      // … 共 5 项
    }
  }
}
```

### 4.5 matsuri 侧解析约定

- `serde` 结构体**不加 `deny_unknown_fields`**（引擎纪律：插件新增字段不得让整帧失败）；
  `#[serde(rename_all = "camelCase")]` 时，`baseGame` 里的 snake_case 键
  （`single_mode_chara_id` / `friend_stage` 等）单独 `#[serde(rename = ...)]`。
- 缺 `festa` 段 → 报「不是骏大祭的帧」，不退化成默认剧本状态。
- `trainLevelTotal` / `assistSupportLeft` / `checkPoints` 的「0 / 空 / 原值」要区分
  「未知」与「零值」，逐字段判空，不用 `unwrap_or(0)`。
- `persons[].targetId` 与 `baseGame.personDistribution` 同编号空间，可直接互查；
  `targetId` 是协议 id，NPC 的 ≥1000 不是角色 id（见 `G11`）。

---

## 5. 两端改动清单

### 5.1 小黑板端（`SendGameStatusPlugin`）——已完成

| 项 | 落点 |
|---|---|
| 帧构造 | `GameStatusSend_Festa.cs`（`FestaFrame` / `FestaActFrame` / `FestaCheckPointFrame` / `FestaPersonFrame`） |
| 端点注册与帧时机 | `Class1.cs` + `Analyzers/FestaAnalyzers.cs`（`FestaResponse.From*` / `ResolveSource` / `ApplyLevelBaseline`） |
| 训练点击数历史 | `ApplyTrainLevelCounts` → `EventLogger.Current.NormalTrainingCounts` 写 `baseGame.trainLevelCount` |
| 人头分布 / 角色 id / 友人阶段 | `ApplyPersonDistribution` / `ApplyPersonCharaIds` / `ApplyFriendStage` |
| 训练预览真值 | `ApplyTraining` → `baseGame.training` |
| 帧写出 | `GameStatusOutput.WritePluginData`（原子写 + 序号） |
| 架构 | 复用 `FestaScenarioAnalyzer` 快照（`FestaTurn` 为跨插件公开 DTO） |

**已无阻塞性缺口**：`G11` / `G4` / `G12` 均已在 matsuri 侧闭合（§3.3 / §3.7 / §3.6），
**不需要插件补字段**。仅保留一个非阻塞的展示层可选补强：

| 可选补强 | 影响 | 触发条件 |
|---|---|---|
| 全量事件 `festa.events[]` | `event_log` 只覆盖 `baseGame.keyEvents` 的关键族 | 展示层需要全量事件时再加 |

> 帧字段本身本轮不动；`SendGameStatusPlugin.zip` 不重打，避免依赖闭包成套同步。

> 插件 zip 的依赖闭包是 5 个包（`EventLoggerPlugin` / `FestaScenarioAnalyzer` /
> `RamenScenarioAnalyzer` / `LegendScenarioAnalyzer` / `PioneerScenarioAnalyzer`），
> 重打 `SendGameStatusPlugin.zip` 需**成套同步**，否则会出现 `MissingMethodException`。

### 5.2 matsuri 端（`umaai-matsuri`）——待实施（P2）

**分层纪律**：`matsuri-core` / `matsuri-ai` 只出 lib，不打印、不解析参数、不落盘；终端依赖只在
`matsuri-cli`。引擎只提供协议 / 帧 / 评分零件；在线编排、剧本段解析、DTO 全在剧本仓。

| crate | 文件 | 内容 |
|---|---|---|
| `matsuri-core` | `game/festa.rs`（新） | `FestaState`：`festa` 9 字段纯数据（camelCase、`Deserialize`）。定义放 core，避免 core←ai 重复结构 |
| `matsuri-core` | `game/restore.rs`（新） | `MatsuriGame::restore(base: BaseGame, festa: &FestaState, data: Arc<MatsuriData>, cfg: MatsuriConfig) -> Result<Self>`（§3.1 四步）；附 `export_festa(&self) -> FestaState` 供 round-trip / 工具面 |
| `matsuri-ai` | `frame.rs`（新） | `{"festa": FestaState}` 解析 + `scenarioId == 15` 校验；缺段 → 「不是本剧本的帧」；不加 `deny_unknown_fields`；`baseGame` 交引擎 `parse_base_status` |
| `matsuri-ai` | `urafile.rs`（新） | `FrameSource { Live(FileChannel), Replay(dir) }` + 目录探测（`--root` > `MATSURI_ROOT` > `.portable`）；**不自建 watcher**，稳定读 / 去重 / 终局帧全用 `FileChannel` |
| `matsuri-ai` | `online.rs`（新） | 在线编排：`next_frame → parse_base_status → to_base_game → parse_festa → restore → set_rule_master → 路由 → 决策`；G7 幂等、G10 过滤、G12 基准、G13 验算开关 |
| `matsuri-ai` | `session.rs`（新） | 装配：`BaseParams`（吾妻 / `809052001`）、`Arc<MatsuriData>`、`MatsuriConfig`、`DecisionSource` |
| `matsuri-ai` | `output.rs`（新） | `OnlineTurnOutput { turn, scenario, kind, record: DecisionRecord, view: MatsuriView, notes }`；JSON 与面板共用同一 DTO |
| `matsuri-cli` | `online.rs`（新） | 循环 + 两渲染器：human 复用 `panel.rs::BoardPanel`；`--json` 逐行序列化旧 10 字段 |
| `matsuri-cli` | `main.rs` | `--version` 前置分流；`online` 子命令（`--root / --replay / --trainer / --json / --verify-training`）手写解析，**不引 clap** |

**必须照抄的 4 条引擎口径**（否则与离线 / 真机不一致）：

1. 每帧 `set_rule_master(seed)`（`session.rs:215-216`）；
2. 事件帧 `EventRng::new(derive_seed(master, &[turn, StreamTag::Event.tag()]))` 与决策 rng 分离（`session.rs:266-279`）；
3. 事件帧仅「多选项」记一条决策（`session.rs:290`）；
4. 终局三道闸顺序：必填解析 → `is_usable` → `matches_epoch` → 算分（`session.rs:322-351`）。

**JSON 口径（路线 A，严格沿用旧 schema）**：每行一条，顶层 10 字段
`type / turn / scenario / decision_kind / action_index / score / candidate_scores /
candidate_descriptions / candidate_n / scenario_extra`；`type ∈ decision/info/error/final`。
`--json` 时 stdout 严格 JSON、横幅与日志走 stderr、关着色。手写档无搜索分：
`score = 0.0`、`candidate_n = 0`，文档标注「0 = 无搜索分，非有效评分」。
`view` / `actions` **不输出**（`OnlineTurnOutput` 内保留，留给未来消费者）。

`restore` 里**必须**保留的两条注释（防止后人「优化」）：

1. `init_persons` 的分配结果会被整段覆盖——离线分配只为 `deck_warnings` 与兜底；
2. 不得从 `Begin` 进入（会重复抽事件 / 重复掷追加稽古）。

### 5.3 引擎端（`umaai_next`）——E1 / E2，非破坏

| 项 | 位置 | 内容 |
|---|---|---|
| **E1** 终局帧按局号 | `protocol/channel.rs` | `FrameKind::from_file_name` 只认 `thisTurn.json` + `game{id}_finalScore.json`；内部 `FrameTarget { kind, file_name }`；`matched_kinds → matched_targets`；按文件名读、按文件名去重；启动多候选取 mtime 最新；新增 `FINAL_SCORE_SUFFIX`，`FRAME_FINAL_SCORE` 保留但标 `deprecated`、不参与匹配 |
| **E2** 终局帧新字段 | `protocol/final_score.rs` | `#[serde(default)] action_schema_version: Option<u32>`、`#[serde(default)] actions: Option<Vec<FinalAction>>`；`FinalAction { turn, endpoint, #[serde(flatten)] fields }`；三道闸不动 |
| 兼容性 | — | `Session::new` 签名不变；`FileChannel::open / next_frame / drain_notes / plugin_dir` 签名不变；`Runner` / `Frontend` / 既有 trait 一行不改；旧 `Session::new` 调用点零改动 |
| 测试同步 | `channel.rs` 内联 6 处、`crates/umaai/tests/channel.rs` 3 处、`umaai-tests/tests/session_basic.rs` 2 处 | 造帧名改新格式；新增「旧 `finalScore.json` 不再识别」反向用例 |

验收：引擎四门 + `umaai-tests` 全绿；`umaai-rs-ref` 不动。

---

## 6. 落地顺序与验收

> 小黑板端写帧（旧 M1）已完成，不再占阶段号；下表从引擎改动开始编号。

| 阶段 | 内容 | 验收 |
|---|---|---|
| **E 引擎** | E1 `channel.rs`（终局帧只认新名）+ E2 `final_score.rs`（`actionSchemaVersion` / `actions`） | 引擎四门 + `umaai-tests` 全绿；`Session::new` / `FileChannel` 旧签名与旧调用点零回归；新增「旧 `finalScore.json` 不再识别」反向用例 |
| **M1 还原** | `FestaState` + `restore` + `export_festa` | 离线 round-trip：`restore(base.clone(), export_festa(&g), …)` 与 `g` 逐字段 `assert_eq`；白名单只排除 `strategy` / `keiko_settling` / `event_log`。另断言：`persons_per_act` 逐演目相等、每训练位人数与 origin 构成相等、卡位 `chara_id` 完全相等（G11 口径）；`level_unknown` 按 G12 规则 |
| **M2 在线编排** | `frame.rs` + `urafile.rs` + `online.rs` + `output.rs` | 单帧一决策不报错；同回合重复帧只决策一次（G7）；`source = special` / `playing_state != 1` 不入决策（G10）；`trainLevelTotal = 0` 的帧等级不跳变（G12）；`--verify-training` 能输出偏差表（G13） |
| **M3 CLI** | `online` 子命令 + 面板 / JSON / `--version` | 终端逐帧出面板；`--json` 每行一条、无杂字；`--version` 可核对；横幅与日志走 stderr |
| **M4 闭环** | 终局帧消费 + `--replay` 全量收束 + 真机换局 | 终局帧三道闸、真机终局不退出、换局状态不串；**决策一致率审计按用户决定留到复盘阶段**（见 §9.10） |

M1 的 round-trip 是**整套方案的核心守门**：它能在没有真机的情况下，先把「帧字段 ↔ 状态字段」
的映射钉死；真机只用来验证取值口径（`§7` 的 `G#`）。M2 起改用真机 `thisTurn.json` 做端到端核对。

---

## 7. 待核清单

### 7.1 已关闭（真机帧核对完毕，结论已写回 §3 / §5）

| 编号 | 结论 | 依据 |
|---|---|---|
| **G1** | 教诲 id 全集已在仓内：`advices = {101,201,301,401,501,601,701,801,901,1001}`（各 10 档）、`fusion = {101..501}` | `data/scenario_matsuri.json`；映射规则见 §3.4 |
| **G4** | 助力支撑「有则 1 回合」：`assistSupportLeft = 1` → `assist_support_left = 1`，下帧仍有保持、消失归零；误差方向为高估 | §3.7；用户已接受 |
| **G6** | 第 1 批 6 名 T1 末尾加入（T2 帧首见）、第 2 批 5 名 T47 末尾加入（T47 `ps=51` 帧首见，同帧 `checkPoints` 新增 `scheduleId=2`）；`resultType=2` = 大成功 | game6309 / game6310 两局一致；§3.3 |
| **G11** | `targetId >= 1000` 即 `chara_id`（4 位口径）；身份不敏感、人数精确；`101–115` 必须过滤 | 78 帧分布只含 `0–7` 与 `>=1000`；§3.3 |
| **G12** | 三条规则：`>0` 更新；`0 + kensan` 沿用基准；`0 + !kensan` 整帧无效、整组沿用基准；绝不写 0 | 78 帧统计；§3.6 |
| **G13** | `baseGame.training` 只用于 `--verify-training` 验算，不参与 restore / rollout | §3.8 |
| **G14** | E1 起引擎只认 `game{id}_finalScore.json`；旧固定名不再识别 | §5.3 |

### 7.2 仍待实施中验证

| 编号 | 问题 | 现状 / 判定 | 影响 |
|---|---|---|---|
| **G2** | `acts[].level` 起点 | 倾向 1 基（T12 见 1、终盘 10 = `maxActLevel`）；M2 用开局帧确认 | 演目 Lv 偏移 1 级 |
| **G3** | `resultType` 的 0 / 1 取值 | 已确认 `scheduleId ∈ {1,2,3}` 对应三场発表会、`resultType = 2` = 大成功；失败 / 普通档未采样 | `check_points` / `finals_done` |
| **G5** | `evaluation_info_array` 的担当覆盖 | 只发 `actType > 0`；`101–115` 占位不在分布（已按人数口径规避） | 已降级为展示层问题 |
| **G7** | 同回合重复帧幂等 | 只按**内容哈希**幂等（同内容只算一次）；`_2` / `_3` 这类同回合不同内容的帧照常决策（2026-11-07 改：玩家会反复进出同一状态，去重会让面板停在旧建议） | 同回合可能多次决策（搜索档按帧计费） |
| **G8** | 疲劳回复 flag 消费时点 | 帧直读状态 45；与引擎一次性 flag 的等价性未验 | 体力 +15 一次性收益 |
| **G9** | 延长段 `campRemain` 是否继续递减 | 未采样到延长段连续帧 | `camp.counted` 反推 |
| **G10** | `playing_state` 取值组合 | 已见 `1`（可决策）、`5`（event）、`49/51`（special）；定「`!= 1` 不决策」 | 中间态误判 |

---

## 8. 与既有文档的关系

- 现行交接：`F:/UmaAI_Active/.workbuddy/handoff-matsuri-wiring_2026-11-06.md`（本轮已按 P2 方案更新）。
- **决策记录**：在线档不走 `umaai::Session` 编排（§0），只复用协议解析 / `FileChannel` /
  `FinalScoreFrame` / 评分零件；该决定**替代**本文早期版本的 `Rc<RefCell<Option<FestaFrame>>>` 共享槽方案。
- 插件侧帧契约：`URA_Plugins/SendGameStatusPlugin/README.md`（骏大祭段、`baseGame.training`、终局帧）。
- 机制口径：`mechanism.md`（何时结算、`plan` 含义）、`matsuri_memo_cn.md` §2.2（担当五步流水线）。
- 术语：`glossary.md`（`MatsuriView` / 追加稽古 / 助力支撑 / 合宿延长）。
- 引擎契约：`../../umaai_next/docs/scenario/contracts.md` §5（`Session` 帧驱动）、§7（View DTO）。
- 进度：`plan/scenario_dev_plan.md` S8「在线外壳」；落地后本文件升为实施记录，`G#` 结论写回 `glossary.md`。

---

## 9. 实施记录（2026-11-06）

### 9.1 落地清单

| 阶段 | 落点 | 验收 |
|---|---|---|
| **E1** | `umaai_next/crates/umaai/src/protocol/channel.rs`：`FrameKind::from_file_name` 只认 `thisTurn.json` + `game{digits}_finalScore.json`；`FrameTarget{kind,file_name}`；`matched_targets` 按文件名去重；启动多候选取 mtime 最新；`FINAL_SCORE_SUFFIX` 新增、`FRAME_FINAL_SCORE` 标 deprecated 不参与匹配 | 引擎全量测试绿；新增「旧 `finalScore.json` 不再识别」「多局终局帧只推最新」「新局新文件名照常入队」反向 / 正向用例 |
| **E2** | `protocol/final_score.rs`：`actionSchemaVersion` / `actions: Option<Vec<FinalAction>>`（端点私有键 `#[serde(flatten)]` 原样收） | 旧帧 `None`、新帧 `Some(259)` 解析用例；三道闸不动 |
| **M1** | `matsuri-core/game/festa.rs`（`FestaState` 9 字段）+ `game/restore.rs`（`restore` / `export_festa` / `check_roster` / `map_distribution`）；`newgame` 的静态装配抽成 `assemble` 供两处复用；`stage::is_forced_rest_turn` 开放到 crate | `crates/matsuri-core/tests/restore.rs` 10 例：决策点 round-trip、T0 名单保持、等级基准沿用、`AfterTrain` 入口、缺 SSR 吾妻拒绝、未知神驹拒绝、人头顺序错位拒绝、分布 id 映射、発表会结果映射 |
| **M2** | `matsuri-ai/{frame,urafile,session,online,output}.rs` | 单测 6 例 + `tests/replay.rs`：真机 250 帧（3 局）**全部还原成功**，决策 187（train / race / event），重复帧幂等 |
| **M3** | `matsuri-cli/src/online.rs` + `main.rs` 分流（`online` 子命令、`--version`） | `matsuri online --replay <dir>` 出面板、`--json` 逐行 10 字段、日志走 stderr |

### 9.2 实施中改正的口径（**以本节为准**）

| # | 文档原判断 | 真机 / 代码事实 | 现口径 |
|---|---|---|---|
| 1 | `persons[].targetId` = 卡位，`targetId-1` 即卡组下标 | **不成立**。开新局：友人占 1、卡从 2 起（6309 T12 见到 `1=friend`）；中途加载局友人不进 `evaluation_info_array`，卡从 1 起（6310 T12 只有 `1–5`） | 按 `targetId` 升序走帧人头，`origin == card` **依次**落到卡组非友人卡位；`origin == friend` 只标记不参与担当。另加 `check_roster`：`baseGame.persons[i].charaId` 必须与 `deck[i]` 对得上，否则拒绝计算 |
| 2 | `festa.persons` 只发 `actType > 0` | T0/T1 帧把**整份名单**都发了（`101–115` 占位 + 理事长 / 记者，`actType` 全 0） | 按 `actType > 0` 过滤担当；`origin == guest` 且 `targetId < 1000` 一律当占位丢弃 |
| 3 | `trainLevelTotal == 0` 只出现在 `load/event/special` | **合宿期 `command` 帧**（T13–T15）也整组为 0 | 逐位判空：`tlt[i] > 0` 用帧值，否则沿用上一帧（首帧退回 `click/4+1`）；不变式 `train_level(i) == tlt[i]` 写成测试 |
| 4 | 神驹 `avail_turn = 2 / 48`、第 2 批需大成功 | 帧内神驹按 `targetId` 升序排列，推不出批次 | 帧内神驹一律 `avail_turn = 0`、不设大成功门槛（出现在帧里即已可用）。M1 白名单因此多了 `persons[].avail_turn` / `needs_great_success` 两项 |
| 5 | `maxActLevel` / `needGauge` / `outingGainLevel` 协议直读 | `MatsuriGame` 没有这三个字段，实际由静态资产决定（`rules.act_level_max_with_friend` / `act_gauge_max` / 融合表） | **不进状态**；帧值只作 `--verify-training` 校验素材 |
| 6 | 入口阶段固定 `Train` | `isAfterTraining = true` 的帧（如 6310 T35）动作已执行，再报训练决策会打架 | 入口阶段由帧决定：`isAfterTraining` → `AfterTrain`，否则 `Train`；`Distribute` 一律跳过（会重掷站位打乱人头） |
| 7 | `playing_state != 1` 一律不决策 | `ps = 5` 的 `event` 帧带待选事件，全跳过会丢事件决策 | 只有 `source == special`（`ps = 49/51`）才跳过；其余按「多选项事件 → 事件决策；`AfterTrain` 且无事件 → 跳过；`Train` → 动作决策」 |
| 8 | `restore(base, frame, data, cfg)` 四参 | `restore` 还要「上一帧等级基准」「入口阶段」「帧人头（卡位自检）」「非卡人头羁绊」 | 改为 `RestoreInput` 结构体（字段见 `game/restore.rs`），新增材料不再改签名 |
| 9 | 「指令 id 101–105 顺序对应 5 个训练位」 | 官方 `ToTrainIndex` 是 `101 速 / 105 耐 / 102 力 / 103 根 / 106 智`（転成 601–605 才顺序）；帧内 `training` 的值在属性 ≥1200 时是折半的**表示値** | 映射按官方表（`command_to_train`）；验算时帧值按 `≥1200 → ×2` 还原，模型本身不减半 |
| 10 | `isAfterTraining` 决定入口阶段（第 6 条） | 真机 6311 T7 **两帧都是 `true`**，其中 `turn7_2` 是完整训练屏（分布齐、预览非零）；该字段会「粘」在后续帧上，引擎侧只解析不使用 | 阶段改按**决策面**定档：帧有人头落位（`>= 0`）或任一训练预览非零 → `Train`，否则按 `isAfterTraining` 落 `AfterTrain`；路由不再单设 `isAfterTraining` 跳过 |
| 11 | 同回合多帧只决策一次（`last_train_turn` / 教诲窗口去重） | 玩家可能反复进出同一状态（选完取消、重进训练屏、中途 `load`），去重会让面板停在旧建议上（6311 T5 / T7 实测） | **同回合允许多次决策**：每张新帧都重算并重新输出；只有内容完全相同的重复帧仍按 `G7` 摘要幂等跳过。运气分同回合复用快照，不重复推进跟踪器 |


### 9.3 `--verify-training` 首轮结果（`G13`）——两次修正后

用 `matsuri online --replay … --verify-training` 跑 3 局 249 帧、773 条训练行。第一轮发现两处口径错误，
修完从「34% 行逐项全等 / 206 帧有偏差」变成 **42% 全等 / 118 帧有偏差，且偏差全部收敛到 ±1–2**。

**① 指令 id → 训练位映射错了（已修）**

官方 `GameGlobal.ToTrainIndex`（小黑板 `LegendScenarioAnalyzer/GameGlobal.cs`）给的是：

```
101→速(0)  105→耐(1)  102→力(2)  103→根(3)  106→智(4)
601→0 602→1 603→2 604→3 605→4      （転成家族，反而顺序）
5101–5105 → 0–4（稽古，与本仓训练位同序，T12 五条逐位验证过）
```

按「101–105 顺排」读会把耐/力/根/智整列搬到隔壁训练位。修好后 **T16 / T24 两帧 5 条全部 0 偏差**，
T20 只剩 1 条（速稽古，见 ②）。落点：`matsuri-ai::online::command_to_train`。

**② 帧内 `training` 是「表示値」：属性 ≥1200 时折半（`ReviseOver1200`）**

真机 78 帧逐轴核对：某属性跨过 1200 的那一回合起，帧里的训练值正好是**实际增量的一半**
（1200 以下两者相等）。例：6310 T60 `5102` 帧 109 / 本仓 219；`5104`（根，仍 <1200）帧 41 / 本仓 41。

⇒ **模型不需要改**（`preview_training` 给的就是实际增量）；要改的是**验算侧**：比对前把帧值
按 `属性 ≥ 1200` 逐轴 ×2 还原。落点：`matsuri-ai::online::verify_training` + 常量
`matsuri_core::DISPLAY_HALF_FROM_STATUS`。

**③ 剩余偏差的形态（2026-11-06 收尾时）**

| 偏差签名（Δ速/耐/力/根/智/PT/体力） | 次数 | 判断 |
|---|---|---|
| `[0,0,0,0,0,0,-1]` / `[0,0,0,0,0,0,-2]` | 53 / 20 | 体力消耗差 1–2（取整方向） |
| `[1,0,0,0,0,0,0]` / `[0,1,0,0,0,0,0]` | 39 / 26 | 单属性 +1（我们略高，取整） |
| `[1,0,1,0,0,0,0]` 等两点组合 | 12+ | 同上，两点各 +1 |

都是 ±1～2 的取整差，不再有结构性错位；对候选排序的影响远小于 ±1 的量级，但**体力方向单边**
（我们恒多消耗 1–2），累计到整局会影响可训练次数，列入 M4 前待查（先查 `vital_cost_drop` 的
取整与 `floor` 顺序）。

### 9.4 待修清单（`--verify-training` 暴露的真问题）

关掉两处口径错误后，773 条训练行里 **328 条逐项全等（42%）**，其余按「每行最大 |Δ|」分布：

| 每行最大 Δ | 0 | 1 | 2 | 3–5 | 6–10 | >10 |
|---|---|---|---|---|---|---|
| 行数 | 328 | 166 | 88 | 19 | 34 | 138 |

**不是「全部落在 1–2」**：191 行超过 2，其中 138 行超过 10。三条独立的真问题：

| 编号 | 问题 | 证据 | 影响 | 建议落点 |
|---|---|---|---|---|
| **W1** | **体力消耗对不上（多数 −1，少数反向 +5）** | ①现有文档**只有百分比减免**（`umaai_next/docs/game/game_rules.md` §215「`vital × (1 − 减免%/100)`」、§485「累乘补」），**没有 flat −1/−2 机制**；②真机帧的 `upper[6]` 从不为负（1278 行里全是 0 / +1…+45，只有回体）⇒「上层记 −1~−2 减轻」在本批帧里不存在；③样本相互矛盾：T2 `102` 表值 −21、减免 0% → 帧 −20（差 1）；T2 `103` 同样表值 −21 → 帧 −21（一致）；T20 `102` 我们有 22% 减免算出 −16，**帧却是 −21（一点没减）** | 单次 1–2 会系统性压低可训练次数；T20 那种「帧忽略减免」的行说明该位可能不是普通训练口径 | 用「同一训练位 + 同减免 + 连续回合」对齐再定；先确认 T20 `102` 那类行是不是另一条指令（该行 `upper` 全 0、值等于纯表值）|
| **W2** | **双彩圈友情的总倍率对不上（不是「加算」问题）** | **引擎已经是乘算**：`CardTrainingEffect::add` 用 `(100+a)(100+b)/100−100`；T40 `5101` 实测卡 30242★35% × 30311★40% = 1.89 → 我们输出友情 89%，与用户口径「30×40=1.82」同构（只是卡值不同）。真正的差在别处：同帧 4 条单彩圈行 0 偏差，只有双彩圈这条 速 248 vs 真值 214（+16%），且**下层与上层同比例 +16%** ⇒ 是一个「整体乘子」偏大，不是某一层的加项 | 友情回合会稳定高估 ~15%，影响「点不点这个位」的判断 | 待验：①剧本友情（発表会 +5% / 協力教え）是 `(1+卡友情)×(1+剧本%)` 还是并入卡友情后再乘；②吾妻（友人）是否计入 `person_count` 的 5%/人；两者都能造成 ~5–16% 的整体偏差 |
| **W3** | **增益没按五维上限裁剪** | T76（局 6309）耐：当前 2366、上限 2506；帧表示値 70 → 实际 140 = **恰好 2506−2366**；我们给 402（`Δ 262`） | 临近上限的回合会把「还能涨多少」估高一个量级，导致 AI 在快满的属性上继续堆训练 | `merge_upper` / `preview_training` 输出前按 `(five_status_limit − five_status)` 逐轴裁剪；也要判断是否该先裁剪再折半 |

**验收口径**（任一问题修完）：`matsuri online --replay … --verify-training` 里
`Δmax > 2` 的行数应大幅下降；W1 修完「体力列非零」应归零；W3 修完 T76 那类行应归零。

> 记录时间：2026-11-06 18:2x；**本轮只记录、不改动**（用户要求：偏差若都在 1–2 可不动，实测超出，故列为待修）。

### 9.5 验算口径修正（2026-11-06 20:xx，本轮已落地）

**① 合宿期帧同时带两套训练预览 —— 普通训练那套是占位值**

合宿期的 `command` 帧里 `101–106`（普通训练）与 `5101–5105`（稽古）**同时出现**，
但此时普通训练不可用，游戏给的那套值被压得很小（T69：`101` 力 帧 1 / 本仓 22）。
把它当公式误差会误判成「偏差几十倍」。⇒ 验算按阶段过滤：帧内出现稽古指令时只比稽考行，
否则只比普通行。落点：`crates/matsuri-ai/tests/w2.rs` 的 `camp` 判定。

**② 单次训练上限（W3）已修**

`clip_to_status_limit`（`matsuri-core/src/game/traits.rs`）在训练值返回前逐轴裁剪到
`five_status_limit − five_status`；`preview_training_layers` 同步裁剪并把裁剪量记进
`TrainingLayers.clip`。真机证据：T76（6309）速 3108 / 上限 3340，本仓未裁 277 → 裁后 232；
T60 耐 2366 / 2506 → 余 140。
> 注意：**裁剪的是最终合计**（游戏的两层 `lower`/`upper` 都是裁剪前的倍数关系，
> 只有和落在上限上才被截断），所以分层视图里 `lower` / `upper` 保持未裁值，`total` 是裁后值。

**③ 修完后的单局水位（6310，按 ① 过滤）**

```
188 条训练行：逐项全等 107（57%），有偏差 81，Δmax>2 的 22 条
超差行形态：**全部是速轴**（速训练位的主属性），我方高 12–45 点（6%–23%），
           其余轴（耐/力/根/智）、PT、体力全在 ±2 内
```

**④ 新增待查 W4：速轴 ≥1200 后系统性偏高 6%–23%（随速值增长）**

| 回合 | 速当前 | 帧（已还原未折半） | 本仓 | 偏高 |
|---|---|---|---|---|
| T35 | — | 200 | 216 | 8% |
| T40 | 1382 | 214 | 248 | 16% |
| T69 | 2966 | 284 | 321 | 20% |
| T72 | — | — | — | 23% |
| T76 | 3108 | 310 | 442 | 42%（未裁前） |

已排除的假设（都做过对照）：

- **成长率（`five_status_bonus`）在 ≥1200 不生效**：试过后偏差变号（T36/T37/T39 从 0 变成 −24），否决；
- **单次上限裁剪**：T40 余量 1906，不裁，仍差 34，不是它；
- **友情乘子（双彩圈）**：同帧的力轴用同一个 `youqing=89%`，逐位一致，说明友情乘子本身没错；
- **人数、训练效果、干劲、成长率**：这些是六轴共用乘子，力/耐/根/智都对，只剩「速轴独有」的
  `basic_row[0]`（21）与 `status_bonus[0]`（15）两个变量；
- **帧值折半还原**：还原成 `×2` 后量的量级自洽（帧 50→100、两帧之间比例一致），不是还原算法问题。

⇒ 下一步建议：拿「速轴 ≥1200 且连续两回合」的样本，反解 `basic_row[0]` 的真实值
（当前表行 21 反解出来需要 ≈19–20），怀疑 1200 以上稽古基础值另有衰减口径。

### 9.6 `training_upper_cap` 的真正含义：**下层单次增益上限**（W4 已修）

§9.5 记的「速轴 ≥1200 系统性偏高 6–23%」根因找到了，不是基础值填错，而是
**剧本 `rules.training_upper_cap`（= 100）从来没有作用到「下层」上**。

**证据（6310，classic 期速位稽古，逐行反解）**

抽帧里速轴的 `lower` 全是 50（表示値，即实际 100），而同期乘子积从 4.49 涨到 5.65：

```
T35 积4.708 → 帧lower 50     T40 积5.075 → 50     T53 积5.286 → 50
T36 积4.494 → 帧lower 49     T76 积5.651 → 50
```

乘子积变了 26%，帧值却卡在 50 —— 只有「下层被上限截断」能解释。而 **4.494 那几帧给的是 49 而不是 50**，
说明截断前的实际值 ≈98（没到 100），因此上限就是 100，而不是 50/98 之类。

反解也印证基础值是对的：把 classic 速位从 21 改成 20 后，T36/T37/T39 立刻变成 −7 ~ −8 的负偏差，
所以 **21 是正确值**，用户的「速基础值填错」猜测排除了。

**实现**（`matsuri-core/src/game/traits.rs::clip_lower_gain`）

```text
lower = 下层公式(...)
lower = min(lower, rules.training_upper_cap)      ← 新增，逐轴，仅五维
upper = 上层公式(lower)                            ← 顺序关键：先裁后算
total = lower + upper
total = min(total, five_status_limit - five_status)  ← 属性上限（§9.5 ②）
```

顺序很关键：先算上层再裁下层，T40 上层会得 132（帧 114）；先裁后算得 115（帧 114）。
PT（`status_pt[5]`）与体力不参与这条上限（T69 帧 PT 288 = 本仓 288）。

**修完后的水位**

| 口径 | 修前 | 修后 |
|---|---|---|
| 单局 6310（188 行，按阶段过滤） | Δmax>2 22 条 | **6 条** |
| 三局回放（448 行，按阶段过滤） | Δmax>2 191 条 / >10 138 条 | **21 条 / 8 条** |

剩余超差没有系统性形态（最大 48，多为单点），集中在 `T0` 首帧与少数稽古回合，
形态与「跨局回放时 `prev_levels` 基准切换」重叠，单局口径下只有 6 条。

### 9.7 与旧实现（`umaai-rs-ref`）的交叉验证

§9.6 的结论在旧实现里有直接对应，不是我们猜出来的新机制：

| 落点 | 旧实现写法 | 本仓对应 |
|---|---|---|
| `umasim/src/game/onsen/game.rs:1651` | `// 下层不超过100`<br>`for i in 0..6 { base_value.status_pt[i] = base_value.status_pt[i].min(100) }` | `clip_lower_gain`（六位，含 PT） |
| `umasim/src/game/ramen/game.rs:650` | `// 1. 下层值：default_calc_training_value 应用卡 buff…然后约束 status_pt 各元素 ≤ 100（剧本规则：下层不超过 100）` | 同上 |
| `umasim/src/game/ramen/effects.rs:380` | `let lower = lower_value.min(100);` | 同上 |
| `ramen/game.rs` 上层 | `status_limit = 100 + ramen_effect.status_limit`；`upper = upper_raw.min(status_limit).max(0)`；PT 单独用 `pt_limit` | `training_upper_with` 的 `status_cap = rules.training_upper_cap + advice.status_limit`，PT 用 `sp_cap` |

**顺序也一致**：两个剧本都是「先算下层 → 裁 100 → 用裁过的下层算上层」，与我们 §9.6 定的顺序相同。

**两点与旧实现不同（本仓保留）**

1. **PT 也裁**：旧实现是 `0..6`（含 PT）。我之前只裁 `0..5`，本轮已按旧实现补齐。
   真机帧里 `lower[5]` 最大只有 91（1278 条里 0 条超 100），所以补齐前后统计不变 —— 属于「口径对齐」而非「修 bug」。
2. **属性上限裁剪**（`clip_to_status_limit`，§9.5 ②）旧实现**没有**。它是本仓新增的：
   旧实现靠「下层 ≤100 + 上层 ≤100+加成」间接限制单次增益，属性本身不再裁；
   本仓保留属性裁剪是因为真机 `five_status` 不可能越过 `fiveStatusLimit`，留在最后一道更安全
   （数据里触发极少，单局只有 T76 一条触发，裁掉 45）。

**`default_calc_training_value` 本体逐项一致**：基础值行取值、`basic + buffs.bonus`、干劲 `(motivation-3)*10`、
训练效果 / 干劲效果 / 人数 `1+0.05n` / 成长率五个乘子、`floor` 位置、智训练回体、体力消耗减免，
两份代码一字不差。差异只在「算完之后裁多少」。

### 9.8 逐条拆解剩余 6 条 → 全部收敛

拆解工具：`cargo test -p matsuri-ai --test w2 -- --nocapture dump_outliers`
（打印帧两层 / 本仓两层 / 未裁下层 / 六位乘子 / 参与人头 / 当前属性）。

| # | 回合·指令 | 现象 | 判断 |
|---|---|---|---|
| 1 | T74 `5102` | 上层耐 帧 200 / 本仓 205 | **真 bug**：`keiko_flat` 加在上限之外 |
| 2 | T30 `101` | 速 帧 41+4 / 本仓 46+9 | 本次训练**跨过 1200**（1163 → 1209） |
| 3 | T33 `5104` | 速 帧 11+5 / 本仓 13+10 | 同上（1191 → 1204） |
| 4 | T54 `5102` | 智 帧 5 / 本仓 10 | 同上（智 1199 → 1209，flat 也跨线） |
| 5 | T54 `5104` | 智 帧 5 / 本仓 10 | 同上 |
| 6 | T51 `5103` | 力 帧 62+34 / 本仓 76+68 | 力 1152 → 1228，同样跨线（下层 76 → 62 ✓）；上层另欠 |

**① `keiko_flat` 在上层上限之内（已修）**

旧写法 `min(gain, cap) + flat`，T74 主属性顶到 `cap = 200` 时会给出 205；帧是 200 —— 说明 flat
被算进那 100 里。改成 `min(gain + flat, cap)`。基础值为 0 的轴也照吃 flat（同样受 cap）。

**② 跨 1200 口径（最终版：帧值永远是「表示値」，模型保持未减半真值）**

~~初版曾按「跨线部分在模型侧实际减半」实现 `clip_cross_1200`~~ ——已**撤回**。最终口径
（user 2026-11-06 拍板）：

- 帧内 `training` 的值**一律是显示值**（`ReviseOver1200`）：当前值 ≥1200 的轴整段按
  `floor(实际/2)` 显示；当前值 <1200 但本次跨线的轴，「未超 1200 的部分照实显示、超出部分
  折半显示」。**只有一种显示规则，没有第二种「实际减半」。**
- 本仓模型内部**永远持有未减半真值**，不做任何折半裁剪；
- 验算 / 对账侧用 [`matsuri_core::restore_displayed_gain`]（`game/mod.rs`）把帧值还原成
  真值再比：`current ≥1200 → shown×2`；`current <1200 且跨线 → room + (shown−room)×2`。

三条「刚好跨线」样本在新口径下同样全对（T30 速 帧 45 → 还原 53 = 本仓 55−取整；T33 / T54 同理）。

**③ 修完后的水位**

| 口径 | 6 条拆解前 | 修 flat | 撤 `clip_cross_1200` 改还原 |
|---|---|---|---|
| 单局 6310（188 行）Δmax>2 | 6 条 | 5 条 | **0 条** |
| 三局回放（448 行）Δmax>2 / >10 | 21 / 8 | 21 / 8 | 10 / 3 |

单局 188 行全部落在 ±2 以内（52% 逐项全等，其余是乘法取整顺序差）。

### 9.9 收尾两修：PT 上限分账 + 载入快照拦截（2026-11-06 晚，本轮已落地）

三局剩余 3 条 >10（6309 T72 PT Δ25 / 6310 T72 PT Δ25 / T0 力）拆完，是两个独立问题：

**① PT 上层上限不吃特化 `status_limit`**

- 现象：6309 T72 `5101` PT 上层 帧 220 / 本仓 245（Δ25）。分解：`status_cap = 150 =
  100 + 特化(25) + 演目Lv10(25)`，`sp_cap` 旧公式同样把特化 25 加进去得 `100+25+120 = 245`。
- 真机把 PT 上层裁到 **220 = 100 + 演目 `sp_limit`(120)**——特化没进 PT 上限。
- 佐证：601 特化教诲文案是「主属性获得量UP + **基础属性**上限UP」，只提基础属性；
  拉面剧本（`umaai-rs-ref` `ramen/effects.rs`）同样 `pt_limit` 与 `status_limit` 分账。
- 改法：`training_upper_with` / `training_breakdown` 的 `sp_cap` 改为
  `training_upper_cap +（稽古且 5 种卡：演目 sp_limit）`，不再加 `advice.status_limit`。
  属性上限 `status_cap` 公式**不变**（6309 T72 速上层 150 = cap 已被帧证实）。

**② T0 载入快照：预览非零 + 分布全空 → 拦截**

- 现象：6308 / 6309 各有一张 `source=load` 的 T0 帧，`personDistribution` 全 `-1`，但同帧
  训练预览已含落位效果（帧自相矛盾——插件写帧不同步）。还原态人数恒 0，验算出假偏差，
  且决策建立在错误分布上、还会用 `last_train_turn` 把同回合 command 帧的正确决策顶掉。
- 关键反例（不能一刀切跳 `load`）：6308 T4、6310 T12 的中途 `load` 帧分布完整（6310 T12
  甚至只有这一张帧）；77 张 `event` 帧分布全空但预览也全零，事件决策不依赖分布。
- 改法：[`online::stale_snapshot`]（预览非零 **且** 分布全空）才拦——route 跳过、验算跳过；
  预览全零的帧（比赛回合 / 事件帧 / 合宿中间帧）照常走原路由。修后 T0 决策落在带完整
  分布的 `command` 帧上（6308 T0 → 训练2·力位，正是 3 人落位处）。

**③ 最终水位（三局全量）**

| 口径 | 收尾前 | 收尾后 |
|---|---|---|
| 三局回放（438 行，剔除 2 张载入快照）Δmax>2 | 10 / 3（>2 / >10） | **0 条** |
| 端到端 CLI 回放 | — | 249 帧 / 187 次决策，T77 URA 决赛收束 |

438 行中 284 行逐项全等，其余全部 ±1–2（乘法取整顺序差），无一条超差。

### 9.10 在线档收尾（2026-11-06 晚，本轮已落地）

**① 终局帧通道**

- `matsuri-ai/src/final_score.rs`（新）：`FinalScoreFrame` 解析 + `scenarioId == 15` 门禁；
  三道闸在 `OnlineSession::handle_final` 按「必填解析 → `is_usable` → 局号匹配 → 算分」执行，
  局号不符 / 数值不可用只拒绝本帧，不推进 / 不清会话。
- `ReplaySource` 在每局最后一个回合帧之后追加 `game{id}_finalScore.json`；旧固定名
  `finalScore.json` 不识别。真机 `FileChannel` 的 `FrameKind::FinalScore` 直接进同一方法。
- CLI：`ThisTurn` / `FinalScore` 按信道分流；人类档打印「终局评分 / 档次」，JSON 档输出
  `type = final`。真机到终局不退出，下一张 `load` 帧照常换局；回放继续下一局。

**② 搜索档候选分**

- `MatsuriSearchTrainer` 新增 `SearchSummary` / `last_search()`：
  Train 全量、教诲剪枝子集（映射回原动作下标）、事件选项三种搜索路径都缓存
  「候选描述 / 加权均分 / 样本数 / 中选下标」；走手写 fallback 时 `last_search() = None`。
- 在线记录：搜索档把候选分与 `candidate_n` 写入 `DecisionRecord`，`json_payload` 的
  `score` 为真实搜索均分、`scenario_extra.score_source = "search"`；手写档仍 `score = 0`、
  `candidate_n = 0`、`score_source = "hand"`，但候选打分表继续供面板显示。
- `DecisionRecord.action_index` 修正为**候选表下标**（教诲分配剪枝后必须映射回子集位置）。

**③ 在线档配置驱动**

- 在线档与离线档读同一套默认层 + 工作目录覆盖层：
  `data/ai_config.toml` + `data/matsuri_config.toml`。
- `--trainer` 未显式给出时按 `[search].search_n` 决定：`0` = 手写，`>0` = 搜索（与离线
  `matsuri` 训练员一致）；`MATSURI_SEARCH_N` / `MATSURI_SEARCH_RADICAL` 仍可临时覆盖。
- **不新增 `--search-n`**：在线预算与离线共用配置文件。

**④ JSON 接入可观测性**

- `--json` 第一行固定 `{"type":"info","event":"connected","scenario":"matsuri"}`。
- 单帧失败 → `{"type":"error","scenario":"matsuri","message":...}`；真机档继续监听下一帧，
  回放档累计错误并在结束时报非零退出。

**⑤ M4 决策一致率审计：暂不做（用户 2026-11-06 决定）**

- 终局帧里的 `actions` 只作为复盘材料保留，在线档不读取、不比对。
- 本轮 M4 验收口径缩为：终局帧三道闸、回放全量无解析错误、真机终局 / 换局不崩。
