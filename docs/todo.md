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

- [ ] **教诲估值窗口级 horizon**：`value.rs` 改用 9–12 回合窗口级 horizon 与合宿感知 `slot_value`，先做全 120 vs Top-16 召回率/遗漏收益对照。
- [ ] **`advice.route` 先验与终局边际重扫**：下调过强先验并评估 `pt_favor_rate` 退火，重录基线。
- [ ] **锁位 CRN 余项**：只剩「有稽古 vs 无稽古」的结构差异——友人补位与访客落位走不同规则；要继续闭合需改规则本身。
- [ ] **搜索失败与新鲜度门禁**：失败帧可重试；搜索结果绑定 generation/state_hash；rollout 失败结果标记 invalid/degraded，不直接正常排名。
- [ ] **训练/协议验算守门**：训练帧 lower/upper 必须为 7 项；验算同时比较收益和 failure_rate；`campRemain` / `assistSupportLeft` 区分 None 与 0。
- [ ] **`max_depth` 叶估与去假令牌**：需要截断叶估时在引擎侧接线；PendingDecision/step+choose 是否重构另行拍板。
- [ ] **凯斯奇迹 T65 迁移**：引擎具备马娘特殊事件记录后，移除 `game/stage.rs` 临时表。
- [ ] **自选比赛与引擎数据契约**：核对 `race_grades`、RaceMask、free-race 校验和通用事件状态位/RNG 轴。

## 3. 在线、插件与启动器

- [ ] **启动器边缘扩展（待议）**（原 `release_launcher.md`）：离线模拟菜单与进阶参数透传（`--explain` / `--deck-profile`）、图标与快捷方式、`matsuri-tools` 是否进 `bin/`。

## 4. 复盘工具链（原 `review_tool_plan_2026-11-13.md`）

- [ ] **report 分数轨迹改 T(n) 主图**：当前「分数轨迹」画 `facts.score`（当前属性分）；改为 T(n) 期望终局分主图，属性分只作参考并标注「不作判据」。
- [ ] **`report --narrative` 支持 markdown**：当前 markdown 被转义成单个 `<p>`；要么渲染，要么改纯文本输出。

- [ ] **在线常驻实测运气分写入**：zip 读取、三态文案、防覆盖门禁已完成（2026-11-19），待一局从 T0 常驻的在线搜索档确认 `ai-decision.json` 的 `luck` 逐回合写入。

> 已取消：跨局聚合、AI sidecar 记录层、引擎 E1–E3（理由见 `changelog.md` 2026-11-16）。
