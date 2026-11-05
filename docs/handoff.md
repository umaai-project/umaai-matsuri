# 交接提示词：两档基线已录，下一步补满 100 局

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-05 晚）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next`（本轮两处改动，见 §4）。
- **验收**：`cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo test --workspace --release --offline`；**不跑 `cargo doc`，Linux 下不跑 `cargo clippy`**。
  剧本仓测试全绿；引擎仓 `umasim` / `umaai-core` 测试全绿。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`、`search_n=8192`）；
  默认测试马娘 **105902**；局外状态默认不注入（`[pre] qie="none"`、`positive_thinking=0`）；
  开局预设 `blue=[18,0,0,0,0]` / `extra=[10,10,120,140,0,100]`。
- **搜索参数**：`search_group_size=1024`（首轮每候选一组，即每候选样本下限）、
  `expected_search_stdev=3300`（`0` = 实测合并标准差）、`radical_factor_max=0`、`use_ucb=true`。
- **基线（2026-11-05 录，seeds 73200 起）**：

| 档 | 手写 100 局 | 搜索 10 局（N=8192、20–24T） |
|---|---|---|
| 普通马娘 105902，无 buff | 80291.3 / PT 11262.5（sd 2635） | 84639.2 / PT 12247.4（sd 1526） |
| 狄杜斯 106302，小切 + 盾 2 | 84219.9 / PT 11890.8（sd 2566） | 87646.6 / PT 12923.4（sd 491） |

  搜索单局耗时约 21–23 s（N=8192，24 线程，空载）。10 局的均分标准误约 ±483 / ±155。

- **分辨率实测**（固定根，均匀大预算作真值，95% 最小可分差）：T10 ≈ 91、T32 ≈ 68、
  T50 ≈ 101、T65 ≈ 12。单次 rollout 标准差 σ 在 T5–T40 为 2985–3460，T50 后收窄到 913。

## 2. 下一步

1. 两档搜索基线各补到 100 局（`bench game --games 100 --seed 73200 --uma … --qie … --shield …`），
   把标准误压到 ±150 / ±50 量级。
2. 转入评分口径与事件质量：搜索排序把 PT 项放大到官方权重的 1.73 倍（`parts.pt` 已含 ×2.0，
   再乘 `[trainer].pt_favor_rate=1.7271`），是当前已知的系统性偏差点。
3. 不要再加搜索预算：8192 与 12288 的分辨率差不到 10 分，加预算的整局收益落在噪声内。
4. 调参换新种子块：73200–73299 已用，下一块 73300 起。

## 3. 本轮口径变更

- `[search].search_n` 默认 12288 → **8192**。
- `search_group_size` 512 → **1024**；首轮仍每候选一组（每候选样本下限 = 1024）。
- `expected_search_stdev` 2200 → **3300**（取实测 σ 的前中期典型值）。
- `[pre]` 由 `xiaoqie: bool` 改为 `qie: none | xiaoqie | qiezhe`；默认 `none` / 盾 0，理想测试档用命令行或覆盖层开启。

## 4. 改动落点与边界

| 内容 | 落点 |
|---|---|
| 事件搜索补跑 Begin 尾部（T2 教诲授予、首分配、追加稽古预掷） | `crates/matsuri-core/src/game/stage.rs`（`begin_tail`）、`search.rs` |
| 局外切状态三选一 + 默认小切 / 盾 2 | `crates/matsuri-core/src/config.rs`（`Qie` / `PreConfig`）、`game/mod.rs` |
| `bench` 固定条件覆盖 `--uma` / `--qie` / `--shield` | `crates/matsuri-tools/src/bin/bench.rs` |
| 搜索参数默认层 | `data/ai_config.toml`、`data/matsuri_config.toml` |
| UCB：`expected_search_stdev=0` 时用实测合并标准差 | `../umaai_next/crates/umasim/src/search/{flat_search,result}.rs` |

- 引擎侧改动默认行为等价（`stdev > 0` 走原常数路径）；剧本侧 `search_n` 变更属口径变更。

## 5. 关键命令与纪律

- 基线（A 档）：`RAYON_NUM_THREADS=24 ./target/release/bench game --games 100 --seed 73200 \
  --warmup 0 --uma 105902 --qie none --shield 0`。
- 基线（B 档）：同命令行，改 `--uma 106302 --qie xiaoqie --shield 2`。
- 手写档：追加 `--search-n 0`（每局约 0.3 ms）。
- 测量纪律：A/B 交替、各自独立启动；耗时只声明当轮；配对种子数 ≥21 才能分出 600 分量级的差异。
- 并发 cargo：同一 target 一次只跑一个；单次 `bench` 会吃满线程，测耗时前确认无后台任务
  （`ps -eo pcpu,args --sort=-pcpu | head`）。
- 种子块已用：61444–61543、61544–61643、62200–64499、64500–65999、66000–67199、
  67200–70199、70200–73199、73200–73299；下一块 **73300 起**。

## 6. 文档

- 口径：`glossary.md` 的「默认搜索预算 8192」「搜索分组与探索尺度」「理想测试固定条件」三条。
- 机制：`mechanism.md` §5.5（`[pre]` 三个取值与心情盾默认 2 层）。
- 性能：`perf_profiling.md` §13（第四轮优化与剖面）；本轮预算与分辨率结论记在 README 现状段。