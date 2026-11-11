# 第四轮（2026-11-05）：pprof 接入 + DiagGuard 冗余 store 修复

## 工具
- `bench prof <game|micro|root|trace|stages|flat|parts|allocs> [原参数] [--freq HZ] [--out SVG]`
  （`cargo build --release --features profile -p matsuri-tools`）
- pprof 0.15 = SIGPROF + setitimer(ITIMER_PROF)，不需要 perf_event / root；SVG + 自耗时 TopN。
- 内核 `perf` 已解锁（paranoid=-1 / mlock 65536 / kptr_restrict=0），可用于硬件计数器与内核栈。

## 发现
- 16T flat 剖面：`Data::get` 13.6%、`DiagGuard::suppress` 8.9% self；1T 剖面两者均不在榜。
- 根因：训练 / 资源预览每个候选建一次 DiagGuard，对进程级 `DIAG_ENABLED`（0x3c4de8）
  重复 store；该行紧邻 `data::GLOBAL`（0x3c4bc0 + 0x228 = 0x3c4de8）尾部，16 线程下
  缓存行来回失效（false sharing）。单次 rollout 约 1664 次 guard。
- 修复（引擎 `umaai-core/src/diag.rs`，默认行为等价）：仅在值需要变化时写开关。

## 配对测量（同轮交替、独立进程；base = 修复前引擎，其余同）
| 测量 | base | fixed | 变化 |
|---|---:|---:|---:|
| flat 16T 120k | 34.9–36.2 µs/rollout | 13.0–13.2 µs | −63% |
| flat 24T 120k | 33.4–35.2 µs/rollout | 11.3–11.7 µs | −66% |
| flat 1T 6k | 143.4 µs/rollout | 143.1 µs | −0.3%（噪声） |
| root N=256 16T | 73.2 ms | 26.9 ms | −63% |
| game N=256 16T | 6.00 s | 2.27 s | −62% |
| game N=1024 16T | 14.45 s | 5.23 s | −64% |
| game N=256 24T | 6.04 s | 1.81 s | −70% |
| game N=1024 24T | 13.40 s | 4.13 s | −69% |

- 并行倍率：1T 143 µs → 24T 11.3 µs = 12.6×（修复前 4.1×）。
- 等价性：30 种子手写 CSV（seed/score/pt/turn）逐位一致（79132.5 / 11191.6）；
  root N=256 best=5、mean 95848.6 一致；game N=256 85544/12360、N=1024 85566/12371 一致。
- 测试：matsuri 221 通过 + 2 忽略；引擎 diag 5 通过；引擎全量另跑。

## 修复后 N=1024 24T 剖面 Top（pprof，6667 样本）
advice_effect_of 6.9% / run_stage 6.8% / calc_training_value_inner 6.3% /
Model::train_value 4.0% / distribute_person 3.8% / Model::resource_value 3.3% /
distribute_others 3.1% / preview_training_full 3.0% / preview_action_resources 2.9% /
AdviceSlotEffect::add_assign 2.8% / SupportCard::calc_training_effect 2.7% /
apply_advice 2.2% / Model::value_of_with 2.2% / Model::new 2.2% / settle_training 2.1%。
