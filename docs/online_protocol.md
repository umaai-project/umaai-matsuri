# 在线帧协议与还原契约（`matsuri` / `scenarioId = 15`）

> 本文档定义小黑板（`SendGameStatusPlugin`）与 `matsuri-ai` / `matsuri-core::game::restore` 之间的现行在线协议、解析规则与验算口径。
> 无状态还原、同回合去重与静默容错等架构原则见 [`adr/已采纳/0006-已采纳-在线无状态帧还原与协议容错契约.md`](adr/已采纳/0006-已采纳-在线无状态帧还原与协议容错契约.md)。

---

## 1. 顶层结构与写出时机

顶层包含 `festa`（剧本段）与 `baseGame`（通用段）：

```jsonc
{ "festa": { /* 剧本段，§2 */ }, "baseGame": { /* 通用段，§3 */ } }
```

- `scenarioId = 15` 位于 **`baseGame.scenarioId`**；剧本段名固定为 **`festa`**。
- 插件侧写出时机：精确端点 `load` / `start` / `exec_command` 及 `check_event` 类 8 端点正则（`attach_advice|check_event|check_point|continue|festa_live|gain_skills|race_entry|race_out`）；`change_short_cut` / `finish_claw_crane` 不写；`islegal == false`、`festa == null` 或待选单选项中间态不写。
- 同回合可能写出多帧（`_2` / `_3` 后缀）。

---

## 2. `festa` 段字段表（`FestaFrame`）

| 字段 | 类型 | 协议来源 | 语义 / 注意 |
|---|---|---|---|
| `acts` | array | `festa_data_set.act_info_array`（按 `act_type` 升序） | 5 个演目（见下表） |
| `maxActLevel` | int | `festa_info.max_act_level` | 演目 Lv 上限（真机 10；不进状态，由静态资产决定） |
| `campRemain` | int | `festa_info.summer_camp_remain_turn_num` | 合宿剩余回合 |
| `assistSupportLeft` | int `0\|1` | 状态 47 有无 | 仅表示有无；剩余次数由事件/消耗口径推算 |
| `fatigueRecovery` | bool | 状态 45 有无 | 协议直读（消除疲劳） |
| `checkPoints` | array | `check_point_result_array` | `{scheduleId:int, resultType:int}` 原值（`resultType=2` 为大成功） |
| `trainLevelTotal` | int[5] | `training_level_info_array[].level` + 快照 | `0` 表示本帧无有效值（合宿/`load`/`event` 帧常见） |
| `kensan` | bool[5] | `FestaTraining.UpperTraining` | 逐训练位追加稽古标记（引擎取 OR） |
| `persons` | array | `festa_data_set.evaluation_info_array` | `{targetId:int, actType:int, origin:string}`；`origin ∈ card/friend/guest/yayoi/reporter` |

`acts[]` 元素（`FestaActFrame`）：

| 字段 | 类型 | 协议来源 | 语义 |
|---|---|---|---|
| `actType` | int 1–5 | `act_type` | 与训练位 0–4 同序 |
| `gauge` | int | `act_gauge` | 当前演目槽 |
| `level` | int | `level` | 演目 Lv（1 基） |
| `needGauge` | int | `need_act_gauge` | 升级所需槽（真机恒 5；不进状态） |
| `adviceId` | int | `advice_id` | 当前教诲 id（未転成 101–501；転成后 601–1001） |
| `fusionId` | int | `advice_info_array[advice_id].fusion_advice_id` | 未転成 0；転成后指回原基础 101–501 |
| `outingGainLevel` | int | `friend_outing_gain_act_level` | 友人出行演目 Lv 增益（不进状态） |

---

## 3. `baseGame` 段相关字段

`baseGame` 采用 camelCase 与 snake_case 混用，反序列化按原名映射：

| 字段 | 类型 | 说明 |
|---|---|---|
| `scenarioId` | int | 15 |
| `turn` | int | **0 基**（0–77） |
| `vital` / `maxVital` / `motivation` | int | 体力 / 上限 / 干劲 1–5 |
| `fiveStatus` / `fiveStatusLimit` | int[5] | **已做 `ReviseOver1200` 折半显示压缩**（≥1200 需还原） |
| `skillPt` / `skillScore` / `totalHints` / `skillPtSpent` | int | 技能点 / 已买技能分 / Hint 总等级 / 累计花费 |
| `trainLevelCount` | int[5] | 成功普通训练次数 |
| `training` | `map<int, {failureRate, lower[7], upper[7]}>` | 训练预览协议真值（§5），仅骏大祭 |
| `cardId` | int[6] | 支援卡 id |
| `persons` | array[6] | `{personType, charaId, friendship, isHint, cardRecord}` |
| `personDistribution` | int[5][5] | 每训练位最多 5 人，`-1` 为空位 |
| `lockedTrainingId` | int | 骏大祭恒 `-1` |
| `friend_stage` / `friend_outgoingUsed` | int | 友人阶段 0/1/2、已出行段数 |
| `friendship_noncard_yayoi` / `friendship_noncard_reporter` | int | 非卡理事长 / 记者羁绊 |
| `playing_state` | int | 1 = 可决策；5 = 待选事件；49/51 = special |
| `source` | string | `load / command / event / special` |
| `isFullGame` | bool | 中途加载时为 `false` |
| `isAfterTraining` / `islegal` | bool | 本回合是否已执行指令（存在粘连问题）/ 帧是否合法 |
| `raceHistory` / `keyEvents` | int[] | 取胜回合（0 基）/ 关键族 `story_id` |
| `inheritGains` / `inheritLimitGains` | int[] / int[][] | 继承属性增量 / 五维上限增量 |
| `charaEffects` | int[] | **全量状态 ID**（替代旧 `illEffects`）：1–6 为病种（按文本库分类 142：`1 熬夜 / 2 懒惰 / 3 皮肤 / 4 变胖 / 5 偏头痛 / 6 不擅长`，勿按 `Ill` 位序推），其余含切者 7 / 正向思考 25 / 消除疲劳 45 / 助力支撑 47 / 情热 100–104 等。缺字段或空数组时不覆盖 `isIll` 落点 |
| `storyId` | int? | 待选多选项事件 `story_id`（`choice_array.Length > 1`） |

---

## 4. 解析与还原现行口径

1. **宽松解析与缺段门禁**：`serde` 结构体**不加 `deny_unknown_fields`**；缺 `festa` 段直接拒绝为非骏大祭帧，不退化。`trainLevelTotal` / `assistSupportLeft` / `checkPoints` 严格区分未知与零值。
2. **`persons` 担当映射与名单校验（`check_roster`）**：
   - T0/T1 会发整份名单，必须先过滤 `actType > 0`，并丢弃 `origin == "guest"` 且 `targetId < 1000` 的占位项。
   - **不可用 `targetId - 1` 算卡位**：按 `targetId` 升序遍历，`origin == "card"` 依次落入卡组非友人位，`origin == "friend"` 仅标记不参与担当；帧内神驹一律置 `avail_turn = 0`、无大成功门槛。
   - 身份以本地卡表为准：`persons[i].charaId == 0`（插件未加载）时跳过该位校验并照常还原；非 0 且与本地不符才拒绝。另加 `personType` 1/2 ↔ `card_type >= 5` 结构校验守住友人错位；本地缺卡时提示更新 AI 数据。
   - 越界的 `actType` / `scheduleId` 与无法映射的分布单点丢弃并记 `diag!`，不整帧失败。
3. **训练等级基准**：`trainLevelTotal[i] > 0` 时取帧值，否则沿用上一帧基准（首帧退回 `click / 4 + 1`）。
4. **载入快照拦截（`online::stale_snapshot`）**：仅当帧同时满足「`training` 预览非零 **且** `personDistribution` 全 `-1`」时跳过路由与验算（避免 T0 矛盾快照压掉后续 `command` 帧）；预览全零帧（事件/比赛/合宿）照常路由。
5. **入口阶段与决策路由**：
   - 入口阶段按**决策面**定档：有人头落位（`>= 0`）或任一训练预览非零 → `Train`；否则按 `isAfterTraining` 落 `AfterTrain`（`Distribute` 一律跳过）。
   - `source == "special"`（`playing_state` 49/51）跳过；其余按「多选项 `storyId` → 事件决策（本地五表优先，**查不到直接跳过、绝不回落训练**）；`AfterTrain` 且无事件 → 跳过；`Train` → 动作决策」。
   - **教诲搜索根阶段**：不读易粘连的 `isAfterTraining`，只按 `plan.advice_window` 定档（窗口 0/1/3/5 → `Begin`，窗口 2/4 → `End`）。
6. **同回合多帧与幂等**：仅按内容哈希（`G7`）做同内容幂等跳过；玩家重进训练屏等产生的同回合不同内容帧正常重算输出，同回合复用运气快照。

---

## 5. `baseGame.training` 与 `--verify-training` 验算口径

- `baseGame.training` 按指令 id（`101 速 / 105 耐 / 102 力 / 103 根 / 106 智`，`5101–5105` 稽古，`601–605` 転成）给出 `{ failureRate, lower[7], upper[7] }`，7 项为五维 + SP + 体力，**未做 1200 衰减**。
- **G13 铁律**：`training` **只用于 `--verify-training` 离线/在线验算**，严禁参与 `restore` 与 rollout 决策。
- 验算时需将 `matsuri-core` 模型预测值与帧内 `lower + upper` 比对；注意 `baseGame.fiveStatus` / `fiveStatusLimit` 为 `ReviseOver1200` 折半值，还原时对 `> 1200` 分量按 `2 * v - 1200` 展开后再比较。
