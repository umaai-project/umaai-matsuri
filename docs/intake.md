# `master.mdb` 数据表字典与资产溯源（`intake.md`）

> 本文件仅保留仓库外 `master.mdb` 的表结构字典、`text_data` 类别索引与 [`data/scenario_matsuri.json`](../data/scenario_matsuri.json) 重导出映射。
> 机制公式与事件明细见 [`mechanism.md`](mechanism.md)，数值占位见 [`stubs.md`](stubs.md)，结构体与载入校验见 [`crates/matsuri-core/src/data.rs`](../crates/matsuri-core/src/data.rs)。

## 1. `master.mdb` 原表与 `text_data` 字典

`single_mode_15_*` 共 11 张表，另有 `single_mode_training_effect` 的 `scenario_id = 15` 行：

| 表名 | 字段 / 形态 | 行数 | 说明 |
|---|---|---|---|
| `single_mode_15_act_effect` | `id, level, condition_type, condition_value, effect_type, effect_value_1..4` | 66 | 演目 Lv1–10；5 演目共用 |
| `single_mode_15_act_info` | `act_type, command_id` | 5 | `act 1–5` → `command 5101–5105` |
| `single_mode_15_act_member` | `id, act_type, chara_id, priority` | 5 | 编成外 5 名神駒；`priority = 100` |
| `single_mode_15_advice` | `id, fusion_type, add_schedule_type` | 10 | 初始 `101–501`；追加 `601–1001` |
| `single_mode_15_advice_effect` | `id, advice_id, level, condition_type, condition_value, effect_type, effect_value_1..4` | 153 | 10 种 × Lv1–10 |
| `single_mode_15_advice_recommend` | `id, turn_num` | 6 | 1 基回合（24 节选/窗口节点） |
| `single_mode_15_check_point_effect` | `id, schedule_id, result_state, effect_type, effect_value_1..4` | 20 | 発表会永久 buff（`effect_type`：`202` 友情 / `4` 得意 / `2` 灵感 / `204` 比赛） |
| `single_mode_15_finals_act_effect` | `id, condition_type, condition_value, effect_type, effect_value_1..4` | 4 | 本番固定加成 |
| `single_mode_15_fusion_effect` | `id, advice_id, condition_type, condition_value, effect_type, effect_value_1..4` | 9 | 転成效果（`17 / 19 / 20`） |
| `single_mode_15_outing_effect` | `id, support_card_id, story_step, add_level` | 10 | 卡 `30323` / `10149`，5 段 |
| `single_mode_15_schedule` | `id, turn_num, need_act_num, need_act_level` | 3 | 3 次発表会门槛 |
| `single_mode_training_effect` | `id, command_id, sub_id, result_state, target_type, effect_value, scenario_id` | 56（15） | 普通训练 + 演目稽古基础值 |

- `single_mode_scenario` row 15：`max_speed 1000 / max_stamina 650 / max_pow 600 / max_guts 500 / max_wiz 600`（仅作静态表参考，非运行时上限）、`upper_limit_* 2500`、`skill_upgrade_num 2`、`gimmick_discount_rate 5000`、`max_skill_point 99999`、`support_num 5`。
- `data/text_data_dict.json` 译文索引（标准术语以 [`../GLOSSARY.md`](../GLOSSARY.md) 为准）：

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

## 2. `scenario_matsuri.json` ← mdb 映射与口径转换

> 回合字段从 `master.mdb` 导入时统一 **1 基 → 0 基**（`turn_num - 1`）；命令覆盖行（`301–304`）与 `106`/`5105` 失败档不入资产。

| 资产节（`MatsuriData`） | 外部来源 | 映射 / 转换规则 |
|---|---|---|
| `scenario_id` | `single_mode_scenario.id` | 常量 `15` |
| `limits` | official + `memo` + `single_mode_scenario` | 五维基础 `[2200,1850,1800,1700,1800]`、上限 `[2500;5]`，按「1200 以上不减半」`x > 1200 ? 2x - 1200 : x` 换算为 `[3200,2500,2400,2200,2400]` 与 `[3800;5]`；`support_num` / `max_skill_point` 取同表 |
| `acts` | `single_mode_15_act_info` + `act_member` + `single_mode_training.base_command_id` | `act_type → command_id(5101–5105) → train(0–4) → npc_chara_id` |
| `training.normal` | `single_mode_training_effect`（Lv1）+ `memo`（Lv2–5） | `[训练 0-4][等级 0-4] → [速 耐 力 根 智 SP 体力]` |
| `training.keiko.junior` / `classic` | `single_mode_training_effect`（5101–5105）/ `memo` | `junior` 取静态表；`classic` 覆盖 Classic 与 Senior（`turn 24` 切换） |
| `act_effects` | `single_mode_15_act_effect`（66 行） | `effect_type`：`105` 全属性 / `202` 友情 / `201.v4` SP 排练 / `106` 灵感事件 / `107` 全灵感 / `109` SP 上限 / `206` 属性上限 / `201.v1=30` (`sp_training_hidden`) |
| `advices` | `single_mode_15_advice` + `advice_effect`（153 行） | `effect_type`：`16/13` 友情、`4` 得意、`2` 灵感、`203/20` 助力、`3` 体力、`13/206` 特化、`8` 研鑽、`202/18` 協力、`205+cond2/203` 不屈、`21/203` 雙壁 |
| `fusion` | `single_mode_15_fusion_effect`（9 行） | 基础教诲 `101–501` 転成效果 |
| `check_points` | `single_mode_15_check_point_effect`（20 行） | 永久 buff 映射（见 §1） |
| `check_point_rewards` / `career_end_rewards` | `memo` + Wiki 文案 | `schedule` 1–3 成功/大成功 + 育成终了全成功/全大成功 |
| `finals_act` | `single_mode_15_finals_act_effect`（4 行） | 本番固定加成 |
| `outing` | `single_mode_15_outing_effect`（10 行） | `30323`/`10149` × `story_step` 1–5 |
| `schedule` / `advice_windows` | `single_mode_15_schedule` / `advice_recommend` | `turn_num - 1` 转 0 基 |
| `scenario_events` / `event_extras` | `single_mode_story_data` + `memo` | `400015104` / `400015106` 及 `(400015106, 0)` |
| `camp` / `rules` | 官方帮助 / `memo` / 数据口径 | 合宿窗口与全局参数 |

> 通用事件库 [`data/events.json`](../data/events.json) 中的赛后奖励（`race_*`）、吾妻友人链（`single_mode_story_data`：`first`/`click`/`out`/`newyear`/`1..5`/`end`）与「大胃王」（`5010`）归属及字段详情见 [`mechanism.md`](mechanism.md) §5。
