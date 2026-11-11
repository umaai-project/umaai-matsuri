# 搜索分配方案：Successive Halving 取代 UCB（设计稿）

> 状态：**设计稿，未实施**（2026-11-09）。
> 范围：`umaai-matsuri` + `umaai_next`。只改搜索分配与终局选择，不改剧本机制、
> 估值口径、随机流、卡组参数与训练员权重。
>
> 关联：[`round_2026-11-09.md`](round_2026-11-09.md)（UCB 审计现状）、
> [`../../umaai_next/crates/umasim/src/search/flat_search.rs`](../../umaai_next/crates/umasim/src/search/flat_search.rs)（当前实现）。

## 1. 问题：UCB 的「不等样本终局比较」

当前 `FlatSearch` 的分配（[`flat_search.rs`](../../umaai_next/crates/umasim/src/search/flat_search.rs)）：

1. 首轮每个候选跑 `search_group_size`（生产 1024）；
2. 之后每轮按 `value + cpuct × expected_stdev × sqrt(total_n) / n` 选一个候选追加一组；
3. 某个候选的 **planned** 次数达到 `search_n`（生产 8192）即停；
4. 终局在**全部候选**里取 `weighted_mean` 最大者（`radical_factor=0` 即算术均值），
   不按样本数做置信修正。

因此 8 候选的典型分配是 `1024×7 | 8192×1`；16 候选是 `1024×15 | 8192×1`。
终局实际是拿「8192 样本的均值」和「1024 样本的均值」直接比较：

- 被追加的候选之所以被追加，是因为它的前 1024 均值偏高；样本增加后它会向真值回归；
- 其余候选停在 1024，均值噪声约 `σ/√1024 ≈ 103`（`σ≈3300`）；
- 于是会出现「1024 时 A>B，A 搜到 8192 后 A<B，最后选 B」的倒挂。
  选 B 可能是正确的，也可能是 B 的 1024 噪声造成的错误翻转；问题是决策依赖样本数、
  不稳定且不可解释。

这不是 UCB 公式本身写错，而是**目标错配**：UCB 面向累计遗憾最小化（要持续采样所有臂），
而本剧本每个决策点需要的是**最佳臂识别**（只关心最终选谁）。同时终局用不等 n 的均值硬比，
放大了「领先者回归、挑战者仍带噪声」的问题。

> 顺带记录：`select_ucb_action` 的探索项分母用成功样本数 `count`，预算推进用 `planned`；
> rollout 失败时会额外偏向失败候选。这是独立的次要偏差，SH 方案不依赖该路径。

## 2. 目标与非目标

### 2.1 目标

- 终局**只在同 n 的候选之间比较**，消除不等样本倒挂；
- 用分轮淘汰替代「一直加注领先者」，把预算按候选数自动摊开；
- 用户侧只保留**一个搜索预算旋钮**；
- 默认行为对其他剧本仓（ramen / onsen / umaai）逐位不变；
- 保留可审计性：每轮存活集、每候选计划/成功次数、终局差距与 SE 可观测。

### 2.2 非目标

- 不接叶子估值器、不改 `max_depth`；
- 不改剧本随机流（`turn_fixed` / `event` / `strategy`）与 CRN 种子派生；
- 不改淘汰后的单候选精修（选择已由等 n 比较决定；精修只影响展示分）；
- 不改卡组、训练员权重、`pt_favor_rate`、`radical_factor`；
- 不做「全局共享可变流」「人头级独立流」等此前已证伪/回退的方向。

## 3. 方案：固定组 Successive Halving（SH / Racing）

### 3.1 核心公式

对某个决策点的候选数 `N`：

```
R(N)  = ceil(log2 N)                  # 决赛者参与的轮数（含终局轮）
G(N)  = N + ceil(N/2) + … + 2         # 总组数（选完即停，不再精修唯一胜者）
g     = max(ALIGN, floor_to_multiple_of_ALIGN(target_n / R(N)))
```

- `target_n`：**终局两名候选各自的目标样本数**，即用户唯一旋钮（建议 4096）。
- `ALIGN = 256`：组大小按 256 对齐，兼顾并行分批。
- 每轮每个存活者追加 `g`；所有存活者**共用同一段 seeds**（`seeds[offset..offset+g]`，
  `offset` 等于当前每人累计 n），因此同轮同 n、可配对。
- 终局两名候选的累计 n 为 `R(N) × g`（若 `target_n` 不能被 `R` 整除，则略低于目标）。

### 3.2 每轮流程（selection-only）

```
alive = 全部候选
n = 0                                  # 每人累计数，所有人始终相同
while |alive| > 1:
    for c in alive:
        simulate(c, g, seeds[n .. n+g])
    n += g
    rank alive by weighted_mean(radical_factor)     # 生产 radical=0 → 算术均值
    keep = ceil(|alive| / 2)
    # 置信保留（racing）：边界差距不显著时多留一名，避免噪声误杀
    while keep < |alive|
          and |mean[keep-1] - mean[keep]| < ELIMINATE_Z * sqrt(SE[keep-1]^2 + SE[keep]^2):
        keep += 1
    alive = top keep
choose best in alive by mean
```

参数常量（不进用户配置）：

- `ELIMINATE_Z = 1.0`：只淘汰均值明显落后的候选；差距不显著就多留。
- `FINAL_Z = 1.5`：终局两名候选，挑战者需超过领先者 `FINAL_Z × 配对SE` 才允许翻转。
- 若候选成功样本为 0（rollout 全部失败），视为不可用，回退手写或参与下一轮；
  预算按 `planned` 计，保证种子偏移确定。

### 3.3 终局配对

- 决赛双方共享完整的 `seeds[0..n]`，应做**逐种子配对差分**，而不是独立 SE。
- 当前 `ActionResult` 只存聚合值；`record_ordered_rollouts` 默认关闭。
  实现时需在 SH 路径内部保留每候选的按种子原始分（`Vec<Option<f64>>`），
  内存上限约 `N × target_n × 8B ≈ 16×4096×8 ≈ 512KB`，可接受。
- 失败种子记 `None`，配对时只取双方都有值的序号，保持 CRN 对齐。
- 终局选择必须**只在存活集**里进行。当前 `SearchOutput::with_terminals` 在全部候选里取最大，
  需要内核把存活集（或预先算好的 `best_action_idx`）带给输出层；默认 `None` 时保持旧行为。

### 3.4 边界与回退

- `N <= 1`：不搜索，直接回退手写。
- `N == 2`：不做淘汰，双方各 `target_n`（`R=1, g=target_n`）。
- `g < ALIGN`：钳到 `ALIGN`（N≤16、`target_n=4096` 时不会触发）。
- `target_n` 不能整除 `R` 时向下取整到 `ALIGN` 的倍数（例：`4096/3 → 1280`，
  决赛 n=3840，略低于 4096）。
- `radical_factor > 0`：同轮同 n 仍然成立；但不同阶段的排名加权口径要一起审计，
  生产档保持 `0`。

## 4. 推荐参数：单旋钮 `search_n = 4096`

`matsuri_config [search].search_n` 的语义从「每候选上限」改为「终局候选目标 n」，
默认由 8192 改为 **4096**。其余由 `N` 派生，不再配置。

以 `σ≈3300`、`target_n=4096` 计算：

| 决策 | N | R | g | 首轮筛查 n | 终局同 n | 总 rollout | 现行总量 | 成本比 |
|---|---|---|---|---|---|---|---|---|
| 事件 | 2 | 1 | 4096 | 4096 | 4096 | 8192 | 9216 | 0.89× |
| Train | 4 | 2 | 2048 | 2048 | 4096 | 12288 | 11264 | 1.09× |
| Train | 8 | 3 | 1280 | 1280 | 3840 | 17920 | 15360 | 1.17× |
| Train | 12 | 4 | 1024 | 1024 | 4096 | 23552 | 19456 | 1.21× |
| 教诲 | 16 | 4 | 1024 | 1024 | 4096 | 30720 | 23552 | 1.30× |

- 现行总量（典型）`(N + 7) × 1024`：首轮 `N×1024`，UCB 持续选同一领先者时再追加 7 组；
  若 UCB 的追加分散到多个候选，总量会略高于该值。新总量 `G(N) × g` 与追加分布无关。
- SE 参考：n=1024→103、1280→92、2048→73、3840→53、4096→52。
- 配对终局的实际 SE 通常远低于独立 SE；主要收益来自「决赛双方等 n + 共享种子」。

对照其他目标 n（供评审，不推荐直接采用）：

| target_n | N=16 首轮 | N=16 终局 | N=16 总量 | 相对现行 | 备注 |
|---|---|---|---|---|---|
| 3072 | 768 | 3072 | 23040 | 0.98× | 算力持平，但 16 候选筛查偏薄（SE≈119） |
| **4096** | **1024** | **4096** | **30720** | **1.30×** | **推荐：筛查与终局均衡** |
| 6144 | 1536 | 6144 | 46080 | 1.96× | 筛查更稳、终局 SE≈42，但接近 2× 算力 |

为什么推荐 4096：Train 是最频繁的决策，N≈5–12，成本约 +9%–21%；教诲只有 6 个窗口，
N=16 成本 +30% 但总量占全局限；事件 N=2 反而更便宜。6144 的边际精度（SE 52→42）
不足以抵消约 2× 的搜索耗时；3072 的 16 候选首轮 768 误杀风险偏高。

## 5. 配置与兼容

### 5.1 用户侧配置

```toml
# matsuri_config.toml
[search]
# 终局两名候选的目标样本数（语义变更；0 = 手写档）
search_n = 4096
threads = 0
train = true
advice = true
event = true
```

- `data/matsuri_config.toml` 注释、`SearchScopeConfig::default()`、
  `config.rs` 测试断言（`search_n = 1024` 等）同步改。
- `data/ai_config.toml` 的 `[search].search_n = 0` 仍是惰性占位（matsuri 用自己的值）。

### 5.2 引擎侧配置

- `SearchConfig` 新增 opt-in 方案（建议命名 `scheme`）：
  - `Ucb`（默认，逐位保持旧行为）；
  - `Sh { target_n, eliminate_z, final_z }`。
- 新字段加 `#[serde(default)]`，旧 `ai_config.toml` 不写也能解析；
  仓内检索未发现 `SearchConfig { … }` 的结构体字面量构造，新增字段不会破坏调用方。
- `use_ucb`、`search_cpuct`、`expected_search_stdev` 在 `Sh` 下变为惰性字段；
  `search_group_size` 改语义为「组大小 / 对齐单位」。
- 其他剧本仓不设 `Sh`，默认仍是 `Ucb`，行为不变。

## 6. 实现落点

### 6.1 引擎 `umaai_next`（通用能力，默认关闭）

| 文件 | 改动 |
|---|---|
| `crates/umasim/src/search/config.rs` | 新增 `SearchScheme` + SH 参数（serde default 保持旧配置可解析） |
| `crates/umasim/src/search/flat_search.rs` | 新增 `search_sh`；`search_ucb` 保留；按 `scheme` 分发 |
| `crates/umasim/src/search/flat_search.rs` | SH 内部保留按种子原始分（配对终局）；每轮存活集与 planned 计数进 probe |
| `crates/umasim/src/search/result.rs` | 终局 best 支持「只在存活集 / 同 n 候选」里取；默认路径不变 |
| `crates/umasim/src/search/mod.rs` | 导出新枚举与构造入口 |
| 引擎测试 | 轮次/预算公式、同轮同 offset、终局同 n、并行确定性、`Ucb` 默认档 golden 不变 |

### 6.2 剧本 `umaai-matsuri`（只做接线与审计）

| 文件 | 改动 |
|---|---|
| `crates/matsuri-core/src/search.rs` | `MatsuriSearchTrainer::new` 设 `scheme=Sh`、`target_n=search_n`；日志口径改为「target/存活数」 |
| `crates/matsuri-core/src/config.rs` | `search_n` 语义与校验；默认 4096 |
| `data/matsuri_config.toml`、`data/ai_config.toml` | 默认值与注释；惰性字段标注 |
| `crates/matsuri-tools/src/bin/audit_search.rs` | 新增 SH 模式；输出每轮分配、早轮误杀率、终局翻转率；沿用 `gap/indep_se/counts` |
| `benchmark/baselines/`、`README.md`、`docs/plan/round_2026-11-09.md` | 新分配下重录基线与本轮记录 |

`ADVICE_TOPK = 16` 的候选剪枝仍留在 matsuri（属剧本决策面）；引擎只接收剪枝后的候选。

### 6.3 过渡顺序

1. 引擎加 `Sh`，默认 `Ucb`；全仓测试确认默认档逐位不变。
2. matsuri 默认切 `Sh` + `target_n=4096`，保留 `use_ucb` 覆盖用于 A/B。
3. `audit_search` 对比 UCB / SH 的 regret、翻转率、早轮误杀率。
4. 数据达标后再把 UCB 标为「旧档保留、默认不用」；不立即删除，保证旧基线与实验可复现。

## 7. 验收计划

### 7.1 单元 / 集成测试

- 分配：`G(N)`、`g`、总 rollout 与上表一致；`N=2` 不淘汰。
- 配对：同轮所有存活者的 `seeds` 偏移相同；失败种子不压缩槽位。
- 终局：候选按同 n 比较；已淘汰候选不会被 `best_action_idx` 选中。
- 确定性：同种子、并行 / 串行结果一致；浮点累加顺序不变。
- 默认档：`scheme=Ucb` 与改动前逐位一致（旧基线可复现）。

### 7.2 搜索审计

- `audit_search --turns 32,47 --seeds 20`（T32 为 8 候选，教诲窗口补 16 候选固定根审计）。
- 指标：
  - `chosen != best_at_min_n` 的翻转率（目标接近 0）；
  - 首轮淘汰后真最优仍存活的比例（早轮误杀率）；
  - `regret_off / regret_prod`；
  - 每候选 planned 直方图；
  - 终局 `gap / SE` 分布。
- 额外记录每局实际出现的候选数（2 / 5–8 / 12 / 16），确认上表假设。

### 7.3 配对 A/B 与基线

- 同 seed 块（下一块 **73300 起**）做 20 局配对搜索 A/B：现行 UCB vs SH-4096。
- 记录 wall time：预期 Train +9%–21%、教诲 +30%、事件 −11%；若整体超过 +40%，
  回落到 `target_n=3072` 或只在 Train 启用 SH。
- 重录基线：`a-normal` / `b-ideal` 四项；同步 `README`、`benchmark/baselines/README.md`。

## 8. 风险与回滚

| 风险 | 影响 | 缓解 |
|---|---|---|
| 16 候选首轮 n=1024 误杀 | 教诲选错方案 | `ELIMINATE_Z=1.0` 置信保留；审计确认差距普遍 <200 时把该决策的 target 提到 6144（首轮 1536），或临时对 N≥16 用更保守的淘汰比例 |
| 预算语义变更 | 旧 sweep / GA 结果不可直接比较 | 保留 `scheme=Ucb` 开关；重录基线；文档标注语义 |
| `radical_factor>0` 时的排名口径 | 不同 n 不可比，但 SH 已同 n | 生产保持 0；若要开启，先单独审计 |
| 失败 rollout 偏移 | planned 与成功数不一致 | 偏移与预算一律用 `planned`；零成功候选回退手写 |
| 内存（配对缓冲） | 16×4096×8B ≈ 512KB / 决策 | 可接受；仅 SH 路径分配 |

回滚：配置改回 `scheme=Ucb` / 不设 SH 即可恢复逐位旧行为，无需回退代码。

## 9. 待决事项

1. `search_n` 键名是否改为 `target_n`（语义变更更清晰，但影响所有配置与文档）。
2. `SearchScheme` 放 `SearchConfig` 顶层字段，还是嵌套 `[search.sh]`。
3. 教诲 N=16 是否单独设更高 target（6144），还是维持单一 4096。
4. 终局配对缓冲是否常开（仅 SH）还是复用 `record_ordered_rollouts`。
5. 胜者是否需要额外精修一轮（只影响展示分 / luck 报告，不影响选择）。

## 10. 结论摘要

- 新方案**不再是 UCB**，而是固定组 Successive Halving / Racing；置信界只用于淘汰与终局翻转门限。
- 终局从「8192 vs 1024」变成「同 n 比较」，结构性消除倒挂。
- 用户侧只留 `search_n=4096`（终局目标样本数）；`cpuct` / `expected_search_stdev` /
  `use_ucb` 退休为惰性字段。
- 分配算法放在 `umaai_next`，默认 `Ucb` 不变；matsuri 只负责启用、候选剪枝与审计。
- 本文件为设计稿，**本轮不实施**。
