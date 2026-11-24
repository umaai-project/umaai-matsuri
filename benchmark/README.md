# benchmark/ — 基准与测卡数据

本目录存放**基准门禁数据、测卡夹具与试验结论归档**。需要留存的结论 `.md` 统一落盘至 `benchmark/_local/<主题>/`；`logs/` 仅用于运行期输出（`data/ai_config.toml` `[record].root`）。

## 目录分工

| 路径 | 跟踪状态 | 用途与说明 |
|---|---|---|
| `README.md` | ✅ 跟踪 | 本文件：目录分工、定卡/扫参门禁与种子块规范 |
| `baselines/` | ✅ 跟踪 | `a-normal.json` / `b-ideal.json` 与 [`baselines/README.md`](baselines/README.md)；`bench baseline` 门禁及 `repo_baselines_parse` 自检对象 |
| `decks/` | ✅ 跟踪 | 长期卡组夹具（`measured` / `panel` / `speed_build` / `stamina_build` / `wisdom_build.csv`），供 `cardtest` 与 `ga_trainer.py` 读取 |
| `_local/**/*.md` | ✅ 跟踪 | 实验结论归档（`.gitignore` `!` 例外放行）。历史记录中偶见的 `logs/<X>`、`baselines/*.json`、`decks/<夹具>.csv` 旧路径分别对应 `_local/<X>`、`baselines/*.json`、`decks/<夹具>.csv` |
| `_local/` 其余文件 | ❌ 忽略 | 原始 CSV / TXT / 帧数据等临时试验产物（`.gitignore` 过滤） |

## 定卡、扫参与种子块规范

- **定卡方法（单卡控制变量）**：固定其余 5 个位置，每次只换 1 个槽位；按 build × 类型/槽位做配对排名，先选主卡组并保留影子卡组对照。
- **判据门禁**：「独立块 95% CI 不跨零 + 影子卡组同向」才允许落入 `data/matsuri_config.toml` 的 `[decks.*]`，遵循「换卡与换参分开」原则。
- **配置入口**：测卡与扫参通过 `cardtest` / `bench` 的 `--matsuri-config` 传入变体配置（`matsuri-cli` 仅读取默认配置）。

| 用途 | 已分配种子块 |
|---|---|
| 基线重录（`a-normal` / `b-ideal`） | `73200`（+ `93000` 独立复核） |
| 定卡排名 / 定卡确认 | `85000–85999` / `86000–86999` |
| 扫参初筛 / 精修复核 / 留出确认 | `87000–89999` / `90000–90999` / `91000–92999` |
| GA `run2`（default）选择 / 复核 / 验收 | `5,000,000+` / `6,000,000` / `7,000,000` |
| GA `run3` 耐（+10M）/ 智（+20M） | `15,000,000+` / `16M` / `17M`；`25,000,000+` / `26M` / `27M` |
| 搜索档验收 | `30,000,000–30,000,085` |

## 延伸指针

- 基线复跑与重录纪律：[`baselines/README.md`](baselines/README.md)
- 测试清单与基线快照：[`docs/tests_overview.md`](../docs/tests_overview.md)
- 性能剖面与火焰图（`bench prof` 缺省输出至 `_local/perf/`）：[`docs/perf_profiling.md`](../docs/perf_profiling.md)

## 可选的搜索蒸馏策略

`bench game --distilled-policy benchmark/policies/distilled-v1.json` 加载冻结权重，使用普通动作打分策略跑整局。显式指定搜索预算时设为 `--search-n 0`。无此参数时沿用原配置。逐局 CSV 沿用 `seed,score,pt,turn,elapsed_us`。

```shell
cargo build --locked --release -p matsuri-tools --bin bench
./target/release/bench game --search-n 0 --uma 105902 --seed 500000 --games 2048
./target/release/bench game --distilled-policy benchmark/policies/distilled-v1.json --uma 105902 --seed 500000 --games 2048
```

权重作用于守门决策之后的打分阶段。`groups` 的 15 行依次为普通训练五轴、稽古五轴、比赛、休息、普通出行、友人出行、治病；每行系数对应常数、回合 / 72、体力 / 100。`board` 两行分别为训练预览系数及其与回合 / 72 的交互系数。前六个特征是按成功概率加权的五维增量和技能点增量（除以 50），后三个是体力增量 / 25、训练失败概率、闪彩人数。

该权重用目白多伯默认卡组的 4693 个决策局面训练。搜索教师以官方终局评分为目标，用手写策略完成后续回合；按配对终局分差拟合动作间的相对价值。评估中普通训练以外的守门、教诲及事件由原策略决定。基准结果见 [冻结策略配对验收](_local/distillation/benchmark.md)。
