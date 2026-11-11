# benchmark/ — 基准与测卡数据

本目录放**基准门禁数据、测卡夹具与试验归档**。`logs/` 只作运行时输出（`.gitignore` 整目录忽略），
不再承载需要留存的结果；该约定已写入 `AGENTS.md` §输出目录约定。

## 目录分工

| 路径 | 是否入库 | 内容 |
|---|---|---|
| `README.md` | ✅ 跟踪 | 本文件：分工、跟踪口径、复跑方式 |
| `baselines/` | ✅ 跟踪（3） | `a-normal.json` / `b-ideal.json` + `README.md`。`bench baseline` 门禁数据、`repo_baselines_parse` 自检对象，人工维护，口径变更须重录 |
| `decks/` | ✅ 跟踪（5） | `measured.csv` / `panel.csv` / `speed_build.csv` / `stamina_build.csv` / `wisdom_build.csv`。`cardtest` 与 `ga_trainer.py` 的默认卡组夹具 |
| `_local/*.md` | ✅ 跟踪（25） | 结论文档：`FINAL` / `RESULT` / `STATUS` / `REPORT` / `SUMMARY` / `STAGE_*` / `ranking_*` / 退役备份 `README.md`。由 `.gitignore` 的 `!` 例外放行，**入库** |
| `_local/` 其余 | ❌ 忽略（173） | 试验产物：原始 CSV / TXT / TOML、回放帧、一次性卡组夹具等，由 `.gitignore` 的 `/benchmark/_local/**` 过滤 |

**跟踪口径：`benchmark/` 共 34 个文件入库（3 基线 + 5 夹具 + 本 README + 25 结论 md），
其余 173 个试验产物不入库。** 结论 md 与配套原始行同目录存放，但只有 md 随仓库分发。
`.gitignore` 的三段写法不能合并：先排除 `_local/` 全部内容，再放开子目录，最后放开 `*.md`——
父目录若被排除，`!` 例外不会生效。

## `_local/` 里有什么

按原目录名平铺，来源均为 `logs/` 里被文档当作结论或复现依据引用的数据：

- `newcard/` — 2026-11-10 新卡测卡（`RESULT.md` + 配对 CSV / TXT，36 文件）
- `deckselect/` / `advscan/` / `statusscan/` / `knobscan/` — 四轮扫描的 `FINAL.md` / `RESULT.md` / `STATUS.md` / `STAGE_*.md`、`ranking_p*.md`，以及结论数字来源 `REVIEW_*_result.csv`、`*_result.csv`
- `deckselect/retired_2026-11-07/` — 文档引用的退役夹具备份
- `decks/newcard_3032*.csv` — 11-10 一次性测卡夹具（18 个，与上面 5 个长期夹具区分）
- `replay_*` / `scan/` / `_scratch/` / `cardtest_*` / `perf/` / `inherit_sweep/` — 在线回放帧、扫参夹具、3000 局测卡结果、性能与继承报告

其中 **25 个 `*.md` 结论文档入库**，随仓库分发：README、`docs/plan/deck_param_scan.md`、
`docs/plan/round_2026-11-09.md` 里的「详见 `benchmark/_local/.../FINAL.md`」在克隆仓库后仍可查。
其余 173 个原始产物（CSV / TXT / 帧数据）**不入库**，克隆后这些数字来源需按下面的方式复跑或从备份还原。

## 复原与备份

- 清理前 `logs/` 的**完整快照**：`../Backup/umaai-matsuri-logs-20261111/`（2289 文件 / 711,380,024 bytes，
  已核对文件清单、字节总数与 25 个结论文档 md5）。`_local/` 里的内容都在其中。
- **被清出的 2109 个文件（隔离区）**：`../Backup/_trash-umaai-matsuri-logs-20261111/`。
  本环境系统回收站 API 不可用，按「不永久删除」纪律改为移到隔离区：
  `logs/` 已清空，但内容可完整还原。确认无需回溯后可手动删除该目录释放约 575 MB（完整快照仍保留）。
- 全量 md5 对账已通过：备份 2289 个文件在「隔离区 + `_local/`」中**零缺失**；相对 logs 快照额外 27 个文件为
  本目录的 8 个夹具 / `README.md` 与 18 个 `newcard_*.csv`（原本在仓根 `decks/`，不在 logs 快照内）。
- 重新生成：`cardtest run` / `bench` / `sweep` 的输出按 `docs/plan/deck_param_scan.md`、
  `docs/tests_overview.md`、`docs/perf_profiling.md` 的命令复跑；回放帧取自真机帧目录。
- 基线重录仍按 `baselines/README.md` 与 `docs/changelog.md` 写入纪律执行，路径已改为
  `benchmark/baselines/*.json`。

## 关于历史文档里的旧路径

`docs/changelog.md` 遵守「只追加、不改历史行」，2026-11-10 及更早的决定行里仍有
`baselines/*.json`、`logs/knobscan/FINAL.md`、`logs/deckselect/retired_2026-11-07/` 等旧写法。
映射规则：`logs/<X>`（结论 / 备份 / 帧 / 夹具）→ `benchmark/_local/<X>`；
`baselines/*.json` → `benchmark/baselines/*.json`；`decks/<夹具>.csv` → `benchmark/decks/<夹具>.csv`。
`logs/diag/prior_ab/`、`logs/perf/base/bench*` 等纯试验产物已随清理移除，仅在上方备份中保留。

`docs/plan/scenario_dev_plan.md` 2026-11-07 决策行里的「实验输出入 `logs/`（已忽略）」是旧约定，
自 2026-11-11 起以 `AGENTS.md` §输出目录约定为准，历史行不改写。

## 清理范围（2026-11-11）

`logs/` 里除本节列出的 198 个归档文件外，其余 2109 个旧 stdout/stderr 日志、旧测试输出与试验中间产物
（大头为 `logs/perf/scratch/` 441M 的构建产物）已移入上述隔离区；`logs/` 目录本身保留，供
`data/ai_config.toml [record].root = "logs"` 与 `bench prof` 缺省输出使用。
