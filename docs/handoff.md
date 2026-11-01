# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> **当前状态：S3 局面与阶段机 + S4 前置（剧本人头 / 名单 / 担当分配 / 编成检测）已提交**
> （2026-11-01，单提交；hash 见 `git log -1`）；下一步 **S4 动作与结算**。
> 新窗口可直接按 §四 开工，不用再等确认；完成后向用户汇报 diff，**不自动 commit / push**。

## 一、git / 环境现状

- **本仓（umaai-matsuri）**：S3 阶段机 + S4 前置（人头 / 名单 / 担当分配 / 编成检测）+ 参考卡组 /
  数据 / 文档已**提交**（2026-11-01，单提交，hash 见 `git log -1`）；工作树干净、**未 push**。
  上一个提交 = `98967e4 feat(core,data): S2 数据层落地并接入引擎新基础`。
- **引擎仓 `../umaai_next`**：`HEAD = a8859e0 feat(engine,sim): 事件额外属性效果与比赛奖励真机口径落地`，
  **已提交但未 push**；本仓以 path 依赖引用。**本阶段不改引擎**；「马娘特殊事件记录」数据结构
  引擎层改造是后续独立事项（改造后再回本仓迁移 T65 临时表）。
- 数据：外部 `../../UmaAI/master.mdb`（工作区外，md5 `ef481f95…`，不入仓）；`data/` 里的
  cardDB / umaDB / text_data_dict 已由用户更新（新卡池含 SSR 吾妻 30323），引擎静态数据已拷入 `data/`。
  资产 ↔ mdb 映射见 `docs/plan/intake.md` §4.1。
- 日常命令一律 `--offline`（`~/.cargo` 只读，`cargo add` / `fetch` 需先请用户提权）；
  从**本仓仓根**执行 cargo；同一 target 一次只跑一个 cargo。

## 二、已完成（S3 收口，2026-11-01）

1. **阶段机（`game/stage.rs`）**：五阶段 `Begin(0) → Distribute(3) → Train(1) → AfterTrain(4) → End(2)`；
   `Stage` 无数据 + `Copy`，`StageId` **只增不改**。比赛 / 合宿 / 発表会 / 本番 / URA 收尾是**回合类型**，
   不各开阶段；`Race / ActTrain / Outing` 是 `Train` 阶段的动作（S4），`CheckPoint` / 合宿 +1 /
   教诲分配都在 `End`。
2. **回合结构 `TurnPlan`**（每回合 `Begin` 派生）：`race`（目标赛 / URA 决赛分类）、`ura_race`、
   `ura_period`（≥68，由 `schedule` 末项 +1 派生）、`camp_year / camp_zone / camp_regular /
   camp_extended / camp_rest`、`camp_level_up`（15/39/63）、`check_point`（23/47/67 → 0/1/2）、
   `advice_window`（2/11/23/35/47/59）。
3. **合宿计数 `CampState`**：进入 `window.start` 开始，每回合 `End` 计数，**目标赛 / 强制休息不计入**，
   计满窗口长度（=4）结束 → 顺延自动成立；`camp_rest` = 原窗口内 && 年次 ≥ 1（Junior / 延长段为
   普通休息）；**全演目 +1 仍固定 turn 15/39/63**（`plan.camp_level_up` 由 S5 应用）。
4. **马娘特殊事件（强制休息）**：凯斯奇迹 T65 在 umaDB 仍是目标赛回合（`plan.race = true`），实机由
   固定事件改为**实际执行休息**（退赛，不产生比赛事件 / 奖励）；S3 用临时常量表
   `FORCED_REST_TURNS: &[(chara_id, turn)] = &[(1093, 65)]` 置 `forced_rest`，等引擎
   「马娘特殊事件记录」数据结构落地后改读引擎数据（并删除临时表）。
5. **资源账本 + View（`game/mod.rs`）**：`ActLedger`（gauge 全 0、演目 Lv 从 1 起）、
   `AdviceLedger`（5 槽 `slots` + 転成 `fused`）、`check_points[3]`、`finals_done`；
   `MatsuriView` 全量快照（演目 / 教诲 / 合宿 / 発表会 / 本番 / URA + 回合类型
   `race / finals / check_point / advice_window / forced_rest`）。
6. **建局门禁**：仅 **SSR 吾妻**（`card_id 30323` / idrank `303234`）可建局；
   R 吾妻 / 不带友人 / 其他友人卡一律报错；`act_level_cap()` = 带友人档 10。
7. **参考卡组**（`data/ai_config.toml`，全部满凸 rank4）：
   速·待兼诗歌剧 `303174` / 速·乐透心 `303114` / 耐·樱花千代王 `303004` /
   力·摩耶重炮 `303224` / 智·爱慕律动 `303204` / 友·SSR 吾妻 `303234`。
8. **测试**：`game/stage.rs` 内联 12 例（阶段链 / 合宿窗口 / 比赛 / 强制休息顺延 / 発表会 / 教诲窗口 /
   URA 判定）+ `tests/mechanics.rs` 5 例（78 回合回合结构、合宿顺延、T65 强制休息、友人门禁、View 往返）。
   四档绿：**34 内联 + 11 L2 + 5 机制 = 50 测例**；`cargo run --bin matsuri -- matsuri` 整局冒烟通过（390 阶段）。
9. **口径确认（用户 2026-11-01，均已回写 docs）**：
   - 合宿回合 5 个训练动作**全部可稽古**（不受 gauge 影响）；**不改变演目 Lv、不进演目槽**；
     Lv 只在该年固定结算点 +1。
   - **URA 收尾期（68–77）不做特殊化**：本番结算已完成，训练 / gauge / 稽古 / Lv 升级回到日常机制；
     专属收益只来自已结算的本番演出。（旧记「每回合全变稽古、所选训练对应演目 +1」已作废。）
   - **turn 67（本番）是正常回合**，本番结算在 `End`。
   - **所有结算时机在 `End`**：発表会 / 本番 buff + 一次性奖励、合宿 +1 先结算；**教诲分配 / 调整
     也在 `End`、剧本结算之后**（含 turn 23/47 転成）。不另开 `Advice` 阶段。
   - 参考卡组 + 全满凸；只有 SSR 吾妻可用。

10. **S4 前置：剧本人头与名单（2026-11-01）**：
    - `game/person.rs`：`MatsuriPerson` = 引擎 `BasePerson`（委托）+ 剧本字段
      `act: Option<usize>` / `origin: PersonOrigin` / `avail_turn` / `needs_great_success`；
      契约 `Person` impl 整体在 `traits.rs`。
    - `PersonOrigin`（剧本标签）与 `PersonType`（引擎契约语义）**分工**：构造时单向决定
      （Card→Card / Friend→ScenarioCard / Guest→Npc / **Yayoi→Yayoi** / Reporter→Reporter）；
      引擎只看 `PersonType`、剧本只看 `PersonOrigin`；**两批神驹规则同质 → `Guest` 不区分批次**。
    - `game/roster.rs`：名单 = 卡组前缀（含友人卡，与 `deck` 同序）→ 理事长 → 记者 →
      神驹第 1 批（5 固定 + 带吾妻余额 1）→ 第 2 批 5；担当分配五步流水线
      （link 钉位 → 卡按类型本位 → 外派链 → NPC 每演目 1 人 → 余额），memo 三条算例成内联回归。
    - **门禁**：仅 SSR 吾妻（R / 不带 / 其他友人报错）、无团队卡（card_type 6 报错）、
      普通支援卡恰好 `support_num` 张、**同 `chara_id` 不允许两个人头**（只能分身）。
    - **随机非 link 角色**（余额 / 第 2 批 / 冲突顶替）：离线从 `chara_id` 1001 起按序取第一个满足
      条件者（非 link / 非育成马娘 / 未出现）；**育成马娘本身是 link 角色时，该演目固定神驹换随机
      非 link 顶替**；理事长 / 记者按引擎基础局默认处理。
    - **编成检测（`game/deck.rs`）**：禁止项（缺 SSR 吾妻 / 友人 >1 / 非 SSR 吾妻友人 / 通用规则
      〔必须 6 张、无重复人物、不含担当马娘、卡类型正确（含团队卡拒绝）〕）拒绝建局；
      警告项（W1 link 类型≠固定演目 / W2 同类 ≥3 / W3 外派 / W4 支援卡类型 <5 种（含友人）/
      W10 各演目「卡+第 1 批 NPC」人物数极差 ≥2）随 `MatsuriGame.deck_warnings` /
      `MatsuriView.deck_warnings` 下发，CLI 跑局前打印；不警告：某演目 0 张卡、link=固定演目
      导致的神驹顶替。宿主可先调 `inspect_deck` 自查。
    - 测试：`roster.rs` 内联 4 例 + `tests/mechanics.rs` 8 例（名单 / 可用回合 / link 顶替 /
      编成门禁 / 编成自查 / 警告三项）；四档绿（**38 内联 + 11 L2 + 13 机制 = 62 测例**）。

## 三、当前代码现状（S4 起点）

> 本轮提交（2026-11-01）：S3 阶段机 + S4 前置 + 参考卡组 / cardDB / umaDB 更新 + 文档回写
> （含 `shuntaisai_memo` 复核收口 U02）；四档绿 **62 测例**；引擎仓无改动、默认行为不变；
> 本仓暂无基线数值（S4 结算后补）。

- `game/stage.rs`：`Stage` / `TurnPlan` / `CampState` / 强制休息常量表；`impl MatsuriGame` 内
  `reset_strategy_stream` / `derive_plan` / `stage_body`。`stage_body` 现状：
  - `Begin`：`derive_plan` + 按特殊事件记录置 `forced_rest`（事件链 S6 待接）；
  - `Distribute` / `AfterTrain`：占位（S5 / S4+S6 填）；
  - `Train`：`list_actions` + 样例训练员选择 + `apply_action`（动作集仍 S1 的 5 训练 + 休息）；
  - `End`：仅合宿计数（発表会 / 本番 / 合宿 +1 / 教诲分配待 S5）。
- `game/mod.rs`：`MatsuriGame` 字段（`acts` / `advice` / `camp` / `plan` / `forced_rest` /
  `check_points` / `finals_done`）、`newgame`（含友人门禁）、`view`、`do_train`（S2 版：直接按
  `training.normal` 入账）、`act_level_max`、`build_game`。
- `game/person.rs` / `game/roster.rs`：剧本人头 + 名单 / 担当分配（S4 前置已落，见 §二.10）；
  `traits.rs` 的 `type Person = MatsuriPerson`、`init_persons` → `init_roster`、`person_is_available`
  已覆写；`person_index_of_*` / `deck_index_of` 等引擎默认仍可用。
- `game/action.rs`：`Action { Train(u8), Rest }`；`Rest` 仍是 +20 体力占位。
- `game/traits.rs`：契约 impl；`list_actions` 仍是 5 训练 + 休息；`distribution` 语义未接。
- `trainer.rs` / `cli/manual.rs`：样例训练员 / 手动训练员仍 S1 占位（S7 换）。
- **尚未接线**（S4/S5/S6）：比赛动作与结算、演目稽古动作、出行、休息规则、训练失败判定、
  羁绊 / 红点 / `default_after_train`、gauge 入账、発表会 / 本番结算、教诲分配 / 融合、事件链。

## 四、下一步 = S4 动作与结算

目标：**动作集 + 单动作结算**，训练值上层剧本加成 / gauge / 発表会仍留 S5。

建议顺序：
1. **动作集**（`list_actions`，按 `plan` 分叉）：
   - 强制比赛回合（`plan.race && !forced_rest`）→ `[Race]`；**`forced_rest` 回合 → `[Rest]`**
     （实际执行休息、不 push 比赛事件 / 奖励）；
   - 合宿回合（`plan.camp_train`）→ 5 个 `ActTrain`（`camp_rest` 时休息按合宿版）+ 第 1 年可出行；
   - URA 期 / 普通回合 → 日常动作集（5 训练 + gauge 满的 `ActTrain` + 休息 + 出行 + 自选比赛
     `can_self_race` 语义，13–71 且等级非 0）；
   - 动作 `Action` 扩展与 `Display`；`ActionEnum::apply` 分派。
   - 担当 / 可用人群从 `matsuri.persons`（`origin` / `act` / `avail_turn`）直取；亮起锁定与分布
     留 S5（`distribute_all` 覆写），S4 先接动作集与单动作结算。
2. **比赛结算**：`Race` → `base.push_race_event(scale, 1.0)`，`scale = (100 + race_bonus +
   演目/発表会比赛加成) / 100`（S4 先接 `race_bonus` + 発表会 `check_points` 的 `race_bonus` 已有资产，
   20/30/100 等；floor 在引擎）。
3. **训练结算**：`do_train` 改为走 `calc_training_value`（下层 = `default_calc_training_value`）
   + 失败判定（`calc_training_failure_rate`，不屈留 S5）→ 成功入账 / 失败事件；拿不准的先按引擎默认档。
4. **休息 / 出行**：普通休息与合宿版休息规则（memo 有数值待核）、吾妻出行 5 段（`outing` 资产）。
5. **训练后处理**：`default_after_train`（羁绊 / 红点 / hint 引擎默认）；`set_last_train` 留给 S5 追加稽古。
6. **测试**：各回合类型动作集、比赛奖励入账、强制休息不比赛、休息 / 出行、失败判定；
   内联 + `tests/mechanics.rs`；四档绿。之后补 S2 遗留的基线（S9 前）。

参照：`docs/mechanism.md` §3 / §4 / §6、`../umaai_next/docs/scenario/contracts.md`、
`../umaai-onsen` 的动作结算形态（**只借形态，不借数值**）。

## 五、S5 / S6 备忘（不要提前实现）

- **S5 剧本机制**：gauge 入账（担当每人 +1 / 闪彩 +2 / 目标赛 +4 三则落点 / 休息外出 +1；
  排练回合不涨）、演目稽古成功 Lv+1 + 槽清零（失败完全无效）、演目 Lv 上限（SSR 吾妻 10；建局已门禁）、
  教え效果（按绑定演目 Lv、按参与人数叠加；絆 +3 平摊例外）、融合（turn 23/47）、発表会 / 本番
  buff + 一次性奖励、合宿 +1（turn 15/39/63）、本番演出、追加稽古、训练值上层乘区、教诲分配决策
  （`End` 阶段、剧本结算后、120 排列）。
- **S6 事件**：固定回合 / 概率事件 / 伝受授予 / 合宿事件抑制表（剧本自持，不复用引擎 `no_event_turns`）、
  turn 66 进化事件；并把「马娘特殊事件记录」接进 `forced_rest`（先本仓临时表 → 引擎改造后读引擎数据）。

## 六、未决 / 风险

- 待真机复核：成功档一次性奖励、普通训练 Lv2–5、非目标赛「随机 1 项」、比赛加成叠加口径（204 / レスボ）。
- TBD：U19（延长段支援卡事件）、同名双人头、NPC 不在率、U40（追加稽古上下档）、
  `act_effect 201` 隐藏值、U46（第三年底目标赛剧本加成）、引擎「马娘特殊事件记录」改造。
- 参考卡组刚换成满凸新卡组：**尚无基线**；S4 动作结算完成后再补基线（S9 前）。
- 神驹第 2 批人選 / 随机非 link 取人为离线简化（stubs 登记）；接在线模式时需换随机。
- 早先文档「同名双人头可并存」已被用户 2026-11-01 决策覆盖：**模拟不允许同 chara 两个人头**。
- 数值位移：S2 训练表 / 上限换真实数据后跑局数值已变；S3 未落数值结算；S4 接入比赛 / 训练后
  会再位移，届时补基线。
- URA 收尾期口径修正（2026-11-01）已回写 `mechanism.md` / `intake.md` / `memo_cn.md` / `glossary.md`；
  旧 memo 章节的旧读法如果再次引用，按修正口径处理。

## 七、关键命令 / 约束

```bash
# cwd = 本仓仓根；同一 target 一次只跑一个 cargo；一律 --offline
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
cargo run --release --offline --bin matsuri -- matsuri   # 整局冒烟
```

- 测试 cwd 在各 crate 目录；数据根统一经 `matsuri_core::testing::data_dir()` 取仓根。
- **参考源纪律**：`../matsuri-ref-01` 不可作为机制 / 数值 / 实现依据；数据只用 `master.mdb` 原始表 +
  本仓 `docs/` + 日文 memo（`../mee-umasim/data/shuntaisai_memo.md`）。
- **不改引擎仓**；需要通用能力时先与用户确认、走引擎仓流程。
- **提交纪律**：不自动 commit / push；提交前统一更新 README 与剧本仓文档（AGENTS §文档规则），由用户确认。
