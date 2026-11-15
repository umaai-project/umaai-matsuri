# 性能分析与优化历史归档（2026-11-05 至 2026-11-07）

> 本文归档 2026-11-05 至 2026-11-07 五轮性能剖面与配对测量快照。
> 当前 `bench` 工具用法、测量口径、性能红线与已知热点见 [docs/perf_profiling.md](../../../docs/perf_profiling.md)。
> 注：文中提及的临时基线二进制路径（如 `logs/perf/base/`）已在 2026-11-11 清理移除，见 [benchmark/README.md](../../README.md)。

## 第一轮（2026-11-05 凌晨）：预览零克隆 / 教诲查表 / 决策原因惰性化

### 环境与快照

- 工具链：rustc 1.99.1（2026-10-13），workspace Release（`opt-level=3` / `codegen-units=1` / `lto=thin` / `debug=true`）。
- 快照：2026-11-05 01:00–01:20（HEAD `21b278c` + 本轮性能改动）；`radical_factor_max=0`；整局 61444 起；固定根 61444（16 线程）。
- 注：快照后工作树另有一处不属于本轮的 T47 落位先验调整，手写 30 种子同步后复验仍逐位一致（3.04 → 1.15 ms，均值同为 78149.7 / 11138.0）。

### 热点定位

- **单次 rollout（T32 固定根，串行）**：总计 2.46 ms → 0.64 ms（机制 0.26 ms 不变，手写决策 2.20 ms → 0.38 ms）。
- **决策链分段（T40，热循环）**：
  - `clone()`：2.97 μs（不再进决策链；其中约 77% 为 `event_log` 与 `event_queue` 深拷贝）
  - 训练预览 ×5：14.41 μs → 1.62 μs
  - 资源预览 ×8：19.00 μs → 0.15 μs
  - `select_action`：32.26 μs → 2.91 μs
  - `explain_action`（两次模型）：65.09 μs → 5.00 μs
- **rollout 内决策分布（T32 → 终局）**：64 个决策点 1.986 ms → 0.284 ms（教诲分配 44–58 μs/次 → 12–13 μs/次；普通动作 ~7 μs/次）。

### 核心改动

- **预览零克隆**：`keiko_settling` 改 `AtomicBool`（`KeikoSettling`）`replace` 切换；演目账本操作抽为自由函数，资源预览只在局部 `ActLedger` 副本模拟；`gain_training_gauge` 改为局部账本 + `training_gauge_on` 一次写回。
- **教诲分配查表**：惰性建立 `slot[5][5]` 与 `target_801`，按 `effective_assignment` 查表累加。
- **决策原因惰性化**：新增 `reason(explain, format_args!(...))`，仅 `explain_action` 构造字符串。
- **搜索层小改**：`search_advice` 剪枝复用 `score_actions` 首项；`search_train` 传候选切片去 `to_vec()`。
- **口径变更**：`data/ai_config.toml` 的 `radical_factor_max` 由 50.0 改为 0.0。

### 配对测量

| 测量 | 优化前 | 优化后 | 变化 |
|---|---:|---:|---:|
| 手写整局（30 种子，seeds 61444–61473） | 2.92 ms | 1.14 ms | **-61%** |
| 固定根单次 rollout（串行，T32） | 2.46 ms | 0.64 ms | **-74%** |
| 固定根 N=256（16 线程，中位） | 420 ms | 134 ms | **-68%** |
| 整局 N=256（seed 61444，24 线程） | 29.54 s | 12.66 s | **-57%** |
| 整局 N=1024（seed 61444，24 线程） | 55.69 s | 24.70 s | **-56%** |

- 一致性：30 种子（78100.1 / PT 11074.9）、固定根 N=256（best=5）、整局 N=256（83381 / 11643）与 N=1024（84875 / 12097）逐位一致。

---

## 第二轮（2026-11-05 下午）：机制侧分配与估值去重

### 口径与热点归因（改动前）

- 基线 HEAD `57c646d`（本轮重启后机器测量：同一基线重启前 N=256 为 12.4 s、重启后 7.4 s）。
- 串行 rollout 阶段占比（308–360 μs）：Start/Begin 22–24 μs（7%）、Train 161–176 μs（50%，训练员 112–125 μs）、Distribute 54–59 μs（17%）、AfterTrain 57–62 μs（18%）、End 20–27 μs（7%）、切换 ~8 μs（2%）。
- 单次 rollout 分配：4080 次 / 162.9 KB（Begin 405、Train 1271、Distribute 486、AfterTrain 1898、End 18）。

### 核心改动（A–D 组）

- **A 组（`game/{distribute,train,action,diag}.rs`）**：`diag!` 外参数改为 `Display` 包装或宏内构造；`distribute_round` / `distribute_hints` 等临时列表改为定长栈数组或按需现算。
- **B 组（`game/{mod,traits}.rs`）**：`MatsuriGame` 增 `record_events: bool`，`fork_for_rollout` 置 `false`，rollout 跳过 `event_log` 格式化与追加。
- **C 组（`game/stage.rs`、`search.rs`）**：`run_event_queue` / `search_event` 用 `std::mem::take` 取走当前槽占位，去每事件深拷贝。
- **D 组（`game/{train,traits}.rs`、`value.rs`、`trainer.rs`）**：`preview_training_full` 一次算好 `participant_advice` / `shining_count` 向下传；教诲分配改单遍取最大 + 5×5 先验表；出行估值去临时分配。

### 配对测量（同轮交替）

| 测量 | 第二轮前 | 第二轮后 | 变化 |
|---|---:|---:|---:|
| 手写整局（30 种子均值） | 0.528 ms | 0.385 ms | **−27%** |
| 串行 rollout（T32 → 终局） | 329 μs | 245 μs | **−26%** |
| 固定根 N=256（16 线程，中位） | 86.4 ms | 70.6 ms | **−18%** |
| 整局 N=256（seed 61444） | 7.40 s | 5.93 s | **−20%** |
| 整局 N=1024（seed 61444） | 13.32 s | 11.42 s | **−14%** |
| 单次 rollout 分配次数 / 字节 | 4080 / 162.9 KB | 2085 / 85.5 KB | **−49% / −47%** |

- 阶段变化：Begin 22 → 17.4、Train 168 → 123.9（训练员 118 → 83.6）、Distribute 56 → 41.2、AfterTrain 59 → 36.8、End 23 → 13.8；微基准 `select_action` 1200 → 891 ns、`preview_training` 163 → 116 ns、`explain_action` 2442 → 1837 ns。终局与第一轮逐位一致。

---

## 第三轮（2026-11-05 晚）：引擎侧诊断载荷惰性化

### 改动与配对测量

- 基线：引擎仓 `a8859e0` + 剧本仓 `7252b84`，powersave。
- 引擎改动（默认行为等价）：`umaai-core/src/diag.rs` 新增编译期且运行时开关查询出口；`umasim/src/game/base/mod.rs` 对事件执行 / 训练后处理 / 选项结算三处诊断载荷整段门控。剧本侧零改动（曾试做 `deyilv` 面板快路径因收益仅 0.7% 撤回）。

| 测量 | 基线 | 第三轮 | 变化 |
|---|---:|---:|---:|
| 单次 rollout 分配次数 / 字节 | 2085 / 87.6 KiB | 1017 / 54.1 KiB | −51% / −38% |
| 串行 rollout（T32→77，`stages`） | 244.0 μs | 211.4 μs | −13% |
| 手写整局（30 种子均值） | 0.412–0.421 ms | 0.347 ms | −15% |
| 整局 N=256，16 线程 | 6.72 s | 6.44 s | −4.2% |
| 整局 N=1024，16 线程 | 12.53 s | 11.61 s | −7.3% |
| 固定根 N=256，16 线程 | 72.6 ms | 70.5 ms | −3%（抖动内） |
| `flat` 16 线程 μs/rollout | 35.8 | 35.2 | −1.5%（抖动内） |

- 分配分项：Begin 241 → 100、AfterTrain 929 → 21、Train 890 → 871、End 15、Distribute 8。
- 对照结论：`LD_PRELOAD=libjemalloc` 无提升（6.05 → 5.92 s）；`MALLOC_ARENA_MAX=1` 24T 劣化 ~7 倍。

---

## 第四轮（2026-11-05 晚）：pprof 剖面接入与诊断 guard 假共享修复

### 定位与改动

- 接入 `matsuri-tools` 的 `profile` 特性（`pprof 0.15`）。
- 16T `flat` 剖面前二为 `Data::get`（13.6%）与 `DiagGuard::suppress`（8.9%）；`data::GLOBAL`（`0x3c4bc0`, 长 `0x228`）与 `DIAG_ENABLED`（`0x3c4de8`）同处一条 64B 缓存行，每 rollout ~1.6k 次 guard 在关闭态仍执行 store，导致多核缓存行失效（推翻第二/三轮关于“内存受限 / 6× 并行上限”的归因）。
- 引擎改动（`umaai-core/src/diag.rs`）：`suppress()` 仅在开关当前为开时写 false；`Drop` 仅在当前值 ≠ 进入前值时恢复。
- 注：本轮测量默认马娘为 100101（同日晚些时候改为 105902，见 [docs/changelog.md](../../../docs/changelog.md)）。

### 配对测量

| 测量 | 修复前 | 修复后 | 变化 |
|---|---:|---:|---:|
| flat 16T 12 万 rollout | 34.9–36.2 μs/rollout | 13.0–13.2 μs | **−63%** |
| flat 24T 12 万 rollout | 33.4–35.2 μs/rollout | 11.3–11.7 μs | **−66%** |
| flat 1T 6000 rollout | 143.4 μs/rollout | 143.1 μs | −0.3%（噪声） |
| 固定根 N=256（16T，中位） | 73.2 ms | 26.9 ms | **−63%** |
| 整局 N=256（16T / 24T） | 6.00 s / 6.04 s | 2.27 s / 1.81 s | **−62% / −70%** |
| 整局 N=1024（16T / 24T） | 14.45 s / 13.40 s | 5.23 s / 4.13 s | **−64% / −69%** |

- 并行倍率：1T 143 μs → 24T 11.3 μs = **12.6×**（修复前 ~4.1×）；30 种子（79132.5 / PT 11191.6）、N=256（85544 / 12360）、N=1024（85566 / 12371）逐位一致。
- 预算基准：24T 单局 N=8192 = 19.3 s（85700 / 12438）、N=12288 = 24.3 s（85804 / 12490）；`[search].search_n` 定为 **8192**。

---

## 第五轮（2026-11-07 夜）：教诲候选整局克隆回归与修复

- **问题**：`dcaabdb` 在 `assignment_candidates → effective_assignment` 引入整局 `self.clone()`（120 候选 × 3 教诲窗口 + 搜索剪枝调用），机制成本由 65–70 μs 抬升至 1361 μs，整局 N=1024 退化至 185.7 s，24T 并行塌到 2.4×。
- **修复**：`effective_assignment` 改为仅在教诲两排（`slots` / `fused`）上局部模拟并复用 `advice_constraint_target`。
- **结果**：
  - `bench root --turn 32 --search-n 1024`：1577 μs → 214 μs（机制 1361 μs → 70 μs）。
  - `bench allocs --turn 32`：分配数 31,019 → **1,019**，分配字节 2.65 MB → **56 KB**，End 段分配 652 → 4.5 / 回合。
  - 端到端：整局 N=1024 **185.7 s → 9.94 s（18.7×）**；N=8192 **26.2 s**（87532 / 11716）；CAND 均值与终局（88545 / PT 12255）逐位一致。
