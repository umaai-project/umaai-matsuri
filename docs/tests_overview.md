# 测试一览（骏大祭 / matsuri）

> **前期验证以对话核对为主**：机制是否成立由用户按 [`plan/intake.md`](plan/intake.md) 的确认清单逐条核对，
> 自动化测试随业务落地逐步补齐，不用测试代替机制确认。
> 分工：单元（内联，私有细节 / 纯函数，`assert`）/ 机制集成（`tests/`，打印 + `Result`）/
> 冒烟（`cli/tests/`）/ 基线（`baselines/`，自动判定）。
> 测试 cwd 在各 crate 目录；路径统一经 `matsuri_core::testing` 取仓根。

## 1. 分 crate

| crate | 测试面 | 现状 |
|---|---|---|
| `matsuri-core` | 内联单测 + `tests/data.rs`（L2 数据层）+ `tests/mechanics.rs`（机制集成） | 内联单测 24 个 + L2 数据层 11 个；机制集成待补（S3+） |
| `matsuri-sim` | `tests/baseline.rs`（固定种子基线） | 待补 |
| `matsuri-cli` | `tests/smoke.rs`（一局冒烟 + 可复现） | 待补 |
| `matsuri-ai` / `matsuri-tools` | 骨架，暂无 | — |

## 2. 单元测试（内联）

| 文件 | 用例 | 守门内容 |
|---|---|---|
| `core/src/data.rs` | `parse_repo_data_ok` / `unknown_field_rejected` / `bad_row_width_rejected` / `wrong_act_count_rejected` / `act_effects_regression_rejected` / `advice_level_count_rejected` / `advice_kind_mismatch_rejected` / `fusion_kind_mismatch_rejected` / `check_point_buff_regression_rejected` / `reward_schedule_order_rejected` / `career_end_reward_with_schedule_rejected` / `keiko_classic_disabled_rejected` / `schedule_order_rejected` / `camp_settlement_point_rejected` / `limits_order_rejected` | 资产解析 / 未知字段 / 行宽 / 演目与教诲 id 对齐 / 等级表单调 / 场次顺序 / 合宿结算点 |
| `core/src/config.rs` | `parse_repo_default` / `unknown_key_rejected` / `overlay_replaces_year2_only` / `duplicate_base_rejected` / `wrong_target_rejected` / `wrong_count_rejected` | 默认层 / 覆盖层 / 未知键 / 融合配对校验（2+3、base 不重复、target 年份内唯一） |
| `core/src/game/stage.rs` | `next_chain` / `stage_ids_are_stable` | 阶段链 / 阶段号稳定 |
| `core/src/trainer.rs` | `preferred_training_wraps` | 样例训练员轮转 |

### 2.1 L2 数据层集成（`core/tests/data.rs`）

> 从仓根 `data/` 载入**真实资产 + 配置**并抽查关键数值；mdb 重导出过错时在这里失败，
> 不必等 S3+ 跑局才发现。数据来源映射见 [`plan/intake.md`](plan/intake.md) §4.1。

| 用例 | 守门内容 |
|---|---|
| `load_repo_data` / `load_repo_config` | 资产 16 个顶层键可载入并通过校验；配置默认层 = `MatsuriConfig::default()` |
| `limits` / `acts` | 五维基础上限 2200 组 / 绝对上限 2500 / 演目 → 命令 → 训练位 → 神驹映射 |
| `training_table` | 普通训练 Lv1–5（含力量 Lv4/Lv5、根性 Lv5）与稽古 Junior 档（Classic 暂时下架） |
| `act_effects` | Lv1 基线 / Lv8 跳档 / Lv9 基础上限 / Lv10 顶点 |
| `advices` | 絆平摊 +3 / 不屈阈值 45→0 / 両道 25% + 15% |
| `fusion_and_check_points` | 転成 5 条 / 本番大成功 buff 20-80-150-100 / 成功档一次性奖励 / 育成终了全大成功 |
| `schedule_camp_rules_and_outing` | 23/47/67 与 Lv3/5/6、教诲窗口、合宿结算点、gauge 入账、羁绊基础值、出行 |
| `engine_events_race_rewards` | 仓内 `data/events.json` 已同步引擎新基础：目标赛 5/72、URA 决赛 40/60/80、非目标赛随机 1 项 |
| `finals_act` | 体力 +30 / 干劲 +1 / 失败率 -100 / 追加 1 人 |

## 3. 机制集成（`core/tests/mechanics.rs`）

> 每条机制至少一个用例；机制未确认前不写「按猜测实现」的测试。

| 用例 | 守门内容 | 对应机制文档 |
|---|---|---|
| `<待补>` | 演目 / gauge / 稽古 / 教え / 合宿 / 発表会 | `mechanism.md` §1–§6 |

## 4. 冒烟与基线

| 项 | 内容 | 判定 |
|---|---|---|
| 冒烟 | 固定种子跑满回合、两次逐位一致 | `Result` / 非零退出 |
| 基线 | 一基线一 JSON（`input` 自持、`expect` 逐项容差、`meta` 不含路径） | 自动比对，失败返回 `Err` |

- 重录命令：`<cargo ... baseline --update>`（S9 落地）
- 数值占位替换后必须重录，并在 `glossary.md` 记影响面。

## 5. 对话核对记录（前期主验证方式）

| 日期 | 核对范围 | 参与人 | 结论 | 遗留 |
|---|---|---|---|---|
| 2026-10-26 | 建仓 / 代号 / master.mdb 表结构 | 用户 + Agent | 代号 `matsuri`、五 crate 建仓、11 张表结构登记 | 机制数值与回合坐标待收口 |
| 2026-10-26 | `single_mode_15_*` / `training_effect` / `text_data` | 用户 + Agent | 数值与效果编号已解码；回合统一 **0 基**（原始 1 基 -1） | gauge 归属 / 合宿延长 / 友人分身 / 双人头 / 事件回合 |
| 2026-10-26 | 合宿延长 / 合宿期支援卡事件 | 用户 + Agent | 每年保证 4 个非比赛合宿回合；正常合宿不触发支援卡事件 | 延长段连续事件 / Lv+1 结算点待测 |
| 2026-11-01 | S2 数据层：资产 / 配置 / 校验 / L2 | 用户 + Agent | mdb 11 张表 + 训练表 + memo 口径 → 资产 16 个顶层键；配置 = 融合预设；四档验收绿（35 测例）；赛后奖励已接入 `umaai-next` 新基础（`data/events.json`，L2 守门） | 成功档 / 普通训练 Lv2–5 / 非目标赛随机项 待真机复核 |
