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

- [ ] **report 分数轨迹改 T(n) 主图**：当前「分数轨迹」画 `facts.score`（当前属性分）；改为 T(n) 期望终局分主图，属性分只作参考并标注「不作判据」。
- [ ] **`report --narrative` 支持 markdown**：当前 markdown 被转义成单个 `<p>`；要么渲染，要么改纯文本输出。

> 已取消：跨局聚合、AI sidecar 记录层、引擎 E1–E3（理由见 `changelog.md` 2026-11-16）。
