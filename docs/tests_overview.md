# 测试一览（骏大祭 / matsuri）

> **前期验证以对话核对为主**：机制是否成立由用户按 [`plan/intake.md`](plan/intake.md) 的确认清单逐条核对，
> 自动化测试随业务落地逐步补齐，不用测试代替机制确认。
> 分工：单元（内联，私有细节 / 纯函数，`assert`）/ 机制集成（`tests/`，打印 + `Result`）/
> 冒烟（`cli/tests/`）/ 基线（`baselines/`，自动判定）。
> 测试 cwd 在各 crate 目录；路径统一经 `matsuri_core::testing` 取仓根。
> **测试基准马娘 = 105902（目白多伯）**；106301 / 106302 只做「铁心」专项验证，不作普适调参 / 基线（评分虚高）。
> 基线复跑：`bench baseline [--name …] [--trainer hand|search|both] [--games N]`，判定在 `matsuri-sim::baseline`。

## 1. 分 crate

| crate | 测试面 | 现状 |
|---|---|---|
| `matsuri-core` | 内联单测 + `tests/data.rs`（L2 数据层）+ `tests/mechanics.rs`（机制集成）+ `tests/restore.rs`（还原层）+ `tests/diag.rs`（诊断守门）+ `tests/search.rs`（搜索档） | 内联单测 **172** 个（含 1 个 `#[ignore]` 微基准）+ L2 数据层 **16** 个 + 机制集成 **36** 个 + 还原层 **17** 个 + 诊断 **1** 个 + 搜索 **4** 个（另 1 个 1024 手动 `#[ignore]`） |
| `matsuri-sim` | `tests/baseline.rs`（固定种子基线） | 待补 |
| `matsuri-cli` | 内联（呈现件纯函数）+ `tests/manual_smoke.rs` + `tests/explain_smoke.rs` + `tests/search_smoke.rs` | 内联 **14** 个 + 冒烟 **3** 个（manual / explain / search）；基线待补 |
| `matsuri-ai` | 内联 17 + `tests/replay.rs` 2 + `tests/w2.rs` 3（诊断）+ `tests/layers.rs` 1（诊断） | 帧解析（含 `charaEffects` 全量状态 / `storyId`）/ **按帧内卡组选档** / **路由（事件帧查不到数据 → 跳过，不回落训练）** / 终局帧门禁 / 回放顺序与局末终局帧 / 一帧一决策 / 训练验算与偏差拆解 |
| `matsuri-tools` | 性能测量 bin `bench`（整局 / 分段 / rollout 分布 / 固定根） | 无单测（测量工具，按 docs/perf_profiling.md 复现） |

## 2. 单元测试（内联）

| 文件 | 用例 | 守门内容 |
|---|---|---|
| `core/src/data.rs` | `parse_repo_data_ok` / `unknown_field_rejected` / `bad_row_width_rejected` / `wrong_act_count_rejected` / `act_effects_regression_rejected` / `advice_level_count_rejected` / `advice_kind_mismatch_rejected` / `fusion_kind_mismatch_rejected` / `check_point_buff_regression_rejected` / `reward_schedule_order_rejected` / `career_end_reward_with_schedule_rejected` / `keiko_classic_row_count_rejected` / `schedule_order_rejected` / `camp_settlement_point_rejected` / `limits_order_rejected` | 资产解析 / 未知字段 / 行宽 / 演目与教诲 id 对齐 / 等级表单调 / 场次顺序 / 合宿结算点 / 稽古两档行数 |
| `core/src/config.rs` | `parse_repo_default` / `unknown_key_rejected` / `overlay_replaces_pre_and_search` / `deck_overlay_rejected` / `search_scope_overlay_and_unknown_key` / `for_deck_falls_back_to_default` | 默认层与 `[decks.default]` 同值、耐 / 智档按构成自动触发 / 覆盖层只接受 `[pre]` / `[search]` / 未知键 / 卡组档不受覆盖层影响 / 未匹配 comp 回落 default |
| `core/src/deck_config.rs` | `default_matches_builtin` / `patch_deep_merges_trainer_and_advice` / `variant_is_explicit_only` / `duplicate_non_variant_comp_rejected` / `missing_default_rejected` / `bad_advice_permutation_rejected` / `unknown_trainer_key_rejected` | `[decks.default]` 与 `DeckParams::default()` 逐字段一致；trainer 逐字段 / advice 分窗口 / fusion 分段深合并；`variant` 只显式可选；同 comp 非 variant 重复 / 缺 default / 教诲非排列 / 训练员未知键均报错 |
| `core/src/game/roster.rs` | `card_assign_five_speed` / `card_assign_link_pins_and_dispatches` / `card_assign_reference_deck_shape` / `guest_balance_follows_min_persons` | 担当分配五步流水线：memo 三条真机算例（5 速 / 智大和+3速+2毅 / 参考卡组形状）、余额 NPC 落点与并列规则 |
| `core/src/game/stage.rs` | `next_chain` / `stage_ids_are_stable` / `camp_starts_only_at_window_start` / `camp_basic_window` / `camp_race_turn_extends` / `camp_race_at_window_start_extends` / `camp_forced_rest_turn_extends` / `camp_consecutive_races_keep_extending` / `camp_rest_version_only_in_original_window` / `check_point_index_matches_schedule` / `advice_window_index_matches` / `ura_race_turns` / `ura_boundaries_and_interlude` | 五阶段链 / 阶段号稳定；合宿窗口起点、基础 4 回合、比赛 / 强制休息顺延、连续顺延、合宿版休息只限原窗口；発表会 / 教诲窗口；**URA 边界：68–71 空窗期 / 72–77 URA 回合 / 73·75·77 决赛** |
| `core/src/trainer.rs` | `trainer_prefers_heal_when_bad_trainer` / `explain_action_reports_candidates_and_reason` | 手写训练员守门链：不擅长优先治病；解释 API 与决策同源 |
| `core/src/game/advice.rs` | `microbench_participant_advice`（`#[ignore]`） | 参与教诲聚合微基准：`participant_advice` / `advice_slot_effect` / `shining_count` 的 ns/op（性能轮次立项依据，不参与常规测试） |
| `core/src/search.rs` | `event_token_roundtrip` / `score_leaf_returns_search_score` / `search_consumes_one_decision_draw` / `search_summary_matches_selected_candidate` / `baseline_and_stdev_weight_by_samples` / `deadline_turn_searches_race_only` | 事件搜索 dummy token 往返；叶估占位接口与搜索分同口径；**搜索恰好消费一个决策流随机字（ramen 口径），局面规则流独立派生**；**期望分与单局终局 σ 按样本数加权合并（无样本 → 0）**；**自选比赛期限回合只搜比赛（中选不变、只取一个决策流随机字、基线 = 比赛分）** |
| `cli/src/online.rs` | `candidate_color_has_three_tiers` / `render_luck_appends_expected_score` / `deck_line_shows_comp_and_profile` | 候选分档配色；**生效档行 `卡组 12101 → 档 …`（构成算不出标「无法识别」）**；**运气行 `期望 E （±S，档）` 的拼装 / 上色（E 与档亮青、σ 不着色）/ 退化（无摘要 / σ=0 / 档位空）** |
| `core/src/game/traits.rs` | `apply_root_action_uses_strategy_stream` / `search_score_uses_pt_favor_knob_and_actual_failed_score` | 搜索根动作走剧本自持策略流；**搜索分用 `pt_favor_rate` 口径，失败分支按实际分** |
| `core/src/value.rs` | `big_eater_choice_follows_vital` / `assignment_puts_801_on_most_assigned_act` / `status_marginal_saturates_at_limit` / `kensan_bonus_uses_participants_for_keiko` / `overflow_tilts_away_from_capped_axis` / `overflow_keep_preserves_secondary_value_when_main_capped` / `endgame_keiko_pressure_boosts_full_gauges` / `endgame_window_excludes_ura` / `endgame_window_starts_after_third_camp` / `flags_value_signs_benefit_and_harm` / `assignment_value_rejects_out_of_range_slots` | 估值层：事件选项按体力取舍（大胃王）/ 教诲分配 801 硬约束同口径 / 五维边际到上限归零 / **追加稽古兑现按稽古训练位参与者判定（与 try_kensan 同口径）** / **溢出层按生效上限近顶并保留副属性 / PT** / **末段窗口（第三年合宿后～T71）满槽稽古加分与普通训练机会成本；URA 不进入** |
| `core/src/game/mod.rs` | `lowest_act_follows_three_rule_order` / `gauge_three_rule_skips_full_and_splits_ties` / `gauge_gain_capped_per_act` / `scenario_resources_start_at_turn2` / `act_level_capped_and_three_rule` / `presentation_bonus_follows_latest_resolved_row` / `race_scale_multiplies_deck_and_scenario_bonus` / `dictus_iron_heart_bonus_window_and_kind` / `judge_presentation_uses_act_levels` / `keiko_period_switches_at_turn_24` / `practice_acts_camp_all_and_gauge_lit` / `ura_round_all_keiko` / `train_level_bonus_raises_all_facilities` / `deck_five_types` / `max_level_act_still_accumulates_and_practices` | 三则落点 / gauge 满槽跳过与上限 / **剧本机制 turn 2 启动闸门（0–1 不入槽）** / 演目 Lv 夹取 / 发布会 buff 取行与结果判定 / **比赛系数乘算（卡面赛后 × 剧本赛后；本番前 / 成功 ×1.3 / 大成功 ×2.0）** / **狄杜斯铁心（T12–71 生涯赛 五维×2.5 / PT×0.46，与卡面乘算；T11 / 自选 / URA 不加；View 同步）** / 稽古换档 / 排练集合 / **URA 回合全变稽古（`all_keiko_round` / `practice_acts` 全真；空窗期不启用）** / 训练 Lv 提升 / 5 种卡条件 / **演目 Lv 满（10）后仍可攒 5 槽并排练、排练后槽清空且 Lv 保持 10** |
| `core/src/game/view.rs` | `view_mirrors_game_state` / `view_is_pure_and_idempotent` / `view_schedule_derivations` / `year_boundaries` / `event_log_keeps_recent_and_monotonic_seq` / `view_exposes_kensan_and_assist_acts` | 回合信息快照：分节字段与局面逐项一致（含 `ura_period` / `interlude` 回合标记、`kensan_act` / `kensan_rate` / `assist_act` 派生）/ 只读幂等 / 赛程派生（発表会门槛 / 合宿总长 / 自选赛过滤）/ 年次边界 / 事件记录滚动 + 序号单调 |
| `core/src/game/diag.rs` | `diff_reports_only_changes` | 诊断快照变化摘要：只列变化项、格式与面板「变化」同口径 |
| `core/src/game/advice.rs` | `permutation_count_and_uniqueness` / `advice_effect_aggregation` / `slot_effect_uses_bound_act_level` / `fusion_adds_new_and_upgraded_effects` / `participant_advice_stacks_by_count` / `assignment_candidates_enumerate_and_apply` / `enforce_801_on_most_assigned_act` / `kensan_act_found_after_fusion` / `fusion_failure_leaves_slots_untouched` | 教诲：按绑定演目 Lv 取级 / 融合 = 新教诲 + 固定升级效果 / 参与人数叠加与絆平摊 / 120 排列与校验 / 401+801 硬约束 |
| `core/src/game/distribute.rs` | `absent_weight_uses_scenario_values` / `deyilv_includes_presentation_baseline` / `distribution_locks_act_members_and_fills_friend` / `yayoi_and_reporter_never_on_practice_positions` / `friend_fills_practice_position_when_any` / `at_most_one_social_person_per_position` / `cooperate_advice_adds_split_appearance` / `finals_extra_member_only_for_edition_cards` / `practice_five_types_lights_all_card_hints` | 分布：不在率 / 得意率基线 / 亮起锁人 + 吾妻补位 + 容量 / **理事长·记者不落稽古位（全稽古回合不出现）** / **吾妻有稽古必补未满稽古位（唯一位必中、不去普通位）** / **每训练位至多 1 名友人 / 团队 / 理事长 / 记者（40 seed）** / 協力与 URA 分身 / 排练全灵感 |
| `core/src/game/train.rs` | `upper_*`（8 条）/ `ura_training_effects`（72–77 生效、68–71 空窗期不生效、净回体）/ `train_level_counts_successful_normal_training_and_stacks_bonus`（成功 4 次 +1、稽古不计、与発表会 +1 相加、夹上限 5）/ `unyielding_drops_failure_to_zero` / `kensan_pushes_choice_event` / `kensan_triggers_on_normal_training_too` / `gauge_gain_by_participants_with_shining` / `preview_action_resources_matches_actual` / `keiko_flat_applies_to_all_five_stats` / `friend_click_pushes_first_then_click_by_probability` / `fat_zeroes_training_speed_only` / `fat_cleared_by_clinic_and_camp_rest` / `preview_matches_actual_training` / `previews_do_not_leak_between_actions`（预览零克隆后候选间不串味）/ `preview_training_restores_keiko_context`（稽古标记含报错路径恢复） | 训练上层公式 / 失败率 / 追加稽古事件化 / 训练入槽 / **动作资源预览（槽 / Lv）与实结算一致、不改原局面** / 全基础 +10 flat / 友人点击链（首次必发 + 40%）/ 变胖速度归零与解除 / 预览与实结算一致 |
| `core/src/game/event.rs` | `extra_train_upper_clears_fat`（选项 1 定点治变胖、选项 2 不治）/ `extra_train_upper_keeps_other_ills`（只治变胖，偏头痛 / 不擅长不动）/ `camp_rule_suppresses_random_events` / `extended_camp_runs_normal_event_axis` / `fixed_scenario_event_preempts_random` / `friend_unlock_event_by_probability` / `card_event_candidates_exclude_friend` / `friend_out_unlock_and_fatigue_recovery` / `event_flags_are_applied` / `fat_recovery_roll_by_rules_probability` / `event_extra_gauge_and_debuff` / `kensan_upper_event_applies_all_effects` / `evolution_hint_accounted_at_turn66` / `event_hint_level_is_counted` / `after_train_events_queued` / `event_records_are_logged` | S6 事件管线：合宿抑制（第 2/3 年常规）/ **延长段事件轴 = 正常回合（200 seed 抽样，支援卡连续事件可触发；常规回合仍抑制）** / 固定事件 / 友人解锁与卡事件候选 / 出行解锁与疲劳回复 / flags 结算 / 变胖解除 / 事件扩展（gauge + 解 debuff）/ 追加稽古上档完整结算 / turn 66 hint / **事件 `hint_level` 计入 `total_hints`（View 同步；URA 事件本身无 Hint）** / 回合后事件入队 / 事件记录与容量上限 |
| `core/src/game/settle.rs` | `first_check_point_great_success_applies_reward` / `check_point_success_when_one_act_short` / `finals_sets_finals_done_and_reward` / `camp_level_up_raises_all_acts` / `advice_window_zero_does_not_acquire_at_end` / `advice_fusion_after_begin_acquire` / `career_end_rewards_by_presentation_results` | 発表会 / 本番判定与奖励 / 合宿 +1 / **T2 首发在 Begin、End 窗口 0 不重复授予** / 教诲融合 / 育成终了 |
| `cli/src/panel.rs` / `cli/src/manual.rs` | `choice_effect_label_omits_probability`（面板事件标签：在线回显去概率、手动面板保留）/ `test_paint_respects_switch` / `test_pad_to_counts_display_width` / `test_gauge_text_shape` / `test_advice_fallback_names` / `test_delta_tone` / `test_display_status_compression` / `test_member_color_priority` | 手动面板呈现件：上色开关退化 / CJK 宽度补齐 / 演目槽数字 x/y / 教诲兜底名 / 增减量色调 / **五维显示压缩（>1200 减半，含上限）** / **训练表人物配色优先级（彩圈 > 助力黄 > 类型底色；研鑽不再给人头着色）** |

### 2.1 L2 数据层集成（`core/tests/data.rs`）

> 从仓根 `data/` 载入**真实资产 + 配置**并抽查关键数值；mdb 重导出过错时在这里失败，
> 不必等 S3+ 跑局才发现。数据来源映射见 [`plan/intake.md`](plan/intake.md) §4.1。

| 用例 | 守门内容 |
|---|---|
| `load_repo_data` / `load_repo_config` | 资产 16 个顶层键可载入并通过校验；配置默认层 = `MatsuriConfig::default()`，且带 `stamina_build` / `wisdom_build` 两档 |
| `limits` / `acts` | 五维基础上限 2200 组 / 绝对上限 2500 / 演目 → 命令 → 训练位 → 神驹映射 |
| `training_table` | 普通训练 Lv1–5（含力量 Lv4/Lv5、根性 Lv5）与稽古 Junior / Classic 两档（S5 恢复） |
| `act_effects` | Lv1 基线 / Lv8 跳档 / Lv9 基础上限 / Lv10 顶点 |
| `advices` | 絆平摊 +3 / 不屈阈值 45→0 / 両道 25% + 15% |
| `fusion_and_check_points` | 転成 5 条 / 本番大成功 buff 20-80-150-100 / 成功档一次性奖励 / 育成终了全大成功 |
| `schedule_camp_rules_and_outing` | 23/47/67 与 Lv3/5/6、教诲窗口、合宿结算点、gauge 入账、羁绊基础值、出行 |
| `rules_s5_s6` | S5/S6 规则常量：上层基础上限 100 / 助力支撑 3×5% / 变胖解除 15% |
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
| `person_availability_by_turn` | 吾妻 / 第 1 批神驹 turn 2、第 2 批需 Classic 大成功、记者 turn 13、**理事长 turn 1 起（T0 不出现）** |
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
| `dictus_iron_heart_race_bonus_applied` | 狄杜斯 106301：T11 出道战不吃铁心（7/111）；T22 生涯赛 五维×2.5 / PT×0.46（卡面 55% → 3.875 / 1.7825）→ 19/128；T68/T69/T71（本番大成功 ×2）→ 38/256 | §4 |
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
| `handwritten_trainer_covers_all_decision_points` | 整局：教诲分配决策恰好 6 次（T2/T11/T35/T59 Begin + T23/T47 End）、融合已应用、事件选项决策被调用 | §6 / §6.2 |
| `handwritten_trainer_is_deterministic` | 同种子同配置两次跑局：五维 / PT / 演目账本 / 発表会结果逐位一致 | §6.2 |
| `inheritance_settles_stats_pt_and_limit` | 继承（T30 第二年 / T54 第三年）零值事件按公式结算：速 += 蓝因子 15×6 = 90、上限 += floor(15×5.33) = 79、PT ∈ 500–1500（剧本因子 1000） | §5.4 |
| `pre_xiaoqie_flag_and_score_factor` | 育成前小切：`[pre] qie = "xiaoqie"` → 建局即带小切；评分 PT 项增量 = total_pt × 2.0 × 0.04（截断） | §5.5 |
| `qiezhe_event_flag_clears_xiaoqie` | 通用切：`5007` 数据带 `add_flags.qiezhe` 档；事件结算置切者并清掉小切（切者优先互斥） | §5.5 |
| `pre_motivation_shield_blocks_drops` | 育成前心情盾 ×2：挡两次干劲下降；一次扣多级只耗 1 层；盾尽后正常掉干劲 | §5.5 |

### 3.1 诊断守门（`core/tests/diag.rs`）

> 独立测试进程内装一次性捕获日志器；诊断目标 / 埋点齐备 / 静默不位移结果由该文件守门。

| 用例 | 守门内容 |
|---|---|
| `diag_logs_key_state_changes_without_changing_results` | 整局：target 全为 `diagnostic`；回合 78 / 発表会 3 / 分布·动作·训练后·事件·教诲·决策均非空；T0 回合行 / T2 首次授予 / T2 分布抽样；`DiagGuard::suppress()` 期间零产出且同种子终局局面逐位一致 |

### 3.2 搜索档（`core/tests/search.rs`）

> 只跑代码正确性的小预算（`search_n=1`、`use_ucb=false`）；1024 真实预算用 `#[ignore]`
> 手动执行，不进 CI。

| 用例 | 守门内容 |
|---|---|
| `search_full_game_is_reproducible` | Train / 教诲 / 事件全开、`search_n=1`：跑满整局，同种子两次终局快照 / 评分 / PT 逐位一致 |
| `search_scopes_off_match_handwritten` | 三类开关全关：与手写档同种子逐位一致（搜索档可无损退化） |
| `single_candidate_skips_search` | 单候选直接回落手写，不触发搜索 |
| `rollout_fork_skips_event_log_without_changing_game` | 事件明细开关不改变评分 / PT；`fork_for_rollout` 关闭明细且跑完整局后 `event_log` 为空（性能优化 B 组守门） |
| `search_1024_full_game_is_reproducible`（ignored） | 1024 每候选：同种子两次逐位一致；手动跑 `cargo test --release -- --ignored` |

### 3.3 还原层（`core/tests/restore.rs`）

> 帧 → 局面 round-trip：不依赖真机帧，先把「帧字段 ↔ 状态字段」钉死。

| 用例 | 守门内容 |
|---|---|
| `restore_round_trip_at_decision_point` | 决策点还原与离线态逐项一致（演目 / 教诲 / 合宿 / 発表会 / 分布映射 / 训练等级 / 编成警告） |
| `restore_keeps_build_roster_before_scenario_start` / `restore_after_training_stage` / `restore_carries_level_baseline_when_frame_zero` | T0/T1 名单保持建局结果；`AfterTrain` 入口；帧等级为 0 时沿用跨帧基准（`G12`） |
| `restore_rejects_deck_without_ssr_friend` / `restore_rejects_unknown_guest` / `restore_rejects_roster_order_mismatch` | 编成门禁与卡位顺序自检（缺 SSR 吾妻 / 未知神驹 / 顺序不符 → 明确报错） |
| `distribution_maps_protocol_ids` / `restore_maps_check_point_results` | 协议人头 id → `person_index`；発表会结果按 `scheduleId` 落位 |
| `restore_chara_effects_by_kind` / `restore_fat_comes_from_effect_4` | 帧 `charaEffects` 按 **142 表**还原（1 = 熬夜 / 3 = 皮肤粗糙 / **4 = 变胖**），覆盖引擎粗粒度 `UNKNOWN`；`1` 不再被误判成变胖、`4` 不再被当成摸鱼 |
| `restore_chara_effects_ignores_non_illness_ids` / `restore_empty_chara_effects` / `restore_buffs_only_clears_unknown` | 全量状态数组口径：`[25, 47, 4]` 只还原 `FAT`；`[]` 保持 `NONE` 或按 `isIll` 回退 `UNKNOWN`；`[25, 47]` 覆盖掉 `UNKNOWN`，不误判生病 |
| `restore_injects_event_by_story_id` | 事件帧 `storyId` 按本地事件表注入：追加稽古 `400015106`（`scenario_events`）/ 友人出行第三段 `809052013`（`friend_events`）；未收录 id 不注入假事件 |
| `restore_prefers_local_event_over_frame_story` | **本仓数据优先**：引擎按帧 `story` 建的同 id 事件被本地那份覆盖（同 id 只留一条、选项数与数值都按本仓） |

## 4. 冒烟与基线

| 项 | 内容 | 判定 |
|---|---|---|
| 冒烟 | 固定种子跑满回合、两次逐位一致 | `Result` / 非零退出 |
| 手动档冒烟 | `manual` 管道输入（全 0）跑满 78 回合（非 TTY 动作选择退回编号输入）；面板关键块（剧本资源 / T2 闸门标记 / 未分配 / 训练表 / 事件回显 / **教诲名称 / 演目槽变化行 / 配色图例**）、S6 关键事件回显（含 **駿大祭合宿の申し出**）、非 TTY 无 ANSI | 退出码 + 文本标记 |
| 手写档 explain 冒烟 | `matsuri matsuri --explain` 跑满整局：每回合 **手动档同款面板** + 「【候选打分】」+「【决策理由】」；教诲分配 **6 窗口 top-10**；终局块正常 | 退出码 + 文本标记 + 候选表 ≥80 处 |
| 搜索档冒烟 | `MATSURI_SEARCH_N=1 matsuri matsuri` 跑满整局：退出码 0、stderr 出现 `【搜索】` 摘要、stdout 出现终局评分 | 退出码 + 文本标记 |
| 测卡冒烟 | `cardtest build --comp all --pick panel --with-default --out decks/panel.csv` 出 5 行（4 套 + 默认参照臂，kinds=5、warnings=0）；`cardtest run --decks decks/panel.csv --ref default_21101 --games 300 --seed 82000` 出对比表 / 评分归因 / 决策分布 / 分辨率四块，参照臂 Δ=0 且 W/L/T=0/0/n；**同参数两次 diff 为空**（已核） | 退出码 + 两次输出逐行一致 |
| 基线 | 一基线一 JSON（`input` 自持、`expect` 逐项容差、`meta` 不含路径） | 自动比对，失败返回 `Err` |

- 重录命令：`<cargo ... baseline --update>`（S9 落地）
- 数值占位替换后必须重录，并在 `glossary.md` 记影响面。

### 4.1 基线快照（2026-11-09 重录）

按当前 GA 定档配置（`pt_favor_rate = 1.931954`）重录；本轮守门 ④ / 锁位 CRN 修复后再次重录。

| 档 | 训练员 | 局数 | 均分 | σ | PT | 位移 vs 上轮 |
|---|---|---|---|---|---|---|
| `a-normal` | 手写 | 100 | 83793.9 | 3060.6 | 11731.4 | +373.3 / +11.0 |
| `a-normal` | 搜索 N=8192 | 10 | 87557.1 | 1612.9 | 12620.1 | −193.3 / −27.7 |
| `b-ideal` | 手写 | 100 | 88385.3 | 2225.0 | 12314.2 | +291.3 / +77.4 |
| `b-ideal` | 搜索 N=8192 | 10 | 91535.7 | 765.1 | 13293.8 | +246.3 / +41.3 |

A/B（上一提交 vs 本轮，配对）：手写 +369.4 ±297.9（t 1.24）；搜索 −196.3 ±424.8（t −0.46）。
位移来自分布随机序列（锁位补消费）+ 守门 ④；搜索差值在噪声内。

### 4.2 MCTS − 手写 的差值（同一口径测量）

| 档 | 手写 | 搜索 | 差 | 合并标准误 | 相对 | PT 差 | 分数 σ |
|---|---|---|---|---|---|---|---|
| `a-normal` | 83793.9 | 87557.1 | **+3763.2** | 594.8 | +4.49% | +888.7 | 3061 → 1613 |
| `b-ideal` | 88385.3 | 91535.7 | **+3150.4** | 328.8 | +3.56% | +979.6 | 2225 → 765 |

> 2026-11-08 旧配置（`pt_favor_rate=1.7271`）曾测得同种子配对差 +5899.1 ±1126.6；
> 本轮配置变更后未重跑配对，判读以本表非配对口径为准。

**结论**：当前配置下 MCTS 比手写高 **约 +3150 ~ +3763 分（+3.6%~+4.5%）**，PT 同步多约 +900 ~ +980；
分数标准差降到约 1/2（`a-normal`）到 1/3（`b-ideal`）。

### 4.3 `pt_favor_rate` 扫测（1.0 – 3.0，N=2048 × 10 局）

命令模板（临时配置走 `--matsuri-config`，不动仓内默认层）：

```bash
RAYON_NUM_THREADS=16 ./target/release/bench.exe game \
  --matsuri-config logs/_scratch/favor_<v>.toml --games 10 --seed 73200 --warmup 0 --search-n 2048
```

| `favor` | 均分（SE） | PT（SE） | 总分 − 2×PT | 配对 Δscore vs 1.7271 | 配对 ΔPT |
|---|---|---|---|---|---|
| 1.0 | 85702.7（488） | 11784.7（56） | 62133 | −156 ±416 | −260 ±101 |
| 1.25 | 86151.0（488） | 12047.8（109） | 62055 | +293 ±471 | +3 ±137 |
| 1.5 | 86029.6（539） | 12092.0（99） | 61846 | +171 ±635 | +47 ±126 |
| **1.7271**（默认） | **85858.4**（407） | **12044.7**（59） | 61769 | — | — |
| 2.0 | 86341.8（621） | 12229.3（114） | 61883 | +483 ±543 | +185 ±152 |
| 2.25 | 85904.3（540） | 12261.8（109） | 61381 | +46 ±526 | +217 ±109 |
| 2.5 | 86019.0（600） | 12271.3（108） | 61476 | +161 ±585 | +227 ±146 |
| 2.75 | 86346.4（652） | 12438.3（93） | 61470 | +488 ±593 | +394 ±126 |
| 3.0 | 86081.6（607） | 12374.8（71） | 61332 | +223 ±381 | +330 ±76 |

**读法**：

- **总分与 `favor` 无关**：9 个点的极差 644 分，与单点 SE（≈500）和配对差 SE（381–652）同量级；
  相对默认 1.7271 的 8 个配对 Δscore 均在 1 SE 内（最大 +488 ±593），判为噪声而非趋势；
- **PT 与 `favor` 强相关**：11,785 → 12,375（+5.0%），2.25 之后饱和；3.0 的配对 ΔPT +330 ±76（4.3σ）是本次最硬的信号；
- **代价在面板分**：`总分 − 2×PT`（≈ 技能 + 五维）从 62,133 掉到 61,332（−801），与 PT 增益几乎对冲 ——
  提高 `favor` 是「拿面板分换 PT」，不是「白拿分」；
- 结论：默认 **1.7271 无需改动**；要冲 PT 就把旋钮放 2.25–2.75（饱和区），代价是面板分。
- 精度提醒：想把 ±300 级的分差分辨出来需要 ~30 局 或 N=8192；本轮 10 局 N=2048 只能定「无显著差异」。

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
| 2026-11-06 | lab 实验仓调优成果合并（策略层 + 冠军档默认层）+ 幸运体质盾接线 + CI 修复 | 用户 + Agent | 守门链②–⑤ / 满体力闲置惩罚 / 动态权重层 / 落位先验 / `search_score` 口径修复（关搜索零差）；冠军档 `ga-r2` 成为默认层（`data/matsuri_config.toml` + `config.rs::Default` 同步，`tests/data.rs::load_repo_config` 守门）；幸运体质 `lucky_count` 消费接线（含变胖，5 个新测试）；CI = 引擎并列软链 + 工具链 1.99.0 + `--workspace`。四档绿（**130 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 185 测例**）；冒烟 77538 / LG2、PT 11680；100 局 61444–61543 均 73155.0；lab 口径 62100–62199 均 74051.7（与 lab 冠军逐位一致） | 基线全面位移（S9 前不建基线，已记 README / dev_plan / glossary / handoff）；引擎未改 |
| 2026-11-06 | 自选比赛义务区间（freeRaces）接线 + 紧迫度旋钮 | 用户 + Agent | 按引擎通用规则接线：区间场数 / 等级门槛（只放行不低于门槛的比赛，`grade=1` 只放 G1）；`next()` 推进时 `check_free_race` 未达标 → 育成失败（终止 + 跳过终局奖励）；训练员新增 `[trainer].free_race_urgency`（默认 1.0）+ `need ≥ left` 硬保底守门⓪（不受 `self_race` 限制）；新增 8 个测试（进度 / 无义务 / 终止 / 达标 / 失败跳过奖励 / 守门 / 估值 / 配置校验）。四档绿 **193 测例**；100101 100 局逐位不变（73155.0），102102 100 局 74051.7 → 70876.0（−3175.7、无育成失败） | 有（有义务马娘评分下降属预期；`race_grades` 表对齐真机待核） |
| 2026-11-06 | 手写档「决策解释」模式（复用 manual 面板） | 用户 + Agent | 抽出 `matsuri-cli/src/panel.rs`（`BoardPanel`，manual 与手写档共用）；新增 `--explain` / `MATSURI_EXPLAIN=1`：每回合面板 + 候选打分表（`MatsuriTrainer::explain_action` 同源）+ 决策理由；教诲分配 top-10、事件选项期望值；只打印不改决策（同 seed 终局 77538 逐位一致）；preview 对空分布做防御。测例 **195**（+核心解释 API 1、CLI explain 冒烟 1） | 无（默认关；`--explain` 输出量较大，仅核对用） |
| 2026-11-03 | 合宿延长段事件轴 = 正常回合（U19 结项） | 用户 + Agent | 用户确认：第 2/3 年合宿**延长段**可触发支援卡连续事件等各种事件（**事件轴等同正常回合，仅训练全变演目稽古**）。实现：`random_events_suppressed` 合宿层由「第 2/3 年整个合宿区」收窄为「第 2/3 年**常规窗口**」，延长段回落引擎 `no_event_turns` 基线；新增内联测试 `extended_camp_runs_normal_event_axis`（200 seed 抽样：延长段可抽支援卡连续事件；常规回合仍抑制）。四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；基准 seed 61444 无合宿延长 ⇒ 冒烟 **77538 / LG2、PT 11680** 逐位不变；109301 凯斯奇迹 T39–41 目标赛触发 2 年延长，实测 T40 马娘事件 / T41 支援卡连续事件 / T42 全稽古训练正常 | 有（有合宿延长的局事件数量变化；无延长局零差） |
| 2026-11-03 | 比赛加成改为乘算（支援卡赛后 × 剧本赛后） | 用户 + Agent | 用户确认：発表会 / 本番 `check_point_effect 204` 的 30/100 更可能是**乘算**（×130% / ×200%），floor 位置存疑、先按**一次性 floor**（222–223 更接近游戏实际）。实现 `MatsuriGame::race_scale`：`(100+卡面赛后)/100 × (100+剧本赛后)/100`，结算 `do_race` 与训练员 `value.rs::estimated_race_reward` 共用；新增内联 `race_scale_multiplies_deck_and_scenario_bonus`（本番前 / 成功 ×1.3 / 大成功 ×2.0）。测试配卡 55% × 大成功 2.0 = 3.10：T71 生涯赛 **15/223**（原加算 2.55 → 12/183）。四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；冒烟 **77538 / LG2、PT 11680 → 78397 / LG3、PT 11793** | 有（本番后比赛收益：T69/T71 与 URA 决赛；S9 前不建基线） |
| 2026-11-03 | 狄杜斯「铁心的挑战者」：五维×2.5 / PT×0.46 | 用户 + Agent | 用户实测反推：状态 `31`「铁心的挑战者」对生野狄杜斯 106301 / 106302 生效，出道战后（T12–71）对所有生涯比赛 **五维 ×2.5**（+150%）、**PT ×0.46**（属性多、PT 相对少；与温泉 1063 的 2.7 / 0.45 同型），与卡面赛后**乘算**、在剧本赛后**乘算之前**。实现按马娘 ID 硬编码 `MatsuriGame::special_race_stats_bonus` / `race_pt_rate`：`race_scale = (100+卡面)/100 × 2.5 × (100+剧本)/100`，PT 另乘 0.46；`value.rs::estimated_race_reward` 按同一口径分别缩放面板后再估值。自选赛 / T11 / URA 不生效。View 增 `uma.special_race_stats_bonus` / `special_race_pt_rate`，manual 面板显示「铁心 五维×2.5 / PT×0.46」。新增内联 `dictus_iron_heart_bonus_window_and_kind` + 机制 `dictus_iron_heart_race_bonus_applied`（T11 7/111；T22 19/128；T68/T69/T71 38/256）。四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；100101 等其余马娘冒烟不变 **78397 / LG3、PT 11793**；106301 单局探针 79929/LG6、PT 11500（非基线） | 有（狄杜斯生涯赛收益；其他马娘零差） |
| 2026-11-04 | 理事长出场回合 + 自选比赛区间判定/达标奖励（真机修正） | 用户 + Agent | ①理事长改为 **T1 起出现**（`person_is_available` 的 Yayoi 分支）；②自选比赛区间判定从「结束 +1 回合」前移到**区间最后一回合的 `End`**（`settle_free_race_deadline`）：未达标 → `career_failed` 终止；达标 → **「生涯比赛胜利」奖励事件**（5 / 72 × `race_scale` / `race_pt_rate`），同回合 End 结算、每区间一次；③更新 4 个既有测试（可用回合 / 失败终止 / 达标继续 / 达标奖励）。四档绿 **208 测例**；100101 100 局 **77792.2 → 78124.5**；102102 100 局均 **76406.6**（≥LG 57、`career_failed` 0，每局 T28 / T48 各一次奖励） | README 数字待提交准备；`race_grades` 与真机自由赛格子对齐仍待核 |
| 2026-11-04 | 局外状态收益专项（小切 / 心情盾×2） | 用户 + Agent | 100101 100 局（61444–61543）对照基线 77792.2：**+小切 78672.0（+879.8，t=251，100/100 局）**——纯评分 PT 项 ×1.04，PT 数量与五维不变；**+心情盾×2 78280.0（+487.8，t=2.11，71/100）**——掉干劲被挡后行为位移，PT 均 +106、五维小幅上移；两者叠加 79168.3（+1376.1，t=5.93，82/100） | 心情盾 t 值边缘，换种子块需复验；均为局外状态专项，不改默认档 |
| 2026-11-04 | 狄杜斯专项复测（新配置） | 用户 + Agent | 换新因子并接通继承后复测 106301 / 106302（100 局 61444–61543）：106301 均 **82015.7**、PT 11802.9、≥LG 95（旧档同种子 77388.1，**+4627.6** / t=18.9）；106302 均 **82176.9**、PT 11826.5、≥LG 98；两马 `career_failed` 0；冒烟 61444 分别 84252 / LG12、85672 / LG15（旧 79929 / LG6、PT 11500） | 铁心专项，不作普适基线；README 数字待提交准备同步 |
| 2026-11-04 | 继承 / 切者 / 小切补接 | 用户 + Agent | 查明 T30/T54 继承事件未结算（剧本侧未移植引擎特殊结算 ⇒ `extra_count` 全无效）、切者只有数据层 3% 档、小切无入口；补 4012/4013 结算、`[pre] xiaoqie` 注入与 3 个机制测试；四档绿 **208 测例**（147 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；冒烟 81567 / LG8、PT 11497；100 局基线 **77792.2 / PT 11003.7** | 小切 / 心情盾默认关（按账号开启）；README 状态待提交准备 |
| 2026-11-03 | 溢出处理 + 末段稽古兑现旋钮 | 用户 + Agent | 新增 `[trainer.overflow]`（`margin` / `tilt` / `keep`）：按生效上限判定近顶、非近顶轴加权、主属性近顶时保留副属性 / PT；新增 `[trainer.endgame]`（`keiko` / `scarcity` / `level` / `normal_penalty`）：窗口 = 第三年合宿结束后～T71，只作用普通回合，满槽数超过剩余普通回合时优先满槽稽古，URA 72–77 不参与。默认档 `0.02/0.5/0.15` + `250/1.0/80/120`；100 seed discovery 上 106301 +245；200 seed holdout 未复现（三马 −33/+7/−7），定位为行为旋钮。新增 5 个内联测试；四档绿 **204 测例**（147 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；基准 seed 61444 冒烟不变（matsuri 78397 / LG3、PT 11793；random 27150 / UF6、PT 5153） | 有（部分 seed 的 T64–71 稽古选择位移；S9 前不建基线） |
| 2026-11-04 | 注释 / 冗余符号与测试整理 | Agent | 注释去历史（日期 / S 阶段标记 / 过期口径），README 收敛为现状；删除 `sample_data` / `is_card` / `MatsuriTrainer::config` / `Default for ManualTrainer` / 空 `bench` 模块，合并重复的选项标签函数；删除 3 个内联测试（`trainer_reads_trainer_knobs`、`trainer_forces_free_race_when_deadline_critical`、`view_roundtrip`）与 4 处已删机制的负向断言；剩余 **205 测例**（144 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟） | 无（不改行为） |
| 2026-11-04 | **M2 前置修复（H1/H2 + 4 条 Medium）** | Agent | ① `apply_root_action` 改走自持策略流（新增 `apply_selected_action`，测试 `apply_root_action_uses_strategy_stream`）；② `search_n` 不再被 CLI 缺省 0 覆盖，`data/ai_config.toml` 置 0、env 非法报错；③ `flags_value` 增益/惩罚分号并去掉 `BAD_TRAINER` 双计；④ 友人出行先校验后写；⑤ 融合整批校验；⑥ `AdviceAssignment` 越界拒绝 + 先验查表改 `get`；⑦ 面板教诲窗口 / 友人事件改数据源。新增 4 个内联测试；四档绿 **209 测例**（148 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；seed 61444 冒烟 **81553 / LG8、PT 11490** 逐位不变 | 有（估值 / 面板口径修正；基准种子零差） |
| 2026-11-04 | **M2 搜索档接 FlatSearch（ramen 集成层）** | 用户 + Agent | `MatsuriSearchTrainer` 包 `FlatSearch<MatsuriGame>`，rollout 基策 = 手写训练员（无搜索）；`matsuri_config [search].search_n` 每候选预算 + `train` / `advice` / `event` 分开关；教诲 120 候选剪枝 top-16；事件 dummy token + `search_with`，应用当前选项后从 `event_queue` 下一事件继续；`search_score` 用 `pt_favor_rate` 口径，失败分支按实际分；叶估 `LeafEvaluator` 预留，`max_depth=0`；搜索日志 `DiagGuard` 静默 + 每决策一行摘要；搜索只消费一个决策流随机字，局面规则流独立派生。新增 5 内联 + 4 搜索集成（1 ignored）+ 1 CLI 冒烟；四档绿 **218 测例**（153 内联 + 15 L2 + 36 机制 + 1 diag + 7 CLI 内联 + 3 CLI 冒烟 + 3 搜索集成，另 1 ignored）；seed 61444 单局 1024 → **83326 / LG11、PT 11602**（90.8 s）、2048 → **83519 / LG11、PT 11901**（180 s）；`search_n=0` 手写零差 | 搜索单局验证；批量 / 8192 待机制完全实现后再跑 |

| 2026-11-05 | 基线库与自动判定 | Agent | 新增 `baselines/`（`a-normal` 普通马娘无 buff、`b-ideal` 狄杜斯理想档）与 `matsuri-sim::baseline`（JSON 自检 / 合并标准误 × 相对容差判定）；`bench baseline` 按档复跑并判定，任一档漂移非零退出；新增 4 个内联用例（同分布通过 / 超容差判漂移 / 空样本拒判 / 仓根基线可解析） | 测试数 221 → 225；基线数据不参与 CI 重跑（搜索档单局约 20 s） |
| 2026-11-08 | **变胖（`Ill::FAT`）机制收口** | 用户 + Agent | `rules.fat_recovery_prob` 20 → 25 → **15**（同日回调）；「追加訓練」(`102`) 选项 1 定点治变胖（`apply_event_extra` + `event_choice_value` 同步给分）；`debuff_clear_value` 改为与 `clear_one_debuff` 同序；线上插件补 `baseGame.illEffects`（同日后续改名 `charaEffects` / 全量状态数组），剧本仓 `frame` / `restore::ill_from_effects` 精确还原病种（旧插件退化 `UNKNOWN`）。四档绿（**170 内联 + 16 L2 + 36 机制 + 11 还原 + 1 diag + 4 搜索 + 9 ai 内联 + 8 CLI 内联 + 3 冒烟 + 4 sim = 268 测例**）；同种子块 73200 手写 100 局 80713.3 → **80714.0**（+0.7，25% 档）；15% 档未录基线 | 引擎 / 协议未改；线上需新插件 dll |
| 2026-11-08 | **在线运气行加「期望 ± 单局 σ + 档次」** | 用户 + Agent | `SearchSummary` 增 `baseline_stdev`（候选样本方差按样本数加权合并 = 单局终局分标准差，**不随预算收敛**）与 `expected_score` / `expected_rank`（官方口径：`search − PT加权 × rate × 切者因子 × (favor−1)`，搜索基线本身被 `pt_favor_rate` 放大约 1.6 万）；`online.rs::render_luck` 行尾追加「期望 E （±S，档）」（`E` 与档亮青）；`S=0` / 手写档 / 搜索 fallback 时不显示；JSON 同口径进 `scenario_extra.expected_score` / `expected_stdev` / `expected_rank`。**270 测例**（+2）。实测`logs/replay_smoke`（N=1024）：`[回合 0 运气]本局：+0, 本轮: --, 期望 82123 （±4532，LG9）`（同局搜索基线 98013，差 1.6 万即 PT 项放大额） | 只改呈现层与摘要字段，决策 / 搜索排序 / 基线不受影响 |
| 2026-11-08 | **在线协议接线（handoff）：病种 142 表 + 事件帧 `storyId`** | 用户 + Agent | `ill_from_effects` 改 142 表显式映射（1 熬夜 / 2 懒惰 / 3 皮肤 / 4 变胖 / 5 偏头痛 / 6 不擅长）；`RestoreInput.story_id` → `lookup_event`（`scenario_events` 优先 + 引擎五表）→ 注入 `unresolved_events`；`source=event` 且查不到 → `Route::Skip`（不回退训练建议）；`storyId` 进帧提示与 JSON。四档绿（**276 测例**，+6：3 路由 + 1 帧解析 + 2 还原）；端到端回放：事件帧 `storyId=400015106` → 「回合 23 事件 首选：选项1」，未知 id → 跳过 + 帧提示 | 离线零影响（无帧）；线上事件帧不再误给训练建议 |
| 2026-11-08 | **在线事件帧回显 + 跳过行配色** | 用户 + Agent | 事件决策前打印 `[回合 N 事件]名称 #id` 与逐选项实际效果（属性取还原出的 `EventData`；帧 `story` 有选项文本时附括号，否则「选项N」）；事件名亮蓝、跳过行改亮黄（`33`）。**277 测例**（+1） | 只改呈现层，决策 / JSON 契约不变 |
| 2026-11-08 | **clippy 清零 + 基线重测 + MCTS 档间差** | 用户 + Agent | ①`cargo clippy --workspace --all-targets -- -D warnings` **全绿**（去掉此前 **15 条**存量告警：6 条 `needless_range_loop` / 1 条 `collapsible_if` / 3 条 `type_complexity` / 2 条 `dead_code` / 1 条 `manual_range_patterns` / 2 条冗余引用；全部纯等价改写，行为零变化）；②基线重测两档四项全绿并重录 （见 §4.1）；③MCTS − 手写 = **+5691（a-normal）/ +4451（b-ideal）**，配对校验 +5899 ±1127（见 §4.2）| CI（`.github/workflows/ci.yml`）的 clippy 门禁此前应长期红，现已可绿 | 行为零影响（仅等价改写）；基线与档间差为新参考 |
| 2026-11-08 | **运气显示分标定 + `pt_favor_rate` 扫测** | 用户 + Agent | ①`ai_config [record].luck_turn_bonus` **70 → 100**：MCTS 自博弈实测期望分随回合系统性上升 +135/回合（T0 79,443 → T76 87,772，终局 87,712；95% CI [128,143]），旧 70 只抵消一半 → 平均局读成 +1,179「很幸运」；取 100 后 +96（范围 ±1,900）。三者口径 91/103/135 取中；②确认 **`期望 E` 未接入该参数**（`Expected = raw`，rollout 已跑到终局，再叠回合加成会重复计入）——T0 显示 79–82k 而实际终局 ~87k 的缺口来自估计器自身的乐观/悲观漂移，属另一件事（是否显示投影值待定）；③`pt_favor_rate` 1.0–3.0 扫测（N=2048×10 局）：总分持平（极差 644，与 SE≈500 同量级、配对差均在 1 SE 内）、PT +590（2.25 后饱和）、面板分 −801，属构成偏好非分数旋钮，默认 1.7271 不动（见 §4.3）| user 2026-11-08「调整 luck_turn_bonus + 检查期望分接线 + 扫测 favor」 | 显示口径变化（运气分归零），决策 / 模拟零影响；**副作用：`render_luck` 分档阈值仍按旧标定（<0/<500/<1500），待同步下移** |
| 2026-11-08 | **`charaEffects` 接收端同步 + 跳过帧提示收敛 + 期望分显示投影** | 用户 + Agent | ①按 `URA_Plugins/handoff-2026-11-08.md` 把 `illEffects` → `charaEffects` 全链路改名（`frame` / `session` / `restore` / 测试夹具 / 文档），`ill_from_effects` 的 142 表显式映射**逻辑不变**；新增全量数组解析与还原用例 4 条（buff + 病种混合 / 空数组 / 纯 buff 覆盖 `UNKNOWN` / 含非病 ID 的帧解析）。②跳过帧只留黄色跳过行：删「storyId 未收录」note、`render_notes` 只在 Decision 打印、skip reason 去掉「见帧提示」，JSON 的 `story_id` / `reason` 保留供对账。③期望分按回合计入 `luck_turn_bonus`（`E₀ + (77−t)×b`），**档次 / JSON 同步按投影后的 E 重算 / 输出**；投影统一在 `matsuri-ai::output::projected_summary`，终端与 JSON 只消费同一份（新增 2 例：只平移 E 不改 σ / 档次按投影值重查）。四档绿（**286 测例**：171 内联 + 16 L2 + 36 机制 + 17 还原 + 1 diag + 4 搜索 + 16 ai 内联 + 6 ai 集成 + 12 CLI 内联 + 3 冒烟 + 4 sim）；`cargo fmt --all --check` / `check` / `clippy -D warnings` 全过；dist 已重打包并同步运行目录 | 引擎 / 协议未改；离线零影响；线上字段改名需插件 DLL 与接收端同步部署 |
| 2026-11-08 | **搜索线程数进 `matsuri_config [search]`** | 用户 + Agent | ①`[search].threads` 与 `search_n` 同段（默认 0 = 自动，显式 `RAYON_NUM_THREADS` 优先）；`config.rs::SearchScopeConfig` 加字段 / `Default`（0）/ 文档注释，覆盖层「整段替换」语义不变，新增 2 条断言（覆盖层指定 8 / 未写回落 0）；②CLI 新增 `apply_search_threads`（`rayon::ThreadPoolBuilder::build_global`），在线 `online.rs::run` 与离线 `main.rs::run` 载入配置后各调一次，新增 1 例验证 `threads=0` 不初始化全局池；③workspace 登记 `rayon = "1.12.0"`（已在 `Cargo.lock`）、`matsuri-cli` 引用。四档绿（**287 测例**）；fmt / check / clippy 全过；dist 重打包并同步运行目录 | 默认 0 = 自动，离线 / 在线行为不变；测量工具仍用环境变量 / `--threads` |
| 2026-11-09 | **GA 三档定档 + 测卡友人列** | 用户 + Agent | ①`config.rs::parse_repo_default` 改断言：`stamina_build` / `wisdom_build` 按构成自动触发且不等于 default；`tests/data.rs::load_repo_config` 断言两档存在；「`[decks.default]` == `DeckParams::default()`」守门随 g0269 同步；②`cardtest` 汇总加 `friend_mean` / `friend_full`；`bench micro` 的 `search_score` 改取生效档 `pt_favor_rate`（原写死 1.7271）；③仓内配置按构成选档与 GA 验收逐位一致（三档主 / 影子各 12 场景 × 2500 局）。全量 **287 测例**绿 | 三档决策位移；`baselines/*.json` 未重录（由维护者重录） |
| 2026-11-09 | **在线按帧内卡组选档** | 用户 + Agent | ①`session.rs::deck_profile_follows_frame_cards`（新）：12101 / 11102 / 21101 依次命中 `stamina_build` / `wisdom_build` / `default` 并可切回，空卡组按 default，生效配置与表内档相等；②`online.rs::deck_line_shows_comp_and_profile`（新）：档行拼装；③`config.rs` 两例补 `DeckTable::profile_name` 断言。全量 **289 测例**绿 | 在线 12101 / 11102 决策位移（与离线对齐）；离线 / 测卡 / 基线不受影响 |
| 2026-11-09 | **自选比赛期限回合只搜比赛** | 用户 + Agent | ①`search.rs::deadline_turn_searches_race_only`（新）：102102 第二区间最后可赛回合，`list_actions` 多候选，搜索摘要只剩比赛、中选 = 比赛、基线 = 比赛分、决策流只取一个随机字；②`game/mod.rs::free_race_progress_reports_need_and_left` 补 `forced_race` 断言（未到期限 / 最后可赛回合 / 候选无比赛）；③新旧版本对比：真机 game3535 T19–T23 回放（N=1024）T22 本轮运气 −84618 / 期望 22302 → +729 / 89695，其余回合与首选不变；离线 109001 / 102102 共 10 局（N=64）终局逐位一致，其中 102102 seed 61447 在 T47 / T48 两次只搜比赛。全量 **290 测例**绿 | 无：中选与离线结果逐位不变，只改截止回合期望 / 运气显示 |
