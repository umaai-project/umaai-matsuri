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

## 搜索蒸馏权重

`data/distilled.toml` 是手写档的默认蒸馏权重（与 `matsuri_config.toml` 同目录，加载时自动读取；2026-11-24 冻结，源副本 [`policies/distilled-v1.json`](policies/distilled-v1.json)）。权重**不写进** `matsuri_config.toml`，配置里出现 `[distilled]` 段会严格报错。它在守门链之后对普通动作打分叠加动作组残差，训练 / 稽古再叠加预览特征残差；教诲、事件与守门分支不叠加。

配置带权重时，离线（`bench` / `cardtest` / `sweep`）与在线（`matsuri-cli` 手写档）自动启用；**搜索档的 fallback 与 rollout 教师同样带权重**（全链路）。

```shell
cargo build --locked --release -p matsuri-tools --bin bench
# 默认：配置内联权重（手写档）
./target/release/bench game --search-n 0 --uma 105902 --seed 500000 --games 2048
# 纯手写对照
./target/release/bench game --search-n 0 --no-distilled --uma 105902 --seed 500000 --games 2048
# 外部 JSON 覆盖权重（实验；与 --search-n >0 互斥）
./target/release/bench game --distilled-policy benchmark/policies/distilled-v1.json --uma 105902 --seed 500000 --games 2048
```

- `groups` 15 行 = 普通训练五轴、稽古五轴、比赛、休息、普通出行、友人出行、治病；每行 = 常数 + 回合 / 72 + 体力 / 100。`board` 两行 = 训练预览 9 特征的常数系数与回合交互系数；前六个特征是按成功概率加权的五维增量与技能点增量（÷50），后三个是体力增量 / 25、失败率、闪彩人数。
- 与 `[decks.*.trainer.weights]` **不是同一层参数**：weights 是手写 base 的内部换算，本段是叠加在 base 之上的动作级残差，两者联合校准、不可互相折算。
- 覆盖目录放同名 `distilled.toml` 可整段替换权重（不必改默认层）；未放则沿用默认层。
- 训练来源：目白多伯默认卡组 4693 个决策局面拟合；搜索教师以官方终局评分为目标，用手写策略完成后续回合，按配对终局分差拟合相对价值。验收见 [冻结策略配对验收](_local/distillation/benchmark.md)。
- 验收种子块：`500,000–541,999`（14 组 × 2048）与 `3,100,000–3,141,999`（独立复核），不得与定卡 / 扫参 / 基线块复用。
