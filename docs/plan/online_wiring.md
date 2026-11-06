# 在线接线方案：Festa 帧 → `MatsuriGame` 还原

> 状态：**方案已冻结（P2），待实施**。实施顺序见 §6：**E1+E2（引擎，非破坏）→ M1 → M2 → M3 → M4**。
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
| **M4 闭环** | `--replay` 78 帧 + 真机 game6310 | 决策一致率；终局评分对照只作趋势，不作基线 |

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
| **G7** | 同回合重复帧幂等 | 定「同 `(charaId, turn)` 只取首帧 + 内容哈希」；`_2` / `_3` 多为结算中间态，用 `playing_state` 过滤 | 重复决策 |
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
