# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> **当前状态：S7 手写训练员 + 教诲分配走 Trainer 已实现并提交**（2026-11-02；**未 push**）；
> S6 事件管线 + S8 手动核对面板（先行）此前已提交并 push（`origin/main = ea5f9ad`）。
> 下一步：**S8 其余**（View 字段补全 / 在线外壳 / 选择器）→ **S9 基线** → 待量化（融合 A/B、训练员旋钮扫参、M2 搜索）。
> 新窗口可直接按 §四 开工，不用再等确认；完成后向用户汇报 diff，**不自动 commit / push**。

## 一、git / 环境现状

- **本仓（umaai-matsuri）**：`HEAD` = S7 提交 `feat(core,cli,data): S7 手写训练员 + 教诲分配走 Trainer
  （pt_favor 目标口径 + 配置旋钮）`（2026-11-02；19 文件，+1775/−166，含新增 `core/src/value.rs`；
  哈希以 `git log -1` 为准）；**未 push**
  （`origin/main = ea5f9ad`，仅此 1 个提交待推）。此前链：`ea5f9ad`（S8 面板细化 + 教诲 T2 Begin）→
  `ce6789b`（S8 升级 + T2 闸门）→ `8f4687e`（S6 + S8 先行）→ `00079f3`（S4+S5）→ `9995329`（S3）等。
- 依赖：`matsuri-cli` 已有 `inquire = 0.9.4`（终端动作 / 事件选项选择；非 TTY 退回编号输入）。
- **引擎仓 `../umaai_next`**：`HEAD = a8859e0`，**未改**（S7 未碰引擎，`git diff` 为空）。
- 数据：外部 `../../UmaAI/master.mdb`（md5 `ef481f95…`，不入仓）。**S7 未新增 mdb 表 / 资产键**。
- 日常命令一律 `--offline`（`~/.cargo` 只读，`cargo add` / `fetch` 需先请用户提权）；
  从**本仓仓根**执行 cargo；同一 target 一次只跑一个 cargo。

## 二、已完成

### S7 手写训练员（2026-11-02）

> 决策全部经用户拍板：目标函数 pt_favor、先手写后搜索、教诲分配与首次分配都走 Trainer、
> 教诲分配「120 合并为一次」、融合仍走配置、助力支撑保持参与者口径。
> 决策记录：`docs/plan/scenario_dev_plan.md` §3（2026-11-02 行）、`docs/glossary.md` §9。

1. **目标口径（pt_favor）**：`value.rs`（新增，私有模块）把动作 / 事件 / 教诲分配统一折算成评分点：
   官方评分 = 五维查表分（`five_status_final_score`，按本局上限夹取）+ PT×`pt_score_rate`；
   再对 PT 施加 `pt_favor_rate`（`1.0` = 官方评分，默认 `2.0`）。五维边际到上限归零。
2. **配置旋钮**：`MatsuriConfig` 新增 `[trainer]`（`pt_favor_rate` / `rest_vital` / `checkpoint_margin` /
   `act_level_target` / `self_race` / `friend_outing_early` / `status_weights[5]` / `weights` / `advice`），
   覆盖层给 `trainer` 即**整段替换** + 载入期范围校验；`data/matsuri_config.toml` 同步默认层。
3. **动作决策**（`trainer.rs`）：单候选捷径 → 教诲分配候选分流 → 治病门（不擅长 / 具名病；变胖不占动作）→
   其余统一打分（训练/稽古成功期望 + 失败率、体力曲线 / 干劲 / 羁绊 / 灵感、演目槽 / Lv / 発表会急迫、
   亮起囤积惩罚、追加稽古兑现、休息 / 出行 / 比赛 / 治疗）；自选赛默认关（`self_race`）。
   **决策确定性、不耗决策随机流**。
4. **事件决策**：`select_event_choice` 组内按 `prob` 加权期望 + `ExtraStatus` + 状态增删 + 友人加成；
   追加稽古 `400015106` 上档额外计「最低演目槽 +1 / 解 debuff」；17 个 `player_select` 决策点全走策略。
5. **教诲分配走 Trainer**：新增 `Action::AssignAdvice(AdviceAssignment)`（只作决策点候选，不在 `list_actions`）；
   **T2 Begin 首次 + 11/23/35/47/59 End 每窗口 120 候选合并为一次** `Trainer::select_action`，价值矩阵择优
   （与 `apply_assignment` 的 401+801 硬约束同口径）。融合仍走配置预设：`settle_advice_window` 的
   `fusion_plan(window)` 是唯一替换点（将来若要「交 Trainer 按配置固定选」只改它）。
6. **manual 面板**：教诲分配决策单独提示「当前分配 + 保持现状」（不刷 120 行）；动作标签 / 说明补齐。
7. **测试**：**157 测例**（core 内联 107 / L2 15 / 机制 29 / CLI 内联 5 / CLI 冒烟 1），四档全绿。
   新增：配置 3（`trainer_overlay_*` / 坏值拒绝）、估值 3（大胃王按体力取舍 / 801 硬约束同口径 / 边际饱和）、
   训练员 2（不擅长优先治病 / 旋钮读取）、机制 2（`handwritten_trainer_covers_all_decision_points`：
   分配决策恰好 6 次 [2,11,23,35,47,59]、融合已应用、事件决策被调用；`handwritten_trainer_is_deterministic`）。
   夹具适配：`ScriptedTrainer` 对分配候选保持现状；`outing_actions` 改为段事件增量计数。
8. **冒烟**：`cargo run --release --offline --bin matsuri -- matsuri` = **54683 / UB9、PT 9024、390 阶段**
   （旧样例训练员 40899 / UC、PT 6982）；同种子两次逐位一致。`random` 档 19516 → **29396**
   （分配 / 事件也走随机，属口径变化，非策略提升）。**S9 前不建基线**。
9. **文档**：README / 开发计划（S7 ☑ + 决策记录 + #24/#46）/ mechanism §6.2 + §3.3 助力口径 /
   tests_overview / glossary / stubs（旋钮 = STUB 待扫参）/ AGENTS（结构表加 `value.rs`）。

### S8 先行：手动核对面板（2026-11-01，已提交）

> 目的：用户手动核对机制。`cargo run --release --offline --bin matsuri -- manual`。

- `crates/matsuri-cli/src/manual.rs` 为核对面板：动作决策前打印回合结构 / 五维（含引擎上限）/ 体力干劲 /
  状态 / **剧本资源表**（演目槽 · Lv · 教诲→転成 · 担当；発表会与 buff / 助力 / 追加稽古着色）/
  **彩色训练明细表**（每训练一列，排练列头为演目名 Lv；人物短名 / 未满 100 羁绊 / Hint；友人绿 /
  彩圈卡亮绿 / 支援卡白 / NPC 灰）/ 合宿计数 / 友人链 / 目标赛与自选赛 / 动作候选（收益预览 + 失败率）/
  事件回显 / 选项分支（Inquire）/ 上一步变化；开局打印育成配置；演目 2 字短名；教诲窗口后列分配结果。
- 核心 API：`EventRecord` + `MatsuriView.recent_events`、`preview_training`、`preview_action_resources` /
  `ResourcePreview`（克隆局面、只读、不耗随机流）。
- **剧本机制自 turn 2 起**：0–1 回合无担当分配、不获取演目槽（`SCENARIO_START_TURN` 入口拦截）；
  教诲首次授予在 **T2 Begin**（NPC / 友人加入后立即），效果对 T2 分布 / 训练生效；End 窗口 0 不重复授予。
- **S8 其余未做**：`MatsuriView` 字段补全（目前只补 `recent_events`）、在线外壳 `matsuri-ai`、
  训练员选择器完整化。

### S6 事件管线（2026-11-01，已提交）

- `game/event.rs`：回合开始事件链（固定事件 → 合宿抑制 → 友人出行解锁 → 随机分布）；
  合宿抑制合并单一判定（合宿规则层优先 + 引擎 `no_event_turns` 基线层，STUB）；正月 / 结局 / 抽签入队；
  变胖解除 20%（STUB）；追加稽古事件化；turn 66 进化 hint 记账。
- 数据：`scenario_events`（400015105 / 400015106）+ `event_extras`；`friend_events` 吾妻 `809052xxx` 链
  （first `001` / click `032` 40% / out `004` / newyear `021` / 出行 `011–015` / end `023`）；
  构造事件「大胃王」`5010`（33% 变胖）。
- 详见 `docs/mechanism.md` §5.3–5.4 与 §3 / `docs/glossary.md` §9 2026-11-01 行。

## 三、当前代码现状

- 阶段机不变：`Begin → Distribute → Train → AfterTrain → End`（`StageId` 0/3/1/4/2 只增不改）；
  剧本机制自 turn 2 起；教诲首次授予在 T2 Begin。
- **决策面全部走 Trainer**：动作（`select_action`）/ 事件选项（`select_event_choice`）/ 教诲分配
  （`decide_advice_assignment`，候选 = 120 `Action::AssignAdvice`）。手写训练员确定性、不耗决策随机。
- 关键接口：`MatsuriTrainer::new(MatsuriConfig)` / `config()`；估值 `Model::new(game, &TrainerConfig)`
  （私有，训练员自己的旋钮为准）；`assignment_candidates` / `apply_assignment`（401+801 硬约束）、
  `advice_effect_of`、`view()` 全量快照、`kensan_act` / `roll_kensan`、`card_event_candidates`、
  `preview_training` / `preview_action_resources`、`MatsuriView.recent_events`。
- 数据资产：`scenario_matsuri.json` 18 个顶层键（含 `scenario_events` / `event_extras`）；
  `data/events.json` 为剧本自持事件根（吾妻链 + 大胃王）。
- **尚未接线**：S8 其余（View 字段 / 在线外壳 / 选择器）；`FlatSearchGame` 搜索档（M2；当前
  `RolloutTrainer` 仍 `RandomTrainer`，M2 时换取手写训练员）；基线（S9）；马娘特殊事件记录
  （引擎改造后迁移 T65）；事件⑤（TBD）；技能系统 / 粉丝不建模。

## 四、下一步

> S8 其余 → S9 基线 → 待量化 → M2 搜索。先手写档锁基线，再接搜索档（单独基线，不可混比）。

1. **S8 其余**：`MatsuriView` 字段补全（事件 / 友人 / 变胖等，面板当前直接读局面字段）；
   在线外壳 `matsuri-ai`（帧段解析 / Session / 决策循环，依赖真机帧）；训练员选择器完整化。
2. **S9 基线**：一基线一 JSON（`input` 自持、`expect` 容差、`meta` 不含路径）；数值位移后重录。
   注意：**`random` 档口径已变**（分配 + 事件也走随机，19516 → 29396），旧基线不可直接复用。
3. **待量化（S9 前只记录，不建基线）**：
   - 融合配对 3 套 A/B（`scenario_dev_plan.md` #46：现行 / 攻略 / 混合；用覆盖层 × N 种子配对）；
   - `[trainer]` 策略旋钮扫参（`stubs.md` 已登记 STUB；pt_favor 已在 2.0 档定档，其余权重 / 阈值待扫）。
4. **M2 搜索（按需）**：`FlatSearchGame` 适配 + `MctsTrainer`（引擎无现成实现，需剧本仓自建；
   见引擎 `docs/scenario/dev_process.md` §3.1）；根节点可用教诲分配 top-K。

## 五、备忘与未决

- **数值位移**：S5 34368 → S6 44946 →（T2 闸门 + 教诲 T2 Begin）40853 → 40899 / UC、PT 6982 →
  **S7 手写训练员 54683 / UB9、PT 9024**；`random` 19516 → 29396（口径变化）。S9 前不建基线；
  一有数值占位替换 / 训练员改动就重录并记影响面。
- **待真机复核**（`stubs.md` 已登记）：变胖解除 20%、事件⑤（SSR 技能事件未接）、事件抑制基线层、
  友人事件槽位映射、memo 友人事件值为「基础值」的推断、400015105 数值、通用事件数值、
  技能灵感 `hint_level` 记账、turn 66 进化 hint+1、T65 之外马娘特殊事件、训练员旋钮默认值。
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
cargo run --release --offline --bin matsuri -- matsuri   # 整局冒烟
cargo run --release --offline --bin matsuri -- manual    # 手动核对面板（交互式，q 退出）
```

- 测试 cwd 在各 crate 目录；数据根统一经 `matsuri_core::testing::data_dir()` 取仓根
  （内联测试可用 `testing::sample_game()`）。
- **不改引擎仓**；需要通用能力时先与用户确认、走引擎仓流程。
- **提交纪律**：不自动 commit / push；提交前统一更新 README 与剧本仓文档（AGENTS §文档规则），
  由用户确认。手写训练员基线未锁（S9），此前冒烟数值只作趋势参考。
