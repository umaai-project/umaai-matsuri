# 交接提示词

> **一次性文档**：每次交接**整份重写**，只保留下一步需要的信息；不在旧版本上叠加历史。
> 读法：本文件 + §4 的关键落点即可续接。

## 1. 当前状态（2026-11-03）

- **仓**：`umaai-matsuri`（本仓，cwd）→ 引擎仓 `../umaai_next`（HEAD `a8859e0`，未改，`git diff` 空）。
- **git**：HEAD = 本批提交（`git log -1`，标题含「溢出处理与末段稽古兑现旋钮」）；
  `origin/main`（本地 ref）`9203506`；**领先 3 个提交、未 push**；工作树干净。
- **验收**：四档绿 **204 测例**（147 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；
  `cargo fmt --all --check` 绿；workspace clippy 需 `-A clippy::useless-borrows-in-formatting`
  （本机 1.99.1 对既有 `explain_smoke.rs:35` 的新 lint，与代码无关；CI 钉 1.99.0）。
- **冒烟**（seed 61444）：`matsuri` **78397 / LG3、PT 11793**；`random` **27150 / UF6、PT 5153**。
- **测试马娘**：默认 **100101（特别周）**。106301 / 106302 只做「铁心」专项，**不要拿来做普适调参 / 基线**
  （铁心五维 ×2.5 会把单局评分抬高，容易误判策略收益）。

## 2. 本批已落（调参起点）

1. **溢出层 `[trainer.overflow]`**
   `margin` / `tilt` / `keep`。按**生效上限** `five_status_limit` 判定近顶；非近顶轴权重
   `× (1 + tilt × 近顶轴数 / 5)`；主属性近顶时副属性 + PT 给 `keep` 保留加分，
   避免「速训练 → 力量 / PT」被完全倾斜。只作用于训练估值，事件 / 比赛不套用。
   落点 `value.rs` 的 `near_cap` / `overflow_factor` / `status_gain_with` / `train_value`。

2. **末段稽古层 `[trainer.endgame]`**
   `keiko` / `scarcity` / `level` / `normal_penalty`。窗口 = **第三年合宿结束后～T71**；
   只作用普通回合（非合宿 / 非 URA / 非比赛 / 非强制休息）。
   `pressure = max(0, (满槽数 − 剩余普通回合) / 满槽数)`；满槽 `ActTrain` 加
   `keiko × (1 + scarcity × pressure)`，未满 Lv 加 `level × min(gap,2)`；普通 `Train` 扣
   `normal_penalty × pressure`。**URA 72–77 全稽古，不进入本窗口**。

3. **当前默认档**（`data/matsuri_config.toml` + `config.rs::Default` 同步）
   `margin 0.02 / tilt 0.5 / keep 0.15`；`keiko 250 / scarcity 1.0 / level 80 / normal_penalty 120`。
   100 seed discovery（61444–61543）上 106301 +245（t=3.24），但 200 seed holdout（61544–61743）
   未复现（三马 −33 / +7 / −7，|t|<1.2）；因此当前档位定位为**行为旋钮**，不宣称评分增益。

4. **已提交但仍影响 S9 的口径**（本轮不改，基线需覆盖）
   合宿延长段事件轴 = 正常回合（U19）；比赛加成 = 卡面赛后 × 剧本赛后（统一 floor 一次）；
   狄杜斯「铁心的挑战者」（106301 / 106302，T12–71 生涯赛 五维 ×2.5 / PT ×0.46）。

## 3. 下一步：继续调参（优先）

1. **固定主测 100101（特别周）**；106301 / 106302 只做铁心专项。不要用狄杜斯跑普适均值。
2. **溢出层扫参**：`margin` / `tilt` / `keep` 三轴，重点看「速顶满后速训练仍被选」的比例、
   五维终局余量与总分。建议先做单变量 + 配对种子，别一次全开。
3. **末段稽古扫参**：窗口保持「第三年合宿后～T71」，重点看 T64–71 每个普通回合是否兑现满槽、
   T71 收盘的满槽数 / 五个演目 Lv，以及 `normal_penalty` 是否只压普通训练、不误伤休息 / 治病。
   若多数局面仍是「比赛把槽推满、普通回合不改选择」，下一步再考虑加 `race_pressure` 前瞻，
   而不是把现有 `keiko` 值盲目调大。
4. **指标口径**：评分（mean / min / 档位）+ T71 满槽未用数 + 演目 Lv + 稽古动作数；
   评分不是唯一验收，行为指标必须同时改善。
5. **调参后**：定档 → 重录 100 局 / S9 基线；S9 覆盖有/无 freeRaces 义务、有/无合宿延长、
   本番后比赛（T69/T71/URA）；入口建议 `DiagGuard::suppress()` 或覆盖层 `log_level="off"`。
6. **之后**：M2 搜索（`FlatSearchGame` + 自建 `MctsTrainer`）；真机复核 floor 位置 / 铁心系数 /
   剧本 204 乘算；根演目与 MC 配对量化。

## 4. 关键落点 / 命令 / 纪律

- 溢出：`value.rs::Model::new`（`near_cap` / `overflow_factor`）、`status_gain_with`、
  `train_value` 的 `keep`；配置 `config.rs::OverflowConfig`。
- 末段稽古：`value.rs::Model::new`（`endgame` / `endgame_pressure`）、`endgame_keiko_bonus`、
  `train_value` 的 `normal_penalty`；配置 `config.rs::EndgameConfig`。
- 数据：`data/ai_config.toml`（`uma = 100101`、seed **61444**）；`data/matsuri_config.toml`
  （overflow / endgame 默认档；改这里必须同步 `config.rs::Default`）。
- 文档：`docs/mechanism.md` §6.2、`docs/glossary.md` §9、`docs/plan/scenario_dev_plan.md` §3、
  `docs/tests_overview.md`。
- 命令（cwd = 本仓仓根；同一 target 一次一个 cargo；一律 `--offline`）：

  ```bash
  cargo fmt --all --check
  cargo test --workspace --release --offline
  cargo clippy --workspace --all-targets --offline -- -D warnings -A clippy::useless-borrows-in-formatting
  cargo run --release --offline --bin matsuri -- matsuri
  cargo run --release --offline --bin matsuri -- random
  cargo run --release --offline --bin matsuri -- manual
  ```

- 调参覆盖层放在临时目录，`data/` 用软链；不要把测试马娘 / 临时配置写进仓内 `data/`。
- 纪律：不自动 push；不碰引擎仓；用户确认整理前不跑会改代码的 `cargo fmt` / `cargo clippy`
  （`--check` 只读可跑）；测试 cwd 在各 crate 目录，数据根经 `matsuri_core::testing::data_dir()`。

## 5. 当前待决 / 风险

- 当前档位 holdout 中性；继续调参时若评分与行为指标冲突，按用户此前的取向优先「稽古兑现 /
  溢出处理」的行为正确性，再回看评分。
- 末段稽古只覆盖普通回合；比赛回合无法改动作，若槽仍被比赛推满，属于机制约束而非策略漏选。
- 铁心按马娘 ID 硬编码（状态 31 未建模）；只用于 106301 / 106302 专项，不参与普适均值。
- S9 基线仍未建；本批冒烟只作趋势参考。