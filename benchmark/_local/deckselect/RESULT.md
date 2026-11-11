# 手写档单卡定卡结果（2026-11-07，500 局/组）

## 1. 执行口径

- 范围：只做 `docs/plan/deck_param_scan.md` §3 定卡；§4 参数扫描另开。
- 训练员：`cardtest` 强制 `search_n=0`（`MatsuriTrainer` 手写档），**未跑 FlatSearch / MCTS**。
- 配置：显式 `--matsuri-config data/matsuri_config.toml`（不合并工作目录覆盖层）；
  行名一律避开 `[decks.*]` 档名，保证整批按 comp 落到同一份默认档参数。
- 夹具：`gen_candidates.py` 复刻 `build_pool` 口径（SSR / 满破 / 剔担当 1059 /
  链接卡只在类型==钉位演目时进池 / 剔同角色），每行只换 1 个训练槽，首行 base 作参照臂。
- 跑测：`--games 500 --seed 85000 --threads 24 --correct holm`，按槽分批（族 m≈50–70）；
  每轮全 5 槽只采用 Δ 最大的 1 处显著替换，最多 2 轮；
  双槽 top2×top2 联合确认；独立块 86000–86499、500 局、`--correct none`。
- 采用判据：`p_adj ≤ 0.05 且 Δ>0`；最终确认取「CI 不跨零」（`|Δ| > ci95`）。

## 2. 分辨率（数字来源）

- 实测配对差分 sd：**p1 ≈ 3318、p2 ≈ 3746**（取各批 `cardtest` 输出
  `# 分辨率：配对差分 sd≈` 的均值；该行是各候选配对差分 sd 的均值）。
- 500 局无校正 CI95 半径 = `1.96 × sd / √500` ≈ **291** 分；
  80% 功效可分辨效应 = `2.8 × sd / √500` ≈ **415** 分。
  Holm 校正后 `ci95` 逐行按档位更宽（最严档约 500 分）。
- 单点复跑校验：`speed_build_p1_s0_30084` 在原批与复跑中 Δ 均为 −5344.5、逐位一致，
  排除批次状态污染。
- 结论口径：500 局能稳定剔除明显劣化卡，**分辨不了 ±200 量级的小幅改进**；
  把 speed 槽0 最优的 +226 定到显著所需局数：无校正 95% CI
  `(1.96×3318/226)² ≈ 830` 局/组、80% 功效 `(2.8×3318/226)² ≈ 1690` 局、
  Holm 最严档（z≈3.4）`(3.4×3318/226)² ≈ 2500` 局。

## 3. 四套 build 结论

| build | comp | 最终卡组 | 采用替换 | p1 配对 Δ | 独立块确认 | 结论 |
|---|---|---|---|---:|---|---|
| speed_build | 21101 | `303174 / 303114 / 303004 / 303224 / 303204 / 303234` | 无 | — | 未跑（无替换） | 保留起始卡组 |
| stamina_build | 12101 | `303114 / 303074 / 303004 / 303224 / 303194 / 303234` | 槽2 → 30300 [耐]樱花千代王 | +921.7 | +919.0 ±297.4 | **采用** |
| guts_build | 20111 | `302754 / 303124 / 303224 / 303084 / 303194 / 303234` | 无 | — | 未跑（无替换） | 保留起始卡组 |
| wisdom_build | 11102 | `303114 / 303044 / 303224 / 303194 / 302384 / 303234` | 槽4 → 30238 [智]生野狄杜斯 | +514.5 | +899.5 ±311.7 | **采用** |

完整排名见 `ranking_all_p1.csv` / `ranking_all_p2.csv` / `<build>_ranking_p*.csv`；
按类型高→低汇总见 `ranking_p1.md` / `ranking_p2.md`。
p1 共 1125 个候选、p2 共 545 个候选；
p1 各 build 显著劣化（X tier）数：speed_build 280、stamina_build 265、guts_build 276、wisdom_build 254。

## 4. 双槽联合确认（条件步骤）

四个 build 的两槽 top2 CI 均重叠，按 §3.1 触发 top2×top2；剔除同卡 / 同角色组合后共 10 组。
下表列每组最佳：

| build | 最佳组合 | Δ vs 起始 base | CI95(holm) | p_adj |
|---|---|---:|---:|---:|
| guts_build | guts_build_joint_0_30311_1_30282 | -119.7 | ±192.6 | 0.3269 |
| speed_build | speed_build_joint_0_30302_1_30312 | +308.1 | ±366.9 | 0.1440 |
| stamina_build | stamina_build_joint_1_30279_2_30300 | +402.2 | ±377.2 | 0.0321 |
| wisdom_build | wisdom_build_joint_3_30320_4_30238 | +197.7 | ±431.6 | 0.8183 |

- stamina 的 `30279 + 30300` 为唯一显著项（+402.2，p_adj 0.0321），但明显劣于
  单槽采用 30300 的 +921.7；按控制变量原则**不采用多槽组合**。
- speed 最优 +308.1（p_adj 0.1440）、guts 全负、wisdom +197.7（p_adj 0.8183），均不显著。

## 5. speed 与基线

- speed 21101 在 p1 无 A tier（最优 +284 ±345，p_adj 未过 0.05），未进入 p2；
  **不触发 §3.4 的默认卡组替换与 `baselines/*.json` 重录**；
  `data/ai_config.toml` 的 `[sim].cards`、seed 61444 冒烟 86053 / PT 11864 均保持不变。
- 因此本轮没有搜索档基线重录动作，与「不做 mcts」一致。

## 6. 本轮未做 / 待办

- 未做 §4 扫参（fusion → advice → status_weights → 其余手写旋钮）；
  四套主卡组夹具已落在 `decks/<build>.csv`，可直接作为扫参固定夹具。
- 未改 `data/matsuri_config.toml` / `data/ai_config.toml` / `baselines/*.json`，
  未碰引擎仓、`matsuri-cli` / online，未跑 `cargo test`，未提交。
- 按 `AGENTS.md`，文档在提交准备阶段统一更新；**待登记到
  `docs/plan/scenario_dev_plan.md` 决策记录的种子块**：
  `85000–85499`（p1/p2 排名 + 双槽联合确认，500 局/组）、
  `86000–86499`（stamina / wisdom 独立块确认，500 局）。

## 7. 产物清单

- `decks/{speed,stamina,guts,wisdom}_build.csv`：主卡组 + 影子行，注释行记种子块/局数/确认结果。
- `logs/deckselect/ranking_p1.md`、`ranking_p2.md`：按 build × 类型的汇总排名。
- `logs/deckselect/ranking_all_p*.csv`、`<build>_ranking_p*.csv`：完整排名（含 tier / CI / panel）。
- `logs/deckselect/summary_p*.json`：机读决策。
- 每批原始 `*.txt` / `*_result.csv`、`*_joint_result.csv`、`*_confirm_result.csv` 全部保留。
- 夹具脚本：`gen_candidates.py` / `make_ranking.py` / `gen_joint.py` / `finalize.py`。
