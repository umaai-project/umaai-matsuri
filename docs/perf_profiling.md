# 性能分析与优化（手写档 + MCTS）

> 记录 2026-11-05 两轮性能剖面方法、工具、改动与配对测量。方法沿用旧仓 `perf_profiling` 的结论
> （专用测试优先于 pprof；一次只改一个点），并按引擎 `docs/scenario/perf_plan.md` 的 O0–O4 流程执行。
> 本文只覆盖本仓（骏大祭）的手写决策链与 FlatSearch rollout。
>
> - **第一轮**（§1–§10，2026-11-05 凌晨）：预览零克隆 / 教诲查表 / 决策原因惰性化。
> - **第二轮**（§11，2026-11-05 下午）：机制侧 diag 与分配、rollout 事件明细、事件队列、
>   估值层参与教诲去重。

## 1. 摘要

- 单次 rollout 的成本里，**手写决策链占 90%**（T32 固定根：2.46 ms，其中机制部分 0.26 ms）。
- 决策链最大单项是**每个候选一次整局克隆**：一次决策 5 个训练候选 + 8 个动作候选，逐候选克隆
  约占单次决策的 90%。改为「零克隆预览」后，单次决策 32.3 μs → 2.9 μs。
- 教诲分配的 120 个候选实际只有 5×5 种槽位组合，槽位价值与 801 目标演目与候选无关：
  一次算表后查表，单次分配决策 44–58 μs → 9–13 μs。
- 端到端（同种子、同配置、配对）：

| 测量 | 优化前 | 优化后 | 变化 |
|---|---:|---:|---:|
| 手写整局（30 种子均值） | 2.92 ms | 1.14 ms | **-61%** |
| 固定根单次 rollout（串行，T32） | 2.46 ms | 0.64 ms | **-74%** |
| 固定根 N=256（16 线程） | 420 ms | 134 ms | **-68%** |
| 整局 N=256（seed 61444） | 29.54 s | 12.66 s | **-57%** |
| 整局 N=1024（seed 61444） | 55.69 s | 24.70 s | **-56%** |

- 所有配对测量的决策 / 终局逐位一致：30 种子整局 CSV（score / PT / turn）、固定根 N=256 的
  best 下标与各候选均值、整局 N=256 与 N=1024 的终局 score / PT 全部相同。

第二轮（§11）把热点从「手写决策链」转到**机制侧**，同轮配对结果：

| 测量 | 第二轮前 | 第二轮后 | 变化 |
|---|---:|---:|---:|
| 手写整局（30 种子均值） | 0.528 ms | 0.385 ms | **−27%** |
| 串行 rollout（T32 → 终局） | 329 μs | 245 μs | **−26%** |
| 固定根 N=256（16 线程） | 86.4 ms | 70.6 ms | **−18%** |
| 整局 N=256（seed 61444） | 7.40 s | 5.93 s | **−20%** |
| 整局 N=1024（seed 61444） | 13.32 s | 11.42 s | **−14%** |
| 单次 rollout 分配次数 / 字节 | 4080 / 162.9 KB | 2085 / 85.5 KB | **−49% / −47%** |

> 绝对耗时只声明当轮：同一基线提交在重启前测到整局 N=256 = 12.4 s、重启后 7.4 s，机器热状态
> 差异远大于改动本身；跨会话只比「同轮配对」的方向与幅度。

## 2. 工具：matsuri-tools 的 bench

新增单 bin [bench](../crates/matsuri-tools/src/bin/bench.rs)（不引入新依赖，只挂仓内 path 依赖）：

| 子命令 | 用途 | 输出 |
|---|---|---|
| `bench game --games N --seed S [--search-n M]` | 整局耗时 + 终局 CSV | `seed,score,pt,turn,elapsed_us` + 均值行 |
| `bench micro --turn T --iters N` | 固定回合的决策链分段 | 各段 ns/op（克隆 / 训练预览 / 资源预览 / 整决策 / 搜索分）|
| `bench trace --turn T` | rollout 内逐决策耗时 | `turn,actions,us,advice` |
| `bench root --turn T --search-n M --repeat R` | 固定根搜索 | 每轮耗时、planned / failed、ns/rollout、best |
| `bench stages --turn T [--action I]` | 单次 rollout 按阶段机拆分 | 各阶段 us / 占比 / 调用次数 + 训练员决策耗时 |
| `bench parts --turn T --iters N` | 分布 / 事件生成 / 列动作单点 | ns/op |
| `bench flat --turn T --rollouts N --threads M` | 平铺线程池 rollout（并行上限对照） | 总耗时、ns/rollout、校验和 |
| `bench allocs --turn T [--action I]` | 单次 rollout 分配计数 | 阶段分配数 / 字节 / 训练员分配数 |

测量口径：

- 不装 logger；`diag!` 在 `enabled()==false` 时连参数都不求值，因此测到的是纯计算成本。
- `micro` 是热循环口径（同局面反复调用），绝对值低于真实 rollout 中的冷调用；跨版本比较用同一口径。
- `root` 的 `rollout_noop` 用「永远选 0」的空训练员，把引擎机制成本与训练员决策成本拆开。
- 固定根用生产配置装配（`build_game + with_config`），不能用 `testing::sample_game`（它是默认档）。
- 线程数用 `RAYON_NUM_THREADS` 在进程启动前固定；radical 取配置层（当前 0）。
- `stages` / `allocs` 的基线是「同一固定根的下一次 rollout」：阶段耗时含训练员决策，`Train` 段
  扣掉 `trainer.action_us` 才是机制成本。
- `flat` 用 `std::thread` 每线程串行 rollout，用来区分「引擎嵌套 rayon 调度」与「工作负载本身」
  的并行上限；`allocs` 用 bin 内的计数分配器（关闭时每次分配多一次 `Relaxed` 读）。

## 3. 环境

| 项 | 值 |
|---|---|
| 工具链 | rustc 1.99.1（2026-10-13） |
| 构建 | workspace Release：`opt-level=3` / `codegen-units=1` / `lto=thin` / `debug=true` |
| 数据 / 配置 | 仓内 `data/` 默认层；`radical_factor_max=0` |
| 种子 | 整局 61444 起；固定根 61444 |
| 线程 | 固定根 16；整局为 rayon 默认（本机 24 核） |
| 机器 | 本机当轮观测，不作为跨机承诺 |
| 测量快照 | 2026-11-05 01:00–01:20（HEAD `21b278c` + 本轮性能改动） |

> 快照之后工作树另有一处**不属于本轮**的 T47 落位先验调整（会改变终局分数）。手写 30 种子
> 在该行同步到基线后复验仍逐位一致（3.04 → 1.15 ms，均值同为 78149.7 / 11138.0）；
> MCTS 整局配对数字对应上面的快照，深度复验按用户指示暂停。

## 4. 热点定位

### 4.1 一次 rollout 的构成（T32 固定根，串行）

| 成分 | 优化前 | 优化后 | 说明 |
|---|---:|---:|---|
| 单次 rollout 总计 | 2.46 ms | 0.64 ms | fork → 根动作 → 逐回合 5 阶段 → 终局分 |
| 其中：机制（空训练员） | 0.26 ms | 0.26 ms | 分布 / 事件 / 结算，引擎与剧本机制成本 |
| 其中：手写决策 | 2.20 ms | 0.38 ms | 每回合 1–3 个决策点 |

### 4.2 决策链分段（T40，热循环）

| 段 | 优化前 | 优化后 |
|---|---:|---:|
| 整局克隆 `clone()` | 2.97 μs | 2.97 μs（不再进决策链） |
| 训练预览 ×5 | 14.41 μs（2.88 μs/次） | 1.62 μs（0.32 μs/次） |
| 资源预览 ×8 | 19.00 μs（2.38 μs/次） | 0.15 μs（0.02 μs/次） |
| `select_action` | 32.26 μs | 2.91 μs |
| `explain_action`（两次模型） | 65.09 μs | 5.00 μs |

### 4.3 克隆为什么贵

整局克隆 2.97 μs 中约 77% 是 `event_log`（≤32 条、每条两个 String）与 `event_queue`
（`Vec<EventData>` 深拷贝）；清空这两项后克隆降到 0.67 μs。决策链从不读 `event_log`，
事件队列也只在事件搜索时用——这正是「预览不该克隆整局」的直接证据。

### 4.4 rollout 内决策分布（T32 → 终局，优化后）

- 64 个决策点合计 0.284 ms；优化前为 1.986 ms。
- 普通动作决策约 7 μs/次（8 候选）；教诲分配决策 12–13 μs/次（120 候选）；
  单候选 / 守门短路约 0.1–0.8 μs。
- 教诲分配在优化前是 44–58 μs/次，是 rollout 内最大的单个决策点。

## 5. 改动清单

全部改动保决策与终局逐位一致（RNG 调用、遍历顺序、浮点加法顺序均未变）。工作树另含一处
T47 落位先验调整（不属本轮），配对测量时基线已同步该行，因此不参与等价性归因。

### 5.1 预览零克隆

- 训练预览原来克隆整局只为设置 `keiko_settling` 一个 bool（读取点只有 `training_basic_table`
  与 `calc_training_value` 两处）。改为 `AtomicBool` 包装：预览时 `replace` 切换、结束恢复
  （错误路径也恢复）。用原子量而不是 `Cell`，因为并行搜索要求局面 `Sync`。
- 资源预览的写入面只有 `acts.gauge` / `acts.level`。把演目账本操作抽成自由函数
  （`act_gauge_add` / `act_gauge_add_lowest` / `act_level_add` / `act_lowest_gain`），
  正式结算与预览共用同一实现；预览只在局部 `ActLedger` 副本上模拟，不再克隆局面。
  `gain_training_gauge` 也改为「局部账本 + `training_gauge_on` 一次写回」，避免 `&self` 与
  `&mut self.acts` 的借用冲突。

### 5.2 教诲分配查表

120 个候选 = 5 槽 × 5 演目的组合；`slot_value` 只依赖 `(act, source)`，801 目标演目只依赖
局面。首次分配打分时惰性建立 `slot[5][5]` 与 `target_801`，之后按 `effective_assignment`
的 act 0→4 顺序查表累加；落位先验与 801 swap 逻辑不变。

### 5.3 决策原因惰性化

`choose_action` 原来无条件 `format!` 决策原因，`select_action`（跑局与 rollout）直接丢弃。
新增 `reason(explain, format_args!(...))`：只在 `explain_action` 时构造字符串。守门分支与文案不变。

### 5.4 搜索层小改

- `search_advice` 的剪枝复用 `score_actions` 的排序首项作为手写 argmax，删除第二次
  `select_action`（省一次 Model + 120 次方案打分）；教诲分支的两种 argmax 语义一致。
- `search_train` 不再 `actions.to_vec()`，直接传候选切片（少一次 K 元素克隆）。

### 5.5 口径变更：MCTS 激进度 = 0

按用户拍板，`data/ai_config.toml` 的 `radical_factor_max` 由 50.0 改为 0.0：排序口径退化为
算术均值（引擎 `weighted_mean` 在 `radical_factor < 1e-6` 时直接返回均值，不再扫直方图）。
这是**口径变更**，旧文档里的搜索单局数字（83326 / 83519 等，radical=50）不再可比；
新参考见 §6.3。

## 6. 配对测量

### 6.1 手写整局（30 种子，seeds 61444–61473）

| 版本 | 均值 ms/局 | 总耗时 | score 均值 | PT 均值 |
|---|---:|---:|---:|---:|
| 优化前 | 2.92 | 87.6 ms | 78100.1 | 11074.9 |
| 优化后 | 1.14 | 34.1 ms | 78100.1 | 11074.9 |

逐种子 `score / pt / turn` 逐位一致（CSV 排除 `elapsed_us` 后 diff 为空）。

### 6.2 固定根搜索（T32，8 候选，N=256，16 线程，各 3 轮）

| 版本 | 中位耗时 | ns/rollout | best | 候选均值 | planned |
|---|---:|---:|---:|---|---:|
| 优化前 | 420 ms | ~200 μs | 5 | 一致 | 2048 |
| 优化后 | 134 ms | ~65 μs | 5 | 一致 | 2048 |

### 6.3 整局搜索（seed 61444，本机默认 24 线程）

> 以下为测量快照口径；快照后的并发策略改动未进入这组数字，MCTS 深度复验暂停。

| 档位 | 版本 | 耗时 | score | PT |
|---|---|---:|---:|---:|
| N=256 | 优化前 | 29.54 s | 83381 | 11643 |
| N=256 | 优化后 | 12.66 s | 83381 | 11643 |
| N=1024 | 优化前 | 55.69 s | 84875 | 12097 |
| N=1024 | 优化后 | 24.70 s | 84875 | 12097 |

N=1024 的收益小于固定根，是因为 UCB 的停止判据是「任一候选计划次数达到 search_n」：
N=256 时首组后即停（每候选 256），N=1024 只追加到最大值，总 rollout 数增长远小于 4 倍，
每次搜索的固定开销（每候选 400 KB 直方图分配等）占比更高。

## 7. 验收

- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --offline -- -D warnings` 绿。
- `cargo test --workspace --release --offline`：220 例全绿（含新增 2 例预览不串味守门）。
- 行为一致性：30 种子整局 CSV；固定根 N=256；整局 N=256 / N=1024 的终局分数均逐位一致。
- 新增测试：`previews_do_not_leak_between_actions`、`preview_training_restores_keiko_context`。

## 8. 复现

```bash
# 构建
cargo build --release --offline -p matsuri-tools

# 手写整局（30 种子）
./target/release/bench game --games 30 --seed 61444

# 决策链分段 / rollout 内分布
./target/release/bench micro --turn 40 --iters 3000
./target/release/bench trace --turn 32

# 固定根搜索（16 线程）
RAYON_NUM_THREADS=16 ./target/release/bench root --turn 32 --search-n 256 --repeat 3

# 整局搜索
./target/release/bench game --games 1 --seed 61444 --warmup 0 --search-n 256
```

配对测量时基线版放在并列 worktree（同引擎 path 依赖），两版交替运行、各独立启动。
测量 CSV 落在 `logs/perf/`（不入仓）。

## 9. 后续可做

| 项 | 现状 / 预估 | 备注 |
|---|---|---|
| rollout 的机制成本 | ~~0.26 ms/次~~ → 第二轮后 245 μs 中约 160 μs，仍占 2/3 | 分布 / 事件管线；已做 diag 惰性化与分配削减，剩余大头在引擎 |
| 事件队列克隆 | ~~每事件一次 `EventData` 深拷贝~~ **第二轮已改 `mem::take` 占位** | 见 §11.3 C 组 |
| `Model::new` | 每次决策约 0.2–0.3 μs（按代码体量实测口径修正） | 发布会 buff / 5 种卡判定可缓存，但失效点分散、收益小（第二轮评估后未做） |
| 引擎 `apply_event` 的 diag 载荷 | 每事件 4–8 次分配，diag 关闭仍执行 | 属引擎侧，走 `repo_form.md` §3 钩子流程 |
| 引擎训练卡效果表 / `deyilv` 访问器 | 每训练重复 3–5 次 `calc_training_effect` | 属引擎侧钩子，本仓不偷偷缓存（会与引擎公式分叉） |
| 引擎每候选 400 KB 直方图 | radical=0 时无人读直方图，纯分配 | 属引擎侧，走钩子流程，不在本仓改 |
| 低预算档（N≤16）固定开销 | 直方图与搜索 setup 占比上升 | 需要时另测，不与本报告混比 |

## 10. 产物说明

- 工具源码：`crates/matsuri-tools/src/bin/bench.rs`（入仓）。
- 测量 CSV / 基线 worktree：`logs/perf/` 与仓外临时 worktree（不入仓）。
- 基线二进制在测量结束后可整体删除；本报告只保留当轮数字与相对变化。
## 11. 第二轮（2026-11-05 下午）：机制侧分配与估值去重

### 11.1 口径与环境

- 本机同一台（Intel Ultra 9 275HX / 24 线程），rustc 1.99.1，workspace Release（配置未改）。
- 基线 = **core 改动前**的构建（bench 工具已含本轮新增子命令；core 源码 = HEAD `57c646d`），
  二进制留档 `logs/perf/base/bench`；候选 = 本节改动。双方交替先跑、各独立启动；耗时只声明当轮。
- 该轮机器在重启后测量：**同一基线提交**重启前整局 N=256 = 12.4 s、重启后 7.4 s。跨会话
  绝对值不可比，下文只认同轮配对的方向与幅度。

### 11.2 热点归因（改动前）

单次 rollout（固定根 T32 → 终局，串行，308–360 μs）按阶段机拆分：

| 阶段 | 耗时 | 占比 | 说明 |
|---|---:|---:|---|
| Start/Begin | 22–24 μs | 7% | 回合结构 + 事件生成 + 回合开始事件 |
| Train | 161–176 μs | 50% | 其中手写训练员决策 112–125 μs，其余是动作结算 |
| Distribute | 54–59 μs | 17% | 人头分布 + 灵感 |
| AfterTrain | 57–62 μs | 18% | 事件队列结算（含事件选项估值） |
| End | 20–27 μs | 7% | 回合末结算 / 教诲窗口 |
| next + fork + root | ~8 μs | 2% | 阶段切换与 rollout 分支 |

机制（非训练员）合计约 2/3。这与第一轮文档「手写决策链占 90%」并不矛盾：后者测的是
「每候选整局克隆」时代的决策链，零克隆预览落地后，机制重新成为最大块。

分配归因（计数分配器，单次 rollout）：

| 阶段 | 分配次数 | 字节 |
|---|---:|---:|
| Begin | 405 | — |
| Train | 1271 | — |
| Distribute | 486 | — |
| AfterTrain | 1898 | — |
| End | 18 | — |
| 合计 | **4080** | **162.9 KB** |

手写训练员 `select_action` 内部只有 42 次分配 / 48 次调用——分配几乎都在机制侧，
且大头是**诊断文本与事件明细的格式化**，不是决策算术。

并行上限：固定根 N=256（16 线程）144–166 ms。用 `bench flat`（std::thread，每线程串行
rollout）复测同一负载，8 / 24 线程都只有约 3.8×；同机纯计算 24 线程约 7.7×。结论是
**工作负载内存 / 缓存受限**，不是引擎嵌套 rayon 调度，本仓不改引擎。

### 11.3 改动清单

四组改动全部保持决策与终局逐位一致（RNG 调用次数与顺序、遍历顺序、浮点加法顺序均未变）。

| 组 | 内容 | 落点 |
|---|---|---|
| A | 诊断载荷惰性化 + Distribute 临时 Vec 去分配 | `game/{distribute,train,action,diag}.rs` |
| B | rollout 静默事件明细 | `game/{mod,traits}.rs` |
| C | 事件队列取走占位（去每事件深拷贝） | `game/stage.rs`、`search.rs` |
| D | 估值层参与教诲 / 闪彩一次算好向下传；教诲分配单遍最大 + 先验查表；出行估值去临时分配 | `game/{train,traits}.rs`、`value.rs`、`trainer.rs` |

**A 组**：`diag!` 的参数本来只在运行时开关打开时求值，但若干调用点先在宏外构造了诊断文本。
本轮把人数 / 闪数、锁定担当列表、训练标签、训练后摘要、演目槽变化、比赛 PT 备注都改成
`Display` 包装或直接写进宏参数，日志关闭时零分配；`distribute_round` 的人数 / 闪数数组、
`distribute_hints` 的灵感加成表与卡面加成表、理事长 / 记者 / 吾妻 / 分身的候选列表改为
定长栈数组或按需现算，随机抽签的次数与参数不变。

**B 组**：`MatsuriGame` 增加 `record_events`（默认 `true`），`fork_for_rollout` 置 `false`。
rollout 局面跑完即弃，`event_log` 无人消费；关闭后 `apply_event` 跳过
`ActionValue::to_string()`、状态位文本、事件名克隆与记录追加。事件日志只在手动面板 / 前端
`MatsuriView` 里读，正常整局、解释模式与离线工具不受影响。

**C 组**：`run_event_queue` 与 `search_event` 的续跑循环用 `std::mem::take` 取走当前槽
（留 `EventData::default()` 占位）。事件搜索的 fork 只检查 `event_queue.get(cursor).is_some()`
并从 `cursor + 1` 继续，不读当前槽内容；队列在循环结束时统一清空。错误路径会留下占位事件，
此时局面立即中止、无人读取。

**D 组**：同一次训练候选的 `participant_advice` / `shining_count` 原来各算 2–3 遍
（失败率、训练值、羁绊估值各一遍），改为 `preview_training_full` 一次算好后传给
`training_upper_with` / `calc_training_value_inner` / `calc_training_failure_rate_inner` 与
`training_friendship`。教诲分配由「120 项排序取首项」改为单遍取最大（并列取最小下标；
方案分不含 NaN），落位先验按「槽内容 × 演目」预建 5×5 命中表，候选侧只做加法与计数。
友人出行段事件与普通出行事件列表改为借用 / 单遍累加。

### 11.4 配对测量（同轮交替）

| 测量 | 第二轮前 | 第二轮后 | 变化 |
|---|---:|---:|---:|
| 手写整局（30 种子均值） | 0.528 ms | 0.385 ms | **−27%** |
| 串行 rollout（T32 → 终局） | 329 μs | 245 μs | **−26%** |
| 固定根 N=256（16 线程，中位） | 86.4 ms | 70.6 ms | **−18%** |
| 整局 N=256（seed 61444） | 7.40 s | 5.93 s | **−20%** |
| 整局 N=1024（seed 61444） | 13.32 s | 11.42 s | **−14%** |

阶段拆分（串行 rollout，改动前 → 改动后）：Begin 22 → 17.4、Train 168 → 123.9（其中训练员
决策 118 → 83.6）、Distribute 56 → 41.2、AfterTrain 59 → 36.8、End 23 → 13.8、合计 329 → 241 μs。

决策链微基准（T40 热循环）：`select_action` 1200 → 891 ns、`preview_training` 163 → 116 ns/次、
`explain_action` 2442 → 1837 ns。分配从 4080 次 / 162.9 KB 降到 2085 次 / 85.5 KB，其中
Distribute 阶段 486 → 8 次、AfterTrain 1898 → 929 次（剩余大头在引擎 `apply_event` 的诊断载荷）。

一致性：30 种子整局 CSV 的 `seed/score/pt/turn` 逐位一致（均值 78100.1 / PT 11074.9）；
固定根 N=256 的 best=5 与 8 个候选的 mean / wmean / planned 一致；整局 N=256（83381 / 11643）
与 N=1024（84875 / 12097）终局一致。

### 11.5 验收

- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --offline -- -D warnings`、
  `cargo test --workspace --release --offline`、`cargo doc --workspace --no-deps --offline` 全绿。
- 测试 221 通过 + 2 手动档（1024 整局复现、教诲微基准）。
- 新增：`rollout_fork_skips_event_log_without_changing_game`（记录开关不改变评分 / PT；
  `fork_for_rollout` 后事件日志为空）、`microbench_participant_advice`（`#[ignore]`，给出
  `participant_advice` 20 ns/op、`advice_slot_effect` 10 ns/op、`shining_count` 1.2 ns/op，
  是 D 组「一次算好向下传」的立项依据）。

### 11.6 下一轮候选

- **引擎侧**（需走 `repo_form.md` §3 钩子流程）：`BaseGame::apply_event` 的诊断载荷
  （`value.to_string()` + `format!` + `join`，diag 关闭仍执行，每事件 4–8 次分配）；
  `default_calc_training_buff` 的逐卡效果表缓存；`deyilv` 专用访问器；每候选 400 KB 直方图。
- **本仓剩余**：Train 段仍有约 890 次分配 / 45 次训练（灵感事件构造与事件名拼接）；
  Begin 241 次。单点收益都在 1–3% 量级，需按 §11.4 的同一口径逐项配对。
- **复核**：`Model::new` 按代码体量与微基准判断只有 0.2–0.3 μs，缓存失效点分散，暂不立项。
