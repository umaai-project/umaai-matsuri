# 测试一览（骏大祭 / matsuri）

> **前期验证以对话核对为主**：机制是否成立由用户按 [`plan/intake.md`](plan/intake.md) 的确认清单逐条核对，
> 自动化测试随业务落地逐步补齐，不用测试代替机制确认。
> 分工：单元（内联，私有细节 / 纯函数，`assert`）/ 机制集成（`tests/`，打印 + `Result`）/
> 冒烟（`cli/tests/`）/ 基线（`baselines/`，自动判定）。
> 测试 cwd 在各 crate 目录；路径统一经 `matsuri_core::testing` 取仓根。

## 1. 分 crate

| crate | 测试面 | 现状 |
|---|---|---|
| `matsuri-core` | 内联单测 + `tests/data.rs`（L2 数据层）+ `tests/mechanics.rs`（机制集成） | 内联单测 **81** 个 + L2 数据层 **13** 个 + 机制集成 **25** 个 |
| `matsuri-sim` | `tests/baseline.rs`（固定种子基线） | 待补 |
| `matsuri-cli` | `tests/smoke.rs`（一局冒烟 + 可复现） | 待补 |
| `matsuri-ai` / `matsuri-tools` | 骨架，暂无 | — |

## 2. 单元测试（内联）

| 文件 | 用例 | 守门内容 |
|---|---|---|
| `core/src/data.rs` | `parse_repo_data_ok` / `unknown_field_rejected` / `bad_row_width_rejected` / `wrong_act_count_rejected` / `act_effects_regression_rejected` / `advice_level_count_rejected` / `advice_kind_mismatch_rejected` / `fusion_kind_mismatch_rejected` / `check_point_buff_regression_rejected` / `reward_schedule_order_rejected` / `career_end_reward_with_schedule_rejected` / `keiko_classic_row_count_rejected` / `schedule_order_rejected` / `camp_settlement_point_rejected` / `limits_order_rejected` | 资产解析 / 未知字段 / 行宽 / 演目与教诲 id 对齐 / 等级表单调 / 场次顺序 / 合宿结算点 / 稽古两档行数 |
| `core/src/config.rs` | `parse_repo_default` / `unknown_key_rejected` / `overlay_replaces_year2_only` / `duplicate_base_rejected` / `wrong_target_rejected` / `wrong_count_rejected` | 默认层 / 覆盖层 / 未知键 / 融合配对校验（2+3、base 不重复、target 年份内唯一） |
| `core/src/game/roster.rs` | `card_assign_five_speed` / `card_assign_link_pins_and_dispatches` / `card_assign_reference_deck_shape` / `guest_balance_follows_min_persons` | 担当分配五步流水线：memo 三条真机算例（5 速 / 智大和+3速+2毅 / 参考卡组形状）、余额 NPC 落点与并列规则 |
| `core/src/game/stage.rs` | `next_chain` / `stage_ids_are_stable` / `camp_starts_only_at_window_start` / `camp_basic_window` / `camp_race_turn_extends` / `camp_race_at_window_start_extends` / `camp_forced_rest_turn_extends` / `camp_consecutive_races_keep_extending` / `camp_rest_version_only_in_original_window` / `check_point_index_matches_schedule` / `advice_window_index_matches` / `ura_race_turns` | 五阶段链 / 阶段号稳定；合宿窗口起点、基础 4 回合、比赛 / 强制休息顺延、连续顺延、合宿版休息只限原窗口；発表会 / 教诲窗口 / URA 判定 |
| `core/src/trainer.rs` | `preferred_training_wraps` | 样例训练员轮转 |
| `core/src/game/mod.rs` | `lowest_act_follows_three_rule_order` / `gauge_three_rule_skips_full_and_splits_ties` / `gauge_gain_capped_per_act` / `act_level_capped_and_three_rule` / `presentation_bonus_follows_latest_resolved_row` / `judge_presentation_uses_act_levels` / `keiko_period_switches_at_turn_24` / `practice_acts_camp_all_and_gauge_lit` / `train_level_bonus_raises_all_facilities` / `deck_five_types` | 三则落点 / gauge 满槽跳过与上限 / 演目 Lv 夹取 / 发布会 buff 取行与结果判定 / 稽古换档 / 排练集合 / 训练 Lv 提升 / 5 种卡条件 |
| `core/src/game/advice.rs` | `permutation_count_and_uniqueness` / `advice_effect_aggregation` / `slot_effect_uses_bound_act_level` / `fusion_adds_new_and_upgraded_effects` / `participant_advice_stacks_by_count` / `assignment_candidates_enumerate_and_apply` / `enforce_801_on_most_assigned_act` / `kensan_act_found_after_fusion` | 教诲：按绑定演目 Lv 取级 / 融合 = 新教诲 + 固定升级效果 / 参与人数叠加与絆平摊 / 120 排列与校验 / 401+801 硬约束 |
| `core/src/game/distribute.rs` | `absent_weight_uses_scenario_values` / `deyilv_includes_presentation_baseline` / `distribution_locks_act_members_and_fills_friend` / `cooperate_advice_adds_split_appearance` / `finals_extra_member_only_for_edition_cards` / `practice_five_types_lights_all_card_hints` | 分布：不在率 / 得意率基线 / 亮起锁人 + 吾妻补位 + 容量 / 協力与 URA 分身 / 排练全灵感 |
| `core/src/game/train.rs` | `upper_zero_without_scenario_sources` / `upper_presentation_friendship_applies_on_friend_training` / `upper_keiko_flat_and_act_effects` / `upper_special_caps_and_main_gain` / `upper_non_friend_assist_training_effect` / `upper_keiko_vital_cost_drop` / `upper_sp_training_five_types` / `ura_training_effects` / `unyielding_drops_failure_to_zero` / `kensan_triggers_upper_or_lower_profile` / `kensan_triggers_on_normal_training_too` / `gauge_gain_by_participants_with_shining` / `keiko_flat_applies_to_all_five_stats` | 训练上层公式：友情 / 特化 / 助力 / 体力 / SP 排练 / URA / 不屈 / 追加稽古 / 训练入槽 / 全基础 +10 flat |
| `core/src/game/settle.rs` | `first_check_point_great_success_applies_reward` / `check_point_success_when_one_act_short` / `finals_sets_finals_done_and_reward` / `camp_level_up_raises_all_acts` / `advice_windows_acquire_then_fuse` / `career_end_rewards_by_presentation_results` | 発表会 / 本番判定与奖励 / 合宿 +1 / 教诲窗口与融合 / 育成终了 |

### 2.1 L2 数据层集成（`core/tests/data.rs`）

> 从仓根 `data/` 载入**真实资产 + 配置**并抽查关键数值；mdb 重导出过错时在这里失败，
> 不必等 S3+ 跑局才发现。数据来源映射见 [`plan/intake.md`](plan/intake.md) §4.1。

| 用例 | 守门内容 |
|---|---|
| `load_repo_data` / `load_repo_config` | 资产 16 个顶层键可载入并通过校验；配置默认层 = `MatsuriConfig::default()` |
| `limits` / `acts` | 五维基础上限 2200 组 / 绝对上限 2500 / 演目 → 命令 → 训练位 → 神驹映射 |
| `training_table` | 普通训练 Lv1–5（含力量 Lv4/Lv5、根性 Lv5）与稽古 Junior / Classic 两档（S5 恢复） |
| `act_effects` | Lv1 基线 / Lv8 跳档 / Lv9 基础上限 / Lv10 顶点 |
| `advices` | 絆平摊 +3 / 不屈阈值 45→0 / 両道 25% + 15% |
| `fusion_and_check_points` | 転成 5 条 / 本番大成功 buff 20-80-150-100 / 成功档一次性奖励 / 育成终了全大成功 |
| `schedule_camp_rules_and_outing` | 23/47/67 与 Lv3/5/6、教诲窗口、合宿结算点、gauge 入账、羁绊基础值、出行 |
| `rules_s5` | S5 规则常量：上层基础上限 100 / 助力支撑 3×5% / 追加稽古上下档 / 解锁 debuff 简化口径 |
| `engine_events_race_rewards` | 仓内 `data/events.json` 已同步引擎新基础：目标赛 5/72、URA 决赛 40/60/80、非目标赛随机 1 项 |
| `engine_events_matsuri_outing` | 仓内 `matsuri_outing_1..5` 出行事件数值与 `memo` 2026-10-27 一致（第 3 段上 / 下双选项） |
| `finals_act` | 体力 +30 / 干劲 +1 / 失败率 -100 / 追加 1 人 |

## 3. 机制集成（`core/tests/mechanics.rs`）

> 每条机制至少一个用例；机制未确认前不写「按猜测实现」的测试。

| 用例 | 守门内容 | 对应机制文档 |
|---|---|---|
| `turn_structure` | 逐阶段跑满 78 回合：発表会 3 场 / 本番 / URA 收尾期 / URA 决赛 / 教诲窗口 / 合宿窗口与计数 / View 初值（演目 Lv 从 1 起、Lv 上限 10） | §1 / §2 / §5 |
| `camp_forced_rest_extends` | 原窗口最后一回合被强制休息占用 → 不计入 4 回合、顺延一回合、Lv+1 结算点不推迟 | §1 |
| `keis_miracle_t65_forced_rest` | 凯斯奇迹 T65 仍是目标赛回合但实际强制休息（T64 / T66 无标记） | §1 |
| `friend_must_be_ssr_azuma` | 仅 SSR 吾妻可建局：R 吾妻 / 其他友人 / 不带友人报错；SSR 吾妻 → Lv 上限 10 | §2.1 / 建局门禁 |
| `roster_and_act_assignment` | 19 人头名单：前缀同序 / 同 chara 唯一 / 参考卡组卡落位 2-1-1-0-1 / 余额与第 2 批神驹 / link 卡顶替冲突神驹 |
| `person_availability_by_turn` | 吾妻 / 第 1 批神驹 turn 2、第 2 批需 Classic 大成功、记者 turn 13、理事长恒可用 |
| `trained_uma_link_replaces_fixed_guest` | 育成马娘本身是 link 角色时，该演目固定神驹换成随机非 link 顶替 |
| `deck_rules_rejected` | 团队卡 / 同角色两张卡 / 普通卡数量不对 → 建局报错 |
| `deck_inspection` | 编成自查：参照卡组无禁止项且无警告；缺吾妻 / R 吾妻 / 团队卡 / 重复人物 / 多友人 / 含担当马娘均能报出 |
| `deck_warnings_type_concentration` | 5 速卡组：W2 同类 ≥3 + W3 外派 + W4 类型 <5 种 + W10 人物数极差 ≥2；View 同步下发 |
| `deck_warnings_link_type_mismatch` | 速重炮（link 力演目）→ W1 link 类型≠固定演目 |
| `deck_warning_uneven_persons` | 各演目「卡+NPC」人物数极差 ≥2 → W10 |
| `view_snapshot_roundtrip` | `MatsuriView` 值快照 serde 往返 | 契约 View |
| `action_set_by_turn_type` | 普通 / 合宿年 1 / 合宿年 2 / 目标赛 / 强制休息（T65）/ 自选比赛 / URA 决赛的动作集分叉 | §1 / §6 |
| `target_race_reward_applied` | 目标赛事件在 `AfterTrain` 入账：五维 `floor(5×scale)` / SP `floor(72×scale)`，比赛事件计 1 次 | §4 |
| `self_race_reward_applied` | 自选比赛：随机 1 项属性 + 面板 SP，两者都吃比赛加成 | §4 |
| `forced_rest_turn_does_not_race` | 凯斯奇迹 T65 强制休息：不产生比赛事件 / 奖励，只回体 | §1 / §4 |
| `training_failure_has_no_gain` | 体力 0 连续训练触发失败：无属性 / SP 收益、失败事件入账 | §3.1 / §3.4 |
| `rest_normal_and_camp_versions` | 普通休息 30/50/70；合宿版 +40 体 / +1 干劲（原窗口；演目槽 +1 由 gauge 用例覆盖） | §1 / §6 |
| `outing_actions` | 普通出行 1 个通用事件；友人出行 5 段推进 + 每段最低演目 Lv+1（三则 + 槽入账）+ 段事件数值 / 羁绊 +25 + turn 23 発表会奖励 | §2.4 / §6 |
| `keiko_success_levels_up_and_clears_gauge` | 稽古成功演目 Lv+1、槽清零；数值 ≥ 下层 + 全基础 +10 flat | §3.1 / §3.2 |
| `gauge_gains_from_race_and_rest` | 目标赛槽 +4（4 次三则 + 满槽跳过）、自选赛 / 休息 +1 | §2.3 |
| `gauge_gains_from_training_participants` | 训练参与担当按担当归集入槽（闪彩 +2） | §2.3 |
| `s5_mechanism_chain_over_full_run` | 整局：教诲按窗口获得 / 融合、発表会 3 场结算、本番演出激活、URA 收尾 | §5 / §6 |
| `unresolved_events_drained_each_turn` | 78 回合每回合 `AfterTrain` / `End` 后无待结算事件泄漏 | §1 |

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
| 2026-11-01 | S4 前置：剧本人头 / 名单 / 编成检测 | 用户 + Agent | `MatsuriPerson` + `PersonOrigin`（与 `PersonType` 分工）；卡前缀 + 理事长/记者 + 神驹两批；担当分配（link 钉位 / 外派链 / 余额）；**编成检测**：禁止项（缺 SSR 吾妻 / 多友人 / 非 SSR 吾妻友人 / 通用规则）与警告项（W1 link 类型 / W2 同类 ≥3 / W3 外派 / W4 类型 <5 种 / W10 人物数极差 ≥2）随 View 下发；四档绿（62 测例） | 第 2 批神驹人選与随机非 link 取人为离线简化（stubs 登记）；S4 动作与结算待落 |
| 2026-11-01 | S3 局面与阶段机：阶段链 / 回合边界 / 合宿延长 / 账本 / View | 用户 + Agent | 五阶段机 + `TurnPlan`（比赛 / 合宿 / 発表会 / 本番 / 教诲窗口 / URA）+ 合宿动态计数与顺延；结算与教诲分配均在 `End`；合宿 5 动作全可稽古、不改 Lv / 槽；URA 期回到日常机制；参考卡组换新（满凸）+ 仅 SSR 吾妻可建局；T65 强制休息临时记录落位；四档验收绿（50 测例） | 引擎「马娘特殊事件记录」改造后迁移 T65 表；S4/S5 机制待落 |
| 2026-11-01 | S4 动作与结算：动作集 / 比赛 / 训练 / 稽古 / 休息 / 出行 | 用户 + Agent | `Action` 全集 + 按回合类型动作集（合宿期允许自选比赛、治病纳入）；比赛奖励 `floor(面板 × (1+race_bonus+发表会 bonus))`、训练 / 稽古下层公式 + 失败判定、普通 / 合宿版休息、普通出行、吾妻出行 5 段（memo 数值）；`AfterTrain` 事件排空 + 事件羁绊归属 + `Distribute` 过渡；四档绿（39 内联 + 12 L2 + 22 机制 = 73 测例） | 稽古 Lv+1 / 槽清零 / gauge / 発表会 / 上层加成 S5；友人出行真实卡面事件与技能灵感 S6；休息数值待真机 |
| 2026-11-01 | S5 机制：分布 / gauge / 稽古两档 / 演目 Lv 效果 / 教诲 / 训练上层公式 / 発表会·本番·合宿·URA·终局 | 用户 + Agent | 分布重写（亮起锁担当 / 神驹不在率 50 STUB / 吾妻补位 / 協力·転成·URA 分身）；gauge 入账-消费闭环（三则 + 满槽跳过）；`training.keiko.classic` 恢复、turn 24 切档；演目 Lv 效果全列（+10 / 友情 / SP 排练 / 灵感 / SP·基础上限）；教诲 10 种 × 绑定演目 Lv × 参与人数 + 窗口 + 融合（120 排列接口，用户拍板不择优）；训练两层公式 + 主副 SP 拆分 + 上昇量上限 + 失败率；発表会 / 本番按实际 Lv 判定 + 永久 buff + 一次性奖励 + 訓練 Lv+1；合宿 +1；URA 训练效果；育成终了奖励；`MatsuriView` 补字段；四档绿（81 内联 + 13 L2 + 25 机制 = 119 测例）；冒烟 34368 / UE9、PT 6128 | 技能灵感 / 粉丝不建模；发布会成功档 / 上层上限 / 追加稽古上下档 / NPC 不在率 / Senior +1 担当 / U41·U44 待真机；事件链 S6、策略训练员 S7 |
