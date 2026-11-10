# 2026-11-09 交接：本轮已提交，下一步 P2

> 一次性文档：本次交接整份重写。本轮改动与未改项详见
> [`plan/round_2026-11-09.md`](plan/round_2026-11-09.md)。

## 1. 当前状态

- **仓**：`umaai-matsuri`（cwd）；引擎 `../umaai_next` 为 path 依赖，本轮两仓均有改动。
- **提交**：两仓最新 commit 已包含本轮修改；**未推送**（按纪律由用户手动 push）。
- **验收**：两仓 `cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo test --workspace --release --offline` 全绿。剧本 `cargo clippy` 仍需用户确认后才跑。
- **默认档**：`data/matsuri_config.toml` = GA 冠军 `g0269`（`pt_favor_rate=1.931954`、`search_n=8192`）；
  耐 12101 / 智 11102 各有完整独立档；默认测试马娘 **105902**；局外状态默认不注入。
- **搜索参数**：`search_group_size=1024`、`expected_search_stdev=3300`、
  `radical_factor_max=0`、`use_ucb=true`。
- **基线（2026-11-09 重录，seed0=73200）**：

| 档 | 手写 100 局 | 搜索 10 局（N=8192） |
|---|---|---|
| `a-normal`（105902，无 buff） | 83793.9 / PT 11731.4（σ 3060.6） | 87557.1 / PT 12620.1（σ 1612.9） |
| `b-ideal`（106302，小切 + 盾 2） | 88385.3 / PT 12314.2（σ 2225.0） | 91535.7 / PT 13293.8（σ 765.1） |

- 本轮守门 ④ / 锁位 CRN 修复的 A/B：手写 +369.4 ±297.9（t 1.24）；搜索 −196.3 ±424.8（t −0.46，噪声内）。

## 2. 本轮已完成（摘要）

1. 引擎：`finish_root_stage` 默认钩子、`ActionResult` 样本化（去 100k 直方图）、
   `to_flags` 同步 `Ill::BAD_TRAINER`。
2. 剧本机制：教诲窗口按真机时机拆分（T2/T11/T35/T59 在 `Begin`，T23/T47 在 `End`）。
3. 在线：教诲 / 事件搜索按帧 `isAfterTraining`（事件再叠加 checkPoint 是否已结算）设根阶段；
   事件帧注入 `event_queue`，在线事件搜索保留；搜索失败回落手写。
4. 小型正确性：`preview_training_layers` 变胖速度归零、`pt_value` 切者/小切系数。
5. 守门 ④：存在 0% 失败率训练（901 不屈 Lv10 等）时放行打分表，不再无条件强制休息。
6. 演目满槽锁卡：锁位分支按「未锁位时 `distribute_person` 的抽签次数」推进共享随机流，
   不改落位，恢复 t+1 的 CRN 配对；人头级独立流因搜索 A/B 回归已回退。
7. 基线、README、glossary、changelog、机制、测试一览同步；`docs/plan/round_2026-11-09.md` 记录全部
   已改项与已定位未改项。

## 3. 下一步（按优先级）

1. **UCB 16 候选专项审计**：当前只有 T32 的 8 候选审计（8/8 命中真值、regret 0，
   但分配仍 `1024×7|8192×1`）。先补教诲窗口固定根审计，再决定是否加最小样本 floor /
   Successive Halving；不要直接改预算。
2. **教诲估值窗口化**：`value.rs` 的 `remaining` / `camp_turns` 改为「到下一教诲窗口」；
   同步下调 `advice.route` 硬先验（当前 392–709）。完成后重录搜索基线。
3. **基策与方差**：在线决策种子按决策类型 / 窗口加盐（同回合不同决策点不要共用根种子；
   同一决策重复帧仍保持同种子）。
4. **属性估值**：终局期望落点边际替代当前点位边际；`pt_favor_rate` 随剩余回合退火。
5. **插件端**：按 `URA_Plugins/handoff-2026-11-08.md` §7 修 `isAfterTraining` 粘连
   （改响应确认或新增 `actionPhase`），再收紧在线阶段推断。

## 4. 关键落点

| 内容 | 文件 |
|---|---|
| 教诲窗口时机 / 阶段收尾 | `crates/matsuri-core/src/game/{stage,settle,traits}.rs` |
| 搜索训练员（事件 / 教诲 / 训练） | `crates/matsuri-core/src/search.rs` |
| 在线阶段推断 | `crates/matsuri-ai/src/online.rs`、`session.rs` |
| 估值 | `crates/matsuri-core/src/value.rs`、`game/train.rs` |
| 引擎钩子 / 结果统计 / 协议 | `../umaai_next/crates/umasim/src/search/{searchable,flat_search,result}.rs`、`umaai/src/protocol/base.rs` |
| 基线 | `baselines/*.json`、`baselines/README.md` |
| 本轮记录 | `docs/plan/round_2026-11-09.md` |

## 5. 关键命令

```bash
# 剧本仓验收
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo test --workspace --release --offline

# 引擎仓验收
cargo test --workspace --release --offline   # 在 ../umaai_next

# 基线（手写 / 搜索）
RAYON_NUM_THREADS=24 ./target/release/bench game --games 100 --seed 73200 \
  --warmup 0 --uma 105902 --qie none --shield 0 --search-n 0
RAYON_NUM_THREADS=24 ./target/release/bench game --games 10 --seed 73200 \
  --warmup 0 --uma 105902 --qie none --shield 0

# 在线回放（小预算快速核对）
MATSURI_SEARCH_N=256 ./target/release/matsuri online --replay logs/replay_t23 --trainer search --json
```

- 同一 `target` 目录一次只跑一个 cargo；脚本 / 测量输出放 `logs/_scratch/`，不进 `data/`。
- 种子块：73200–73299 已用于本轮基线；下一块 **73300 起**。
- 提交纪律：不自动 push；提交前同步 `README` / 文档 / 基线，剧本侧 clippy 由用户确认后跑。

## 6. 已知风险 / 近似

1. 在线事件帧只含当前事件，同 stage 后续事件链不可知；rollout 只排空当前注入队列。
2. 在线 T2 事件若发生在教诲授予前，rollout 会用 fallback 训练员做一次未来教诲分配。
3. 教诲 `Begin` 后 `roll_kensan` 与真机改派先后顺序尚无 T35 帧实证，先按「改派后重掷」。
4. `isAfterTraining` 有粘连历史，在线事件搜索失败一律回落手写；插件端修复前阶段推断仍可能偏。
5. 引擎里已在 matsuri 打过补丁的项（`apply_event` / lucky / 训练表上下文 / StrategyRng /
   原始分布 / `Session`）仍未回引擎，本轮按约定暂缓。
