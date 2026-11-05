# 交接提示词：引擎侧性能优化

> **一次性文档**：每次交接整份重写，只保留下一步需要的信息；不在旧版本上叠加历史。

## 1. 当前状态（2026-11-05 下午）

- **仓**：`umaai-matsuri`（cwd）；引擎仓 `../umaai_next` 本轮未改（下一轮要在引擎仓做）。
- **验收**：`cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo clippy --workspace --all-targets --offline -- -D warnings` /
  `cargo test --workspace --release --offline`；**不跑 `cargo doc`**。
  测试 **221 通过 + 2 手动档**（1024 整局复现、教诲微基准，均 `#[ignore]`）。
- **默认档**：`data/matsuri_config.toml` = 冠军档 `ga-r2`（`pt_favor_rate=1.7271`）；
  `[search].search_n = 0`（默认手写；搜索档用 env / 覆盖层显式开启）。
- **手写基线**：100101 种子 61444–61543 百局均 **78124.5** / PT **11052.2**（S9 口径）；
  30 种子冒烟 CSV 在性能轮次中逐位未变。
- **搜索档**：`MatsuriSearchTrainer` 包引擎 `FlatSearch`（扁平 MC + UCB + CRN），
  `train` / `advice` / `event` 分开关；`search_n` = **每候选** rollout 次数；
  教诲 120 候选剪枝 top-16；事件用 dummy token + `search_with`，从 `event_queue` 下一事件续跑；
  排序 `search_score(pt_favor_rate)`，失败分支按实际分；`LeafEvaluator` 仅预留接口，生产 `max_depth=0`。
- **性能**：两轮 O0–O4 已落地，细节与复现见 [`perf_profiling.md`](perf_profiling.md)。
  第二轮同轮配对：手写整局 −27%、串行 rollout −26%、固定根 N=256 −18%、整局 N=256 −20%、
  N=1024 −14%；单次 rollout 分配 4080 → 2085 次。决策与终局逐位一致。
- **单次 rollout 现值**（T32 根 → 终局，串行；本机重启后口径，仅当轮可比）：约 241 μs；
  阶段构成 Begin 17 / Train 124（训练员 84）/ Distribute 41 / AfterTrain 37 / End 14；
  剩余分配 2085 次（AfterTrain 929、Train 890、Begin 241）。
- **随机流**：`FlatSearch` 每次恰好从调用方决策流取一个 `next_u64` 派生 rollout 种子；
  局面规则流由 `fork_for_rollout` 按 rollout 种子独立派生。`search_consumes_one_decision_draw` 守门。

## 2. 下一步：引擎侧优化（唯一主线）

本仓第二轮已经把能安全拿的机制侧与估值侧收益拿完，剩余大头都在引擎仓。按引擎仓
`docs/scenario/repo_form.md` 第 3 节「引擎钩子流程」做：**带默认实现、非破坏**，
引擎默认行为不变；剧本仓不偷偷缓存引擎公式（会与引擎口径分叉）。

优先级（依据 `perf_profiling.md` §11.6 的分配归因）：

1. **`BaseGame::apply_event` 的诊断载荷惰性化**（每事件 4–8 次分配，最大单笔）：
   `umasim/src/game/base/mod.rs` 的 `choice_result.value.to_string()`、`format!("入账 …")`、
   `parts.join` 在 diag 关闭时仍执行。改法：整段包进 `umaai_core::diag::enabled()`，或把文本
   构造挪进 `diag!` 参数；日志文本保持原样。语义零风险，改完剧本仓 AfterTrain 分配应明显下降。
2. **训练卡效果表缓存**：`default_calc_training_buff` 每个训练候选重算参与卡
   `calc_training_effect`（每回合 3–5 次）。需要引擎提供「本回合卡效果表」的显式缓存钩子，
   失效点在回合边界与羁绊 / 灵感变化处，不能靠剧本侧猜。
3. **`deyilv` 专用访问器**：`MatsuriGame::deyilv` 只为读 `.deyilv` 就走了整条
   `calc_training_effect`。引擎加只读 `deyilv(game, train)`，或让分布接受外部权重表。
4. **每候选 400 KB 直方图**：`radical_factor_max=0` 时 `weighted_mean` 直接取算术均值、
   无人读直方图，但每候选仍分配。引擎侧按因子为 0 走无直方图路径。

**测量与验收**（与第二轮同口径，别把机器热状态当收益）：

- 配对交替：A/B 双方交替先跑、各独立启动；耗时只声明当轮。基线二进制留档
  `logs/perf/base/bench`（`logs/` 不入仓）。
- 必比：30 种子整局 CSV 的 `seed/score/pt/turn` 逐位；固定根 N=256 的 best 与各候选
  mean/wmean/planned；整局 N=256 / N=1024 的终局 score/PT。
- 引擎侧改动在**引擎仓**跑它自己的测试，提交说明写明「默认行为是否等价」；剧本仓侧不应
  出现数值位移（若有，按 `glossary.md` 记口径并重录基线）。

## 3. 关键落点 / 命令 / 纪律

- 测量工具（`matsuri-tools` 单 bin `bench`，仓根执行）：
  - `bench game --games 30 --seed 61444 [--search-n M]`：整局 CSV + 均值
  - `bench stages --turn 32 [--action I]`：单次 rollout 按阶段机拆分（含训练员耗时）
  - `bench micro --turn 40 --iters 3000`：克隆 / 预览 / 整决策 / 解释
  - `bench root --turn 32 --search-n 256 --repeat 3`：固定根（`RAYON_NUM_THREADS` 固定线程）
  - `bench parts --turn 32 --iters 2000`：分布 / 事件生成 / 列动作单点 ns
  - `bench allocs --turn 32`：单次 rollout 分配计数（阶段 + 训练员）
  - `bench flat --turn 32 --rollouts 2048 --threads 24`：平铺线程池，量并行上限
- 构建：`cargo build --release --offline -p matsuri-tools`；同一 target 目录一次只跑一个 cargo。
- 引擎仓：`../umaai_next`；契约 `docs/scenario/contracts.md`，钩子流程 `docs/scenario/repo_form.md` §3；
  并行底盘 `crates/umasim/src/search/{flat_search,par}.rs`。
- 机制 / 估值落点：`crates/matsuri-core/src/{game/{mod,train,event,settle,stage,action,traits}.rs,value.rs,trainer.rs,search.rs}`。
- 引擎调优期间剧本仓 `git diff` 只允许「适配钩子」的最小改动，不混入策略 / 数值调整。
- 种子块已用：61444–61543（基线）、61544–61643、62200–62299、62800–63099、63100–63699、
  63700–63799（Workbench 主仓基线）、63800–63899、63900–63999、64000–64499、64500–64599、
  64700–64799、64900–65999、66000–66099、66100–66299、66400–67199；下一轮 **67200 起**。
- 已否杠杆（不要重复）：白给属性 / `pt_favor_rate=16`、根性稽古软旋钮、单演目 focus、`weights.hint`、
  `advice.hint`、普通训练等级进度价值、`kensan.scale=2`、动机 clamp、助力支撑专项、PT 饱和阈值、
  **两道绑耐**（先验 101/401 换位：根 +49 / 分 +76，t≈1.2，n=1000 → 未达定档线）、
  **两道改配**（year2 `301+601` + year3 `101+1001`：−1926，t=−6.2）、
  **友人外出延后**（`friend_outing_early=false`：+109，t=0.89，n=500）、
  **根权重上调**（0.6/1.0/1.5：−565/−1047/−2273）、**末段 `endgame.level` 加码**（200/400：−5/−25，97% 局不变）。
- 搜索档注意：`--explain` 与 `search_n>0` 互斥；`max_depth>0` 会在构造时拒绝（叶估仅接口）。
- 性能轮次注意：`stages` / `allocs` 的 `Train` 段含训练员决策，扣掉 `trainer.action_us` 才是机制；
  机器热状态会让同一提交的绝对值差到 1.7 倍，跨会话只比同轮配对。

## 4. 文档

- 性能：`docs/perf_profiling.md` §11（第二轮改动 / 配对测量 / 剩余候选）。
- 口径：`docs/glossary.md` §9（2026-11-05「rollout 静默事件明细」等决定记录）。
- 测试：`docs/tests_overview.md`（221 通过 + 2 手动档；搜索档守门与教诲微基准）。
- README 只保留现状与基本信息，不堆更新日志。
