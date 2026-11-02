# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> **当前状态：URA 全变稽古已提交；手动面板显示第五轮 + 追加稽古 / 助力支撑显示与判定 + 合宿直前事件改挂
> `400015104` + 友人 / 理事长记者落位修正 + **基准种子 42 → 61444** 已提交
> （2026-11-05；**未 push**）**；
> 此前链：S8 View + diag → S7 手写训练员 →
> S6 事件管线 → S8 手动核对面板（先行），均已在本地 `main`。
> 下一步：**S8 其余**（在线外壳 `matsuri-ai` / 训练员选择器）→ **S9 基线** → 待量化（融合 A/B、
> 训练员旋钮扫参、M2 搜索）。
> 新窗口可直接按 §四 开工，不用再等确认；完成后向用户汇报 diff，**不自动 commit / push**。

## 一、git / 环境现状

- **本仓（umaai-matsuri）**：`HEAD` = 2026-11-05 显示 / 分布批次提交（哈希以 `git log -1` 为准）；
  **工作树干净**（该批次已提交：manual 显示第五轮 + 追加稽古 / 助力支撑显示与判定 + 合宿直前事件改挂
  `400015104` + 友人 / 理事长记者落位修正 + 基准种子改 `61444`；文档同步）。**未 push**
  （`origin/main = ea5f9ad`，6 个提交待推）。
  此前链：`8aa80ac`（训练Lv 守门）→ `5fea7d7`（URA 口径修正）→ `9327da3`（S8 View + diag）→
  `9cee735`（S7）→ `ea5f9ad`（S8 面板细化 + 教诲 T2 Begin）→ `ce6789b`（S8 升级 + T2 闸门）→
  `8f4687e`（S6 + S8 先行）→ `00079f3`（S4+S5）→ `9995329`（S3）等。
- 依赖：`matsuri-cli` 仍有 `inquire = 0.9.4`（终端动作 / 事件选项；非 TTY 退回编号输入）；
  **新增 `log = { version = "0.4.34", features = ["std"] }`**（workspace 依赖，与引擎同版本；
  `matsuri-core` 因 `diag!` 宏展开需要、`matsuri-cli` 装 stderr 日志器需要）。`Cargo.lock` 只加 `log`。
- **引擎仓 `../umaai_next`**：`HEAD = a8859e0`，**未改**（`git diff` 为空）。
- 数据：外部 `../../UmaAI/master.mdb`（md5 `ef481f95…`，不入仓）。**本轮未新增 mdb 表 / 资产键**；
  消费了引擎配置既有的 `[output] log_level`（默认 `info`）。
- 日常命令一律 `--offline`（`~/.cargo` 只读，`cargo add` / `fetch` 需先请用户提权）；
  从**本仓仓根**执行 cargo；同一 target 一次只跑一个 cargo。

## 二、已完成

### 手动面板显示第五轮 + 追加稽古 / 助力支撑（2026-11-05，已提交）

> 用户逐项拍板：属性压缩「计分按原始值、仅显示减半（值 + 上限）」；事件归属「改为 104 事件」；
> 槽入账日志「都加」；未兑现文案「未选择追加稽古训练」；黄色只在稽古列、彩圈优先；助力获取估值留扫参。
> 决策记录：`docs/plan/scenario_dev_plan.md` §3（2026-11-05 两行）、`docs/glossary.md` §9、`docs/mechanism.md` §6.3。

1. **教诲选择补名称**：manual 教诲分配提示「当前」行改 `流镝←101 牵绊 → 601 特化`（复用 `advice_name`）。
2. **追加稽古 / 助力支撑显示**：`ResourceView` 增 `kensan_act` / `kensan_rate` / `assist_act`
   （`view()` 从教诲槽派生，只读）；资源行标「追加稽古 掷中·神旗(22%)」「助力支撑 ×N（触发 大曳）」；
   动作候选可兑现时追加 `追稽✓ / 助力支撑✓`；训练表加配色图例；回合末仍 pending →
   `【追加稽古】T{} 掷中未兑现（未选择追加稽古训练）` 并作废。
3. **演目槽落点**：面板「变化」区新增 `【演目槽】演目 x→y`（清槽显示「清空」）；手写档
   `do_race` / `do_rest` / `do_normal_outing` / `do_friend_outing` 各出 `【演目槽】T{} {动作} 入槽 …`。
4. **合宿直前事件改挂 `400015104 駿大祭合宿の申し出`**（原 `400015105 教え見直し`；Fixed 11/35/59，
   体力+15/干劲+1 数值沿用 memo STUB；`400015105` 不再入运行时事件表）。归属仍待真机复核（wiki 线索
   1 年目 +10 / 2·3 年目 +15）。
5. **显示属性压缩**：五维值与上限显示 = 原始 ≤1200 取原值、>1200 部分减半取整（1600→1400、3179→2189）；
   计分 / 训练收益 / 变化量仍用原始值。
6. **事件名亮蓝**：事件回显 / 事件选项头 / 友人下一段 / 友人出行段统一 `ANSI_BRIGHT_BLUE`。
7. **训练表人物配色**：彩圈亮绿优先；非彩圈研鑽担当（含 NPC）亮紫；稽古列助力担当黄。
8. **判定逻辑修复**：`value.rs::kensan_bonus` 稽古分支改为按稽古训练位参与者判定（与 `try_kensan` 同口径；
   原实现只比较稽古下标）。助力支撑获取暂不单独估值（留扫参）。
9. **友人 / 理事长记者落位修正（user 2026-11-05 实测）**：①**理事长 / 记者不会出现在稽古训练位**——
   有稽古位时只落非稽古位（先判不在）；全稽古回合（合宿 / URA / 5 槽全满）本回合完全不出现；
   ②**吾妻只要有稽古训练就必补到某个未满 5 人的稽古位**——补位顺序改到锁位后、其余人头前（保证必定
   登场），不判不在、不去普通位；只有一个稽古位且未满 5 人时必定命中；无稽古回合走通用不在判定。
   实现 `practice_train_mask` / `place_guest_outside_practice` / `distribute_round` 顺序调整。
10. **测试**：四档绿 **178 测例**（123 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟）；
    新增 `view_exposes_kensan_and_assist_acts` / `kensan_bonus_uses_participants_for_keiko` /
    `kensan_event_presented_to_trainer_at_turn_end` / `yayoi_and_reporter_never_on_practice_positions` /
    `friend_fills_practice_position_when_any` / manual 显示压缩与配色单测；manual 冒烟加
    「骏大祭合宿の申し出 / 演目槽 / 教诲名称 / 配色图例」标记；manual 面板逐回合核实 45 个有稽古回合
    0 违规；整局冒烟 seed 42 **49587 / UB2、PT 8433**（分布修正后位移）。
11. **基准种子 42 → 61444（user 2026-11-05 拍板）**：`data/ai_config.toml` `[sim] seed = 61444`。
    动机：落位修正改变分布随机流消费序列，同 seed 的单局差可达 ±1 万（30 seed 配对均值差仅
    −285，t=−0.28 不显著），旧 seed 42 属不利样本；换参考 seed 仅影响 CLI 跑局数值，测试内部固定
    种子（`testing::sample_game` / `Repo::game` = 42）不受影响。新基准冒烟 **56210 / UA1、PT 9131**；
    `random` **23387 / UG8、PT 4693**（对齐前先看趋势，S9 前不建基线）。

### S8 View 补全 + diag 诊断（2026-11-02）

> 决策全部经用户拍板：方案 B（分节重构）一次定 schema；名称文本 / 动作候选 / 预览 / 建局配置
> **不进 view**；diag 含训练员决策、日志统一按 `output.log_level`（manual 不特判）、复用引擎宏不自造。
> 决策记录：`docs/plan/scenario_dev_plan.md` §3（2026-11-02 两行）、`docs/glossary.md` §9。

1. **`MatsuriView` = 回合信息完整快照**（新增 `crates/matsuri-core/src/game/view.rs`，779 行）：
   分节 9 顶层字段——`turn: TurnView`（turn/max_turn/stage/year/scenario_started + plan 全字段 +
   practice）、`resources: ResourceView`（演目 gauge/Lv/上限/act_trains、教诲槽/転成/acquired、
   発表会累计与结果、助力/追加稽古/訓練Lv加成）、`uma: UmaView`（五维+动态上限、体力/干劲、PT、
   灵感合计、比赛加成、flags、last_train）、`board: BoardView`（train_levels、`persons: Vec<PersonView>`、
   act_members、distribution（MemberView 含 shining、分身按占位））、`schedule: ScheduleView`
   （next_check_point 含 is_finals、goal_races、self_races）、`friend: FriendView`（out_state/out_used/
   next_step/next_event_id/fatigue_recovery）、`camp: CampView`（active/year/regular/extended/rest/
   level_up/training/counted/total）、`deck_warnings`、`recent_events`。`EventRecord` / `EVENT_LOG_CAPACITY` /
   `view()` / `push_event_log` 一并迁入；`PersonOrigin` 补 serde；`MatsuriYear` 新枚举。
2. **manual 面板改纯呈现**：所有状态从 `game.view()` 取，`game` 只剩 `preview_training` /
   `preview_action_resources`（只读预览）、`newgame_config`（建局信息）与动作选择入参；名称文本由 CLI
   查 `Data`（新增 `person_short_name` 复刻 `[NPC]` 规则）。`Snapshot` 也改为 view 派生。
3. **diag 接线**：`matsuri-core` 统一入口 `pub use umaai_core::diag`（模块内 `use crate::diag;` + `diag!`）；
   `game/diag.rs` 提供 `KeyState`（体力/干劲/五维/PT/演目槽/Lv）+ 变化摘要 + 回合类型摘要；
   `matsuri-cli/src/logging.rs` 极简 stderr 日志器，`main.rs` 在配置载入后 `init_logger(&cfg.output.log_level)`。
4. **埋点**（只读、不影响随机流）：回合 Begin/End、事件链分支、事件结算变化 / 状态增删 / 羁绊归属、
   疲劳回复 / 变胖解除、动作（训练 / 稽古 / 比赛 / 休息 / 出行 / 友人出行 / 治病）、训练后处理、
   演目槽、助力支撑、追加稽古、分布（人数 / 锁定 / 吾妻补位 / 分身）、教诲（首次 / 融合 / 分配 / 硬约束 / 掷中）、
   発表会 / 一次性奖励 / 合宿 +1 / 终局、**训练员决策**（动作 / 事件 / 教诲分配各一行得分）。
   **预览函数内 `DiagGuard::suppress()`**（不把「如果选这个」的模拟混进日志）；MCTS rollout 由引擎
   `FlatSearch` 的 `DiagGuard` 自动静默。
5. **测试**：**166 测例**（core 内联 114 / L2 15 / 机制 30 / diag 1 / CLI 内联 5 / CLI 冒烟 1），四档全绿。
   新增：`view.rs` 6 例（镜像 / 幂等只读 / 赛程派生 / 年次边界 / 事件记录滚动 / serde 往返）、
   `mechanics.rs` 的 `view_fields_match_game_at_train_decision`（78 个 Train 决策点逐项核对）、
   `tests/diag.rs`（捕获日志器：target/埋点计数/抽样 + 静默零产出 + 同种子终局局面逐位一致）。
6. **冒烟**：`cargo run --release --offline --bin matsuri -- matsuri` = **54683 / UB9、PT 9024**
   （与 S7 逐位一致，未位移）；`random` 档正常；整局诊断约 1.6k 行，覆盖层 `log_level="off"` 实测
   0 行输出。`manual` 管道跑满整局通过（面板内容不变 + stderr 诊断）。
7. **文档**：README 状态 + 结构表；开发计划 S8 ☑ 进展 + 2 条决策；glossary §6 术语 + §9 决策；
   tests_overview 分层计数 / 用例 / 诊断守门新节 / 对话记录；AGENTS 结构表 + diag 纪律；handoff（本文件）；
   crate 文档注释同步。

### URA 阶段口径修正（2026-11-03，user 逐项核对）

1. **URA 训练限定效果（本番演出）只在 72–77 生效**：`train.rs::ura_training_active` 与
   `distribute.rs::place_splits` 的 finals 判定改 `URA_START_TURN..=MAX_TURN`（`stage::URA_START_TURN = 72`）。
   68–71 空窗期不生效（体力 / 干劲 / 失败率 / 分身 +1 均不享受）。
2. **68–71 = 剧本结束到 URA 的空窗期，不属于 URA**：`TurnPlan.ura_period` 改 72–77；新增
   `TurnPlan.interlude`（由 `data.schedule` 末项 +1 派生到 URA 起点前）与 `MatsuriView.turn.interlude`；
   手动面板 / 诊断日志回合类型标「空窗期」。
3. **体力结算顺序**：先稽古耗体下降 / 训练扣体 / 失败率下降，再判定回体（+30）→ URA 训练常为**净回体**
   （例 -20+30 = +10）；`ura_training_effects` 测试覆盖 68/70/71 不生效与 72/74/77 生效。
4. **通用规则：演目 Lv 满（10）后仍可攒 5 槽并排练**，排练成功后槽清空、Lv 保持 10；新增内联测试
   `max_level_act_still_accumulates_and_practices`。
5. **URA 期全变演目稽古（2026-11-04 用户更正，类似合宿）**：`plan.ura_period`（72–77）时 `all_keiko_round()`
   为真、`practice_acts` 全真；动作集 = 5 个 `ActTrain` + 休息 + 普通出行 + 治病（72/74/76），73/75/77 只有比赛；
   不可自选赛 / 友人出行。**URA 不是合宿**：稽古成功仍按日常结算 Lv+1 / 清槽；`stubs.md` 的「存疑」行已改 RESOLVED。
6. **URA 事件无 Hint（无技能，2026-11-04 更正）**：「URA 期事件 hint 需要记入」作废。`KeyState` / manual 的
   「灵感±N」保留为**通用事件**口径（事件选项 `hint_level` → `total_hints`）；测试改名 `event_hint_level_is_counted`。
7. **72–77 动作限制确认**：不可友人出行 / 自选比赛；72/74/76 可训练 / 稽古 / 休息 / 普通出行 / 治病，
   73/75/77 只有比赛（引擎规则已有，`action_set_by_turn_type` 补守门）。
8. 测试 **171 测例**（core 内联 119 / L2 15 / 机制 30 / diag 1 / CLI 内联 5 / 冒烟 1；含训练Lv 计数叠加
   守门 `train_level_counts_successful_normal_training_and_stacks_bonus`、URA 全变稽古
   `ura_round_all_keiko`）；整局冒烟 54683 / UB9、PT 9024 → 53083 / UB7、PT 8858 →
   **55664 / UA、PT 9136**（URA 全变稽古后位移；S9 前不建基线）。

### S7 手写训练员（2026-11-02，已提交）

> 目标函数 `pt_favor`（官方评分 + PT×`pt_favor_rate`，默认 2.0）、先手写后搜索、教诲分配 T2 + 5 窗口
> 合并 120 候选全部走 `Trainer`、融合仍走配置预设。落点 `core/src/{trainer,value}.rs` +
> `MatsuriConfig [trainer]` 旋钮。决策确定性、不耗决策随机。详见 `docs/plan/scenario_dev_plan.md` §3。

### S8 先行：手动核对面板（2026-11-01，已提交）

- `manual.rs` 核对面板：育成配置 / 数值状态 / 剧本资源 + 彩色训练明细表（人物着色 / Hint / 羁绊）/
  合宿 · 友人链 · 赛程 / 教诲分配结果 / 动作预览（附槽增量）/ Inquire 动作与事件选择 / 事件回显 / 变化。
- 核心 API：`EventRecord` + `recent_events`、`preview_training`、`preview_action_resources` /
  `ResourcePreview`（克隆局面、只读、不耗随机流；现在预览内部还会静默诊断日志）。
- 剧本机制自 turn 2 起；教诲首次授予在 T2 Begin，效果对 T2 分布 / 训练生效。
- **S8 其余未做**：在线外壳 `matsuri-ai`、训练员选择器完整化（View 字段补全本轮已完成）。

### S6 事件管线（2026-11-01，已提交）

- `game/event.rs`：回合开始事件链（固定 → 合宿抑制 → 友人出行解锁 → 随机分布）；正月 / 结局 / 抽签入队；
  变胖解除 20%（STUB）；追加稽古事件化；turn 66 进化 hint 记账。
- 数据：`scenario_events`（`400015104` / `400015106`）+ `event_extras`；吾妻 `809052xxx` 事件链；
  构造事件「大胃王」`5010`（33% 变胖）。
- 详见 `docs/mechanism.md` §5.3–5.4 与 §3 / `docs/glossary.md` §9 2026-11-01 行。

## 三、当前代码现状

- 阶段机不变：`Begin → Distribute → Train → AfterTrain → End`（`StageId` 0/3/1/4/2 只增不改）；
  剧本机制自 turn 2 起；教诲首次授予在 T2 Begin。
- **决策面全部走 Trainer**：动作（`select_action`）/ 事件选项（`select_event_choice`）/ 教诲分配
  （`decide_advice_assignment`，候选 = 120 `Action::AssignAdvice`）。手写训练员确定性、不耗决策随机。
- **回合信息单一取数口**：`MatsuriGame::view() -> MatsuriView`（分节见 §二）。宿主 / 面板 / 前端
  **不得直读局面字段**；名称文本按 `Data` 静态表解析，动作候选与预览走训练员入参 / `preview_*`，
  建局配置不属于回合信息。`MatsuriView` 为 serde 值快照 + `non_exhaustive`。
- **诊断日志**：统一 `crate::diag!`（引擎宏，target = `diagnostic`；编译期 feature + 运行时开关 +
  `DiagGuard` 静默）；日志器在 `matsuri-cli`，级别 = `[output] log_level`（默认 `info`，`off` 静默）。
- **URA 边界**：`URA_START_TURN = 72`；`plan.ura_period` = 72–77、`plan.interlude` = 68–71（剧本结束后的
  空窗期）；**URA 期全变稽古（`all_keiko_round()` / `practice_acts` 全真）**，稽古成功仍按日常 Lv+1 / 清槽；
  本番演出训练效果只在 72–77 生效；URA 事件无 Hint；`KeyState` 含 `total_hints`（通用事件 hint 可观测）。
- 关键接口：`MatsuriTrainer::new(MatsuriConfig)` / `config()`；估值 `Model::new(game, &TrainerConfig)`
  （私有）；`assignment_candidates` / `apply_assignment`（401+801 硬约束）、`advice_effect_of`、
  `view()`、`kensan_act` / `roll_kensan`、`card_event_candidates`、`preview_training` /
  `preview_action_resources`、`MatsuriView.recent_events`。
- 分布口径（2026-11-05）：亮起位置锁担当；**理事长 / 记者只落非稽古位（全稽古回合不出现）**；
  **吾妻只要有稽古训练就必补某个未满 5 人的稽古位**（锁位后、其余人头前补位；不判不在、不去普通位；
  唯一稽古位必中；无稽古回合走通用不在判定）。实现 `distribute.rs`（`practice_train_mask` /
  `place_guest_outside_practice`）。
- 数据资产：`scenario_matsuri.json` 18 个顶层键（含 `scenario_events` / `event_extras`）；
  `data/events.json` 为剧本自持事件根（吾妻链 + 大胃王）。
- **尚未接线**：S8 其余（在线外壳 / 选择器）；`FlatSearchGame` 搜索档（M2；当前 `RolloutTrainer`
  仍 `RandomTrainer`，M2 时换取手写训练员）；基线（S9）；马娘特殊事件记录（引擎改造后迁移 T65）；
  事件⑤（TBD）；技能系统 / 粉丝不建模。

## 四、下一步

> S8 其余 → S9 基线 → 待量化 → M2 搜索。先手写档锁基线，再接搜索档（单独基线，不可混比）。

1. **S8 其余**：在线外壳 `matsuri-ai`（帧段解析 / Session 接线 / 决策循环，依赖真机帧；
   渲染侧直接消费 `MatsuriView`）；训练员选择器完整化。
2. **S9 基线**：一基线一 JSON（`input` 自持、`expect` 容差、`meta` 不含路径）；数值位移后重录。
   注意：**`random` 档口径已变**（分配 + 事件也走随机，19516 → 29396），旧基线不可直接复用；
   基线 / 批量入口建议 `DiagGuard::suppress()` 或覆盖层 `log_level="off"`（避免日志开销与污染）。
3. **待量化（S9 前只记录，不建基线）**：
   - 融合配对 3 套 A/B（`scenario_dev_plan.md` #46：现行 / 攻略 / 混合；用覆盖层 × N 种子配对）；
   - `[trainer]` 策略旋钮扫参（`stubs.md` 已登记 STUB；pt_favor 已在 2.0 档定档，其余权重 / 阈值待扫）。
4. **M2 搜索（按需）**：`FlatSearchGame` 适配 + `MctsTrainer`（引擎无现成实现，需剧本仓自建；
   见引擎 `docs/scenario/dev_process.md` §3.1）；根节点可用教诲分配 top-K。

## 五、备忘与未决

- **数值位移**：S5 34368 → S6 44946 →（T2 闸门 + 教诲 T2 Begin）40853 → 40899 / UC、PT 6982 →
  S7 手写训练员 54683 / UB9、PT 9024（S8 View+diag 后逐位不变）→ URA 口径修正 53083 / UB7、PT 8858 →
  URA 全变稽古 55664 / UA、PT 9136（2026-11-05 显示批次 A/B 逐位不变）→ 友人 / 理事长记者落位修正
  49587 / UB2、PT 8433（seed 42）→ **基准种子 61444：56210 / UA1、PT 9131**；`random` 19516 → 29396 →
  29527 / UE1、PT 5552 → 29537 / UE1、PT 5577（seed 42）→ **23387 / UG8、PT 4693**（seed 61444）。
  S9 前不建基线；一有数值占位替换 / 训练员改动就重录并记影响面。
- **View 口径**：`MatsuriView` 结构变更属宿主契约变更（serde 结构变 = 旧 JSON 反序列化失败）；
  无持久化 consumer 前可改，S9 / 在线外壳接入后需按 `non_exhaustive` + 加字段纪律演进。
- **待真机复核**（`stubs.md` 已登记）：变胖解除 20%、事件⑤（SSR 技能事件未接）、事件抑制基线层、
  友人事件槽位映射、memo 友人事件值为「基础值」的推断、**`400015104` 触发时机与数值（合宿直前 +15/+1
  归属：wiki 线索 1 年目 +10 / 2·3 年目 +15）**、通用事件数值、技能灵感 `hint_level` 记账、
  turn 66 进化 hint+1、T65 之外马娘特殊事件、训练员旋钮默认值。
- **URA 待确认**：URA 期**全变稽古已确认（2026-11-04）**；但「URA 稽古成功是否也按合宿冻结 Lv / 槽」
  未逐字确认——当前按**日常稽古**（Lv+1 / 清槽，不合宿冻结）；如要冻结只改 `settle_training` 一处。
  URA 事件**无 Hint**（无技能），此前「hint 需要记入」作废。
- **不做**：技能系统 / 粉丝、进化技能效果、R / 无凸吾妻分支、团队卡、引擎改造（除非用户另行确认）。
- 参考源纪律：`../matsuri-ref-01` 不可作为机制 / 数值 / 实现依据；数据只用 `master.mdb` 原始表 +
  本仓 `docs/` + 日文 memo（`../mee-umasim/data/shuntaisai_memo.md`）。

## 六、关键命令 / 约束

```bash
# cwd = 本仓仓根；同一 target 一次只跑一个 cargo；一律 --offline
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
cargo run --release --offline --bin matsuri -- matsuri   # 整局冒烟（stderr 出诊断日志）
cargo run --release --offline --bin matsuri -- random    # 引擎随机基线（对照用）
cargo run --release --offline --bin matsuri -- manual    # 手动核对面板（交互式，q 退出）
```

- 测试 cwd 在各 crate 目录；数据根统一经 `matsuri_core::testing::data_dir()` 取仓根
  （内联测试可用 `testing::sample_game()`）。
- 诊断日志级别：`data/ai_config.toml [output] log_level`（默认 `info`）；工作目录放同名覆盖层即可覆盖
  （`off` 静默）。埋点统一 `crate::diag!`，预览内部自动静默，勿直读局面字段渲染。
- **不改引擎仓**；需要通用能力时先与用户确认、走引擎仓流程。
- **提交纪律**：不自动 commit / push；提交前统一更新 README 与剧本仓文档（AGENTS §文档规则），
  由用户确认。手写训练员基线未锁（S9），此前冒烟数值只作趋势参考。
