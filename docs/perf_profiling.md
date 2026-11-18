# 性能分析工具、测量口径与热点红线

> 覆盖本仓（骏大祭）手写决策链与 FlatSearch rollout 的性能测量工具、测量口径、设计红线与当前已知热点。
> 剖面优化流程按引擎 [`umaai_next/docs/scenario/perf_plan.md`](../umaai_next/docs/scenario/perf_plan.md) 的 O0–O4 执行（专用测试优先于剖面，一次只改一个点）；涉及引擎仓改动走 [`umaai_next/docs/scenario/repo_form.md`](../umaai_next/docs/scenario/repo_form.md) §3 钩子流程。

## 1. 工具：matsuri-tools `bench`

入口源码位于 [`crates/matsuri-tools/src/bin/bench.rs`](../crates/matsuri-tools/src/bin/bench.rs)（支持 `--uma` / `--qie` / `--shield` / `--deck-profile` 覆盖）：

| 子命令 | 用途 | 输出 |
|---|---|---|
| `bench game --games N --seed S [--search-n M]` | 整局耗时 + 终局 CSV（缺省读配置，当前默认 8192） | `seed,score,pt,turn,elapsed_us` + 均值行 |
| `bench luck --games N --seed S [--search-n M] [--per-turn]` | 估分随回合漂移与 `luck_turn_bonus` 斜率（`search_n > 0`）；`--per-turn` 追加逐回合明细行 | 逐局首/末回合估分、`b`、`reg_slope`、终局；`--per-turn` 另出 `T,seed,turn,expected,stdev,candidates` |
| `bench micro --turn T --iters N` | 固定回合决策链热循环分段 | 各段 ns/op（克隆 / 训练预览 / 资源预览 / 整决策 / 搜索分） |
| `bench trace --turn T` | rollout 内逐决策点耗时 | `turn,actions,us,advice` |
| `bench root --turn T --search-n M --repeat R` | 固定根搜索（含 `rollout_noop` 空训练员拆解） | 每轮耗时、planned / failed、ns/rollout、best |
| `bench stages --turn T [--action I]` | 单次 rollout 按阶段机拆分 | 各阶段 us / 占比 / 调用次数 + 训练员决策耗时 |
| `bench parts --turn T --iters N` | 分布 / 事件生成 / 列动作单点 | ns/op |
| `bench flat --turn T --rollouts N --threads M` | `std::thread` 平铺并行 rollout（并行上限对照） | 总耗时、ns/rollout、校验和 |
| `bench allocs --turn T [--action I]` | 单次 rollout 分配计数（内置计数分配器） | 阶段分配数 / 字节 / 训练员分配数 |
| `bench baseline [--name X] [--trainer ...] [--games N]` | 复跑 [`benchmark/baselines/`](../benchmark/baselines/README.md) 门禁数据 | 偏差与标准误校验（漂移时非零退出） |
| `bench prof <子命令> [原参数] [--freq HZ] [--out SVG]` | `pprof` 采样包裹（需 `--features profile`） | SVG 火焰图 + 自耗时 TopN（同名 `.txt`） |

## 2. 测量口径与复现

- **纯净计算口径**：不装 logger（`diag!` 关闭时不求值参数）；`stages` / `allocs` 测「同一固定根的下一次 rollout」，`Train` 阶段扣掉 `trainer.action_us` 才是机制成本。
- **固定根装配**：使用生产配置装配（`build_game + with_config`），不用默认档 `testing::sample_game`。
- **配置与线程**：
  - `data/matsuri_config.toml` 默认 `[search].search_n = 8192`，测手写档须显式传 `--search-n 0`。
  - `data/ai_config.toml` 中 `radical_factor_max = 0.0`（引擎 `weighted_mean` 退化为算术均值）。
  - 进程级线程数用 `RAYON_NUM_THREADS` 固定；主入口另由 `matsuri_config [search].threads` 控制默认值（`0` = 自动，环境变量优先）。
- **配对比较原则**：受机器热状态与频率影响，跨会话绝对耗时不可比；配对测量必须同机同轮交替执行、各独立启动进程（可用并列 worktree），只认当轮相对变化与逐位一致性。

```bash
# 常规 Release 构建
cargo build --release --offline -p matsuri-tools

# 常用测量示例
./target/release/bench game --games 30 --seed 61444 --search-n 0
RAYON_NUM_THREADS=16 ./target/release/bench root --turn 32 --search-n 256 --repeat 3
./target/release/bench allocs --turn 32

# CPU 剖面（首次构建需联网拉取 pprof 0.15；缺省输出至 benchmark/_local/perf/，不入仓）
cargo build --release --features profile -p matsuri-tools
./target/release/bench prof flat --turn 32 --rollouts 120000 --threads 16 \
  --freq 997 --out benchmark/_local/perf/flame-flat.svg
```

## 3. 性能红线与已裁定不做项

- **红线 — 候选枚举禁止整局 `self.clone()`**：2.8 μs 的单次整局克隆乘上「120 候选 × 数千 rollout × 多窗口」会造成数量级退化（曾导致 N=1024 耗时从 ~10 s 退化至 185.7 s）与堆锁竞争。预览与约束模拟须使用局部状态（如局部 `ActLedger` / 教诲两排数组）或原子开关；**新增候选枚举前先用 `bench allocs` 验收，单回合分配数应在个位数**。
- **并行与分配认知**：引擎侧已修复 `data::GLOBAL` 与 `DIAG_ENABLED` 同行假共享及热路径诊断载荷构造；当前 24 线程并行倍率约 12.6×，瓶颈已不在内存分配或 rayon 调度。
- **已裁定不做项（避免重复立项）**：
  - `Model::new` 缓存：单次仅 ~0.2–0.3 μs，缓存失效点分散、收益极低。
  - 每候选 400 KB 直方图消除：并行搜索下摊开仅占 ~0.05%，属引擎侧暂不改。
  - 训练卡效果表缓存：`&self + Send + Sync` 无干净落点，5 类失效点漏一处即静默错值。
  - `deyilv` 只读访问器：剧本已覆写且不写回，收益为 0 ns。
  - 事件实例化去复制：涉及引擎契约面 DTO 改造，风险高而搜索档收益仅 2–4%。

## 4. 当前已知热点与后续候选

24 线程整局 N=1024 剖面 Top 热点（第 4 轮修复假共享后，6667 样本）：

- **教诲链（~12%）**：`advice_effect_of`（6.9%）、`AdviceSlotEffect::add_assign`（2.8%）、`apply_advice`（2.2%）。
- **训练估值链（~11%）**：`calc_training_value_inner`（6.3%）、`SupportCard::calc_training_effect`（2.7%）及 `default_calc_training_buff`。
- **值模型（~10%）**：`Model::train_value`（4.0%）、`Model::resource_value`（3.3%）、`Model::value_of_with`（2.2%）、`Model::new`（2.2%）。
- **阶段与分布**：`run_stage`（6.8%）、`distribute_person`（3.8%）、`distribute_others`（3.1%）、`preview_training_full`（3.0%）、`preview_action_resources`（2.9%）。

未立项后续方向：教诲/训练估值/值模型三条热链的 microbench 拆解、搜索固定开销（尤其低预算档）、真实机位线程策略。

## 5. 历史归档

- 2026-11-05 至 2026-11-07 五轮优化的详细配对测量数据、阶段拆解与等价性验证快照见 [`benchmark/_local/perf/profiling_history.md`](../benchmark/_local/perf/profiling_history.md)。
