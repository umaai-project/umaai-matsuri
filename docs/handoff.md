# 交接提示词：默认马娘定档与 S9 基线测量

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-05 晚）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next`（本轮一处改动，见 §4）。
- **验收**：`cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo clippy --workspace --all-targets --offline -- -D warnings` /
  `cargo test --workspace --release --offline`；**不跑 `cargo doc`**。
  剧本仓 **221 通过 + 2 手动档**；引擎仓全量 release 测试通过。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`、
  `search_n=12288`）；默认测试马娘 **105902（目白多伯）**，开局预设 `blue=[18,0,0,0,0]` /
  `extra=[10,10,120,140,0,100]`。手写档用 `MATSURI_SEARCH_N=0` 或覆盖层切回。
- **基线（已录）**：
  - 默认马娘 105902 手写 100 局（seeds 61444–61543）均 **79253.9** / PT **11291.0**；
  - 默认搜索档单局（seed 61444、24T、N=12288）**85458 / PT 12317 / 23.8 s**；
  - 搜索单局参考：N=1024 85309 / 12427（3.7 s）、N=8192 86290 / 12733（17.1 s）；
  - 旧默认马娘 100101 手写 100 局 78476.0 / PT 11080.1 保留为历史口径，不与新默认混比。
- **性能**：四轮 O0–O4 已落地，细节见 [`perf_profiling.md`](perf_profiling.md) §13。
  第四轮 = pprof 剖面接入 + 引擎 `DiagGuard` 冗余 store 假共享修复（默认行为等价）：
  并行倍率约 4× → **12.6×**（24T flat 每 rollout 33.4–35.2 → 11.3–11.7 μs），
  整局 N=256 6.00 → 1.81 s、N=1024 13.40 → 4.13 s（24T），决策与终局逐位一致。
- **单次 rollout 现值**（T32 → 终局）：串行约 143 μs；24T flat 11.3 μs/rollout。
- **工具**：`sweep uma`（全马娘手写扫测，含 `--scores` 逐种子矩阵）与 `bench prof`
  （pprof 剖面，需 `--features profile`）已入仓。

## 2. 下一步：S9 基线测量（只录一次）

1. **默认档整局基线**：`bench game --games 100 --seed 61444`（默认 N=12288、24T，
  seeds 61444–61543，预计约 40 min）；落 CSV 后记录 score / PT 均值与波动。
2. **手写档对照**：`bench game --games 100 --seed 61444 --search-n 0`（已录 79253.9 / 11291.0，
  可复跑确认同值）。
3. 如需先粗看：`--games 30` 快速档，但**不替代** S9 的 100 局。
4. 基线录完后按 `perf_profiling.md` §13.7 的剖面候选（教诲链 / 训练估值链 / 值模型）
   先补 microbench 再改；像素级调参换 67200 起的新种子块。

## 3. 本轮口径变更（同日）

- 默认搜索预算 `0 → 12288`（第四轮后单局约 24 s；0 显式切手写档）。
- 默认测试马娘 `100101 → 105902`（全马娘扫测定档：264 个可测条目里唯一显著领先的普通马娘，
  对第 2 名 +123.4 ± 35.1；第 2–10 名内部不可分辨）。
- 特殊机制马娘单列：106301 / 106302（铁心，约 82680–82739，会占普通榜前二）、
  109301 / 109302（强制休息，约 77473–77513）。8 个条目因默认卡组含本人卡未测
  （102401–3、106201–2、106901–2、111801）。
- 开局预设仍是 100101 扫参结果，未按 105902 重扫。

## 4. 改动落点与边界

| 内容 | 落点 |
|---|---|
| 引擎 `DiagGuard` 仅在切换时写运行时开关（默认行为等价） | `../umaai_next/crates/umaai-core/src/diag.rs` |
| `profile` 特性 + `bench prof`（pprof 0.15，默认不编译） | `crates/matsuri-tools/{Cargo.toml,src/bin/bench.rs}` |
| `sweep uma`（全马娘扫测 + `--scores`） | `crates/matsuri-tools/src/bin/sweep.rs` |
| 默认搜索预算 12288、默认马娘 105902 | `data/matsuri_config.toml`、`data/ai_config.toml`、`crates/matsuri-core/src/config.rs`、`crates/matsuri-cli/tests/explain_smoke.rs` |

- 剧本侧无数值位移（默认档选择变化属于口径变更，已记 glossary）；引擎侧改动默认行为等价。

## 5. 关键命令与纪律

- S9 基线（默认档，100 局）：`RAYON_NUM_THREADS=24 ./target/release/bench game --games 100 --seed 61444`，
  输出落 `logs/perf/`（不入库）。
- 手写档：`bench game --games 100 --seed 61444 --search-n 0`。
- 剖面：`cargo build --release --features profile -p matsuri-tools`；
  `bench prof flat --turn 32 --rollouts 120000 --threads 24 --freq 997 --out logs/perf/x.svg`。
- perf（需已解锁）：`kernel.perf_event_paranoid=-1` / `perf_event_mlock_kb=65536` /
  `kptr_restrict=0`，重启自动还原；pprof 本身不需要这些。
- 配对测量：A/B 交替、各独立启动；耗时只声明当轮；必比 30 种子 CSV 的 `seed/score/pt/turn`、
  固定根各候选 mean/wmean/planned、整局终局 score/PT。
- 种子块已用：61444–61543（基线）、61544–61643、62200–64499、64500–65999、66000–67199、
  67200–70199、70200–73199；下一轮 **73200 起**。
- 并发 cargo：同一 target 一次只跑一个；debug target 偶发卡在 build lock 时，换独立
  `CARGO_TARGET_DIR`（如 `logs/perf/scratch/eng-clippy`）绕过，用完删目录。

## 6. 文档

- 性能：`perf_profiling.md` §13（pprof 工具 / 假共享定位 / 配对测量 / 结论修正 / 新剖面 Top）。
- 口径：`glossary.md` 新增「默认搜索预算 12288」「默认测试马娘 105902」两条决定记录；
  开局预设见 `mechanism.md` §5.6。
- 测试：`tests_overview.md` 未增删测试；`explain_smoke` 显式关搜索。
