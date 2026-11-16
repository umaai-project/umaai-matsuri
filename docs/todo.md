# 活跃待办清单（Todo）

> 本文件收拢跨阶段拆解下来的**真实未完成事项**（已落地与已收口项不留存）。
> 数值占位与待真机核对项见 [`stubs.md`](stubs.md)；Search SH 专项设计见 [`search_sh.md`](search_sh.md)。

---

## 1. 调参、卡组与基线收尾

- [ ] **默认档换耐卡后重扫参数**（原 `deck_param_scan.md` §10.5）：11-11 默认卡组 1 耐位 `30300 → 30324` 换卡后，尚未重扫 `[decks.default]` 参数。
- [ ] **`guts_build`（20111）建档**（原 `deck_param_scan.md` §9.5）：当前回落 `default`；待后续放开 `comp` 构成搜索时一并处理。
- [ ] **GA 适应度与 `wisdom_build` 优化**（原 `deck_param_scan.md` §10.3）：新马娘（`112901` / `114101` / `106301`）进 GA 适应度；改善 `wisdom_build` 主卡组搜索档增益偏小问题。
- [ ] **S9 固定种子基线测试**（原 `scenario_dev_plan.md`）：补齐 `crates/matsuri-sim/tests/baseline.rs`；确认搜索档基线局数（10 局 vs 100 局）口径。

## 2. 剧本估值、随机流与引擎接缝

- [ ] **#8 教诲估值窗口级 horizon**（原 `round_2026-11-09.md` §3）：`value.rs` 改用 9–12 回合窗口级 horizon 与合宿感知 `slot_value`（改动需重录基线）。
- [ ] **#9 下调 `advice.route` 先验**（原 `round_2026-11-09.md` §3）：下调当前偏强的 `advice.route` 先验并重扫。
- [ ] **#10 终局期望落点边际 + `pt_favor_rate` 退火**（原 `round_2026-11-09.md` §3）。
- [ ] **#11 在线 `decision_rng` 加盐**（原 `round_2026-11-09.md` §3）：`crates/matsuri-ai/src/session.rs` `decision_rng(turn)` 按决策类型 / 窗口 / 事件 id 加盐，避免同回合多决策点复用同一组 `RolloutSeeds`。
- [ ] **#12 `max_depth` 叶估与去假令牌**（原 `round_2026-11-09.md` §3）：需要截断叶估时再在引擎侧接线。
- [ ] **#30 凯斯奇迹 T65 迁移**（原 `scenario_dev_plan.md` §4）：引擎「马娘特殊事件记录」改造后，移除 `game/stage.rs` T65 临时表。
- [ ] **#47 自选比赛 `race_grades` 对齐**（原 `scenario_dev_plan.md` §4）：核对 `race_grades` 标准表与真机自选格子。

## 3. 在线、插件与启动器

- [ ] **#13 插件侧修 `isAfterTraining` 粘连**（原 `round_2026-11-09.md` §3 / `review_tool_plan` §3.2）：由插件端改为响应确认或新增 `actionPhase`，并按需补 `festa.events[]`。
- [ ] **启动器边缘扩展（待议）**（原 `release_launcher.md`）：离线模拟菜单与进阶参数透传（`--explain` / `--deck-profile`）、图标与快捷方式、`matsuri-tools` 是否进 `bin/`。

## 4. 复盘工具链（原 `review_tool_plan_2026-11-13.md`）

- [ ] **选择偏差判据统一**：B2/B3/C1/C2 合并为一条判据——`eval` 的 `abs(regret) ≤ ci95` 即命中（实际选择在随机误差内，不展开），显著才分析（第几选、损失、五维/PT 取舍）；`brief`/`report` 增加「实际 / 替代 / 最优」三列与命中标记。不实现 `ai.jsonl` sidecar（原 P0-C 作废）。
- [ ] **`eval --swap <动作名>`**：以指定候选替换实际动作，输出「替代 − 实际」终局差 ± 配对 CI（复用现有候选表与 `OrderedRollouts`）。动作名取 `Action::decision_label`，精确匹配优先、唯一前缀次之；歧义或无匹配时报错并打印该回合候选表。`--all --swap` 仅作逐点敏感性，不作连续换手结论。
- [ ] **统计口径收敛**：新增 `crates/matsuri-tools/src/stat.rs`（`hit` / `paired_ci` / `quantile`，转调 `umaai_core::stats`），替换 `review/eval.rs`、`bin/audit_search.rs`、`bin/rhythm.rs` 三处重复实现，并统一符号为 `best − chosen`。
- [ ] **whatif 反事实（主体已落地）**：`review whatif --turn N --swap <动作名>[,<动作名>…] [--horizon 8|0] [--k 100]`（固定手写训练员，search 口径由 `eval --swap` 承担）；基线默认历史实际动作（失败降级标 `actual_source`），多替代；`--horizon 0` 跑满终局给终局分。统计口径：K=100 手写局（秒级），频数差 >10pt、均值差 >2SE；维度含体力/干劲/五维属性/五维分/演目 Lv/技能点/PT/评分 + 分维。边界：事件回合不做、教诲帧优先、单候选回合提示。`play()` 原语抽取与整局对比暂不做。
- [ ] **节奏分位口径**：以 `benchmark/_local/acts_friend/summary.md` 结论为准，不入库 `baselines/acts_friend_*.json`；`review` 不自动判快慢。

> 已取消：跨局聚合、AI sidecar 记录层、引擎 E1–E3（理由见 `changelog.md` 2026-11-16）。
