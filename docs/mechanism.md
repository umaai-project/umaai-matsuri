# 剧本机制文档（骏大祭 / matsuri）

> 只写**生效公式、流程与实现落点**；术语以 [`../GLOSSARY.md`](../GLOSSARY.md) 为准，口径裁定与实验记录见 [`changelog.md`](changelog.md)，数值占位见 [`stubs.md`](stubs.md)，测试名录见 [`tests_overview.md`](tests_overview.md)，线上协议见 [`online_protocol.md`](online_protocol.md)。
> 未确认机制标 `❓`。静态数值表与默认策略参数分别以 `data/scenario_matsuri.json`、`data/matsuri_config.toml`、`data/ai_config.toml` 为单一事实源。

## 1. 回合与阶段

> 口径：本文 turn 一律 **0 基**（`turn ∈ [0, 77]`）；`master.mdb` 与游戏内显示为 1 基。

- **时间轴**：全 78 回合（`Junior` / `Classic` / `Senior`）。`turn 2` 神驹加入并启用剧本机制（0–1 回合不获取演目槽等剧本资源）；`turn 23 / 47 / 67` 为进度汇报会与午式骏大祭正式演出（门槛全演目 Lv `3 / 5 / 6`，`single_mode_15_schedule` `need_act_num = 5`）；`turn 68–71` 为正式演出后的空窗期；`turn 72–77` 为 URA 决赛（`73 / 75 / 77` 比赛、`72 / 74 / 76` 训练）。
- **阶段机与 `TurnPlan`**：内部阶段链固定 `Begin(0) → Distribute(3) → Train(1) → AfterTrain(4) → End(2)`（对外 `StageId` 编号只增不改）。比赛、合宿、进度汇报会、正式演出、空窗期、URA 均由每回合 `Begin` 派生的 `TurnPlan` 表达，不单开阶段：
  - `Race / ActTrain / Outing` 为 `Train` 阶段动作；
  - `T2 / T11 / T35 / T59` 教诲传授或改派在训练前 `Begin` 收尾；
  - 进度汇报会、正式演出、合宿全演目 +1、`T23 / T47` 教诲转成与改派均在 `End` 结算。
- **骏大祭合宿与延长**：常规窗口 `turn 12–15 / 36–39 / 60–63`。从 `window.start` 起每回合 `End` 计数，目标赛与强制休息回合不计入，累计满 4 个非比赛合宿回合后结束（自动触发合宿延长）。全演目 `Lv +1` 固定在 `turn 15 / 39 / 63` `End` 结算，不随顺延推迟。
  - **合宿期训练**：5 个训练位全转为演目排练（不受演目槽是否蓄满影响），合宿排练**不提升演目 Lv、不清空演目槽**。
  - **第 1 年 vs 第 2/3 年与延长段**：第 1 年合宿（含延长）可普通/友人外出、使用普通休息、触发随机事件；第 2/3 年常规窗口禁止普通/友人外出、使用合宿版休息、抑制随机事件；第 2/3 年延长段禁止普通/友人外出，但恢复普通休息且事件轴等同正常回合。
- **空窗期与 URA 决赛**：`turn 68–71` 按日常机制运行（槽满才转排练，午式骏大祭演出效果不生效）。`turn 72–77` 全部 5 个训练位转为演目排练，可休息、普通出行或治病，不可自选比赛与友人出行；与合宿不同，URA 排练成功仍按日常结算 `Lv +1` 与清槽，且午式骏大祭演出效果生效。
- **强制休息**：目标赛回合可被固定事件转为实际休息（`plan.race = true`，但不触发比赛事件与奖励）；当前通过 `game/stage.rs::is_forced_rest_turn` 临时表（以 `chara_id = uma_id / 100` 为键）与 `MatsuriGame::forced_rest` 判定。

**实现落点**

| 机制点 | 数据字段 | 阶段 / 动作 | 结算函数 | 测试落点 |
|---|---|---|---|---|
| 阶段链与 `TurnPlan` | `MAX_TURN`、`schedule` / `camp` / `advice_windows` | `Begin` / `End` | `game/stage.rs` `derive_plan` / `Stage::next` | `stage.rs` + `tests/mechanics.rs` |
| 合宿窗口、延长与 +1 | `camp` | `Begin` / `End` | `game/stage.rs::CampState`、`game/settle.rs::settle_turn_end` | `stage.rs` + `tests/mechanics.rs` |
| 强制休息 | `FORCED_REST_TURNS` | `Begin` / `Train` | `game/stage.rs::is_forced_rest_turn`、`game/mod.rs::forced_rest` | `tests/mechanics.rs` |
| 回合后事件排空 | `BaseGame.unresolved_events` | `AfterTrain` / `End` | `game/stage.rs::stage_body` | `tests/mechanics.rs` |

## 2. 资源、负责人与分布

- **账本**：5 条演目槽（上限 5，不随回合衰减）+ 5 个演目 Lv（基础上限 6，编入 SSR 吾妻后为 10）。`turn 0–1` 由 `MatsuriGame::scenario_started`（`turn ≥ SCENARIO_START_TURN = 2`）拦截，不获取任何演目槽。演目、训练、`command_id` 与五名固定神驹映射见 `data/scenario_matsuri.json`（`acts`）。
- **入账与分摊**：
  - 对应演目负责人每参与 1 人 `+1`（闪彩负责人 `+2`，同场其余负责人仍 `+1`）；
  - 目标比赛 `+4`（拆为 4 次 `+1` 逐次分摊）；休息、普通出行、友人出行、非目标比赛 `+1`；
  - 通用落点三则（满槽跳过）：**演目 Lv 最低 → 当前演目槽最低 → 本批起始槽最低 → 速→耐→力→毅→智**（`count = 1` 时第三键恒等于当前槽）。
- **消费与延期**：演目槽蓄满后对应训练变为独立动作 `ActTrain(act)`，可延期不消失，多个同时亮起时**所有亮起位置均锁人**。排练成功时该演目 `Lv +1`、演目槽清零；排练失败按训练失败处理，触发失败事件、无属性收益，且**演目槽与演目 Lv 均不变**。执行排练的回合 5 个演目槽均不增长。

### 2.1 担当分配与每回合分布

- **担当分配（建局一次性动作）**：在 `turn 2`（第一批 5 名神驹 + 余额 1 名）及 `turn 47` 大成功（第二批 5 名神驹）生效，**不是游戏阶段，不占回合**。分配流水线：`剧本链接角色钉位 → 支援卡按类型本位（每位容量 2 张）→ 外派链 → 嘉宾每演目 1 人 → 吾妻余额补位`。吾妻自身不参与担当分配。
- **编组检测**：建局前由 `game/deck.rs::inspect_deck` 执行校验，禁止项拒绝建局，警告项写入 `MatsuriGame::deck_warnings` / `MatsuriView.deck_warnings`：
  - **禁止项**：`E1` 未带 SSR 吾妻；`E2` 友人卡 >1 张；`E3` 带 SSR 吾妻以外的友人卡；`E4` 不满足 6 张卡（5 普通 + 1 友人、无重复人物、不含育成马娘、拒绝团队卡）。
  - **警告项**：`W1` 剧本链接卡类型与固定演目不一致；`W2` 同类型卡 ≥3 张；`W3` 卡被外派到非本职演目；`W4` 支援卡类型 <5 种（含友人）；`W10` 各演目「卡 + 第一批嘉宾」人数极差 ≥2。
- **每回合分布（`Distribute`）**：
  - 常规位按卡面得意率加权 / 嘉宾等权（均先判不在率，嘉宾不在率暂定见 [`stubs.md`](stubs.md)）；进入分布后不再受「每训练位 2 张卡」限制，仅受**每训练位至多 5 人（含嘉宾）**与**每训练位至多 1 名社交人头**（`Person::is_friend()`：友人/团队/理事长/记者）约束；
  - **锁人**：任何槽满亮起的演目，其负责人（卡与嘉宾）必锁定在该演目训练位，不判不在率；排练位上的非负责嘉宾仍可随机同框，但不计入加槽人数；
  - **理事长 / 记者**：不会出现在排练位（有排练位时只落普通位，全排练回合本回合不出现）；理事长 `T1` 起出现，记者 `T13` 起出现；
  - **吾妻**：存在排练位时，在锁位之后、其余人头之前必补入某个未满 5 人的排练位（不判不在率，不落普通位）；无排练位的普通回合走通用不在判定与常规落位。
- **分身**：由 `101+601`（合宿）、`801` 协力或午式骏大祭演出触发，以**同一 `chara_id` 在 `distribution` 重复占位**实现（不新建人头），落入**非本职训练位**（不叠在排练位），有效至该演目槽因排练清零为止。分身视同完整人物：计入落点人数（含教诲叠乘）、完整生效卡效果（非本职不触发友情彩圈）、加槽计入自身负责的未满演目，并与本体共享牵绊与灵感状态。

### 2.2 人头实现形态

- `MatsuriPerson`（`game/person.rs`）封装 `BasePerson` 与 `act`、`origin`（`PersonOrigin`）、`avail_turn`、`needs_great_success`。`PersonOrigin` 在构造时单向决定引擎契约 `PersonType`（`game/traits.rs`），互不反推。
- 模拟口径不允许两个不同人头同 `chara_id`；编成卡、育成马娘、5 名神驹与已出现人头均从「随机非链接角色」候选中排除（育成马娘为链接角色时，该演目固定神驹由随机非链接角色顶替，离线按序取人口径见 [`stubs.md`](stubs.md)）。

**实现落点**

| 机制点 | 数据字段 | 阶段 / 动作 | 结算函数 | 测试落点 |
|---|---|---|---|---|
| 演目槽 / Lv 账本 | `acts` / `rules` / `schedule` | `game/mod.rs::ActLedger` | `gain_lowest_act_gauge` / `gain_act_gauge` / `gain_training_gauge` / `do_race` / `do_rest` | `game/mod.rs` + `tests/mechanics.rs` |
| 每回合分布 | `distribution` / `absent_weight` | `Distribute` | `game/distribute.rs::distribute_round` 等 | `distribute.rs` + `tests/mechanics.rs` |
| 人头与担当分配 | `acts`、编组卡组 | 建局 / T2 / T47 | `game/person.rs::MatsuriPerson`、`game/roster.rs::init_roster` 等 | `roster.rs` + `tests/mechanics.rs` |
| 编组检测与视图 | `limits.support_num`、`outing` | 建局 / 视图投影 | `game/deck.rs::inspect_deck`、`game/view.rs::MatsuriView` | `tests/mechanics.rs` |

## 3. 训练与加成

### 3.1 基础值、训练 Lv 与上下层公式

- **命令与基础值**：普通训练 `101 / 105 / 102 / 103 / 106`；演目排练 `5101–5105`（`command_level = 演目 Lv` 仅作效果索引）。普通 Lv1–5 与排练基础值见 `data/scenario_matsuri.json`（`training.normal` / `training.keiko`）。排练基础值仅按时期分两档（`Junior` 与 `turn 24` 起的 `Classic`，无 `Senior` 第三档），与演目 Lv / 训练 Lv 无关。
- **训练 Lv**：`train_level = min(5, floor(普通训练成功次数 / 4) + 1 + train_level_bonus)`（落点 `traits.rs::train_level`、`train.rs::settle_training`）；演目排练与训练失败不计入次数，`train_level_bonus` 源自进度汇报会大成功一次性奖励。
- **两层计算口径**：
  1. **下层截断**：调用引擎 `default_calc_training_value` 算普通/通用加成（友情、训练效果、人数、干劲、成长率等），由 `traits.rs::clip_lower_gain` 先对基础增量逐轴截断 `min(lower, rules.training_upper_cap)`（`100`）。
  2. **上层剧本乘区**（`game/train.rs::training_upper`，全部乘算后统一 **floor**）：
     - **主属性**：`floor(下段 × (1 + 友情合计) × (1 + 训练效果合计) × (1 + 主获得量合计)) - 下段`
     - **副属性**：`floor(下段 × (1 + 友情合计) × (1 + 训练效果合计 + 副获得量合计)) - 下段`
     - **SP**：`floor(下段 × (1 + 友情合计) × (1 + 训练效果合计 + SP训练效果)) - 下段`
     - **排练全五维 +10**（`keiko_flat`）：加在上层上限裁切之外（不受乘区与上限影响）。
- **上层上限与体力**：
  - `status_cap = 100 + 特化 status_limit + (排练且 5 种卡时该演目 Lv9/10 的 status_limit)`；
  - `sp_cap = 100 + (排练且 5 种卡时该演目 sp_limit)`（不吃特化上限）；总增量最后受 `five_status_limit - five_status` 截断。
  - 体力先按体力教诲/转成 `keiko_vital_cost_drop` 降耗，URA 训练（`72–77`）再叠加 `+30` 体 / `+1` 干劲。

### 3.2 演目效果与教诲加成

- **演目效果**：`single_mode_15_act_effect`（66 行）逐级数值见 `data/scenario_matsuri.json`（`act_effects`）。「5 种卡」要求编组支援卡含 ≥5 种训练类型（含友人）。`effect_type` 全局同义：`4` 擅长率、`202` 友情、`2` 灵感率、`204` 赛后加成。
- **教诲叠加与例外**：教诲效果级别取**所绑定演目的 Lv（1–10）**，按绑定演目负责人在该训练位的**参与人数**叠加（转成升级效果同口径）；例外：
  - **牵绊教诲 `+3`**：全编组支援卡平摊，不按人数叠乘；
  - **研习教诲**：每回合 `Begin` 按绑定演目 Lv 共通掷骰一次（不按人数叠加），命中后由绑定演目负责人参与的任意成功训练/排练触发 `400015106`（上/下档收益与概率占位见 [`stubs.md`](stubs.md)）；
  - **助力支撑**：助力绑定演目负责人参与的成功排练后获得/重置为 3 次内训练效果 `+5%`（`rules.assist_support`）。

| 教诲 | 作用 | 关键口径 / `effect_type` |
|---|---|---|
| 牵绊（101） | 全编组卡牵绊 `+3` 平摊 + 排练主基础 % | `16 / 13` |
| 擅长率（201） | 擅长率 `+10…50` | `4` |
| 灵感（301） | 灵感发生率 `+30…80%` | `2` |
| 助力（401） | 非友情训练效果 `+20…40%` + 助力支撑 | `20`（`cond 3`） |
| 体力（501） | 演目排练体力消耗 `-1…15%` | `3`（负值） |
| 特化（601） | 主基础 `+5…25%` + 基础上限 `+5…25`（亦作用于 SP 上限） | `13 / 206` |
| 研习（701） | 追加排练发生率 `20…80%` | `8` |
| 协力（801） | 友情 `+1…10%` + 该演目每名负责人额外 1 处分身 | `202 / 18` |
| 不屈（901） | 体力阈值下失败率 `-100%`（Lv10 全免，Lv9 需体力 ≥5）+ Lv8+ 追加排练效果 | `205`（`cond 2`）/ `203` |
| 两道（1001） | 副基础 `+1…25%` + 演目排练效果 `+1…15%` | `21 / 203` |

### 3.3 失败率与训练后处理

- **失败率**：`基础值 × 卡面失败率下降 × 剧本失败率下降 + 好调/不调偏差(∓2)`，夹 `[0, 100]`；剧本下降（不屈阈值或 URA 演出 `-100%`）≥100 时归 0。
- **训练后处理（`game/train.rs::after_train`）**：
  - **牵绊**：基值取 `rules.friendship_gain`（友人 `+4` / 普通卡 `+7` / 理事长·记者 `+5`；编组外嘉宾不给牵绊；牵绊教诲参与时全编组卡 `+3` 平摊；爱娇在 `add_friendship` 统一 `+2` 一次）；
  - **灵感**：发生率 `7.5% × (1 + (卡面 + 汇报会 + 教诲)/100)`，逐人头独立判定；**全亮**＝该人头所在训练位为排练且编成 5 种卡（该位上的编成卡全亮，不要求演目 Lv）；**全结算**＝该位排练 + 5 种卡 + 该演目 `all_hint`（Lv≥4）时该位红点依次触发，否则随机 1 人（嘉宾除外）；转成 `301` 灵感 `Lv +1`；
  - **灵感效率**：`[pre].hint_efficiency`（默认 0.92）把「单技能封顶」的期望损耗折成确定性折扣——每次技能灵感按系数折算（小数进位，不新增随机点），容量判定仍按 `5 × 技能数` 名义累计；等价于逐技能分桶的期望损耗（默认卡组 6.4% / 6330 卡组 8.8%）。
- **每卡灵感上限**：`5 × 技能数`（`data/cardDB.json` 的 `skillCount`，master.mdb `single_mode_hint_gain` type=0；标准卡 10–14 技能 → 50–70 级，友人卡池为 0 不再发技能灵感）；缺失或 0 回退引擎 `max_hint_per_card`（60）。
- **灵感次数（阈值固有词条 33）**：命中一次灵感后，该人头按 `1 + 当前羁绊下的 Hint数量提升` 发放；次数在**生成灵感事件时现算**（`do_hint` / `do_hint_with_bonus`），因此训练后羁绊跨过门槛时本条灵感立即 `+1`；不读 `effect` 缓存；
  - **追加训练（`102`）**：`after_train` 按 `constants.event_probs.extra_train`（15%，非夏合宿）抽取，选项 1（体力 `-5` / 属性 `+5` / 理事长牵绊 `+5`）定点清除 `Ill::FAT`。

**实现落点**

| 机制点 | 数据字段 | 阶段 / 动作 | 结算函数 | 测试落点 |
|---|---|---|---|---|
| 训练基础值与上下层 | `training.normal`、`rules.training_upper_cap` | `Train` | `train.rs::do_train` / `settle_training`、`traits.rs::clip_lower_gain` | `train.rs` + `tests/mechanics.rs` |
| 演目排练 | `training.keiko.{junior,classic}`、`act_effects` | `ActTrain` | `train.rs::do_act_train` / `training_upper` | `mod.rs` / `train.rs` + `tests/mechanics.rs` |
| 训练后处理 | `rules.friendship_gain` | `Train` / `ActTrain` 后 | `train.rs::after_train` | `distribute.rs` / `train.rs` |
| 教诲叠加 | `advices` / `fusion` | `Train` / `ActTrain` | `advice.rs::advice_slot_effect` / `participant_advice` | `advice.rs` / `train.rs` |

## 4. 比赛与赛程

- **赛程与基础奖励**：固有目标赛、URA 决赛（`73 / 75 / 77`）、自选赛（`13–71` 且等级表非 0）。基础奖励落 `data/events.json`（目标赛 五维 `+5` / `SP +72`；非目标 GI/GIII 随机 1 项 `+10/+8`、`SP +45/+35`；URA 预选/准决/决 `+10`、`SP +40/+60/+80`），所有乘算收益统一 **floor**。
- **赛后加成与特殊状态**：
  - 卡面赛后 `uma.race_bonus` 与汇报会/正式演出 `check_point_effect 204` **相乘**：`scale = (100 + uma.race_bonus) / 100 × (100 + check_points.race_bonus) / 100`；
  - **铁心的挑战者**（生野狄杜斯 `106301 / 106302`）：出道战后（`turn 12–71`）所有生涯赛五维 `×2.5`、PT `×0.46`（五维在剧本赛后之前乘算，PT 经 `race_pt_rate` 独立缩放）；自选赛、出道战与 URA 不生效。
- **自选比赛义务区间（`freeRaces`）**：部分马娘在 `umaDB.json` `freeRaces` 规定区间内需打满 `count` 场不低于 `grade` 门槛的比赛（`1 = G1` 最高）。在**区间末回合 `End`**（`settle_free_race_deadline`）判定：达标触发一次「生涯比赛胜利」（`5/72`，吃 `race_scale` 与 `race_pt_rate`）；未达标置 `career_failed = true`、立即终止育成并跳过终了奖励。
- **属性上限**：基础上限 `2200 / 1850 / 1800 / 1700 / 1800`，上限突破后 `2500`（`❓` `single_mode_scenario.max_*` 与运行时 `max_stats` 关系待核）。

**实现落点**

| 机制点 | 数据字段 | 阶段 / 动作 | 结算函数 | 测试落点 |
|---|---|---|---|---|
| 比赛结算与铁心状态 | `data/events.json`、`check_points[].race_bonus` | `Race` | `action.rs::do_race`、`mod.rs::race_scale` / `race_pt_rate` / `special_race_stats_bonus` | `tests/mechanics.rs` |
| 自选比赛义务区间 | `umaDB.json` `freeRaces` | `End` / `Race` | `settle.rs::settle_free_race_deadline`、`mod.rs::free_race_progress` | `game/mod.rs`（`free_race_*`）+ `tests/mechanics.rs` |
| 休息 / 出行 / 治病 | `consts.rest_probs`、`outing` | `Rest` / `Outing` / `Clinic` | `action.rs::do_rest` / `do_normal_outing` / `do_friend_outing` / `do_clinic` | `tests/mechanics.rs` |

## 5. 教诲窗口、汇报会与事件

### 5.1 教诲传授 / 改派 / 转成

- **5 槽模型与窗口**：永远维持 5 个教诲槽（对应 5 演目），`advice_windows` 对应 `turn 2 / 11 / 23 / 35 / 47 / 59`：
  - `T2 Begin`：获得 5 个基础教诲（`101–501`）并首次分配到 5 演目（必全满，当回合生效）；
  - `T11 / T35 / T59 Begin`：合宿直前可选 5 槽改派（或保持不变）；
  - `T23 End`（进度汇报会后）：从 5 个基础教诲中选 2 个分别与 `601` / `701` 转成，随后 5 槽改派；
  - `T47 End`（进度汇报会后）：剩余 3 个基础教诲分别与 `801` / `901` / `1001` 转成，随后 5 槽改派。
- **转成规则与配置**：转成消耗原基础教诲，由「升级版基础教诲（`fusion_effect`）+ 追加教诲效果」占用同一槽；`T23` 已转成槽位在 `T47` 不再转成，仍参与改派。默认融合预设与校验由 `deck_config.rs` 从 `data/matsuri_config.toml` `[decks.*.fusion]` 加载（`year2` 2 槽、`year3` 剩余 3 槽，不进搜索树），并支持深合并继承 `default`。**在线实机由玩家自选配对，帧内已融合的槽位以帧为准、不回滚不改写**；融合窗口只补当前档预设里尚未生效的目标——预设 base 若已被实机抢先融合，则由配置顺序里下一个未融合的基础教诲承接（`apply_window_fusion`，对账结果进帧提示）。
- **决策空间**：默认融合下每个窗口为 `5! = 120` 种排列（改派窗口额外含「保持不变」），通过单步 `Action::AssignAdvice(AdviceAssignment)` 提交；每个窗口只按当前槽位效果估值，不为未来转成做铺垫。**`801` 协力硬约束**：`T47 / T59` 必须分配到演目负责人最多的演目。

### 5.2 进度汇报会与午式骏大祭正式演出

- `turn 23 / 47 / 67 End` 由 `game/mod.rs::judge_presentation` 与 `game/settle.rs::settle_check_point` 按实际演目 Lv 判定：5 个演目均达 `3 / 5 / 6` 为**大成功**，否则为**成功**。
- 永久骏大祭加成（含 `schedule 0` 基线得意率 `+20`）见 `data/scenario_matsuri.json`（`check_points`）；大成功在 `t24 / t48` 额外授予 `train_level_bonus +1`（独立于训练计数）。技能灵感与粉丝一次性奖励暂不建模（见 [`stubs.md`](stubs.md)）。
- **午式骏大祭演出（`finals_act_effect`）**：体力 `+30`、干劲 `+1`、失败率 `-100%`、全部排练的编组卡 `+1` 处分身；**仅在 URA 训练（`turn 72–77`）生效**，空窗期 `68–71` 不生效（未达大成功口径暂不建模）。

### 5.3 事件管线、友人链与异常状态

- **生成与抑制（`game/event.rs::begin_events`）**：
  1. 固定事件（含 `400015104` 固定 `[11, 35, 59]`）回合不抽随机事件；
  2. 合宿抑制（`random_events_suppressed`）：第 2/3 年常规合宿抑制随机事件；第 1 年合宿与合宿延长段不经合宿抑制，回落引擎 `no_event_turns`；
  3. 通用随机分布（支援卡 31% / 马娘 25% / 掉心情 10% / 无事 34%；`no_event_turns` 已按 49 局标定补 0-based 1/2/12/68）、友人解锁抽取（`809052004` → `AfterUnlock` + 疲劳回复 flag：体力 ≤49 一次性 `+15`）与正月/抽签/结局事件。
- **结算补充（`traits.rs::apply_event` / `event.rs::apply_event_extra`）**：补齐 `add_flags/remove_flags`、事件牵绊归属、`event_extras`（最低演目槽 / 解 debuff）、追加训练 `102` 定点治变胖，以及第二/三年继承事件（`4012 / 4013`：按 `blue_count` / `[pre.inherit].extra_count` 与每场随机 `scale ∈ [0, 2)` 结算五维、PT 与上限；**折扣等级** = `extra_count[6]` × `scale`，其中 `cover` 比例按每卡剩余容量轮转吸收进已有技能桶、其余与溢出部分视作新技能直进总等级，`mode = bucket|flat`；缩放固定 `1.0`（`[pre.inherit].scale` 参数已移除，事件流仍消费一次以保持随机轴）。
- **友人链（吾妻 `809052xxx`）**：首次点击 `001`、训练后 `032`（40%）、出行解锁 `004`、出行 1–5 段 `011–015`、正月 `021`（T24）、结局 `023`（T77，取 5 次档）。
- **变胖（`Ill::FAT`）**：使训练/排练速度分量归零；回合开始按 `rules.fat_recovery_prob`（暂定 15%，见 [`stubs.md`](stubs.md)）自解，也可通过治病、合宿版休息、追加训练 `102` 选项 1、追加排练 `400015106` 上档（按 `不擅长 → 变胖 → 其余` 顺序）解除。
- **线上协议细节**：帧 `charaEffects` 病种映射、`storyId` 事件路由覆盖与在线卡组选档，见 [`online_protocol.md`](online_protocol.md)。

### 5.4 状态：切者 / 小切 / 心情盾

| 状态 | 获得途径 | 效果 | 实现落点 |
|---|---|---|---|
| 切者 | 局内事件 `5007`（3% 档）或 `[pre] qie = "qiezhe"` | 技能花费 PT `-10%`，评分 PT 项 `×1.1` | `UmaFlagsExt::gain_qiezhe` / `MatsuriGame::with_config`（清小切） |
| 小切 | 局外 `[pre] qie = "xiaoqie"` | 技能花费 PT `-4%`，评分 PT 项 `×1.04` | `MatsuriGame::with_config` 置 `UmaFlags::xiaoqie` |
| 心情盾 | 局外 `[pre] positive_thinking = N` | 每次干劲下降消耗 1 层抵消 | `MatsuriGame::with_config` 置 `positive_thinking_count` |

- 切者与小切互斥（切者优先）；局内事件 `5007` 走数据自带 3% 档，不叠加 `BasicGame` 的 5% 近似；评分折算以 `data/constants.json` `pt_score_rate`（`1.88`）为准。

**实现落点**

| 机制点 | 数据字段 | 阶段 / 动作 | 结算函数 | 测试落点 |
|---|---|---|---|---|
| 教诲转成与改派 | `[decks.*.fusion]`、`advices` | `T2/11/35/59 Begin`、`T23/47 End` | `advice.rs::apply_fusion_pairs` / `decide_advice_assignment`、`settle.rs::settle_advice_window` | `advice.rs` / `settle.rs` + `tests/mechanics.rs` |
| 汇报会与正式演出 | `check_points`、`finals_act_effect` | `T23/47/67 End` | `mod.rs::judge_presentation`、`settle.rs::settle_check_point` | `settle.rs` + `tests/mechanics.rs` |
| 事件管线与变胖 / 继承 | `scenario_events` / `friend_events` / `event_extras` | `Begin` / `AfterTrain` / `End` | `event.rs::begin_events` / `apply_event_extra` / `roll_fat_recovery`、`restore.rs::ill_from_effects` | `event.rs` + `tests/mechanics.rs` + `tests/restore.rs` |

## 6. 决策点与训练员

- **动作集边界**：常规回合可选 5 普通训练、亮起 `ActTrain`、休息、自选赛（`self_race` 开启时）、普通/友人出行、治病（生病时）；第 2/3 年合宿（含延长）禁普通/友人出行且 5 训练全转排练；URA 训练回合（`72 / 74 / 76`）仅可选 5 `ActTrain`、休息、普通出行、治病。
- **单候选直通**：强制比赛、强制休息、以及 `freeRaces` 命中 `need ≥ left` 硬保底（`MatsuriGame::forced_race`）时，手写与搜索档均仅保留唯一目标动作。

### 6.1 手写训练员（`MatsuriTrainer`）

- **目标函数**：`官方评分 = Σ status_point(夹取后) + PT × pt_score_rate`；训练员对 PT 施加 `[decks.*.trainer].pt_favor_rate`（当前默认值与各卡组档参数见 `data/matsuri_config.toml`），所有动作、事件与教诲分配折算到同一量纲。
- **动作打分（`Trainer::select_action`）**：
  1. 候选数为 1 直接选；全为教诲排列时交价值矩阵择优；生病/不擅长（不含仅变胖）且可治病时优先治病；
  2. 其余候选按 `成功收益 × (1 − 失败率) + 失败惩罚 × 失败率` 与资源期望打分，内部常数集中于 `value.rs`，策略旋钮由 `config.deck.trainer` 注入。
- **溢出与末段排练**（旋钮数值见 `data/matsuri_config.toml`）：
  - `[trainer.overflow]`：按生效上限 `five_status_limit` 与 `margin` 判定近顶轴，对非近顶轴放大 `1 + tilt × 近顶轴数 / 5`，主属性近顶时副属性与 PT 保留 `keep` 加权（仅作用于训练估值）；
  - `[trainer.endgame]`：作用于第三年合宿后至 `T71` 的普通回合，`pressure = max(0, (pending − normal_turns) / pending)`，对满槽 `ActTrain` 加 `keiko × (1 + scarcity × pressure)`、未满 `act Lv` 加 `level × min(gap, 2)`、普通 `Train` 扣 `normal_penalty × pressure`。
- **事件与教诲决策**：`select_event_choice` 对 `17` 个 `player_select` 点统一按 `prob` 求期望（含 `ExtraStatus`、状态增删与友人加成）；`decide_advice_assignment` 对 120 排列按 `Σ(槽效果 × 演目上下文)` 排序并执行 `801` 硬约束与 `101+601` 软分数，全程确定性、不耗决策随机流。

### 6.2 搜索训练员（`MatsuriSearchTrainer`）

- 封装 `FlatSearch<MatsuriGame>`，以手写 `MatsuriTrainer` 作 fallback 与 rollout 基策（rollout 内禁用搜索防递归）；`[search].search_n` 为每候选 rollout 预算（`0` 退回手写档），`train` / `advice` / `event` 独立开关。
- **作用面**：`train` 全候选搜索；`advice` 先按 `801` 去重并用手写价值矩阵剪枝至 top-16 后搜索；`event` 通过 `FlatSearch::search_with` 应用当前选项并沿 `event_queue` / `event_cursor` 续跑至终局（后续事件由手写基策选择）。
- **口径与预算**：`MatsuriGame::search_score(pt_favor_rate)` 为搜索目标；中选下标按 `weighted_mean(radical_factor)` 计算；`fork_for_rollout(seed)` 按 `(seed, turn)` 独立派生规则流，每次搜索恰好消耗决策流 1 次 `next_u64`；当前无截断 leaf hook，生产 `max_depth = 0`。

### 6.3 呈现口径

- `BoardPanel::compact()`（在线档）隐藏演目分配表、配色图例、可触发追加排练提示及起始说明；完整档与 `--explain`（`MATSURI_EXPLAIN=1`）输出全表与候选打分/理由，仅做只读预览、不耗随机流。
- **在线运气行投影**：`matsuri-ai::output::projected_summary` 将官方口径期望分 `E₀` 统一平移至显示分空间：`E = E₀ + (77 − t) × luck_turn_bonus`（`luck_turn_bonus` 取值见 `data/ai_config.toml` `[record]`），档位按投影后 `E` 查 `constants.get_rank_name(E)`；`render_luck` 与 JSON 仅消费该结果。
- **属性显示压缩**：`matsuri-cli` 呈现层对五维值与上限在 `>1200` 部分减半向下取整；所有计分与结算一律使用原始值。

### 6.4 幸运体质（生病盾）

- 引擎 `UmaFlags.lucky_count` 在本仓由 `traits.rs::apply_event`（`update_flags` 前拦截 `add_flags` 生病位）与 `train.rs::apply_training_failure`（拦截大失败及失败事件生病位）消费：命中时扣 1 层，原子清空 `bad_trainer` 与全部 `ill` 位（含 `Ill::FAT`）。

**实现落点**

| 机制点 | 数据字段 | 结算 / 呈现函数 | 测试落点 |
|---|---|---|---|
| 手写与搜索训练员 | `[decks.*.trainer]`、`[search]` | `trainer.rs::select_action` / `select_event_choice`、`value.rs`、`search.rs` | `trainer.rs` / `value.rs` / `search.rs` + `tests/` |
| 卡组配置解析与合并 | `data/matsuri_config.toml` `[decks.*]` | `deck_config.rs` | `deck_config.rs` |
| 在线投影与面板 | `data/ai_config.toml` `[record]` | `matsuri-ai/src/output.rs::projected_summary`、`matsuri-cli/src/panel.rs` | `matsuri-ai` / `matsuri-cli` 单测 |
| 幸运体质拦截 | `UmaFlags.lucky_count` | `traits.rs::apply_event`、`train.rs::apply_training_failure` | `tests/mechanics.rs` |

## 7. 随机轴与可复现

- **局面流 / 事件流**：由 `BaseGame` 持有，每回合按 `(master, turn)` 派生。
- **策略流**：本局面自持，供动作结算与剧本机制（含 `Begin` 追加排练预掷与训练结算）使用，在 `set_rule_master` 与回合边界重置。
- **回合固定流**：`streams.turn_fixed`，驱动每回合人头分布与灵感分配（与引擎 `distribute_all` 同轴）。
- **决策流**：仅供 `Trainer`（如 `FlatSearch` 每次取 1 个 `next_u64` 派生 rollout 种子），不影响结算随机。
- **`fork_for_rollout`**：克隆局面并注入 rollout 主种子，所有不由传入 RNG 驱动的流必须在此重置以保证逐位可复现。
