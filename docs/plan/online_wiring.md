# 在线接线方案：Festa 帧 → `MatsuriGame` 还原

> 状态：**方案（未实施）** 2026-11-04 / 面向 S8「在线外壳」与 `matsuri-ai` 落地
>
> 本文只回答两件事：**①** `SendGameStatusPlugin` 的 `thisTurn.json` 里 `matsuri` 段该写什么；
> **②** `matsuri` 侧怎样用这份 JSON + 引擎通用段还原出一个**可继续决策**的 `MatsuriGame`。
> 不含代码，不含测试实现；所有「待核」项都按 `❓G#` 登记在 §7。

---

## 0. 目标与非目标

| 目标 | 说明 |
|---|---|
| 一帧一决策 | 引擎 `Session::step` 每读一帧重建一次局面，因此还原必须是**无状态函数**：`帧 + 本地静态数据 → MatsuriGame` |
| 状态够用 | 还原后的局面要能直接 `list_actions` / 预览训练值 / 结算动作，与真机同回合的离线模拟**逐项一致** |
| 不重放 | 不做「从第 0 回合重跑到当前回合」；随机分布 / 事件结果一律取真机值 |
| 非目标 | 不回写决策给小黑板（现协议无回传通道）；不复原呈现层（`event_log` / `View` 可由还原态重算） |

**关键约束（决定整套方案形态）**：`umaai::session::Session::step_turn` 每帧都执行
`BaseGame 重建 → (build)(base) → run_stage`，宿主提供的 `build` 闭包**不能依赖跨帧累积**。
所以凡是「跨回合累计」的量（训练点击数、助力支撑剩余、事件历史），**只能由小黑板插件算好写进帧**——
插件才是常驻、且手里有 `EventLogger` 全量历史的那一侧。

```
真机 ──► 小黑板(Host) ──► SendGameStatusPlugin ──► .portable/PluginData/SendGameStatusPlugin/thisTurn.json
                                    ▲                        │
                              EventLogger 全量历史            ▼
                                                       umaai Session::step
                                                              │
                                                    parse_base_status → BaseGame（通用段，已有）
                                                    parse_matsuri   → restore → MatsuriGame（本方案）
                                                              │
                                                          决策 / 回写
```

---

## 1. 两侧现状

### 1.1 引擎侧（`umaai_next`，已具备）

| 能力 | 位置 | 对本剧本的含义 |
|---|---|---|
| 帧通道 | `crates/umaai/src/protocol/channel.rs` | `thisTurn.json` / `finalScore.json` 已是代码常量，无需改动 |
| 通用段解析 | `protocol/base.rs::parse_base_status` | 容忍未知键，缺 `matsuri` 段只会退化（不会整帧失败） |
| 底座还原 | `BaseStatus::to_base_game` | `turn / uma / deck / friend / distribution / train_level_count / events / story` 全部落 `BaseGame` |
| 剧本分发 | `Session::step_turn` 的 `build: FnMut(BaseGame) -> Result<G>` | **剧本专属还原的唯一入口**，`build` 由宿主注入；注意签名里**没有帧原文**（见 §5.2 注） |
| 终局帧 | `protocol/final_score.rs` | 与本方案无关（不进决策链路） |

`BaseParams`（宿主注入）里与剧本相关的两项：`friend_chara_id`（吾妻）与 `friend_first_event`。

### 1.2 小黑板侧（`URA_Plugins`）

| 现成件 | 位置 | 可直接复用的部分 |
|---|---|---|
| Festa 协议投影 | `FestaScenarioAnalyzer/FestaProjection.cs` | 演目 / 教诲 / 训练位 / 参与者 / 槽预测的读取与校验口径（**internal**，见 §5 决策点 D1） |
| 逐回合记录 | `EventLoggerPlugin`（`EventLoggerRoundSnapshot`） | `Turns[]`（逐回合五维 / `playerChoice` / `isTrainingFailed` / `trainLevel` / `trainLevelCount`）、`AllEvents`（全量 `StoryId + SelectIndex + Turn`）、`KeyEvents`、`RaceHistory`、`SkillPtSpent`、`LastVital` |
| 同类先例 | `GameStatusSend_Ramen.cs` | 训练等级余数用历史回补的具体做法（`ApplyTrainLevelProgressFromHistory`） |
| 帧写出 | `GameStatusOutput` | 原子写 + 同回合序号；`thisTurn.json` 与 `game<id>_turn<n>.json` 双写 |

---

## 2. `MatsuriGame` 剧本专属状态盘点

`MatsuriGame`（`crates/matsuri-core/src/game/mod.rs`）除 `base: BaseGame` 外共 24 个字段，
下表按语义合并同类项（共 22 行），另附 3 个 `BaseGame` 里本剧本依赖但通用段还原不完整的字段。
按「还原来源」分五类：

- **A 协议直读**：帧里已有，取值即可（最多加一层枚举映射）
- **B 需小黑板端收集**：协议不给，但插件用 `EventLogger` 历史能算出
- **C 需小黑板端计算**：协议给的是别的量，要换算 / 反推
- **D matsuri 侧重算**：静态数据 + 回合号即可重建，帧里不必出现
- **E 不进帧**：瞬时量 / 随机流 / 呈现缓存

| # | 字段 | 语义 | 类 | 还原依据 |
|---|---|---|---|---|
| 1 | `persons` | 人头（卡 / 友人 / 理事长 / 记者 / 编成外神驹）+ **担当演目** + 来源 + 可用回合 | **B** | 卡人头由 `baseGame.cardId` 推导；担当取 `festa_data_set.evaluation_info_array[].{target_id, chara_id, act_type}`；神驹名单同表；`avail_turn` / `needs_great_success` 按静态规则 + `❓G6` |
| 2 | `stage` | 阶段机（Begin / Distribute / Train / AfterTrain / End） | **E** | 还原入口固定为 **`Train`**（决策点）；不跑 `Begin`（见 §3.1） |
| 3 | `strategy` | 策略随机流 | **E** | 按会话主种子重新派生（`set_rule_master` 已由 Session 注入） |
| 4 | `data` / `keiko_table` / `keiko_classic_table` | 静态资产 | **D** | 宿主载入 `scenario_matsuri.json` |
| 5 | `keiko_settling` | 稽古结算上下文（瞬时） | **E** | 还原时 `false` |
| 6 | `acts.{gauge, level}` | 演目槽 / 演目 Lv | **A** | `act_info_array[].{act_gauge, level}`；Lv 基点 `❓G2` |
| 7 | `acts` 上限 | 演目 Lv 上限 / 槽上限 | **A** | `festa_info.max_act_level`；槽上限取静态 `rules.act_gauge_max` |
| 8 | `advice.{slots, fused}` | 5 槽基础教诲 id + 転成 id | **A** | `act_info_array[].advice_id` + `advice_info_array[].{advice_id, fusion_advice_id}`；**语义歧义 `❓G1`** |
| 9 | `advice.acquired` | 教诲是否已授予 | **A** | `turn ≥ 2` 或 `slots` 非全 0 |
| 10 | `camp.{year, counted}` | 合宿进度 | **A** | `festa_info.summer_camp_remain_turn_num` + 静态窗口表 `camp.windows` 反推（`counted = 窗口长 − remain`；`year` = 使 `window.start ≤ turn` 且 `remain > 0` 的年次）。见 §3.2 |
| 11 | `plan` | 回合结构（比赛 / URA / 空窗期 / 合宿 / 発表会 / 教诲窗口） | **D** | `derive_plan()` 用 `turn` + 已还原的 `camp` + 静态赛程重算 |
| 12 | `forced_rest` | 本回合是否被强制休息事件占用 | **D** | 引擎现实现是**纯函数** `is_forced_rest_turn(chara_id, turn)`（临时硬编码表：凯斯奇迹 T65）；还原时直接重算，不进帧 |
| 13 | `career_failed` | 自选比赛义务未达标导致终止 | **D** | `base.uma.free_races` + `race_history` 自算（与离线同口径） |
| 14 | `fatigue_recovery` | 友人出行解锁带来的「疲劳回复」一次性 flag | **C** | 事件历史：解锁事件回合置位、首次 `vital ≤ 49` 的回合消费；`❓G8` |
| 15 | `check_points[3]` | 発表会 / 本番结果 | **A** | `check_point_result_array[].{schedule_id, result_type}`；枚举映射 `❓G3` |
| 16 | `finals_done` | 本番是否已结算 | **A** | 由 `check_points[2]` 与 `turn` 派生，不必单列字段 |
| 17 | `deck_warnings` | 编成警告 | **D** | `inspect_deck`（建局静态重算，帧里不必出现） |
| 18 | `event_log` | 最近事件记录（呈现用） | **E** | 可用 `AllEvents` 回填做展示；不参与决策 |
| 19 | `event_queue` / `event_cursor` | 本阶段待结算事件队列 | **A** | 通用段 `story` 已装进 `base.unresolved_events`；决策点帧该队列为空 |
| 20 | `assist_support_left` | 助力支撑剩余次数 | **A/C** | 首选 `home_info.training_bonus_remain_turn_num`（**语义待证 `❓G4`**）；回退：按历史逐稽古重算 |
| 21 | `kensan_pending` | 本回合追加稽古是否掷中 | **A** | `command_info_array[].is_upper_training_event_occurred`（逐训练位；任一为真即置位）——2026-11-04 已定位，与游戏内训练位角标同源 |
| 22 | `train_level_bonus` | 発表会大成功「全训练 Lv+1」累计 | **C** | `matsuri.trainLevelTotal`（协议总等级，含加成）− (`base.train_level_count / 4 + 1`)，下限 0；点击数需 **B** |
| — | `base.train_level_count` | 训练设施点击数（4 次 +1） | **B** | 协议只给 `level`（含発表会加成），**不能**写 `4×(level−1)`；须由历史重建（同拉面口径，但稽古 / 失败不计） |
| — | `base.distribution` | 本回合人头分布 | **A** | `person_distribution`（已有） |
| — | `base.events` | 已发生事件计数 | **A** | `keyEvents`（已有，注意只覆盖关键族；剧本事件见 §3.5） |

**小结**：真正需要小黑板端新增采集/计算的只有 5 项 —— **B 类**：`persons` 的担当与可用回合、
`base.train_level_count`；**C 类**：`fatigue_recovery`、`train_level_bonus`、`assist_support_left` 的回退路径
（首选直接读协议字段）。其余要么已经在 `baseGame` 里，要么 matsuri 自己能算
（`plan` / `forced_rest` / `career_failed` / `deck_warnings`），要么根本不需要（`stage` / 随机流 / 呈现缓存）。

---

## 3. 逐项口径

### 3.1 入口阶段：为什么不能从 `Begin` 进

`stage_body(Stage::Begin)` 有副作用：`derive_plan` / `forced_rest` / `recover_fatigue` /
`roll_fat_recovery` / `generate_events`（抽随机事件）/ `acquire_advice` / `roll_kensan`。
真机跑到决策点时这些**已经发生过**，再从 `Begin` 跑一遍会重复抽事件、重复掷追加稽古。

⇒ 还原必须提供一个**显式恢复入口**（建议 `MatsuriGame::restore(base, frame, data, config)`），四步：

1. **复用会话已建好的 `BaseGame`**，不要丢弃重建：`uma / deck / inherit / turn / flags / friend /
   distribution / events / train_level_count` 都由通用段还原，重建一次只会丢信息；
2. 复用 `newgame` 的静态装配（`data` / `keiko` 两张表 / `config` / `inspect_deck` → `deck_warnings` /
   `init_persons`），把 `base` 原样接上；
3. **覆盖**剧本字段：`persons`（协议名单，见 §3.3）、`acts`、`advice`、`camp`、`check_points`、
   `finals_done`、`assist_support_left`、`kensan_pending`、`train_level_bonus`、`fatigue_recovery`；
4. `plan = derive_plan()`（纯派生：`camp.begin_turn` 幂等，只读窗口表 + `turn`）、
   `forced_rest = is_forced_rest_turn(chara_id, turn)`、`career_failed` 按赛程自算、`stage = Stage::Train`。

`init_persons` 一定会跑（它决定 `deck_warnings` 与卡的默认担当），随后被第 3 步整段替换；这一点要写进
`restore` 的注释，否则会被误读成「重复建人头」。

**门禁沿用建局口径**：装配阶段会跑 `inspect_deck`，编成不支持（例如没带 SSR 吾妻）时直接报错。
真机遇到这种局 → 还原失败 → 宿主决定是否提示「本剧本暂不支持该编成」；不要为了能跑而放宽门禁。

### 3.2 合宿：`remain` 足够，不需要历史

窗口表固定（0 基）：`12–15 / 36–39 / 60–63`，每年 4 回合；只有**训练类回合**计入 `counted`，
比赛 / 强制休息不计 → `counted` 可能落后于「窗口内已过回合数」，这才是延长段的来源。

`summer_camp_remain_turn_num` 是**存储值**（服务端下发，非客户端算术），因此：

```
counted = 窗口长度(4) − remain
year    = max{ i | window[i].start ≤ turn 且 remain > 0 }
remain == 0 ⇒ year = None（本次合宿已在上一回合 End 结束）
```

延长段（`turn > window.end` 且 `remain > 0`）自然落入 `year = 该窗口年次`、`camp_extended = true`，
与 `derive_plan` 的判定一致。**唯一风险**：`remain` 在延长段是否继续减 1（`❓G9`）。

### 3.3 人头与担当

离线 `init_roster` 是**五步流水线 + 序取随机马娘**；真机的编成外神驹是**随机角色**，两者名单必然不同。
所以还原必须用协议的名单覆盖，而不是复用离线分配结果：

| 人头 | 来源 | 备注 |
|---|---|---|
| 6 张编成卡（含友人卡） | `baseGame.cardId` → `SupportCard` | 顺序即 `deck` 顺序，与 `persons` 前缀同序 |
| 卡的担当 | `evaluation_info_array[].act_type`（`target_id` = 卡位 0–5） | 覆盖离线算出的 `report.card_acts` |
| 编成外神驹 | `evaluation_info_array[]` 中 `chara_id` 不属于卡组 / 育成马娘者 | 同时给出 `chara_id`（协议带）与担当 |
| 理事长 / 记者 | `MatsuriPerson::yayoi()` / `reporter()` | 羁绊已由通用段 `friendship_noncard_*` 提供（当前引擎标「未接线」，需在本剧本启用） |
| 吾妻 | 卡组里的友人卡（`card_type ≥ 5`） | `FriendState` 已由通用段还原 |

神驹的 `avail_turn` / `needs_great_success` **协议不区分**：第 1 批（每演目 1 名固定 + 1 名余额，共 6 名）
turn 2 起可用；第 2 批（每演目 1 名，共 5 名）turn 48 起、且需 Classic 進捗発表会大成功。只能按 matsuri
静态规则 + `check_points[1]` 反推；`❓G6` 记录待核点（真机神驹数量与批次是否恒为此形态）。

### 3.4 演目与教诲

- `gauge` / `level`：`act_info_array[].{act_gauge, level}` 直读；`act_type 1–5 ↔ 训练位 0–4` 与离线一致。
- **`❓G2`**：协议 `level` 的起点是 0 还是 1（离线 `ActLedger::default()` 为全 1）。若协议 0 起，还原时 +1。
- **`❓G1`（最重要）**：`advice_info_array` 在**已転成**后到底怎么给——
  - 解释 A：`advice_id` = 槽位基础教诲（101–501），`fusion_advice_id` = 転成目标（601–1001）→ 直接映射 `slots / fused`；
  - 解释 B：転成后 `advice_id` 已换成目标教诲、`fusion_advice_id` 归 0 → 需要额外保留「原基础教诲」才能算 101 的固定升级效果。
  
  当前 `FestaScenarioAnalyzer` 的投影按 A 实现（面板显示 `牵绊→特化`）。**接线的第一步就是抓一局転成前后的帧确认**，
  否则 `fused` 会错，助力支撑 / 特化主属性 / 兼修副属性全部连带错。

### 3.5 事件历史与 `fatigue_recovery`

`forced_rest` 不需要事件历史（见 §2 #12，纯函数重算）。真正依赖历史的是「一次性 flag」：

| 用途 | 算法 |
|---|---|
| `fatigue_recovery` | 事件 id = `809052004`（`friend_events.out`，友人出行解锁）：该回合置 true；此后第一个 `vital ≤ 49` 的回合置 false（引擎在 Begin 处消费）。`❓G8` |
| `event_log`（可选） | 把 `AllEvents` 投影成 `EventRecord`，纯呈现，不参与决策 |
| 事件额外效果 | **不需要还原**（已体现在 `gauge` / 五维 / 体力里），仅诊断用 |

> 注意：`baseGame.keyEvents` 只覆盖「友人首次点击 + 支援卡连续事件」两族，**不等于**全量剧本事件；
> 任何按事件 id 判定的逻辑都必须走 `AllEvents`，或由插件单独写一个 `matsuri.events` 数组（见 §4）。

### 3.6 训练等级：两笔账必须分开

离线口径：`level = train_level_count / 4 + 1 + train_level_bonus`（夹 `normal_train_level_max`）。

协议只给**结果等级** `training_level_info_array[].level`，而且它已经吃掉了発表会大成功的 +1。
所以两笔账要分开存：`baseGame.trainLevelCount` 存**点击数**（引擎语义），`matsuri.trainLevelTotal` 存**总等级**：

```
base.game.train_level_count[i] = 该训练位的「成功普通训练」次数   （排除稽古、排除训练失败）
matsuri.trainLevelTotal[i]     = training_level_info_array[i].level（含発表会 +1）
train_level_bonus[i]           = max(0, total − (click / 4 + 1))
```

这与拉面的处理同型（拉面用 `4×(level−1) + 余数`，本剧本因为多了発表会 +1，必须显式拆开）。
拉面的 `ApplyTrainLevelProgressFromHistory` 可直接抽成共用工具（仅「稽古不计」这一条是剧本差异）。

### 3.7 助力支撑与追加稽古

| 量 | 首选 | 回退 |
|---|---|---|
| `assist_support_left` | `home_info.training_bonus_remain_turn_num`（语义为推测，`❓G4`） | 历史重算：逐回合看「稽古成功且参与者含助力绑定演目担当」→ 重置 3，成功后 −1 |
| `kensan_pending` | `command_info_array[].is_upper_training_event_occurred`（已定位） | 无（拿不到就不置位，与面板一致） |

两者都直接决定训练值，「拿不到就不显示」在面板可接受，**在决策链路上不可接受**（少 5% 训练效果会改变排序）。
因此 §7 把这两条列为**接线前必须核对**项。

---

## 4. `thisTurn.json` 的 `matsuri` 段草案

约定沿用现有插件：`baseGame` 段不动，剧本段用剧本名小写（拉面 = `ramen`）→ 本剧本 = **`matsuri`**。
`scenarioId = 15`。字段命名与 `baseGame` 一致用 `camelCase`；`null` 表示「本次取不到」，不要用 0 冒充。

```jsonc
{
  "baseGame": { /* 现有通用段，不改 */ },
  "matsuri": {
    // A：协议直读
    "acts": {
      "gauge": [3, 5, 2, 0, 4],        // act_info_array[].act_gauge（顺序 = act_type 1..5）
      "level": [4, 6, 3, 1, 7],        // act_info_array[].level（❓G2：0 起还是 1 起）
      "cap": 10,                        // festa_info.max_act_level
      "gaugeMax": 5                     // 静态 rules.act_gauge_max（便于 matsuri 交叉校验）
    },
    "advice": {
      "slots": [101, 201, 301, 0, 501], // 基础教诲 id（0 = 该槽未分配）
      "fused": [601, 0, 0, 0, 901],     // 転成 id（0 = 未転成；❓G1 决定这两行怎么填）
      "acquired": true
    },
    "camp": { "remain": 2 },            // summer_camp_remain_turn_num；year/counted 由 matsuri 反推
    "checkPoints": [                    // check_point_result_array，按 schedule 顺序 3 项
      { "scheduleId": 1, "result": "great" },  // ❓G3：result_type → none/success/great
      { "scheduleId": 2, "result": "success" },
      { "scheduleId": 3, "result": "none" }
    ],
    "kensanPending": false,             // 任一 command.is_upper_training_event_occurred
    "assistSupportLeft": 2,             // ❓G4：0 表示已用完；null 表示取不到
                                        // finalsDone 由 checkPoints 派生，不单列

    // B：小黑板端用历史补齐
    "trainLevelTotal": [4, 5, 3, 1, 6],  // training_level_info_array[].level（含発表会 +1）；
                                         // 点击数写在 baseGame.trainLevelCount（见 §3.6）
    "persons": [                        // 只列「非卡人头」与卡的担当，卡本身由 cardId 推导
      { "targetId": 0, "charaId": 1001, "act": 0, "origin": "card" },
      { "targetId": 6, "charaId": 1096, "act": 1, "origin": "guest", "availTurn": 2, "needsGreatSuccess": false },
      { "targetId": 12, "charaId": 1128, "act": 4, "origin": "guest", "availTurn": 48, "needsGreatSuccess": true }
    ],

    // C：小黑板端用历史算好的量（plan / forcedRest / career_failed 等不写，matsuri 自算）
    "fatigueRecovery": true,
    "events": [                         // AllEvents 全量（Turn + StoryId + SelectIndex）；G7 去重后只写已发生的
      { "turn": 34, "storyId": 809052004, "choice": 0 }
    ]
  }
}
```

**字段取舍原则**

- `plan` / `forced_rest` / `deck_warnings` / `career_failed` 一律不写：matsuri 有静态表 + 纯函数，重算比传值更不容易漂。
- `trainLevelTotal` 必须写（协议等级里有発表会加成，拆不开）；`baseGame.trainLevelCount` **改取值口径**
  （scenario 15 走历史点击数，不用 `4×(level−1)`），两者一起才能还原 `train_level_bonus`。
- `assistSupportLeft` 用 `null` 表达「取不到」，让 matsuri 决定是退回历史推算还是保守按 0。
- `persons` 的 `targetId` 同时给出：卡位（0–5）与神驹的协议 id，便于 matsuri 对齐 `persons` 下标。

---

## 5. 两端改动清单

### 5.1 小黑板端（`SendGameStatusPlugin`）

| 项 | 内容 |
|---|---|
| `GameStatusSend_Matsuri.cs` | 新文件；`scenarioId = 15`；注册端点 `single_mode_festa/{load,exec_command,check_event,attach_advice,check_point,continue,...}`（与 `FestaScenarioAnalyzer` 同一套） |
| `MatsuriStatus` | 由 `SingleModeFestaDataSet` + `chara_info` + `home_info` 构造 A 段 |
| `MatsuriHistory` | 由 `EventLogger.Current` 算 B/C 段：训练点击数（写进 `baseGame.trainLevelCount`）、`assistSupportLeft` 回退、`fatigueRecovery`、`events` |
| 帧时机 | 只在**决策点**写：`playing_state == 1`、无待处理事件、非比赛、非中间态（复用 `GameStatusSend_Base` 的 `source` 解析：`command` / `event`）；同回合重复帧按 `❓G7` 去重 |
| `trainLevelCount` 口径 | `GameStatusSend_Base` 现有实现给 `4×(level−1)`（等级内余数恒 0）。本剧本须走 scenario 15 分支：用历史点击数（排除稽古 / 失败），否则 matsuri 会把「距下一级还差 4 次」判错（拉面踩过同坑） |
| 共用工具 | 把拉面的 `ApplyTrainLevelProgressFromHistory` 抽成「点击数回补」工具，加「稽古不计」分支 |

**决策点 D1：要不要依赖 `FestaScenarioAnalyzer`？**

| 方案 | 优点 | 代价 |
|---|---|---|
| **D1-a 自持投影**（推荐） | 不新增 submodule；帧内容与呈现解耦（面板改版不影响帧） | 约 100 行重复投影代码；演目 / 教诲 id 语义要各自维护 |
| D1-b 复用 `FestaScenarioAnalyzer` 快照 | 与拉面先例一致（`RamenScenarioState.Snapshot()`）；口径单一 | 需把 `FestaTurn` 提升为公开 DTO；两个插件版本必须成对发布；面板字段变化会波及帧 |

推荐 **D1-a**：本剧本的帧字段比面板少（不要参与者姓名、不要颜色），重复面只在「act_type ↔ 训练位」与
「advice 槽位」两处，且这两处正是 `❓G1/G2` 要单独核对的地方——分头维护反而更安全。

### 5.2 matsuri 端（`umaai-matsuri`）

| 项 | 位置 | 内容 |
|---|---|---|
| 帧段解析 | `crates/matsuri-ai/src/frame.rs`（新） | `serde` 结构体，**不加** `deny_unknown_fields`（引擎纪律：插件新增字段不得让整帧失败）；缺 `matsuri` 段 → 报错「不是本剧本的帧」 |
| 还原入口 | `crates/matsuri-core/src/game/restore.rs`（新） | `MatsuriGame::restore(base: BaseGame, frame: MatsuriFrame, data: Arc<MatsuriData>, config: MatsuriConfig) -> Result<Self>`（§3.1 四步）；`base` 由会话传入，**不重建** |
| 回合结构 | 复用 `derive_plan` | 不进帧 |
| Session 接线 | `crates/matsuri-ai/src/lib.rs` | `Session::new(cfg, trainer, fe, params, src, build)`；`build: FnMut(BaseGame) -> Result<MatsuriGame>`；`BaseParams` 填 `friend_chara_id = 吾妻`、`friend_first_event = 809052001`（`friend_events.first`）。**注**：`build` 只拿得到 `BaseGame`，拿不到帧原文 → 须由 `matsuri-ai` 在 `session.step()` **之前**解析 `matsuri` 段，存进闭包捕获的 `Rc<RefCell<Option<MatsuriFrame>>>`，`build` 里 `take()`。不改引擎契约（不为单剧本给 `build` 加参数） |
| 决策回写 | 引擎 `record.rs` | 现协议无回传通道，仅本地记录 / 呈现 |
| 呈现 | `matsuri-cli` 在线档 | 打印还原态 + `MatsuriView`，与手动面板并排比对 |

`restore` 里**必须**保留的两条注释（防止后人「优化」）：

1. `init_persons` 的分配结果会被整段覆盖——离线分配只为 `deck_warnings` 与兜底；
2. 不得从 `Begin` 进入（会重复抽事件 / 重复掷追加稽古）。

---

## 6. 落地顺序与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| M0 采样 | 真机抓 1 局完整帧（重点：転成前后、発表会前后、合宿延长、追加稽古掷中回合） | 各 `❓G#` 有结论 |
| M1 写帧 | 小黑板端产出 `matsuri` 段（A 段先落地，B/C 段后补） | 字段与本文第 4 节逐项对上；`thisTurn.json` 可被 `parse_base_status` 照常解析 |
| M2 还原 | `restore` + round-trip 守门 | **`restore(game.base.clone(), export_matsuri(&game), …)` 与原局面逐字段 `assert_eq`**。只测「剧本段 ↔ 剧本字段」这一层，通用段交给引擎既有测试；`export_matsuri` 只服务工具面 / 测试，字段映射与本文第 4 节同源。白名单只排除 `strategy` / `keiko_settling` / `event_log`（`data` 为 `Arc`，比的是值，可留在比较里） |
| M3 接线 | `Session` 在线档跑通一局 | 每帧决策与同状态离线局一致；`on_decision` 不报错 |
| M4 闭环 | 连续 78 帧回放 | 决策一致率、终局评分与真机对照（只作趋势，不作基线） |

M2 的 round-trip 是**整套方案的核心守门**：它能在没有真机的情况下，先把「帧字段 ↔ 状态字段」的映射钉死；
真机只用来验证 A 段的取值口径（`❓G#`）。

---

## 7. 待核清单

| 编号 | 问题 | 判定方法 | 影响 |
|---|---|---|---|
| **G1** | `advice_info_array` 転成后的形态（基础 id 保留？还是换成目标 id？） | 抓転成前 1 帧 + 后 1 帧对比 `advice_id / fusion_advice_id` | `slots` / `fused` → 教诲全部效果（助力 / 特化 / 兼修 / 101 固定升级） |
| **G2** | `act_info_array[].level` 起点（0 还是 1） | 开局第 1 帧看 level | 演目 Lv 偏移 1 级 → 全演目效果与考核判定 |
| **G3** | `check_point_result_array[].result_type` 的取值枚举、`schedule_id` 与 3 场発表会的对应 | 抓発表会前后帧 | `check_points` / `finals_done` / 第 2 批神驹解锁 |
| **G4** | `home_info.training_bonus_remain_turn_num` 是否真是「助力支撑剩余次数」 | 触发与消耗助力支撑时逐帧观察该值 | `assist_support_left` → 训练效果 5%（最多 3 次） |
| **G5** | `evaluation_info_array` 是否覆盖全部担当（含未登场 / 已解雇） | 与游戏内演目负责人列表比对 | `persons[].act` 完整度 |
| **G6** | 编成外神驹的批次数与 `avail_turn` 规则（第 2 批是否恒 48 / 恒需大成功） | 抓 T2 与 T48 前后的 `evaluation_info_array` | 神驹可用回合 → 分布与训练值 |
| **G7** | 同回合**重复帧**（`attach_advice` / `fusion_advice` / `check_point` 后仍会再出一帧）如何识别与幂等 | 抓「教诲分配后」「转成后」的连续帧，看 `turn` + `playing_state` + `source` 组合 | 在线循环会把同一回合决策两次；需定「同 (charaId, turn) 只取首帧」或「帧序号去重」 |
| **G8** | 疲劳回复 flag 的消费时点（是否恒在回合 Begin、`vital ≤ 49` 判定用哪个时点的体力） | 解锁后连续几帧看体力与 flag 隐式效果 | 体力 +15 一次性收益 |
| **G9** | 延长段（`turn > window.end`）`summer_camp_remain_turn_num` 是否继续递减 | 抓延长段连续帧 | `camp.counted` 反推 |
| **G10** | 帧时机：`playing_state` 在比赛 / 事件 / 转成中途的取值组合 | 逐端点采样 | 防止把中间态当决策点（会误导决策） |

---

## 8. 与既有文档的关系

- 机制口径：`mechanism.md`（何时结算、`plan` 含义）、`matsuri_memo_cn.md` §2.2（担当五步流水线）。
- 术语：`glossary.md`（`MatsuriView` / 追加稽古 / 助力支撑 / 合宿延长）。
- 引擎契约：`../../umaai_next/docs/scenario/contracts.md` §5（`Session` 帧驱动）、§7（View DTO）。
- 进度：`plan/scenario_dev_plan.md` S8「在线外壳」；落地后本文件升为实施记录，`❓G#` 转为结论写回 `glossary.md`。
