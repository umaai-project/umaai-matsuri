# 测试一览（骏大祭 / matsuri）

> **前期验证以对话核对为主**：机制以 [`mechanism.md`](mechanism.md) 与 [`stubs.md`](stubs.md) 为准，不用测试代替机制确认。
> **环境与基准约定**：测试路径统一经 `matsuri_core::testing` 取仓根；**测试基准马娘 = 105902（目白多伯）**，106301 / 106302 仅作「铁心」专项验证（不作普适基线）。
> **CI 门禁**：`cargo clippy --workspace --all-targets --offline -- -D warnings`（仅 Windows 侧运行）、`cargo fmt --all --check` 与 `deslop-please`。
> **deslop 扫描**：按仓根 `.deslop.toml` 手工运行（只扫 Rust，`scripts/`、`benchmark/` 抑制；详见 [`../.deslop.toml`](../.deslop.toml)），暂未进 CI。

## 1. 分 crate 测试布局

| crate | 测试位置 | 定位与职责 |
|---|---|---|
| `matsuri-core` | 内联单测 + `tests/{data,mechanics,diag,search,restore,diag_advice_choice}.rs` | L1 单元、L2 仓内资产/配置、全流程机制集成、诊断静默、搜索与还原层守门 |
| `matsuri-sim` | `src/baseline.rs` 内联单测 | 基线 JSON 自检与合并标准误 × 相对容差判定（复跑入口在 `bench baseline`） |
| `matsuri-cli` | 内联单测 + `tests/{manual,explain,search}_smoke.rs` | 面板呈现纯函数 + 手动/解释/搜索三档冒烟 |
| `matsuri-ai` | 内联单测 | 帧解析（`charaEffects` / `storyId`）、按帧卡组选档、路由边界、回放顺序；原 `tests/{replay,advice_boundary,w2,layers,diag_advice_shift,diag_npc_roster}.rs` 只读仓外真机散帧，2026-11-16 删除 |
| `matsuri-tools` | `src/stat.rs` 内联单测 + `src/bin/*`（`bench` / `review` / `rhythm` 等）+ `tests/distilled_policy.rs` | 复盘统计口径（配对差 / 分位 / 命中判定）+ 性能与基线测量工具（用法见 [`perf_profiling.md`](perf_profiling.md)）；`distilled_policy` 锁同目录 `distilled.toml` 与 `--distilled-policy` 文件同分、零权重退化手写、参数互斥 |
| `matsuri-launcher` | `src/{update,startup,main,program}.rs` 内联单测 | 四频道白名单守卫、启动自检汇总、菜单编号、`program` 解压与 `.old` 回滚 |

## 2. 核心守门职责（索引到文件）

具体用例以源码为准，以下只列各测试文件的守门契约与边界：

- **单元测试（内联）**：
  - `core/src/{data,config,deck_config}.rs`：18 个顶层资产键校验、覆盖层边界（只认 `[pre]` / `[search]`；蒸馏权重走同目录 `distilled.toml`，权重须全部有限）、`[decks.*]` 深合并与 `variant` 显式约束。
  - `core/src/game/{roster,stage,advice,distribute,train,event,settle,mod,view,diag}.rs`：担当五步分配、5 阶段链、合宿顺延、空窗期（68–71）/ URA（72–77）、教诲 120 排列与 801 约束、融合窗口落地（预设逐位一致 / 重复落地幂等 / 实机抢先融合时顶替 base）、社交人头唯一性与吾妻补稽古位、双层训练公式、S6 事件链与变胖、発表会/本番/育成终了、`MatsuriView` 只读幂等、预览零克隆不串味。
  - `core/src/{search,trainer,value,game/traits}.rs`：单决策流随机字、自选比赛期限只搜比赛、`pt_favor_rate` 搜索分、手写守门链、溢出与末段稽古估值。
  - `cli/src/{panel,manual,online}.rs`：CJK 宽度、五维 >1200 显示压缩、图例配色、运气行期望/σ/档次渲染与按帧卡组选档行。
  - `tools/src/stat.rs`：命中判定三态与边界（`abs(delta) <= ci95`）、配对差跳过缺样本并按 `变动臂 − 对照臂` 计、不足两对不下结论、分位线性插值。
  - `tools/src/review/{eval,whatif}.rs`：**多决策面选帧**——同回合多帧 = 事件/训练/教诲三个面，`eval` 单点多面必须 `--seq`、`--all` 逐训练/教诲面各一点（**事件面 2026-11-17 起停用**）；`whatif` 教诲帧优先（special + ps=51）、事件面不支持、单候选回合直接拒绝。两者均无单测，靠真机局包手验；出行动作按 `group=390`、出行回合按命令回合与 `eval` 对齐。
  - `launcher/src/{update,startup,main,program}.rs`：频道白名单（`ai_data` 三表 / `ura_data` `.br` / `ura_plugins` `Plugins/*.zip`）、无状态自检汇总、Authenticode 白名单与失败逆序回滚。
- **L2 数据层（`core/tests/data.rs`）**：直接加载仓内 `data/` 资产与配置，在 `mdb` 重导出或配置异常时于早期拦截。
- **机制集成（`core/tests/mechanics.rs`）**：覆盖 78 回合全流程、赛程/合宿/URA 动作集分叉、比赛加成乘算与狄杜斯铁心、继承/小切/切者/心情盾、自选比赛义务区间与奖励、追加稽古端到端与手写确定性。
- **还原层（`core/tests/restore.rs`）**：帧 → 局面 round-trip，守卫 T0/T1 重建、`charaId = 0` 容错、越界项丢弃、`charaEffects` 142 病种映射与本地 `storyId` 事件优先覆盖。
- **诊断 / 搜索 / 冒烟**：
  - `core/tests/diag.rs`：`DiagGuard::suppress()` 静默不改变同种子终局。
  - `core/tests/search.rs`：小预算（`search_n=1`）全流程可复现、全关搜索退化手写档、`fork_for_rollout` 跳过 `event_log`。
  - `cli/tests/{manual,explain,search}_smoke.rs`：验证非 TTY 面板关键块、候选打分表（≥80 处）与 `[搜索]` 摘要输出；`cardtest` 同参数两次 diff 为空。

随机流专项的配对场景、实测反例和仓根复跑命令见[评审复核 §8](review_status_2026-11-23.md#8-随机流第一步离线-release-配对实验)。其中锁位 seed sweep 和局包 7084 在线回放需显式运行，不由默认测试覆盖。

### 手动 / 专项测试（`#[ignore]`，不进 CI）

- **教诲微基准**：`cargo test --release -p matsuri-core microbench_participant_advice -- --ignored`
- **搜索 N=1024 复现**：`cargo test --release -p matsuri-core search_1024_full_game_is_reproducible -- --ignored`
- **教诲分配实测**：`cargo test --release -p matsuri-core --test diag_advice_choice -- --ignored`（默认 10 局 × N=2048，可设 `MATSURI_ADVICE_GAMES` / `MATSURI_ADVICE_N`；基线结论见 [`benchmark/_local/advice_choice/RESULT.md`](../benchmark/_local/advice_choice/RESULT.md)）
- **启动器联网冒烟**：设 `MATSURI_SMOKE_INSTALL` 指向真实安装目录后跑 `cargo test -p matsuri-launcher smoke_live_inspect -- --ignored`

## 3. 基线与复跑

- 基线 JSON 位于 [`benchmark/baselines/`](../benchmark/baselines/README.md)（`input` 自持、`expect` 逐项容差、`meta` 不含路径），由 `matsuri-sim::baseline` 做合并标准误与相对容差判定。
- 复跑命令：`cargo run --release --bin bench -- baseline [--name a-normal] [--trainer hand|search|both] [--games N] [--sigma 3.0] [--rel 0.01]`（漂移时非零退出）。
- 数值占位或默认配置变更后须按 [`benchmark/baselines/README.md`](../benchmark/baselines/README.md) 手工重录，并在 [`changelog.md`](changelog.md) 记录影响面。

## 4. 基线快照与档间差

### 4.1 基线快照（2026-11-24 重录）

2026-11-24 重录：510 固有分移除（2026-11-23）+ 蒸馏权重（当时内联 `[distilled]`，2026-11-27 拆为独立 `data/distilled.toml`，数值不变）对手写档与搜索档 rollout 教师全链路生效；默认卡组不变，1 耐位 `30324`、默认 `pt_favor_rate = 1.931954`：

| 档 | 训练员 | 局数 | 均分 | σ | PT（含 Hint） | 实际 PT（不含 Hint） | 位移 vs 上轮（2026-11-17） |
|---|---|---:|---:|---:|---:|---:|---|
| `a-normal` | 手写 | 100 | 82709.0 | 2753.2 | 12258.4 | 9431.5 | +515.9 / +407.8 |
| `a-normal` | 搜索 N=8192 | 10 | 85907.7 | 1470.2 | 12746.2 | 9911.8 | −600.4 / +289.5 |
| `b-ideal` | 手写 | 100 | 87498.7 | 1892.5 | 12767.9 | 9995.0 | +576.2 / +349.3 |
| `b-ideal` | 搜索 N=8192 | 10 | 89966.3 | 756.2 | 13575.5 | 10752.8 | +232.1 / +206.1 |

> 「实际 PT」= `Uma.skill_pt`（终局面板「技能点」），不含 Hint 折算；「PT（含 Hint）」= `total_pt` = `floor(skill_pt + total_hints × 6.5)`，用于评分换算。

### 4.2 MCTS − 手写 的差值（非配对口径）

| 档 | 手写 | 搜索 | 差 | 合并标准误 | 相对 | PT 差 | 实际 PT 差 | 分数 σ |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| `a-normal` | 82709.0 | 85907.7 | **+3198.7** | 540.3 | +3.87% | +487.8 | +480.3 | 2753 → 1470 |
| `b-ideal` | 87498.7 | 89966.3 | **+2467.6** | 305.0 | +2.82% | +807.6 | +757.8 | 1893 → 756 |

- **历史配对参照**：2026-11-08 旧配置（`pt_favor_rate = 1.7271`）曾测得同种子配对差 `+5899.1 ±1126.6`。
- **`pt_favor_rate` 性质**：2026-11-08 扫测（1.0–3.0，N=2048 × 10 局）确认总分对 `favor` 不敏感（极差在 1 SE 噪声内），提高 `favor` 本质是**用面板分换 PT**（1.0 → 3.0 时 PT +590、非 PT 分 −801），当前默认由 GA 定为 `1.931954`。

> 历史决策、机制核对与基线变更流水统一记录于 [`docs/changelog.md`](changelog.md)。
