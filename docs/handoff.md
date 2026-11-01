# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> **当前状态：S4 动作与结算 + S5 机制闭环已提交**（2026-11-01，单提交；hash 见 `git log -1`）；
> 下一步 **S6 事件管线**。
> 新窗口可直接按 §四 开工，不用再等确认；完成后向用户汇报 diff，**不自动 commit / push**。

## 一、git / 环境现状

- **本仓（umaai-matsuri）**：S4 动作与结算 + S5 机制 + 文档 / 数据已**提交**（2026-11-01，单提交，
  hash 见 `git log -1`）；工作树干净、**未 push**。
  上一个提交 = `9995329 feat(core,cli,data): S3 阶段机 + S4 前置（人头 / 担当 / 编成检测）`。
- **引擎仓 `../umaai_next`**：`HEAD = a8859e0 feat(engine,sim): 事件额外属性效果与比赛奖励真机口径落地`，
  **已提交但未 push**；本仓以 path 依赖引用。**本阶段不改引擎**（S5 验证过 `git diff` 为空）；
  「马娘特殊事件记录」数据结构改造是后续独立事项（改造后回本仓迁移 T65 临时表）。
- 数据：外部 `../../UmaAI/master.mdb`（工作区外，md5 `ef481f95…`，不入仓）；`data/` 里 cardDB / umaDB /
  text_data_dict 已更新（新卡池含 SSR 吾妻 30323）。当日资产映射见 `docs/plan/intake.md` §4.1。
- 日常命令一律 `--offline`（`~/.cargo` 只读，`cargo add` / `fetch` 需先请用户提权）；
  从**本仓仓根**执行 cargo；同一 target 一次只跑一个 cargo。

## 二、已完成（S4 + S5 收口，2026-11-01）

### S4 动作与结算

1. `game/action.rs`：`Action` 全集（`Train / ActTrain / Race / Rest / NormalOuting / FriendOuting / Clinic`）
   + `Display` + `ActionEnum::apply` 分派；`list_actions` 按回合类型分叉（强制比赛 → `[Race]`，
   强制休息 → `[Rest]`，合宿 → 5 `ActTrain` + 休息 + 第 1 年出行 + 可自选比赛，日常 / URA → 槽满
   `ActTrain` / 槽未满 `Train` + 休息 + 普通出行 + 友人出行 + 自选比赛 + 治病）。
2. 比赛：`push_race_event(scale, 1.0)`，`scale = (100 + uma.race_bonus + 発表会 race_bonus) / 100`
   （204 与卡面「赛后」**同表加算**，2026-11-01 拍板；floor 在引擎）。
3. 训练 / 稽古：S4 时只下层公式 + 失败判定；S5 起为两层（见下）。
4. 休息（普通 / 合宿版引擎默认）、普通出行、吾妻出行 5 段（`outing` 资产 + `matsuri_outing_1..5`）、
   治病；`AfterTrain` 统一排空 `unresolved_events`，`apply_event` 补事件羁绊归属。
5. `data/events.json` 已同步引擎新基础：目标赛 5/72 覆写、URA 决赛三事件、非目标赛随机 1 项。

### S5 机制（2026-11-01，用户 4 点拍板后落地）

6. **人头分布（`game/distribute.rs`）**：亮起（槽满）演目担当卡 + 担当神驹锁位、不判不在；
   普通卡按类型 + 得意率权重、神驹等权且 **`absent_weight = 50`（STUB U29）**；每位置 ≤5、同位置不重复、
   至多 1 友人；吾妻普通训练按通用规则、**排练必补 1 个未满排练位**（不可分身）；分身三源
   （協力 801 / 転成 101 合宿 / URA 本番演出）落另一训练位、不叠排练位；得意率 = 卡面 + 发布会
   deyilv（基线 20）+ 教え / 転成；灵感分配走剧本公式（基础 7.5% × (1 + (卡面 + 发布会 + 教え)/100)；
   排练 + 5 种卡时编成卡全亮图标，演目 Lv≥4 时全灵感发动）。
7. **gauge（`game/mod.rs` + `train.rs` + `action.rs`）**：训练参与按担当归集 +1 / 闪彩 +2；
   目标赛 +4（4 次 +1，三则落点：演目 Lv 最低 → 槽最低 → 属性序，满槽跳过、并列逐次分摊）；
   休息 / 出行 / 自选赛 +1；稽古成功清槽 + Lv+1（**合宿稽古不升不清**，合宿结束全演目 +1）；
   排练回合 5 槽不涨。
8. **稽古两档 + 演目 Lv 效果**：恢复 `training.keiko.classic`（turn 24 起切档，turn 23 発表会
   「演目稽古强化」成功 / 大成功都切）；演目 Lv 效果全列接入（全基础 +10 / 友情 / SP 排练 /
   灵感 / SP·基础上限，5 种卡条件列在稽古时生效）。
9. **教诲（`game/advice.rs`）**：10 种效果 × **绑定演目 Lv** × **参与担当人数**叠加；絆 +3 平摊；
   助力支撑（3 次内 +5%，再次获得重置）；研鑽追加稽古（Begin 策略流预掷、成功训练触发、50/50 上/下档
   STUB）；不屈（体力阈值）、両道（副获得）、特化（主获得 + 上限）、協力（友情 + 分身）；
   生命周期：turn 2 获授、窗口 2/11/23/35/47/59、turn 23/47 転成（`MatsuriConfig.fusion` 预设，
   就地融合）、**120 全排列 + `apply_assignment` 接口（用户拍板：S5 只落接口、不自动择优，策略 S7）**、
   401+801 硬约束到担当数最多的演目。
10. **训练两层公式（`game/train.rs`）**：下层 = `default_calc_training_value`；上层 = 剧本侧合计
    按主 / 副 / SP 三式乘算后 floor；**子项经过的上限 = 规则 100 + 特化 status_limit（按参与聚合）+
    稽古且 5 种卡时该演目 Lv9/10 status_limit；SP 上限同法 + sp_limit**（2026-11-01 拍板）；
    稽古「全基础 +10」= 上层之后**全五维 flat 加**（2026-11-01 拍板）；失败率 = 引擎公式 + 不屈 /
    本番演出 -100%；体力 = 稽古体力教え / 転成降耗 + URA 训练 +30 体 / +1 干劲。
11. **训练后处理（剧本口径）**：羁绊基础值 friend +4 / card +7 / 理事长·记者 +5、絆教え全编组卡 +3
    平摊、爱娇统一 +2 一次；神驹只计人头不给羁绊；灵感 / 加练 / 理事长·记者点击保留；
    **友人点击事件不推**（真实吾妻事件留 S6）。
12. **回合末结算（`game/settle.rs`）**：発表会 / 本番**按实际演目 Lv 判定**（5 演目 ≥3/5/6 → 大成功，
    否则成功；成功档数值真实生效）→ 永久 buff（取最新已结算累计行；开局基线 deyilv +20）→ 一次性奖励
    （属性 / SP / 体力 / 干劲 / 理事长羁绊 / 訓練 Lv+1 用独立 `train_level_bonus`）；
    合宿结束全演目 +1；教诲窗口与融合；本番结算置 `finals_done`（URA 68–77 训练效果生效）；
    育成终了按「三场全大成功 / 否则成功」发 `career_end_rewards`（`on_simulation_end`）。
13. **数据 / 配置**：`training.keiko.classic` 恢复；`rules` 扩 `training_upper_cap=100` /
    `assist_support{count:3,effect:5}` / `kensan{上/下档数值 + upper_prob:50 STUB}`；
    `MatsuriGame` 持有 `MatsuriConfig`（`with_config`，CLI 注入），`newgame` 默认层兜底。
14. **View**：补 `presentation_friendship / presentation_deyilv / presentation_hint_rate /
    presentation_race_bonus / assist_support_left / kensan_pending / train_level_bonus`。
15. **测试**：**81 内联 + 13 L2 + 25 机制 = 119 测例**；四档全绿；
    冒烟（样例训练员）**34368 分 / UE9、PT 6128、390 阶段**；random 基线 19516 / SS+、PT 3985。
    新增文件自带内联机制测试；`tests/mechanics.rs` 有 S5 链路用例（分布入槽 / 目标赛 +4 / 整局窗口链）。

## 三、当前代码现状（S6 起点）

- `game/stage.rs`：五阶段 `Begin(0) → Distribute(3) → Train(1) → AfterTrain(4) → End(2)`
  （`StageId` 只增不改）；`TurnPlan` / `CampState`；`stage_body` 现状：
  - `Begin`：`derive_plan` + `forced_rest`（临时表）+ `roll_kensan`（策略流预掷）；
  - `Distribute`：`distribute_all` + `distribute_hint`（回合固定流）；
  - `Train`：`list_actions` + 训练员选择 + `apply_action`（策略流）；
  - `AfterTrain`：排空 `unresolved_events`（`run_event`，事件流）；
  - `End`：合宿计数 + `settle_turn_end`（発表会 / 本番 / 合宿 +1 / 教诲窗口）。
- `game/mod.rs`：`MatsuriGame` 字段（`base` / `persons` / `stage` / `strategy` / `data` / 训练表两档 /
  `acts` / `advice` / `camp` / `plan` / `forced_rest` / `check_points` / `finals_done` /
  `deck_warnings` / `assist_support_left` / `kensan_pending` / `train_level_bonus` / `config`）；
  helper：`presentation_bonus` / `judge_presentation` / `keiko_is_classic` / `keiko_basic_table` /
  `gain_lowest_act_gauge` / `gain_act_gauge` / `add_act_level` / `act_effect` /
  `deck_has_five_types` / `scenario_deyilv` / `scenario_race_bonus` / `practice_acts` / `lowest_act`。
- 新增文件：`game/distribute.rs`（分布 / 得意率 / 灵感 / 分身）、`game/train.rs`（训练结算 / 上层 /
  失败率 / 训练后处理 / 追加稽古）、`game/advice.rs`（教诲效果 / 融合 / 分配接口）、
  `game/settle.rs`（回合末结算 / 终局）。
- `game/traits.rs` 契约覆写：`training_basic_table`（两档切表）、`absent_weight` / `deyilv` /
  `distribute_all` / `distribute_hint`、`train_level`（含 bonus）、`calc_training_value`（两层）、
  `calc_training_failure_rate`（不屈 / 本番）、`on_simulation_end`（育成终了）。
- `game/action.rs`：动作全集 + 比赛 / 休息 / 出行 / 治病入口与演目槽入账。
- `trainer.rs`：样例训练员（轮转 + 休息阈值 40 + 治病；S7 换）；`cli/manual.rs` 手动训练员。
- **尚未接线**（S6+）：事件链（固定回合 / 概率事件 / 伝受 / 合宿抑制表 / turn 66 进化 /
  友人真实事件与出行卡面事件）、马娘特殊事件记录（T65 临时表迁移）、教诲策略 / 搜索（S7）、基线（S9）。

## 四、下一步 = S6 事件管线

目标：把「事件」从动作 / 训练后事件扩展到**回合开始事件链**，并接回剧本时间轴与友人事件。

建议顺序：
1. **事件数据与来源**：`Data::get()?.events`（主剧本 `400015xxx`）/ 友人（`809052xxx`）/ 支援卡事件 /
   `data/events.json` 里的通用事件。先核对事件 → 数值 / 选项 / 羁绊归属的目标结构（引擎 `EventData`），
   把剧本专属事件需要的**数值型效果**（属性 / SP / 体力 / 干劲 / 羁绊 / 演目 Lv / gauge 事件入账）
   走 `apply_event` 扩展点；事件给的 gauge / 演目 Lv 要**受上限拦截**（`is_overflow_act_gauge_event_gain`）。
2. **回合开始事件链（`Begin`）**：固定回合事件 + 概率事件（事件流）+ 合宿抑制表（剧本自持，
   不复用引擎 `no_event_turns`；第 1 年合宿两类事件都发生，第 2/3 年常规合宿回合都不发生，
   延长段个体固定事件仍发生）+ turn 66 进化事件；事件可能把目标赛回合改成休息（T65 一类），
   落地方式是「回合开始先跑事件，再决定实际动作」。
3. **传说中的教え / 友人事件**：turn 2 吾妻登场事件（真实「首次 / 点击」事件，替换 S5 的
   “不推事件”占位）；吾妻评价提升 ×4、合宿申请、教え见直、追加稽古、稽古准备、特别延长、
   お大事に、成长提示等（`400015100–400015110`）；出行 5 段真实卡面事件（`809052xxx`）替换
   `matsuri_outing_1..5`（S5 的 memo 数值占位，见 `stubs.md`）。
4. **马娘特殊事件记录**：若用户确认引擎侧改造，改为读引擎数据；否则保留 T65 临时表并登记。
5. **测试**：固定回合 / 概率事件 / 抑制表（按年次 + 延长段）/ turn 66 / 友人事件 / 事件数值
   上限拦截；更新 `tests/mechanics.rs` 与 L2（事件数据守门）；四档绿 + 冒烟。
6. 注意：S5 已把「训练后事件」（灵感 / 加练 / 点击）改为剧本口径；S6 只接回合开始链与友人事件，
   不要把两条链混在一个函数里。

参照：`docs/mechanism.md` §5（事件 / 発表会）、`docs/matsuri_memo_cn.md` 时间轴章节、
`docs/plan/intake.md` §3.12 / §5、`../umaai_next/docs/scenario/contracts.md`。

## 五、S7+ 备忘与未决

- **S7 训练员 / 搜索**：教诲分配价值矩阵 + top-K（S5 只落接口，`assignment_candidates` / `apply_assignment`
  已就绪）；训练员守门链与动作优先顺序；是否接 MCTS / 预算 / 配置段；`FlatSearchGame` 已接
  `RandomTrainer` rollout。
- **S9 基线**：一基线一 JSON；数值占位替换后必须重录并在 `glossary.md` 记影响面。
- 待真机复核（stubs 已登记）：发布会成功档数值、上层上昇量上限口径、追加稽古上/下档 50/50 与
  解 debuff 简化、NPC 不在率 50、技能灵感 / 粉丝不建模、Senior 各演目 +1 担当、U41 / U44、
  发布会影响率叠加方式、第 2 批神驹人選 / 随机非 link 取人简化。
- 参考源纪律：`../matsuri-ref-01` 不可作为机制 / 数值 / 实现依据；数据只用 `master.mdb` 原始表 +
  本仓 `docs/` + 日文 memo（`../mee-umasim/data/shuntaisai_memo.md`）。
- 数值位移：S4/S5 接入后冒烟从 7690 / B → 34368 / UE9（分布 + 稽古 + 教诲 + 发布会放大），
  S9 前不建基线。

## 六、关键命令 / 约束

```bash
# cwd = 本仓仓根；同一 target 一次只跑一个 cargo；一律 --offline
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
cargo run --release --offline --bin matsuri -- matsuri   # 整局冒烟
```

- 测试 cwd 在各 crate 目录；数据根统一经 `matsuri_core::testing::data_dir()` 取仓根
  （内联测试可用 `testing::sample_game()`）。
- **不改引擎仓**；需要通用能力时先与用户确认、走引擎仓流程。
- **提交纪律**：不自动 commit / push；提交前统一更新 README 与剧本仓文档（AGENTS §文档规则），
  由用户确认。
