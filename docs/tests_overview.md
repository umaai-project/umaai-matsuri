# 测试一览（骏大祭 / matsuri）

> **前期验证以对话核对为主**：机制是否成立由用户按 [`plan/intake.md`](plan/intake.md) 的确认清单逐条核对，
> 自动化测试随业务落地逐步补齐，不用测试代替机制确认。
> 分工：单元（内联，私有细节 / 纯函数，`assert`）/ 机制集成（`tests/`，打印 + `Result`）/
> 冒烟（`cli/tests/`）/ 基线（`baselines/`，自动判定）。
> 测试 cwd 在各 crate 目录；路径统一经 `matsuri_core::testing` 取仓根。

## 1. 分 crate

| crate | 测试面 | 现状 |
|---|---|---|
| `matsuri-core` | 内联单测 + `tests/data.rs`（L2 数据层）+ `tests/mechanics.rs`（机制集成）+ `tests/diag.rs`（诊断守门） | 内联单测 **119** 个 + L2 数据层 **15** 个 + 机制集成 **30** 个 + 诊断 **1** 个 |
| `matsuri-sim` | `tests/baseline.rs`（固定种子基线） | 待补 |
| `matsuri-cli` | 内联（呈现件纯函数）+ `tests/manual_smoke.rs`（手动档管道输入跑满整局 + 面板关键块） | 内联 **5** 个 + 冒烟 **1** 个（manual 档）；策略档冒烟 / 基线待补 |
| `matsuri-ai` / `matsuri-tools` | 骨架，暂无 | — |

## 2. 单元测试（内联）

| 文件 | 用例 | 守门内容 |
|---|---|---|
| `core/src/data.rs` | `parse_repo_data_ok` / `unknown_field_rejected` / `bad_row_width_rejected` / `wrong_act_count_rejected` / `act_effects_regression_rejected` / `advice_level_count_rejected` / `advice_kind_mismatch_rejected` / `fusion_kind_mismatch_rejected` / `check_point_buff_regression_rejected` / `reward_schedule_order_rejected` / `career_end_reward_with_schedule_rejected` / `keiko_classic_row_count_rejected` / `schedule_order_rejected` / `camp_settlement_point_rejected` / `limits_order_rejected` | 资产解析 / 未知字段 / 行宽 / 演目与教诲 id 对齐 / 等级表单调 / 场次顺序 / 合宿结算点 / 稽古两档行数 |
| `core/src/config.rs` | `parse_repo_default` / `unknown_key_rejected` / `overlay_replaces_year2_only` / `duplicate_base_rejected` / `wrong_target_rejected` / `wrong_count_rejected` / `trainer_overlay_requires_full_section` / `trainer_overlay_replaces_whole_section` / `trainer_bad_values_rejected` | 默认层 / 覆盖层 / 未知键 / 融合配对校验（2+3、base 不重复、target 年份内唯一）/ **`[trainer]` 段整段替换与范围校验** |
| `core/src/game/roster.rs` | `card_assign_five_speed` / `card_assign_link_pins_and_dispatches` / `card_assign_reference_deck_shape` / `guest_balance_follows_min_persons` | 担当分配五步流水线：memo 三条真机算例（5 速 / 智大和+3速+2毅 / 参考卡组形状）、余额 NPC 落点与并列规则 |
| `core/src/game/stage.rs` | `next_chain` / `stage_ids_are_stable` / `camp_starts_only_at_window_start` / `camp_basic_window` / `camp_race_turn_extends` / `camp_race_at_window_start_extends` / `camp_forced_rest_turn_extends` / `camp_consecutive_races_keep_extending` / `camp_rest_version_only_in_original_window` / `check_point_index_matches_schedule` / `advice_window_index_matches` / `ura_race_turns` / `ura_boundaries_and_interlude` | 五阶段链 / 阶段号稳定；合宿窗口起点、基础 4 回合、比赛 / 强制休息顺延、连续顺延、合宿版休息只限原窗口；発表会 / 教诲窗口；**URA 边界：68–71 空窗期 / 72–77 URA 回合 / 73·75·77 决赛** |
| `core/src/trainer.rs` | `trainer_prefers_heal_when_bad_trainer` / `trainer_reads_trainer_knobs` | 手写训练员守门链：不擅长优先治病；配置旋钮读取 |
| `core/src/value.rs` | `big_eater_choice_follows_vital` / `assignment_puts_801_on_most_assigned_act` / `status_marginal_saturates_at_limit` / `kensan_bonus_uses_participants_for_keiko` | 估值层：事件选项按体力取舍（大胃王）/ 教诲分配 801 硬约束同口径 / 五维边际到上限归零 / **追加稽古兑现按稽古训练位参与者判定（与 try_kensan 同口径）** |
| `core/src/game/mod.rs` | `lowest_act_follows_three_rule_order` / `gauge_three_rule_skips_full_and_splits_ties` / `gauge_gain_capped_per_act` / `scenario_resources_start_at_turn2` / `act_level_capped_and_three_rule` / `presentation_bonus_follows_latest_resolved_row` / `judge_presentation_uses_act_levels` / `keiko_period_switches_at_turn_24` / `practice_acts_camp_all_and_gauge_lit` / `ura_round_all_keiko` / `train_level_bonus_raises_all_facilities` / `deck_five_types` / `max_level_act_still_accumulates_and_practices` | 三则落点 / gauge 满槽跳过与上限 / **剧本机制 turn 2 启动闸门（0–1 不入槽）** / 演目 Lv 夹取 / 发布会 buff 取行与结果判定 / 稽古换档 / 排练集合 / **URA 回合全变稽古（`all_keiko_round` / `practice_acts` 全真；空窗期不启用）** / 训练 Lv 提升 / 5 种卡条件 / **演目 Lv 满（10）后仍可攒 5 槽并排练、排练后槽清空且 Lv 保持 10** |
| `core/src/game/view.rs` | `view_mirrors_game_state` / `view_is_pure_and_idempotent` / `view_schedule_derivations` / `year_boundaries` / `event_log_keeps_recent_and_monotonic_seq` / `view_roundtrip` / `view_exposes_kensan_and_assist_acts` | 回合信息快照：分节字段与局面逐项一致（含 `ura_period` / `interlude` 回合标记、`kensan_act` / `kensan_rate` / `assist_act` 派生）/ 只读幂等 / 赛程派生（発表会门槛 / 合宿总长 / 自选赛过滤）/ 年次边界 / 事件记录滚动 + 序号单调 / serde 往返 |
| `core/src/game/diag.rs` | `diff_reports_only_changes` | 诊断快照变化摘要：只列变化项、格式与面板「变化」同口径 |
| `core/src/game/advice.rs` | `permutation_count_and_uniqueness` / `advice_effect_aggregation` / `slot_effect_uses_bound_act_level` / `fusion_adds_new_and_upgraded_effects` / `participant_advice_stacks_by_count` / `assignment_candidates_enumerate_and_apply` / `enforce_801_on_most_assigned_act` / `kensan_act_found_after_fusion` | 教诲：按绑定演目 Lv 取级 / 融合 = 新教诲 + 固定升级效果 / 参与人数叠加与絆平摊 / 120 排列与校验 / 401+801 硬约束 |
| `core/src/game/distribute.rs` | `absent_weight_uses_scenario_values` / `deyilv_includes_presentation_baseline` / `distribution_locks_act_members_and_fills_friend` / `yayoi_and_reporter_never_on_practice_positions` / `friend_fills_practice_position_when_any` / `at_most_one_social_person_per_position` / `cooperate_advice_adds_split_appearance` / `finals_extra_member_only_for_edition_cards` / `practice_five_types_lights_all_card_hints` | 分布：不在率 / 得意率基线 / 亮起锁人 + 吾妻补位 + 容量 / **理事长·记者不落稽古位（全稽古回合不出现）** / **吾妻有稽古必补未满稽古位（唯一位必中、不去普通位）** / **每训练位至多 1 名友人 / 团队 / 理事长 / 记者（40 seed）** / 協力与 URA 分身 / 排练全灵感 |
| `core/src/game/train.rs` | `upper_*`（8 条）/ `ura_training_effects`（72–77 生效、68–71 空窗期不生效、净回体）/ `train_level_counts_successful_normal_training_and_stacks_bonus`（成功 4 次 +1、稽古不计、与発表会 +1 相加、夹上限 5）/ `unyielding_drops_failure_to_zero` / `kensan_pushes_choice_event` / `kensan_triggers_on_normal_training_too` / `gauge_gain_by_participants_with_shining` / `preview_action_resources_matches_actual` / `keiko_flat_applies_to_all_five_stats` / `friend_click_pushes_first_then_click_by_probability` / `fat_zeroes_training_speed_only` / `fat_cleared_by_clinic_and_camp_rest` / `preview_matches_actual_training` | 训练上层公式 / 失败率 / 追加稽古事件化 / 训练入槽 / **动作资源预览（槽 / Lv）与实结算一致、不改原局面** / 全基础 +10 flat / 友人点击链（首次必发 + 40%）/ 变胖速度归零与解除 / 预览与实结算一致 |
| `core/src/game/event.rs` | `camp_rule_suppresses_random_events` / `fixed_scenario_event_preempts_random` / `friend_unlock_event_by_probability` / `card_event_candidates_exclude_friend` / `friend_out_unlock_and_fatigue_recovery` / `event_flags_are_applied` / `fat_recovery_roll_by_rules_probability` / `event_extra_gauge_and_debuff` / `kensan_upper_event_applies_all_effects` / `evolution_hint_accounted_at_turn66` / `event_hint_level_is_counted` / `after_train_events_queued` / `event_records_are_logged` | S6 事件管线：合宿抑制 / 固定事件 / 友人解锁与卡事件候选 / 出行解锁与疲劳回复 / flags 结算 / 变胖解除 / 事件扩展（gauge + 解 debuff）/ 追加稽古上档完整结算 / turn 66 hint / **事件 `hint_level` 计入 `total_hints`（View 同步；URA 事件本身无 Hint）** / 回合后事件入队 / 事件记录与容量上限 |
| `core/src/game/settle.rs` | `first_check_point_great_success_applies_reward` / `check_point_success_when_one_act_short` / `finals_sets_finals_done_and_reward` / `camp_level_up_raises_all_acts` / `advice_window_zero_does_not_acquire_at_end` / `advice_fusion_after_begin_acquire` / `career_end_rewards_by_presentation_results` | 発表会 / 本番判定与奖励 / 合宿 +1 / **T2 首发在 Begin、End 窗口 0 不重复授予** / 教诲融合 / 育成终了 |
| `cli/src/manual.rs` | `test_paint_respects_switch` / `test_pad_to_counts_display_width` / `test_gauge_text_shape` / `test_advice_fallback_names` / `test_delta_tone` / `test_display_status_compression` / `test_member_color_priority` | 手动面板呈现件：上色开关退化 / CJK 宽度补齐 / 演目槽数字 x/y / 教诲兜底名 / 增减量色调 / **五维显示压缩（>1200 减半，含上限）** / **训练表人物配色优先级（彩圈 > 助力黄 > 类型底色；研鑽不再给人头着色）** |

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
| `rules_s5_s6` | S5/S6 规则常量：上层基础上限 100 / 助力支撑 3×5% / 变胖解除 20% |
| `engine_events_race_rewards` | 仓内 `data/events.json` 已同步引擎新基础：目标赛 5/72、URA 决赛 40/60/80、非目标赛随机 1 项 |
| `engine_events_friend_chain` | 吾妻 `809052xxx` 友人链 10 个槽位全就位（first / click / out / newyear / 出行 1–5 / end）、无温泉残留、出行占位事件已删 |
| `engine_events_big_eater` | 构造事件「大胃王」`5010`：Random max_time 3、二选一、33% 附加变胖（`Ill::FAT`） |
| `scenario_events_asset` | 剧本固定事件 `400015104` 駿大祭合宿の申し出（Fixed 11/35/59，体力+15/干劲+1）与 `400015106`（Code，上下档 + `event_extras`） |
| `finals_act` | 体力 +30 / 干劲 +1 / 失败率 -100 / 追加 1 人 |

## 3. 机制集成（`core/tests/mechanics.rs`）

> 每条机制至少一个用例；机制未确认前不写「按猜测实现」的测试。

| 用例 | 守门内容 | 对应机制文档 |
|---|---|---|
| `turn_structure` | 逐阶段跑满 78 回合：発表会 3 场 / 本番 / **空窗期（68–71）/ URA 回合（72–77）** / URA 决赛 / 教诲窗口 / 合宿窗口与计数 / View 初值（演目 Lv 从 1 起、Lv 上限 10） | §1 / §2 / §5 |
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
| `view_fields_match_game_at_train_decision` | 整局 78 个 Train 决策点：`MatsuriView` 与局面逐项一致（数值 / 资源账本 / 人头 / 分布与闪彩 / 担当 / 训练位 / 赛程 / 友人 / 事件 / `ura_race`·`ura_period`·`interlude` 回合标记），守「面板只被允许从 view 取数」 | 契约 View |
| `action_set_by_turn_type` | 普通 / 合宿年 1 / 合宿年 2 / 目标赛 / 强制休息（T65）/ 自选比赛 / URA 决赛 / **URA 回合（72/74/76：5 个演目稽古 + 休息 / 普通出行 / 治病，不可比赛 / 友人出行）** 的动作集分叉 | §1 / §6 |
| `target_race_reward_applied` | 目标赛事件在 `AfterTrain` 入账：五维 `floor(5×scale)` / SP `floor(72×scale)`，比赛事件计 1 次 | §4 |
| `self_race_reward_applied` | 自选比赛：随机 1 项属性 + 面板 SP，两者都吃比赛加成 | §4 |
| `forced_rest_turn_does_not_race` | 凯斯奇迹 T65 强制休息：不产生比赛事件 / 奖励，只回体 | §1 / §4 |
| `training_failure_has_no_gain` | 体力 0 连续训练触发失败：无属性 / SP 收益、失败事件入账 | §3.1 / §3.4 |
| `rest_normal_and_camp_versions` | 普通休息 30/50/70；合宿版 +40 体 / +1 干劲（原窗口；演目槽 +1 由 gauge 用例覆盖） | §1 / §6 |
| `outing_actions` | 普通出行 1 个通用事件；友人出行 5 段推进（真实卡面事件）+ 每段最低演目 Lv+1（三则 + 槽入账）+ 段事件数值下界 / 羁绊 +25 | §2.4 / §5.4 |
| `keiko_success_levels_up_and_clears_gauge` | 稽古成功演目 Lv+1、槽清零；数值 ≥ 下层 + 全基础 +10 flat | §3.1 / §3.2 |
| `gauge_gains_from_race_and_rest` | 目标赛槽 +4（4 次三则 + 满槽跳过）、自选赛 / 休息 +1 | §2.3 |
| `gauge_gains_from_training_participants` | 训练参与担当按担当归集入槽（闪彩 +2） | §2.3 |
| `s5_mechanism_chain_over_full_run` | 整局：教诲按窗口获得 / 融合、発表会 3 场结算、本番演出激活、**空窗期（68–71）/ URA 回合（72–77）** | §5 / §6 |
| `scenario_fixed_event_at_turn11` | turn 11 行动前 **駿大祭合宿の申し出**（`400015104`）事件结算（体力 +15 / 干劲 +1） | §5.3 / §5.4 |
| `s6_event_chain_over_full_run` | 整局事件链：通用固定（马娘登场 / 新年 / 继承 / 抽签 / 育成结束）、合宿直前友人事件 ×3、友人链（first+click+out+newyear+end）、随机分布、大胃王 ≤3、旧出行占位不再结算 | §5.4 |
| `kensan_event_presented_to_trainer_at_turn_end` | 追加稽古端到端：掷中 + 研鑽担当参与成功训练 → 训练后入队 → **`AfterTrain` 由 Trainer 选择 `400015106`** 并完成结算 | §5.4 / §6.2 |
| `unresolved_events_drained_each_turn` | 78 回合每回合 `AfterTrain` / `End` 后无待结算事件泄漏 | §1 |
| `handwritten_trainer_covers_all_decision_points` | 整局：教诲分配决策恰好 6 次（T2 Begin + 11/23/35/47/59 End）、融合已应用、事件选项决策被调用 | §6 / §6.2 |
| `handwritten_trainer_is_deterministic` | 同种子同配置两次跑局：五维 / PT / 演目账本 / 発表会结果逐位一致 | §6.2 |

### 3.1 诊断守门（`core/tests/diag.rs`）

> 独立测试进程内装一次性捕获日志器；诊断目标 / 埋点齐备 / 静默不位移结果由该文件守门。

| 用例 | 守门内容 |
|---|---|
| `diag_logs_key_state_changes_without_changing_results` | 整局：target 全为 `diagnostic`；回合 78 / 発表会 3 / 分布·动作·训练后·事件·教诲·决策均非空；T0 回合行 / T2 首次授予 / T2 分布抽样；`DiagGuard::suppress()` 期间零产出且同种子终局局面逐位一致 |

## 4. 冒烟与基线

| 项 | 内容 | 判定 |
|---|---|---|
| 冒烟 | 固定种子跑满回合、两次逐位一致 | `Result` / 非零退出 |
| 手动档冒烟 | `manual` 管道输入（全 0）跑满 78 回合（非 TTY 动作选择退回编号输入）；面板关键块（剧本资源 / T2 闸门标记 / 未分配 / 训练表 / 事件回显 / **教诲名称 / 演目槽变化行 / 配色图例**）、S6 关键事件回显（含 **駿大祭合宿の申し出**）、非 TTY 无 ANSI | 退出码 + 文本标记 |
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
| 2026-11-01 | S6 事件管线：固定 / 随机分布 / 合宿抑制 / 友人链 / 追加稽古事件化 / 变胖 / 通用回合后事件 | 用户 + Agent | 判定并落地：主剧本只接 `400015105` + `400015106`（其余记录 / 忽略）；友人链槽位按名字 + memo + 温泉同构核对（first `001` / click `032` 40% / out `004` / newyear `021` / 出行 `011–015` / end `023`；`022/031` 不用）；通用事件按引擎原始值全接（含 turn 48 抽签 / turn 77 育成结束）；追加稽古 50/50 作废改训练员二选一；变胖 = 训练 / 排练速度归零 + 20% 解除 + 治病 / 合宿休息解除；构造事件「大胃王」（max_time 3、33% 变胖）；四档绿（95 内联 + 15 L2 + 27 机制 = 137 测例）；冒烟 44946 / UC6、PT 7496 | 事件⑤ 未接 / 变胖解除率 / 抑制基线层 / 友人槽位映射 待真机复核（大胃王按构造口径收口） |

| 2026-11-01 | S8 先行：手动核对面板（ManualTrainer） | 用户 + Agent | 面板升级：回合结构 / 五维上限 / 状态 / 演目·教诲 / 発表会·buff / 合宿 / 友人链 / 目标赛·自选赛 / 分布（设施 Lv）/ 动作预览与失败率 / 事件回显 / 选项分支 / 上一步变化；核心新增 `EventRecord`（`MatsuriView.recent_events`）+ `preview_training`；manual 档前端 `Quiet`；CLI 冒烟（管道跑满整局）；四档绿（97 内联 + 15 L2 + 27 机制 + 1 CLI = 140 测例） | S7 训练员 / S8 其余（View 字段补全 / 在线外壳 / 选择器）待做 |
| 2026-11-01 | S8 显示升级：彩色训练明细表 + 剧本资源/advice 分配 | 用户 + Agent | 动作决策前新增剧本资源表（演目槽 · Lv · 教诲→転成 · 担当人物）与彩色训练明细表（每训练一列、排练列头为演目名、人物短名 / 未满 100 羁绊 / Hint 亮黄叹号、友人绿 · 彩圈卡亮绿 · 普通支援卡白 · NPC 灰）；発表会 · 永久 buff · 助力支撑 · 追加稽古着色；复用 `umaai-cli` 呈现原语（TTY / `NO_COLOR` 门禁）；四档绿（98 内联 + 15 L2 + 27 机制 + 5 CLI 内联 + 1 CLI 冒烟 = 146 测例） | 纯呈现层（机制 / 结算 / 随机轴不变）；S7 / S8 其余待做 |
| 2026-11-01 | 手动面板第二轮调整 | 用户 + Agent | 动作选择改 Inquire 列表（非 TTY 退回编号输入）；演目槽改数字 x/y；训练表普通支援卡改白色；状态行仅显示已有状态（缺席项含疲劳回复不显示）；0–1 回合资源面板标「剧本机制自 T2 开始 / 担当未分配」 | 呈现辅助；S7 / S8 其余待做 |
| 2026-11-01 | 剧本机制 T2 启动闸门 | 用户 + Agent | 用户核对确认：剧本机制自 turn 2 起，0–1 回合无担当分配、训练 / 休息 / 出行 / 事件不获取演目槽；实现 `SCENARIO_START_TURN` + `MatsuriGame::scenario_started` 在入槽入口拦截；四档绿（98 内联 + 15 L2 + 27 机制 + 5 CLI 内联 + 1 CLI 冒烟 = 146 测例）；样例训练员冒烟 44946 → 40853 / UC、PT 7496 → 6978 | 有（早期槽 / 冒烟位移；S9 前不建基线） |
| 2026-11-01 | 手动面板第三 / 四轮：育成配置 / NPC 短名 / 动作资源预览 / 演目短名 / 教诲结果 / 事件 Inquire | 用户 + Agent | 开局育成配置（马娘 / 卡组 / 继承）；`[嘉宾]`→`[NPC]` 且 NPC 用角色名前两字；动作预览在成功收益后附演目槽增量（核心新增 `preview_action_resources` / `ResourcePreview`，克隆局面只读）；「上一步后变化」增演目 Lv 行；事件选项改 Inquire 且事件名亮蓝；演目名紧凑位置改 2 字短名；教诲窗口后列分配结果（首次传授 / 転成 / 改派·保持）并紧跟刷新后的剧本资源表；非合宿回合不再显示合宿状态；四档绿（100 内联 + 15 L2 + 27 机制 + 5 CLI 内联 + 1 CLI 冒烟 = 148 测例） | 纯呈现 + 只读预览；S7 / S8 其余待做 |
| 2026-11-02 | S7 手写训练员：目标口径 / 旋钮 / 守门链 / 事件选项 / 教诲分配走 Trainer | 用户 + Agent | `[trainer]` 配置段（pt_favor=2.0、阈值、权重、教诲分配权重；覆盖层整段替换 + 范围校验）；守门链 + 官方评分边际打分表；事件选项通用期望值（17 个 player_select 全走）；教诲分配 T2 + 5 窗口合并 120 候选交 Trainer（确定性、不耗决策随机）；融合仍配置预设（`fusion_plan` 留替换点）；四档绿（107 内联 + 15 L2 + 29 机制 + 5 CLI 内联 + 1 CLI 冒烟 = 157 测例）；整局冒烟 54683 / UB9、PT 9024（旧样例 40899 / UC、PT 6982），同种子两次逐位一致 | 融合配对 A/B（3 套）/ 训练员旋钮扫参 / M2 搜索 待做（S9 前不建基线） |
| 2026-11-02 | S8 View 补全 + diag 诊断接线 | 用户 + Agent | `MatsuriView` 升级为回合信息完整快照（B 方案分节：turn / resources / uma / board / schedule / friend / camp + 事件 / 编成警告）；manual 面板改 `view()` 单一数据源（纯呈现）；复用引擎 `diag!` + `log` 依赖 + CLI stderr 日志器（`[output] log_level`）+ 剧本侧关键状态变化埋点（回合 / 动作 / 事件 / 教诲 / 発表会 / 分布 / 决策；预览与 rollout 静默）；四档绿（114 内联 + 15 L2 + 30 机制 + 1 diag + 5 CLI 内联 + 1 CLI 冒烟 = 166 测例）；整局冒烟 54683 / UB9、PT 9024（与 S7 逐位一致） | S8 其余（在线外壳 `matsuri-ai` / 训练员选择器）→ S9 基线 |
| 2026-11-03 | URA 阶段口径修正（用户逐项核对） | 用户 + Agent | ①URA 训练限定效果**只在 72–77 生效**（`ura_training_active` / `place_splits`）；②`TurnPlan.interlude` + `MatsuriView.turn.interlude`（68–71 空窗期，不属于 URA），`ura_period` 改 72–77，panel / 日志标「空窗期」；③体力顺序 = 先耗体下降 / 训练扣体 / 失败率下降，再回体（净回体）；④演目 Lv 满后仍可攒槽 / 排练 / 清槽（Lv 保持 10）；⑤URA 期是否全变稽古（**已被 2026-11-04 更正**）；⑥事件 `hint_level` 计入 `total_hints`，诊断「变化」/ 面板变化增「灵感±N」（URA 事件本身无 Hint）；⑦72–77 不可友人出行 / 自选比赛，72/74/76 可训练·稽古·休息·普通出行·治病；四档绿（118 内联 + 15 L2 + 30 机制 + 1 diag + 5 CLI 内联 + 1 CLI 冒烟 = **170 测例**）；整局冒烟 54683 / UB9、PT 9024 → **53083 / UB7、PT 8858**（68–71 不再吃 URA 训练加成；S9 前不建基线） | 全变稽古 / URA 事件 hint 已由 2026-11-04 更正；S8 其余 → S9 基线 |
| 2026-11-04 | URA 期全变稽古 + URA 事件无 Hint（用户更正） | 用户 + Agent | ①URA 期（72–77）**全变演目稽古（类似合宿动作集）**：5 训练位全 `ActTrain`、`practice_acts` 全真（锁担当 / 全灵感 / 编成卡 +1 人按全排练口径）；72/74/76 可休息 / 普通出行 / 治病，73/75/77 只有比赛；不可自选赛 / 友人出行；②**URA 稽古成功仍按日常 Lv+1 / 清槽**（URA 不是合宿，不冻结）；③**URA 事件无 Hint（无技能）**——「URA 期事件 hint 需要记入」作废，通用事件 `hint_level` 仍计入 `total_hints` 并展示；四档绿（119 内联 + 15 L2 + 30 机制 + 1 diag + 5 CLI 内联 + 1 CLI 冒烟 = **171 测例**）；整局冒烟 53083 / UB7、PT 8858 → **55664 / UA、PT 9136**（URA 全变稽古后位移；S9 前不建基线） | URA 稽古是否也按合宿冻结 Lv/槽 待确认；S8 其余 → S9 基线 |
| 2026-11-01 | 教诲首次授予提前到 turn 2 Begin | 用户 + Agent | 用户核对「NPC / 友人在 T2 开始时加入后立即分配」「教诲效果要对 T2 训练生效」；`stage.rs` Begin（事件链后、分布 / 训练前）调 `acquire_advice`；`settle.rs` End 窗口 0 改空操作；机制链新增「T2 Begin 已授予 / T1 前为零」断言；四档绿（148 测例）；样例训练员冒烟 40853 → 40899 / UC、PT 6978 → 6982 | 有（数值位移；S9 前不建基线） |
| 2026-11-05 | 手动面板显示第五轮 + 追加稽古/助力支撑显示与判定核对 + 合宿直前事件改挂 | 用户 + Agent | ①教诲提示补名称；②追加稽古 / 助力支撑显示：`ResourceView` 增 `kensan_act` / `kensan_rate` / `assist_act`，资源行标掷中演目 / 触发演目，动作候选标 `追稽✓` / `助力支撑✓`，训练表图例，回合末未兑现诊断「未选择追加稽古训练」；③生涯赛 +4 / 休息 / 出行入槽显示（面板「变化」`【演目槽】` 行 + 手写档 `【演目槽】… 入槽` diag）及 `value.rs::kensan_bonus` 稽古参与者口径修正；④合宿直前固定事件改挂 `400015104 駿大祭合宿の申し出`（数值沿用 STUB）；⑤五维显示值按 >1200 减半（含上限，评分仍原始值）；⑥全部事件名亮蓝；⑦训练表人物配色（非彩圈研鑽担当亮紫含 NPC / 稽古列助力担当黄）。四档绿（**121 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 176 测例**）；整局冒烟 **55664 / UA、PT 9136**（与上轮逐位一致） | 合宿直前事件归属 / 数值待真机复核（wiki 线索 1 年目 +10）；助力支撑获取估值留扫参 |
| 2026-11-05 | 友人 / 理事长记者落位修正（用户实测） | 用户 + Agent | ①**理事长 / 记者不会出现在稽古训练位**：有稽古位时只落非稽古位（先判不在）；全稽古回合（合宿 / URA / 5 槽全满）本回合完全不出现（例：4 稽古 + 1 普通 → 仍可能出现在那 1 个普通位）。②**吾妻：只要有稽古训练就必补某个未满 5 人的稽古位**——补位顺序改到锁位后、其余人头前（保证必定登场）；不判不在、不去普通位；只有一个稽古位且未满 5 人时必定命中；无稽古回合走通用不在判定。实现：`practice_train_mask` / `place_guest_outside_practice` / `distribute_round` 顺序调整；新增 `yayoi_and_reporter_never_on_practice_positions` / `friend_fills_practice_position_when_any`（含 50 seed 唯一稽古位必中）。四档绿（**123 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 178 测例**）；manual 面板逐回合核实 45 个有稽古回合 0 违规；整局冒烟 **49587 / UB2、PT 8433**（seed 42，分布修正后位移）→ **56210 / UA1、PT 9131**（**基准种子改为 61444** 后；S9 前不建基线） | 冒烟位移已记录；后续真机可复核理事长 / 记者出现率 |
| 2026-11-05 | 追加稽古呈现调整 + 训练位社交人头唯一性 | 用户 + Agent | ①**追加稽古呈现**：资源行「追加稽古 掷中·演目(概率)」→ 亮黄「**可触发追加稽古**」（不写概率 / 演目）；训练表中**该列参与者含研鑽担当**的列标题下加一行紫色「追加稽古」（与触发判定同口径、可多列；比赛回合等无训练位的掷中不提示）；取消人头紫色着色与动作后缀「追稽✓」；诊断日志保留「掷中」。②**每训练位至多 1 名「友人 / 团队 / 理事长 / 记者」**（引擎 `Person::is_friend()` 口径）：修复剧本侧自定义放位漏校验导致理事长 / 记者同列，新增 `at_most_one_social_person_per_position`（40 seed）。四档绿（**124 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 179 测例**）；manual 面板逐回合核实：12 个可触发回合均标对列（0 错标）、0 处社交人头同列、0 处「掷中 / 追稽」残留；冒烟 **56489 / UA1、PT 9207**、`random` **23010 / UG8、PT 4644**（分布随机序列变化位移；S9 前不建基线） | 真机可复核「可触发」文案与列标题指示是否直观 |
