# 交接提示词：搜索效率与更大预算

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-05 晚）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next`（本轮已改两处，见 §3）。
- **验收**：`cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo clippy --workspace --all-targets --offline -- -D warnings` /
  `cargo test --workspace --release --offline`；**不跑 `cargo doc`**。
  剧本仓 **221 通过 + 2 手动档**；引擎仓 **356 通过 + 3 跳过**。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`）；
  `[search].search_n = 0`（默认手写；搜索档用 `MATSURI_SEARCH_N` 或工作目录覆盖层开启）。
- **手写基线**：100101 种子 61444–61543 百局均 **78124.5** / PT **11052.2**（S9 口径）。
- **性能**：三轮 O0–O4 已落地，细节与复现见 [`perf_profiling.md`](perf_profiling.md) §11 / §12。
  第三轮（引擎侧诊断载荷门控）同轮配对：分配 2085 → 1017、串行 rollout −13%、手写整局 −15%、
  整局 N=256 −4.2%、N=1024 −7.3%；决策与终局逐位一致。
- **单次 rollout 现值**（T32 根 → 终局，串行）：约 211 μs；阶段构成 Begin 13 / Train 124
  （训练员 84）/ Distribute 41 / AfterTrain 12 / End 14；剩余分配 1017 次（Train 871 为主）。
- **搜索预算**：实测 N=12288 分数继续提升（LG14），边际收益仍为正；生产档曾定 8192。

## 2. 下一步：效率瓶颈已从「分配」转到别处

第三轮把分配砍半，但 16–24 线程并行吞吐基本不动（`flat` −1.5%、固定根 −3%，均在噪声内），
`jemalloc` 对照无提升，`MALLOC_ARENA_MAX=1` 反而劣化 7 倍。结论：

- **分配不是当前并行上限的主因**；不要再做「少分配换并行」的假设性改造。
- **并列上限约 6×**（同机纯计算 8 线程 8×、24 线程 14×），16–24 线程落在平台区，调线程数无收益。
- **缺剖面能力**：本机 `perf_event_paranoid=4` 挡住 `perf`；`pprof` 需另立独立工程。

优先级建议：

1. **补 CPU 剖面**（一次性投入）：独立 crate 或独立目录 + `pprof`（依赖不进 core / 不进下游），
   在 `bench flat` / `bench root` 上采样，定位 16 线程下每个 rollout 的 35 μs 墙钟去向。
2. **按剖面结果**决定下一刀：候选方向是事件实例化（静态表引用 + 补丁，风险高）、
   搜索固定开销、真实机位线程策略；**没有剖面就先不动**。
3. 剖面站住后再谈 N=8192 / N=12288 的常态跑批与 S9 基线（只录一次）。

## 3. 本轮落地与边界

引擎仓改动（**默认行为等价**，不改契约、不动数值）：

| 内容 | 落点 |
|---|---|
| `diag::active()`（编译期且运行时）+ 单测 | `crates/umaai-core/src/diag.rs` |
| 事件执行 / 训练后处理 / 选项结算诊断载荷整段门控 | `crates/umasim/src/game/base/mod.rs` |

- 剧本侧本轮零改动；曾试做「得意率面板快路径」，串行只快 0.7%、搜索档测不出，已撤回。
- 引擎仓 `git diff` 非空是**本轮预期**（与「接入新剧本时引擎零改」不冲突）；引擎提交说明需点明
  「默认行为等价」。

## 4. 关键命令与纪律

- 测量（剧本仓根，`bench` 已预编译）：`bench allocs --turn 32`、`bench stages --turn 32`、
  `bench flat --turn 32 --rollouts 1536 --threads 16|24`、
  `RAYON_NUM_THREADS=16 bench root --turn 32 --search-n 256 --repeat 3`、
  `bench game --games 30 --seed 61444`、`bench game --games 1 --seed 61444 --search-n 1024`。
- 跑一局 MCTS：`MATSURI_SEARCH_N=1024 RAYON_NUM_THREADS=16 cargo run --release --offline --bin matsuri -- matsuri`；
  `--explain` 与搜索互斥。
- 配对测量：A/B 交替、各独立启动；耗时只声明当轮；必比 30 种子 CSV 的 `seed/score/pt/turn`、
  固定根各候选 mean/wmean/planned、整局终局 score/PT。
- 种子块已用：61444–61543（基线）、61544–61643、62200–64499、64500–65999、66000–67199；
  下一轮 **67200 起**。
- 并发 cargo：同一 target 一次只跑一个；debug target 偶发卡在 build lock 时，换独立
  `CARGO_TARGET_DIR`（如 `logs/perf/scratch/eng-clippy`）绕过，用完删目录。

## 5. 文档

- 性能：`docs/perf_profiling.md` §12（第三轮改动 / 配对测量 / 「并行上限不是分配」的证伪）。
- 口径：`docs/glossary.md`（本轮无数值位移，未新增决定记录）。
- 测试：`docs/tests_overview.md`（221 通过 + 2 手动档，本轮未增删剧本侧测试）。
