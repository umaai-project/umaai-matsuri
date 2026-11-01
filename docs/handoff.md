# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> **当前状态：S6 事件管线 + S8 手动核对面板（先行）已实现并提交**
> （2026-11-01 提交；**未 push**）。
> 下一步 **S7 训练员 / 搜索**（S8 其余：View 字段补全 / 在线外壳 / 选择器）。
> 新窗口可直接按 §四 开工，不用再等确认；完成后向用户汇报 diff，**不自动 commit / push**。

## 一、git / 环境现状

- **本仓（umaai-matsuri）**：S6 事件管线 + S8 手动核对面板（先行）已提交（2026-11-01）；
  此前一个提交 = `00079f3 feat(core,cli,data): S4 动作与结算 + S5 机制闭环`（S3 = `9995329`）。
- **引擎仓 `../umaai_next`**：`HEAD = a8859e0`，**未改**（S6 未碰引擎，`git diff` 为空）。
- 数据：外部 `../../UmaAI/master.mdb`（md5 `ef481f95…`，不入仓）。**S6 脚本 / 数据均未新增 mdb 表**；
  事件数据来自 `master.mdb` 的 `single_mode_story_data`（id / 标题 / 卡归属）+ memo 数值，已入仓。
- 日常命令一律 `--offline`（`~/.cargo` 只读，`cargo add` / `fetch` 需先请用户提权）；
  从**本仓仓根**执行 cargo；同一 target 一次只跑一个 cargo。

## 二、已完成（S6 事件管线，2026-11-01）

> 口径全部经用户逐条拍板（决策记录见 `docs/plan/scenario_dev_plan.md` §3、`docs/glossary.md` §9）。

1. **数据层**
   - `data/scenario_matsuri.json` 新增 `scenario_events`（`400015105` 教え見直し Fixed [11,35,59]，
     体力+15/干劲+1；`400015106` 追加の稽古 Code，训练员上/下二选一）与 `event_extras`
     （`(400015106, choice 0)` → 最低演目 gauge+1 + 解一个 debuff，走上限拦截）；
     `rules.kensan` 删除 → `rules.fat_recovery_prob = 20`（剧本自持 STUB）。
   - `data/events.json`：`friend_events` 换为吾妻 `809052xxx`（first=`001` / click=`032`（40%）/
     out=`004`（上/下）/ newyear=`021` / 出行 `1..5`=`011–015` / end=`023`，5 次档）；
     新增构造事件「大胃王」`5010`（Random max_time 3，选项 1 体力+10/PT+5，选项 2 体力+30/PT+10
     + 33% 变胖）；删除 `system_events` 里的 `matsuri_outing_1..5` 占位。
2. **`game/event.rs`（新建）**：回合开始事件链 `begin_events`（固定事件 → 合宿抑制 → 友人出行解锁 →
   随机分布）；`random_events_suppressed`（合宿规则层优先 + 引擎 `no_event_turns` 基线层，
   **合并为单一判定**）；`append_begin_friend_event` / `queue_after_train_events`（正月 turn 24、
   结局 + 育成结束 turn 77、抽签 turn 48）；`recover_fatigue` / `roll_fat_recovery` / `is_fat`；
   `apply_event_extra` / `clear_one_debuff`；`trigger_kensan_event`；`apply_evolution_hint`（turn 66）。
3. **`traits.rs`**：`generate_events` → `begin_events`；`apply_event` 补 ①`add/remove_flags` 结算
   （引擎 `BaseGame::apply_event` 不结）、②事件羁绊归属、③`809052004` → `AfterUnlock` + 疲劳回复 flag、
   ④剧本事件扩展。通用固定事件 `4012/4013` 继承结算由 `base.apply_event` 自身完成（引擎基础局同型）。
4. **`stage.rs`**：`Begin` 接线（疲劳回复 / 变胖解除 → 事件链 → 正月 → 回合后事件入队 → 追加稽古预掷）；
   `End` 加 turn 66 进化 hint 记账。
5. **`train.rs`**：`after_train` 加友人点击链（首次点击 → `809052001` + `BeforeUnlock`；之后按
   **引擎常量 40%** 推 `809052032`，与追加稽古预掷**独立**）；`try_kensan` 改为推 `400015106` 事件；
   **变胖 = 训练 / 排练速度增量归零**（训练后事件 / 休息 / 出行 / 比赛 / 事件仍可加速度）。
6. **`action.rs`**：友人出行推真实卡面事件（`friend_events["1".."5"]`）；合宿版休息解除变胖。
7. **`mod.rs`**：新增 `MatsuriGame.fatigue_recovery`（剧本自持，后续可下沉 `umaai-next` 基础库）。
8. **测试**：**95 内联 + 15 L2 + 27 机制 = 137 测例**，四档全绿；
   整局冒烟（样例训练员）**44946 分 / UC6、PT 7496、390 阶段**（S5 为 34368 / UE9、PT 6128；
   S6 事件收益接入后的预期位移，S9 前不建基线）。
9. **文档**：`README.md` / `mechanism.md` §5.3–5.4 / `glossary.md` / `stubs.md` / `scenario_dev_plan.md` /
   `tests_overview.md` / `intake.md` 已同步。
### S8 先行：手动核对面板（2026-11-01，用户指定先做）

> 目的：用户手动核对机制。`cargo run --release --offline --bin matsuri -- manual`。

- `crates/matsuri-cli/src/manual.rs` 重写为核对面板：每次决策前打印回合结构（年次 / 阶段 / 回合类型）/
  五维（含引擎上限）/ 体力干劲 / 状态（flags / 疲劳回复 / 追加稽古 / 助力支撑 / 上次训练）/ 演目
  gauge·Lv·亮起·绑定教诲与転成 / 発表会结果与永久 buff / 合宿计数 / 友人链（状态 / 出行段 / 下一段事件）/
  目标赛与自选赛窗口 / 人头分布（按训练位，含设施 Lv 与计数）/ 动作候选（成功收益预览 + 失败率）/
  「自上次以来新结算事件」/「上一步后数值变化」；事件选择打印选项组（概率 / 结果档 / 数值 /
  ExtraStatus / 状态增删）。
- 核心新增（`game/mod.rs`）：`EventRecord` + `MatsuriGame.event_log`（容量 32，`push_event_log`，
  `MatsuriView.recent_events` 同步）+ `TrainingPreview` + `MatsuriGame::preview_training`（克隆局面、
  临时切稽古上下文；不改原局面、不耗随机流）。`traits.rs::apply_event` 结算后写记录。
- `main.rs`：manual 档 `PrintFrontend` 切 `Verbosity::Quiet`（面板唯一信息源）。
- 测试：97 内联（+`event_records_are_logged` / `preview_matches_actual_training`）+ 15 L2 + 27 机制 +
  **1 CLI 冒烟**（`matsuri-cli/tests/manual_smoke.rs`：管道输入全 0 跑满整局、面板关键块与 S6 关键事件回显）。
- **S8 其余未做**：View 字段补全（当前只补了 `recent_events`）、在线外壳（`matsuri-ai`）、训练员选择器完整化。

## 三、当前代码现状（S7 起点）

- 阶段机不变：`Begin → Distribute → Train → AfterTrain → End`（`StageId` 0/3/1/4/2 只增不改）。
- 事件链全部在 `game/event.rs` + `traits.rs::apply_event`；**事件决策点**（trainer 选选项）已有：
  追加稽古上/下、友人出行解锁上/下、外出第 3 段上/下、大胃王、抽签 / 新年 / 通用事件等。
  当前样例训练员 `MatsuriTrainer::select_choice` 恒返回 0（上档 / 首项）——**S7 需做事件选项策略**。
- 数据资产：`scenario_matsuri.json` 18 个顶层键（新增 `scenario_events` / `event_extras`）；
  `data/events.json` 为剧本自持事件根（好友人链 + 大胃王）。
- 可复用接口：`assignment_candidates` / `apply_assignment`（教诲 120 排列）、`view()` 全量快照、
  `kensan_act()` / `roll_kensan`、`card_event_candidates()` 等。
- 手动核对面板（S8 先行）可用：`manual` 档；事件记录 `MatsuriView.recent_events`、训练预览
  `preview_training` 已进入核心 API。
- **尚未接线**：教诲分配策略 / top-K（S7）；`FlatSearchGame` 适配（M2）；基线（S9）；
  马娘特殊事件记录（引擎改造后迁移 T65）；事件⑤（TBD）；技能系统 / 粉丝不建模。

## 四、下一步 = S7 训练员 / 搜索（S8 其余随后）

目标：把「能跑」换成「会打」——手写守门链 + 事件选项策略 + 教诲分配策略（必要时接搜索）。

> S8 剩余项：`MatsuriView` 字段补全（事件 / 友人 / 变胖等，手动面板当前直接读局面字段）、
> 在线外壳 `matsuri-ai`（帧段解析 / Session / 决策循环，依赖真机帧）、训练员选择器完整化。

建议顺序：
1. **事件选项策略**：`select_event_choice` 按事件语义给分——追加稽古（上档通常优，体力低时下档）、
   出行解锁（上 = 体力 / 下 = 属性 + 灵感）、外出第 3 段（体力 vs 属性 + SP）、大胃王（体力 vs 变胖风险）、
   抽签 / 新年 / 通用事件（按当前面板缺口取值）。
2. **动作守门链**：治病 → （目标赛）比赛 → 演出稽古 / 训练优先序 → 休息 → 出行 / 友人出行；
   结合 S5 的演目 gauge / 教诲 / 发布会达标节奏（守住 Lv3/5/6 门槛）。
3. **教诲分配 / 融合策略**：用 S5 的 `assignment_candidates`（120 排列）+ 价值矩阵（卡数 / NPC 数 /
   训练频率 / 演目 Lv / gauge / 是否合宿）做每窗口择优；融合按配置预设 + 在线覆盖。
4. **搜索（M2，按需）**：`FlatSearchGame` 适配 + `MctsTrainer`（见引擎 `docs/scenario/dev_process.md` §3.1）；
   先手写档稳定并锁基线，再接搜索档（单独基线，不可混比）。
5. **测试**：守门链覆盖全部决策点、事件选项无遗漏、教诲分配排列生效；四档绿 + 冒烟。

## 五、S7+ 备忘与未决

- **数值位移**：S5 → S6 冒烟 34368 → 44946（事件收益）。S7 换训练员 / 策略后必须重录并在
  `glossary.md` / `tests_overview.md` 记影响面；S9 一基线一 JSON。
- **待真机复核**（`stubs.md` 已登记）：变胖解除概率 20%、
  事件⑤（SSR 技能事件，未接）、事件抑制基线层（引擎 `no_event_turns`）、友人事件槽位映射、
  memo 友人事件值为「基础值」的推断、400015105 数值、通用事件数值（引擎基础局近似值）、
  技能灵感 `hint_level` 记账口径、turn 66 进化 hint+1、T65 之外马娘特殊事件。
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
  由用户确认。
