# 交接提示词

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-04）

- **仓**：`umaai-matsuri`（本仓，cwd）→ 引擎仓 `../umaai_next`（未改，`git diff` 空）。
- **git**：HEAD `27f9c29`（「溢出处理与末段稽古兑现旋钮」）；`origin/main` 本地 ref `9203506`；
  领先 3 个提交、未 push。**工作树有未提交改动**（本轮全部）：`data/ai_config.toml`（种马因子）、
  `data/matsuri_config.toml`（`[pre]`）、`matsuri-core`（继承 / 切者 / 小切 / 心情盾 / 理事长 T1 /
  自选比赛 End 判定 + 达标奖励）、`game/mod.rs` 内联测试（4 个自选比赛）、`mechanics.rs`（3 个新测试 +
  可用回合）、文档 5 份。
- **验收**：workspace **208 测例全绿**（147 内联 + 15 data + 1 diag + 36 mechanics + 7 CLI 内联 +
  1 explain smoke + 1 CLI smoke）；`cargo fmt --all --check` 与
  `cargo clippy --workspace --all-targets --offline -- -D warnings` 绿。
- **育成前配置**：`data/ai_config.toml` `blue_count = [18, 0, 0, 0, 0]`、
  `extra_count = [30, 30, 60, 80, 80, 100]`；`data/matsuri_config.toml` `[pre] xiaoqie = false`、
  `positive_thinking = 0`（小切 / 心情盾默认关，按账号开）。
- **基线（当前默认档，100 局 seeds 61444–61543）**：
  - **100101（默认基准）**：均 **78124.5**（min 69087 / max 81755，P10 74682 / P50 78600 / P90 81022；
    ≥US1 100 / ≥US9 91 / ≥LG 79），均 PT **11052.2**（10065–12064），
    五维均 [3355.1, 2316.1, 2159.5, 1615.0, 2400.0]；seed 61444 冒烟 **81553 / LG8、PT 11490**。
  - **102102（freeRaces 专项）**：均 **76406.6**（min 64558 / max 82516、≥LG 57、PT 10979.2、
    `career_failed` 0；seed 61444 冒烟 76002 / LG、PT 10620）；每局 T28 / T48 各入账一次
    「生涯比赛胜利」奖励。
  - **狄杜斯专项**（不作普适均值）：106301 均 **82211.3**（≥LG 95、PT 11805.2）；
    106302 均 **82209.4**（≥LG 97、PT 11830.8）；两马 0 失败。
  - **局外状态专项**（对照基线 78124.5）：+小切 **79008.2（+883.7）**；+心情盾×2
    **78538.6（+414.2，t=2.03）**；两者叠加 **79428.5（+1304.0）**。心情盾 t 值边缘，需换块复验。
- **测试马娘**：默认 **100101（特别周，无 freeRaces 义务）**；**102102** 用于 freeRaces 专项；
  106301 / 106302 只做「铁心」专项，不拿来做普适均值。

## 2. 本轮落地的机制（下一轮别再重复实现）

| 机制 | 落点 | 口径 |
|---|---|---|
| 继承（T30 / T54） | `game/traits.rs::apply_event` | 五维 `+= 蓝因子×6 + round(剧本因子×scale)`、PT `+= round(extra[PT]×(0.5+0.5scale))`、上限 `+= floor(蓝因子×5.33)`；`scale` 每场继承抽一次 |
| 通用切 | 事件 `5007` 数据（3% 档 `add_flags.qiezhe`） | 引擎 `gain_qiezhe` 清小切；**不叠加**引擎样例局的「5007 + 5%」近似 |
| 小切 | `[pre] xiaoqie` → `with_config` | 建局注入 `UmaFlags.xiaoqie`；评分 PT 项 ×1.04；获得切者后失效 |
| 心情盾 | `[pre] positive_thinking` → `with_config` | 挡 N 次掉干劲（一次扣减无论几级只耗 1 层） |
| 理事长出场 | `traits.rs::person_is_available` | **T1 起出现**（T0 不出现；引擎基础局默认 T0） |
| 自选比赛区间判定 | `game/settle.rs::settle_free_race_deadline`（`settle_turn_end` 内） | **区间最后一回合的 `End`** 判定；未达标 → `career_failed`；达标 → 入队 `race_career`（5/72，吃 `race_scale` / `race_pt_rate`），同回合 End 由 `stage.rs` 排空结算，每区间一次 |

文档同步位置：`mechanism.md` §2.2 / §6.5、`glossary.md`（术语 + 口径决定）、`tests_overview.md`
（测试与核对记录）、`plan/scenario_dev_plan.md`（决策记录 + 状态）。README 状态行按纪律留到提交准备。

## 3. 已否掉的杠杆（不要重复）

以下都实际扫过，均未在留出块转正，代码已全部回退。补丁在 `logs/diag/*.patch`，只作参考，不要直接应用。

| 杠杆 | 结果 |
|---|---|
| 白给属性 / `pt_favor_rate=16` | 训练填满或接近上限后，模型转去休息 / 外出；PT16 均值 65599，基准 74061，−8462 |
| 根性稽古软旋钮 | 根 Lv 6.65→7.55、根稽古 1.88→2.33，评分 −137～−724 |
| 单演目 focus（年 2 冲 Lv8） | 评分 −45～−425；速 Lv 曲线基本不动 |
| `weights.hint` 抬高 | 训练集 +239，全新块 62200–62299 为 −184～−313；不采纳 |
| `advice.hint`（教诲分配 hint 项） | 四档结果逐位相同，route 先验压过 hint 项；不采纳 |
| 普通训练等级进度价值 | 小块 +145.7（t=1.38），600 局大块 −162.5；不采纳 |
| `kensan.scale=2` | 追加稽古作废率 62%→55%，评分中性（−0.2%），不单独保留 |
| 动机 clamp | seed 61444 从 78397 降到 70032，回退 |
| 助力支撑专项 | 3 层 +5% 训练效果，单独动作价值不足，不保留 |
| PT 饱和阈值（≥15000） | 正常局总 PT 约 11k，从不触发 |

结论：这些方向收益太薄或方差太大，小块容易出假阳性。**任何新杠杆都要在 100 局以上、且换全新种子块复验**；
小块的 +200～+400 不可信。

## 4. 下一步（按优先级）

1. **合并外部新策略改进（预计下一轮；用户 2026-11-04 提出）**——外部实验仓成果迁入本仓，按上次 `ga-r2` 合并的形态落：
   - **前置**：先由用户确认整理 / 提交本轮未提交改动（208 测例 + 基线已重录），避免两批改动叠在同一个工作树里；
   - 策略旋钮一律进 `[trainer.*]`（`config.rs` 与 `data/matsuri_config.toml` 两边同步，`tests/data.rs::load_repo_config` 守门）；
   - 机制改动由本仓自持，**不碰引擎**（`../umaai_next` 的 `git diff` 必须为空）；
   - 合并后先过四门（fmt `--check` / test / clippy / 冒烟），再以本轮基线（100101 **78124.5** / 102102 **76406.6**）做配对，
     100 局起步 + 全新种子块（63700 起）复验；外部小块收益不可信；
   - 若外部成果基于旧因子 / 旧继承口径测得，差值不能直接搬（本仓 100101 基线已从 74008.5 / 77792.2 走到 78124.5）。
2. **S9 基线（顺延，只读，不改代码 / 配置）**：100101 与 102102 各 100 局，记录逐局评分 / 评级 / PT / 五维 /
   五演目 Lv / 训练等级 / 失败次数 / 治病天数 / 休息天数 / 空转回合 / T71 满槽未用数 / 追加稽古作废次数 /
   友人出行次数 / 比赛场数；分组看中位数与尾部，不要只看均值。产物写 `logs/diag/s9/`（`logs/` 已 gitignore）。
3. **待验证 / 待拍板**：
   - 心情盾×2 的 +414（t=2.03）换新种子块复验；
   - 合宿选位策略（现状：智/速为主、根性三年 0 次、终局根性 1581/2200 差 619）：
     待跑 A/B —— 合宿强补根 vs 现状、合宿首回合休息 vs 现状；
   - 通用切口径：本仓走事件数据 3% 档，是否要引擎样例局的 5% 近似（叠加会双计，暂不取）；
   - `race_grades` 标准表与真机自选赛格子对齐（影响义务区间可达性）；
   - README 状态行与提交整理（用户确认后再动）。
4. **种子块已用**：61444–61543（基线 / 训练）、61544–61643（旧留出）、62200–62299（hint）、
   62800–63099（训练等级三小块）、63100–63699（训练等级 600 局）。下一轮调参请另选全新块，建议 **63700 起**。

## 5. 关键落点 / 命令 / 纪律

- 训练员：`crates/matsuri-core/src/trainer.rs`（守门链 + 打分表 + 教诲分配）。
- 估值：`crates/matsuri-core/src/value.rs`（官方评分边际、`pt_favor`、动作 / 事件 / 分配折算）。
- 机制：`crates/matsuri-core/src/game/{mod,traits,settle,stage,event,train,action}.rs`；
  继承 / 状态在 `traits.rs::apply_event`，自选比赛判定在 `settle.rs`，End 排空在 `stage.rs`。
- 配置：`crates/matsuri-core/src/config.rs`；默认层 `data/matsuri_config.toml`，改默认必须两边同步。
- 数据：`data/ai_config.toml`（因子 / 卡组 / 种子）、`data/constants.json`、`data/scenario_matsuri.json`。
- 文档：`docs/mechanism.md`、`docs/glossary.md`、`docs/plan/scenario_dev_plan.md`、`docs/tests_overview.md`；
  提交准备时统一更新。
- 命令（仓根执行；同一 target 一次只跑一个 cargo；一律 `--offline`）：

  ```bash
  cargo fmt --all --check
  cargo test --workspace --release --offline
  cargo clippy --workspace --all-targets --offline -- -D warnings -A clippy::useless-borrows-in-formatting
  cargo run --release --offline --bin matsuri -- matsuri
  cargo run --release --offline --bin matsuri -- random
  cargo run --release --offline --bin matsuri -- manual
  ```

- 覆盖层只放临时目录 / `logs/`，`data/` 用软链；不要把临时配置写进仓内 `data/`。
- 不自动 `commit` / `push`；不碰引擎仓；用户确认整理前不跑会改代码的 `cargo fmt` / `cargo clippy`
  （`--check` 只读可跑）。
