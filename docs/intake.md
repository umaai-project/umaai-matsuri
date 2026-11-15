# S0 数据表字典与资产提取映射（`intake.md`）

> 本文件原为 S0 阶段情报接入包；机制公式与数值口径已全量下沉至 [`mechanism.md`](mechanism.md)（含 §9 外部评分参考），
> 数值占位与待真机核对项见 [`stubs.md`](stubs.md)。此处**仅保留 `master.mdb` 数据表字典与 `data/scenario_matsuri.json` 重导出溯源映射**。

---

## 4. 数据表清单

> 来自 `master.mdb`：`single_mode_15_*` 共 11 张，另有 `single_mode_training_effect` 的 `scenario_id = 15` 行。

| 表名 | 字段 / 形态 | 行数 | 状态 | 备注 |
|---|---|---|---|---|
| `single_mode_15_act_effect` | `id, level, condition_type, condition_value, effect_type, effect_value_1..4` | 66 | 已解码 | 演目 Lv1–10；5 演目共用 |
| `single_mode_15_act_info` | `act_type, command_id` | 5 | 已解码 | act 1–5 → command 5101–5105 |
| `single_mode_15_act_member` | `id, act_type, chara_id, priority` | 5 | 已解码 | 编成外 5 名神駒；priority 100 |
| `single_mode_15_advice` | `id, fusion_type, add_schedule_type` | 10 | 已解码 | 初始 101–501；追加 601–1001 |
| `single_mode_15_advice_effect` | `id, advice_id, level, condition_type, condition_value, effect_type, effect_value_1..4` | 153 | 已解码 | 10 种 × Lv1–10 |
| `single_mode_15_advice_recommend` | `id, turn_num` | 6 | 已解码 | turn 2 / 11 / 23 / 35 / 47 / 59（原始 `turn_num` 为 1 基） |
| `single_mode_15_check_point_effect` | `id, schedule_id, result_state, effect_type, effect_value_1..4` | 20 | 已解码 | 発表会永久 buff：`202` 友情 / `4` 得意 / `2` 灵感 / `204` 比赛；一次性奖励另计 |
| `single_mode_15_finals_act_effect` | `id, condition_type, condition_value, effect_type, effect_value_1..4` | 4 | 已解码 | 体力 / 干劲 / 失败率 / 追加人数 |
| `single_mode_15_fusion_effect` | `id, advice_id, condition_type, condition_value, effect_type, effect_value_1..4` | 9 | 已解码 | 転成效果；`17 / 19 / 20` 已对上文案 |
| `single_mode_15_outing_effect` | `id, support_card_id, story_step, add_level` | 10 | 已解码 | 卡 30323 / 10149，5 段各 +1 |
| `single_mode_15_schedule` | `id, turn_num, need_act_num, need_act_level` | 3 | 已解码 | turn 23 / 47 / 67（原始 24 / 48 / 68）；Lv 3/5/6 |
| `single_mode_training_effect` | `id, command_id, sub_id, result_state, target_type, effect_value, scenario_id` | 56（15） | 已提取 | 普通训练 + 演目稽古基础值 |

- `single_mode_scenario` row 15：`max_speed 1000 / max_stamina 650 / max_pow 600 / max_guts 500 / max_wiz 600`（与运行时上限不同），`upper_limit_* 2500`，`skill_upgrade_num 2`，`gimmick_discount_rate 5000`，`max_skill_point 99999`，`support_num 5`。
- `data/text_data_dict.json` 为剧本仓自持译文数据；标准领域术语以 [`../GLOSSARY.md`](../GLOSSARY.md) 为准。

| `text_data` 类别:索引 | 内容 |
|---|---|
| `119:15` / `120:15` | 剧本名 / 剧本介绍 |
| `55:5101–5105` / `462:1–5` | 演目名 |
| `170:1009/1096/1024/1029/1128/9052` | 角色名 |
| `75:30323` / `75:10149` | 吾妻卡名（SSR / R） |
| `463:101–1001` / `63:391–401` | 教诲名 / 玩法帮助 |
| `458:1–66` / `461:1–152` / `466:1–9` / `499:1–4` | 演目 / 教诲 / 转成 / 午式骏大祭演出文案 |
| `333` / `430` | 效果类型 / 汇报会标签 |
| `181:809052xxx` / `181:400015xxx` | 吾妻事件标题（已译）/ 主剧本与链接事件标题（`000–031`, `100–110`, `300–315`，日文原名） |

### 4.1 资产 ↔ mdb 表映射（S2 落地，一次性提取）

> `data/scenario_matsuri.json` 由一次性提取脚本从外部 `master.mdb` 生成（脚本与 mdb 均不入仓）。回合字段已 **1 基 → 0 基**；
> 未知字段 `deny_unknown_fields`、载入期校验见 `crates/matsuri-core/src/data.rs`。重导出时按本表核对。

| 资产节 | 来源 | 形态 / 说明 |
|---|---|---|
| `scenario_id` | `single_mode_scenario.id = 15` | 常量 |
| `limits.five_status_limit_base` | official + `memo` | `[2200,1850,1800,1700,1800]`（不取 `single_mode_scenario.max_*`） |
| `limits.five_status_limit_upper` | `single_mode_scenario.upper_limit_*` | `[2500;5]` |
| `limits.support_num / max_skill_point` | 同表 `support_num` / `max_skill_point` | `5` / `99999` |
| `acts`（5 条） | `single_mode_15_act_info` + `act_member` + `single_mode_training.base_command_id` | `act_type → command_id(5101–5105) → train(0–4) → npc_chara_id` |
| `training.normal`（5×5×7） | **Lv1** = `single_mode_training_effect`；**Lv2–5** = `memo` | `[训练 0-4][等级 0-4] -> [速 耐 力 根 智 SP 体力]` |
| `training.keiko.junior`（5×7） | `single_mode_training_effect` 命令 5101–5105 | 与静态表主属性一致 |
| `training.keiko.classic`（5×7） | `memo` 2026-10-29 / 10-31 | Classic 及以后档（覆盖 Classic 与 Senior，turn 24 切换） |
| `act_effects`（10 条） | `single_mode_15_act_effect`（66 行） | `all_status_bonus`(105) / `friendship`(202) / `sp_training`(201.v4) / `hint_event`(106) / `all_hint`(107) / `sp_limit`(109) / `status_limit`(206) / `sp_training_hidden`(201.v1=30, TBD) |
| `advices`（10 种 × 10 级） | `single_mode_15_advice` + `advice_effect`（153 行） | `friendship`(16/13) / `deyilv`(4) / `hint`(2) / `assist`(203/20) / `vital`(3) / `special`(13/206) / `kensan`(8) / `cooperate`(202/18) / `unyielding`(205+cond2/203) / `dual`(21/203) |
| `fusion`（5 条 = 9 行） | `single_mode_15_fusion_effect`（9 行） | 每个基础教诲（101–501）的転成升级效果 |
| `check_points`（7 条 = 20 行） | `single_mode_15_check_point_effect`（20 行） | 永久 buff：`friendship`(202) / `deyilv`(4) / `hint_rate`(2) / `race_bonus`(204) |
| `check_point_rewards`（6 条）+ `career_end_rewards`（2 条） | `memo` 2026-10-31 + Wiki 文案 | `schedule` 1–3 × 成功/大成功 + 育成终了全成功/全大成功（一次性奖励） |
| `finals_act` | `single_mode_15_finals_act_effect`（4 行） | 体力 +30 / 干劲 +1 / 失败率 -100 / 追加 1 人 |
| `outing`（10 行） | `single_mode_15_outing_effect`（10 行） | 卡 30323 / 10149 × `story_step` 1–5，各 `add_level = 1` |
| `schedule`（3 条） | `single_mode_15_schedule`（3 行） | `turn_num - 1` → 23 / 47 / 67；Lv 3/5/6、`need_act_num = 5` |
| `advice_windows`（6 个） | `single_mode_15_advice_recommend`（6 行） | `turn_num - 1` → 2 / 11 / 23 / 35 / 47 / 59 |
| `camp` / `rules` | 官方帮助 / `memo` / 数据口径 | 3 窗口 (12–15)/(36–39)/(60–63)；gauge 规则、上限、助力与追加稽古参数 |
| `data/events.json`（赛后奖励） | `memo` / 引擎新基础 | 目标赛 `race_career`（5×+5/SP72）；非目标赛 `race_g1–g4`；URA 决赛 `race_ura_*` |
| `data/events.json`（吾妻友人链） | `mdb` `single_mode_story_data` + `memo` | `30323`/`10149`：`first`(001)、`click`(032)、`out`(004)、`newyear`(021)、`1..5`(011–015)、`end`(023) |
| `data/scenario_matsuri.json`（剧本事件） | user + `memo` | `400015104`（合宿の申し出，Fixed 11/35/59）、`400015106`（追加の稽古，Code）；`event_extras` `(400015106, 0)` |
| `data/events.json`（大胃王） | 通用构造 | 马娘事件 `5010`（Random `max_time 3`，选项 2 含 33% 变胖） |

- **重导出自检**：提取脚本内置断言校验普通训练 Lv1 与 mdb、演目稽古 Junior 与静态表 5101–5105 逐值一致。
- **未入资产**：技能表（`skill_data.json` / `skill-pt.json` / `skill-evolutions.json` / `skill-chains.json`）暂未入仓；命令覆盖行（301–304）与 `106`/`5105` 失败档不入资产。
