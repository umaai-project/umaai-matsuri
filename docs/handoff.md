# 交接提示词

> **一次性文档**：每次交接**整份重写**，只保留下一步需要的信息；不在旧版本上叠加历史。
> 读法：本文件 + §4 的关键落点即可续接。

## 1. 当前状态（2026-11-03）

- **仓**：`umaai-matsuri`（本仓，cwd）→ 引擎仓 `../umaai_next`（HEAD `a8859e0`，未改，`git diff` 空）。
- **git**：HEAD = 本批提交（`git log -1`；标题 `feat(core,cli,docs): 合宿延长段事件轴 + 比赛加成乘算 + 狄杜斯铁心`）；
  `origin/main`（本地 ref）`9203506`；**领先 2 个提交、未 push**；工作树干净。
- **验收**：四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；
  `cargo fmt --all --check` 绿；workspace clippy 需 `-A clippy::useless-borrows-in-formatting`
  （本机 1.99.1 对既有 `explain_smoke.rs:35` 的新 lint，与代码无关；CI 钉 1.99.0）。
- **冒烟**（seed 61444）：`matsuri` **78397 / LG3、PT 11793**；`random` **27150 / UF6、PT 5153**；
  106301 单局探针 **79929 / LG6、PT 11500**（非基线）。**100 局均值未重跑**（S9 前不建基线）。

## 2. 本批口径（S9 需要覆盖的位移源）

1. **合宿延长段事件轴 = 正常回合（U19 结项）**
   合宿抑制层只盖第 2/3 年**常规 4 回合**；延长段不经过合宿层，回落引擎 `no_event_turns`
   基线（表内无延长回合），支援卡连续 / 普通马娘 / 掉心情等随机事件照常，仅训练全变稽古。
   固定事件不受抑制。落点 `game/event.rs::random_events_suppressed`，判定含 `plan.camp_regular`。

2. **比赛加成改为乘算**
   `race_scale = (100 + 卡面赛后)/100 × (100 + 剧本赛后 204)/100`，铁心生效时再 ×2.5；
   引擎在面板乘完后**统一 floor 一次**。`do_race` 与训练员 `value.rs::estimated_race_reward` 共用。
   本番前 204 = 0 退化为卡面单乘；本番大成功 +100% 时测试配卡 55% → **3.10**：T71 生涯赛
   5/72 → **15/223**；本番仅成功 ×1.3 → 10/145。影响 T69/T71 生涯赛与 URA 决赛 73/75/77。

3. **狄杜斯「铁心的挑战者」：五维 ×2.5 / PT ×0.46**
   生野狄杜斯 **106301 / 106302**：出道战后（**T12–71**）对**所有生涯比赛** 五维 **×2.5**、
   PT **×0.46**。与卡面赛后乘算，在剧本赛后乘算之前；PT 折扣由引擎 `pt_rate` 单独缩放。
   自选赛 / 出道战 T11 / URA 73+ 不生效。状态 `31` 未建模，按马娘 ID 硬编码
   （`game/mod.rs::special_race_stats_bonus` / `race_pt_rate`）；View 与 manual 面板已同步。
   测试配卡 55%：T22/T58 **19/128**；T68/T69/T71（本番大成功后）**38/256**。

## 3. 下一步

1. **push**：`origin/main = 9203506`，本仓领先 2 个提交（不自动 push）。
2. **S9 基线**：一基线一 JSON（`input` 自持 / `expect` 容差 / `meta` 不含路径）。本批位移集中在
   「本番后比赛」「狄杜斯生涯赛」「训练员比赛估值（PT 折扣）」；覆盖组合：有义务 freeRaces / 无义务、
   有合宿延长 / 无延长、本番后收益（T69/T71/URA）、狄杜斯 106301。入口建议
   `DiagGuard::suppress()` 或覆盖层 `log_level="off"`。
3. **待量化（S9 前只记录）**：`race_grades` 标准表 vs 真机自选格子（dev_plan #47）；
   `free_race_urgency` 扫参；根演目；MC 配对；铁心对 106301 配卡的影响。
4. **M2 搜索**：`FlatSearchGame` + 自建 `MctsTrainer`（当前 `RolloutTrainer` 仍 `RandomTrainer`）。
5. **待真机复核**：
   - 比赛加成 floor 位置：当前一次性（T71 15/223）；若分步 floor → 14/222。
   - 铁心系数：2.5 / 0.46 为实测反推的近似值（温泉 1063 口径为 2.7 / 0.45）；拿到状态表 /
     更多观测点后再校准，状态系统落地后改读 `status 31`。
   - 剧本 204 乘算是否真机成立（30/100 → ×1.3 / ×2.0）。
6. **100 局均值重跑**：本批未重录（S9 前不建基线）。

## 4. 关键落点 / 命令 / 纪律

- 比赛：`game/action.rs::do_race` → `game/mod.rs::race_scale` / `special_race_stats_bonus` /
  `race_pt_rate`；训练员 `value.rs::estimated_race_reward` 按同一口径分别缩放五维与 PT。
- 事件：`game/event.rs::random_events_suppressed`。
- View / 面板：`game/view.rs`（`UmaView.special_race_stats_bonus` / `special_race_pt_rate`）、
  `matsuri-cli/src/panel.rs`（「铁心 五维×2.5 / PT×0.46」）。
- 数据：`data/`（`scenario_matsuri.json` / `events.json` / `ai_config.toml`，基准 seed **61444**；
  测试内部固定种子 42）。
- 文档：`docs/mechanism.md` / `docs/glossary.md` / `docs/plan/scenario_dev_plan.md` /
  `docs/plan/stubs.md` / `docs/tests_overview.md` / `docs/matsuri_memo_cn.md`。
- 命令（cwd = 本仓仓根；同一 target 一次一个 cargo；一律 `--offline`）：

  ```bash
  cargo fmt --all --check
  cargo test --workspace --release --offline
  cargo clippy --workspace --all-targets --offline -- -D warnings -A clippy::useless-borrows-in-formatting
  cargo run --release --offline --bin matsuri -- matsuri
  cargo run --release --offline --bin matsuri -- random
  cargo run --release --offline --bin matsuri -- manual
  ```

- 纪律：不自动 commit / push；提交前统一更新 README + 文档；**不碰引擎仓**；用户确认整理前
  不跑会改代码的 `cargo fmt` / `cargo clippy`（`--check` 只读可跑）；测试 cwd 在各 crate 目录，
  数据根经 `matsuri_core::testing::data_dir()`。

## 5. 待决 / 风险

- floor 位置未定（一次性 T71 15/223；分步 14/222），已按用户「先一次性」实现。
- 铁心系数（2.5 / 0.46）为实测反推的近似值，状态 31 未建模；106301 / 106302 之外不覆盖。
- 100 局均值 / S9 基线未建；本批冒烟只作趋势参考。