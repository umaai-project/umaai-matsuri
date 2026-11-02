# 数值占位登记（STUB / TBD）

> 机制已确认、数值未到时的统一登记处。**占位不阻塞开发**，但必须可整体替换、可盘点。
> 规则：
> - 占位集中放在剧本资产 / 配置 / 常量表，代码里不散落魔法数；
> - 代码标记统一为 `STUB:`（临时占位）与 `TBD:`（待补逻辑/数值），并在此表登记；
> - 替换为确认值后：重录基线，并在 [`glossary.md`](../glossary.md) 的决定记录写明位移原因与影响面。
> - 机制口径以 [`../matsuri_memo_cn.md`](../matsuri_memo_cn.md) 为准（未决清单见其 §14）；本表只登记**数值 / 数据占位**；
> - 状态取值：`OPEN`（未开工）/ `STUB`（有占位值可跑）/ `TBD`（逻辑缺失）/ `RESOLVED`（已定）；

## 登记表

| 项 | 当前占位值 | 来源 | 存放位置 | 替换条件 | 基线影响 | 状态 |
|---|---|---|---|---|---|---|
| `training.normal` | 5 训练 × Lv1–5 × 7 值 | Lv1 = `master.mdb`；Lv2–5 = `memo` 2026-10-31 | `data/scenario_matsuri.json` / `MatsuriData` | **S2 已落地**；Lv2–5 为 memo 中可信度，待真机确认后替换并重录基线 | 会位移（若 Lv2–5 修正） | RESOLVED（已导入）/ TBD（Lv2–5 复核） |
| `training.keiko.junior` | Junior 档 × 5 行 | `single_mode_training_effect` | `data/scenario_matsuri.json` | **S2 已落地** | 会位移（若 mdb 变化） | RESOLVED |
| `training.keiko.classic` | Classic 及以后档 × 5 行（速 21 / 耐 20 / 力 20 / 根 23 / 智 14；SP 10/12/12/12/9；体力 -25 / +5） | `memo` 2026-10-29 / 10-31 | `data/scenario_matsuri.json` / `data.rs` | **S5 已恢复资产 / 校验 / L2**；turn 24 起切档（turn 23 発表会「演目稽古强化」）；**第二档覆盖 Classic 与 Senior，无第三档**（U02） | 无（结构确定）/ 数值待真机 | RESOLVED（S5 落地） |
| `check_point_rewards` / `career_end_rewards` | 発表会 / 本番 6 条（`schedule` 1–3 两档）+ 育成终了 2 条 | `memo` 2026-10-31 / Wiki | `data/scenario_matsuri.json` | **S2 已落地**；成功档为中可信度、无实测样本 | 会位移（若成功档修正） | RESOLVED（已导入）/ TBD（成功档复核） |
| 粉丝奖励 | **不建模**（不参与任何计算） | `memo` | —（`intake.md` §3.6 留档） | 本项目不处理粉丝机制；若日后需要再入资产 | 无 | RESOLVED（不建模） |
| master.mdb `single_mode_15_*` 导入 | 已解码并导入 | `single_mode_15_*` 11 张表 + `single_mode_training_effect` | `data/scenario_matsuri.json`（映射见 `docs/plan/intake.md` §4.1） | **S2 已完成**；一次性提取脚本不入仓，重导出按 §4.1 核对 | 会位移 | DONE |
| `scenario_id` | `15` | data `single_mode_scenario` | `data/scenario_matsuri.json` | 已确认 | 无 | DONE |
| gauge 归属规则 | 已实现 | user 2026-10-30 / memo | `game/mod.rs` `gain_lowest_act_gauge` / `gain_training_gauge` + `game/action.rs` / `game/train.rs` | 参与担当**每人给自己担当的演目 +1**（按担当归集）；**闪彩担当 +2**，同场其他仍 +1；目标比赛 +4（按 4 次 +1，三则落点 + 满槽跳过）；休息 / 外出 / 非目标比赛 +1；**排练回合全 5 槽不涨**；稽古成功清槽 + Lv+1（合宿稽古不升不清） | 会位移 | RESOLVED（S5 实现） |
| 合宿延长规则 | 未实现（S3 计数已落） | user 2026-10-31 / memo 2026-10-31 | `game/stage.rs`（`TurnPlan` / `CampState`） | 每年保证 **4 个非比赛合宿回合**；目标比赛 / 强制休息顺延；**U18：所有年份的延长回合都算合宿（全部排练）**；**休息：原本夏合宿期间（Classic/Senior 7–8 月）为合宿版，延长回合为普通休息；第 2/3 年外出不可（含延长段）**；**离线模拟硬编码，不依赖在线下发**；S3 已落逐回合动态计数 + 标志（训练 / 事件分支 S4/S5/S6） | 会位移 | RESOLVED（口径）/ S3 已落（计数 / 标志） |
| 合宿期事件 | 已实现（S6） | user 2026-10-30 | `game/event.rs` `random_events_suppressed` | **第 1 年**：支援卡连续事件 + 普通马娘事件**都发生**；**第 2、3 年**常规合宿回合与**延长段**都抑制随机事件；固定事件不经过抑制判定；**抑制判定合并**为「合宿规则层（优先）+ 引擎 `no_event_turns` 基线层」；延长段支援卡连续事件按抑制实现（U19 待真机） | 会位移 | RESOLVED（S6）/ TBD（引擎基线层待剧本表） |
| 合宿 Lv +1 结算点 | 已实现 | user 2026-10-30 / 2026-10-31 | `game/settle.rs` | **固定为 8 月后半结束＝turn 15 / 39 / 63 结束时**（不随顺延推迟）；**全演目 +1**（上限 10）；S5 在 `End` 结算（`settle_turn_end`） | 会位移 | RESOLVED（S5 实现） |
| 吾妻补位 | 单次补位 | user 2026-10-30 | `game/distribute.rs` `place_friend_filler` | **不参与职责分配**：不占演目座位、不占训练位卡配额；普通训练按通用规则出现（不在率 100）；**排练必补 1 个未满排练位、不可分身** | 会位移 | RESOLVED（S5 实现） |
| 分身「+1 人」来源 | 已实现 | memo / user 2026-10-30 / 2026-10-31 | `game/distribute.rs` `place_splits` | **URA 回合（turn 72–77）全部演目稽古的编成卡各 +1 处（68–71 空窗期不生效）/ 協力 801 该演目每个担当 +1 处 / 转成 101 合宿中该演目每个担当 +1 处**；**落点 = 另一个训练种类、不叠排练位**；槽满不点则每回合持续有效；**等级归属（U41）未建模**（分身按占位计、不复制面板等级） | 会位移 | RESOLVED（S5 实现）/ TBD（U41） |
| 同名双人头 | **不允许同 chara 两个人头** | user 2026-11-01 | `game/roster.rs`（建局门禁） | 规则允许同名并存，但本模拟约束为「同 `chara_id` 只能是 distribution 分身」；编成卡同角色 / 团队卡 / 数量不对均报错；育成马娘是 link 角色时固定神驹换随机非 link 顶替 | 会位移（报错组合不可用） | RESOLVED（模拟口径） |
| 随机非 link 角色取人 | 从 `chara_id` 1001 起按序取第一个满足条件者 | user 2026-11-01 | `game/roster.rs` | 真机是从马娘池随机抓；离线简化为确定性取人（身份不参与结算），接在线模式可换随机；排除 5 link 角色 + 吾妻 + 编成卡 + 育成马娘 + 已出现人头 | 无（外观身份） | STUB（简化口径） |
| 神驹第 2 批人選 | 随机非 link（每演目 1 人） | memo / user 2026-10-30 / 2026-11-01 | `game/roster.rs` | 数据只有 `act_member` 5 行默认名单，第 2 批（turn 47 大成功 +5）人選未给；先按随机非 link 取人，待真机 / 剧情名单替换 | 无（外观身份） | STUB（人選） |
| 团队卡 | **不模拟**（出现即报错） | user 2026-11-01 | `game/roster.rs` | 规则允许团队卡，但本剧本模拟不考虑：`card_type 6` 建局报错 | 无（不可用组合） | RESOLVED（不模拟） |


| NPC 不在率 | `absent_weight = 50`（≈ 9.1%） | user 2026-10-30 / 参考 `traits.rs` | `game/traits.rs` `absent_weight` | 只作用于**未亮位置的普通训练随机站位**；**所有亮起演目**的担当卡 / NPC 锁定担当位、**不判不在**；备选档 50 / 100 / 200 对应 9.1% / 16.7% / 28.6%，数值待测（U29） | 会位移 | STUB（S5 已实现占位） |
| 多亮起锁人 | 所有亮起位置都锁定 | user 2026-10-31 | `game/distribute.rs` `place_locked` | 亮起演目的担当（卡+NPC）锁定担当位、不判不在；只有分身 / 未亮位置人物可移动；亮起过多会减少期望人头 | 会位移 | RESOLVED（S5 实现） |
| `check_point_effect 204` | レースボーナス 30 / 100 | data / memo / user | 剧本资产 | **比赛加成百分比**（30% / 100%）；所有赛事的五维与 SP 都吃 | 会位移 | RESOLVED（语义 / 百分比） |
| `check_point_effect 202` | 友情加成 5 / 10 / 20 | data / memo | 剧本资产 | memo 20/20 行吻合 | 会位移 | RESOLVED（语义） |
| `check_point_effect 2` | 灵感发生率 20 / 50 / 100 / 150 | data / memo | 剧本资产 | memo 20/20 行吻合 | 会位移 | RESOLVED（语义） |
| 発表会 / 本番结果 | **按实际演目 Lv 判定** | user 2026-11-01 | `game/settle.rs` `judge_presentation` | 5 个演目达 Lv3/5/6 → 大成功，否则成功；成功档数值取 memo 中可信度并真实生效；结算在 `End`、教诲分配之前 | 会位移 | RESOLVED（S5 实现）/ TBD（成功档数值真机复核） |
| `act_effects[].sp_training_hidden`（原 `act_effect 201` 的 value 30） | 保留原值 30 | data | `data/scenario_matsuri.json` | 全等级恒定；隐藏参数语义（待真机） | 可能位移 | TBD |
| 助力支撑 `20 → 47` | `rules.assist_support = { count: 3, effect: 5 }` | data / user | 剧本资产 / `game/train.rs` | 47 是内部编码、不是次数；效果 = **3 次内 +5%**，**未用完再次获得时剩余次数重置为 3**；稽古获得、训练消耗 | 会位移 | RESOLVED（S5 实现） |
| 教诲槽位 | **永远 5 槽** | user 2026-10-31 | `game/advice.rs` / `game/settle.rs` | turn 2 获得 5 个基础教诲并分配；追加 5 种不单独占槽；**分配 / 调整在每回合 `End`、剧本结算之后处理**；默认机械分配（按照获顺序 / 保持 / 融合就地） | 会位移 | RESOLVED（S5 实现） |
| 融合配对 | **不固定**（2! / 3!） | user 2026-10-31 | 剧本状态 | turn 23 选 2 个基础教诲配特化 / 研钻（`2!`）；turn 47 剩余 3 个配协助 / 不屈 / 兼修（`3!`）；可预设默认配对 | 会位移 | RESOLVED（结构） |
| 融合升级效果口径 | 升级基础效果取 `fusion_effect` 固定档 + 新教诲效果 | user 2026-10-31 / data | `game/advice.rs` | 融合版本 = 升级后的基础教诲效果（固定档）+ 所绑定新教诲效果（按演目 Lv）；原基础教诲效果被消耗 | 会位移 | RESOLVED（S5 结构）/ TBD（固定档数值真机） |
| 融合默认方案 | year2 `101+601` / `201+701`；year3 `401+801` / `301+901` / `501+1001` | user 2026-10-31 | 剧本配置 | 固定预设 + 在线可覆盖；融合不进搜索树；加载校验 2/3 槽与不重复 | 会位移 | RESOLVED（策略） |
| 协力分身数量 | 每个担当（卡+NPC）各一处 | user 2026-10-31 | 训练结算 / 教诲分配 | **801 只在第三年与 401 融合后出现**；2 卡 + 1 NPC = 3 分身；turn 47/59 硬约束到担当数最多的演目 | 会位移 | RESOLVED（结构） |
| 101 合宿额外 1 人 | 每个担当（卡+NPC）各一处 | user 2026-10-31 | 训练结算 / 教诲分配 | 仅合宿窗口作软分数，不与 401+801 抢硬约束 | 会位移 | RESOLVED（结构） |
| 教诲分配策略 | **S5 只落接口 + 机械默认** | user 2026-11-01 | `game/advice.rs` / 训练员（S7） | 120 全排列 + `apply_assignment`（校验排列）+ 401+801 硬约束已落；**价值矩阵 / top-K 择优留 S7**；改派 +1 保持；401+801 并列处理待定（主流卡组无此情况） | 会位移（策略） | RESOLVED（S5 接口）/ TBD（S7 策略 / 并列） |
| 属性基础上限口径 | 2200/1850/1800/1700/1800；突破后 2500 | official + memo | 剧本资产 / 常量 | 已定主口径；机制包 2150 一组不入代码 | 会位移 | RESOLVED |
| 演目稽古基础值口径 | 静态表按 `command_level = 演目 Lv` | memo 2026-10-29 / data | 剧本资产 / 训练结算 | **按时期两档**：Junior `16/14/14/15/12`、Classic 以后 `21/20/20/23/14`（第二档覆盖 Classic 与 Senior，**无第三档**，U02 2026-11-01 收口）；**基础值无 Lv2–10 维度**（mdb 5101–5105 只有 `sub_id=1` 一组行；`command_level` 行只定义命令；+10 来自 `act_effect 105`，全 Lv 恒 10） | 无（口径已收口） | RESOLVED |
| 训练公式两层 | **下层 = 基础库原始公式；剧本加成全部走上层** | user 2026-10-31 | `game/train.rs` `training_upper` | 下层用 `default_calc_training_value`；演目 Lv / 教え / 発表会 buff / 稽古 +10 等走上层；上下层分别 floor 后相加（子项 floor 再叠加）；具体乘区见 mechanism §3.1 | 会位移 | RESOLVED（S5 实现） |
| 比赛奖励表（全部赛事） | 目标赛 5×5 / SP72；目标外 GI 单项 +10 / SP45；GIII 单项 +8 / SP35；URA 预选 / 准决赛 / 决赛 五维 +10、SP 40 / 60 / 80 | memo 2026-10-29 / user 2026-11-01 / `umaai-next` | `data/events.json`（引擎事件）/ 比赛结算 | **只建模基础值**；**有馬記念等特殊比赛的额外数值不属于本表、不建模**；比赛加成乘算、**统一 floor**；非目标赛随机 1 项用引擎 `extra_status.random`（均匀无放回，真机复核待办） | 会位移 | RESOLVED（数值 / 实现）/ TBD（真机复核随机性） |
| 第三年底目标赛剧本加成（U46） | 未实现 | memo 2026-10-31 观察 | 比赛结算 | `floor(基础值 × (1 + レスボ))` 是否作用于 Senior 11 月前半 / 12 月后半的目标赛；有馬需先扣特殊比赛自带值 | 会位移 | TBD（待真机） |
| 目标赛事 SP 奖励 | `+72` | memo 2026-10-29 定稿 | `data/events.json`（`race_career` 覆写）/ 比赛结算 | 基础 **+72**，**比赛加成 30% 时给 93（乘算、floor）**；参考实现 +3 / +45 不采用 | 会位移 | RESOLVED（数值/取整） |
| 目标比赛 gauge 增量 | `+4` | user 2026-10-31 | 剧本资产 | **按 4 次 +1 入账；落点统一三则：Lv 最低 → 槽最低 → 属性序，满槽跳过；并列逐次分摊** | 会位移 | RESOLVED（数值 / 落点） |
| Senior 每演目 +1 担当 | memo 已答 | memo | 人头布局 | 归档旁证 `evaluation_info_array` 25 → 30（5×5 → 5×6）；逐元素核对后收口 | 会位移 | TBD（复核） |
| 事件触发回合 | memo 时间轴已给；**10 月前半进化 = turn 66** | data / memo / user | `game/event.rs` | 其他事件仍可故事脚本复核 | 会位移 | RESOLVED（turn 66）/ TBD（其他） |
| 追加稽古发生率 | `p = r(绑定演目 Lv)`，**1 回合 1 次共通掷骰** | `memo` 2026-10-31（推翻旧 `min(100%, r×N)`） | `data/scenario_matsuri.json`（`advices` id=701 → `levels[].rate`）+ 训练结算 | `r` = 研钻 Lv1–10 → **20/22/24/27/30/35/40/50/65/80(%)**；**不按担当人数加算**；掷中后该演目担当参与的成功训练都触发；回合开始预掷、图标＝掷中，不点则作废 | 会位移 | RESOLVED（口径）/ TBD（真机复核） |
| 追加稽古 gauge 增量 | +1（最低演目） | memo | `scenario_matsuri.json` `event_extras`（`400015106`, choice 0） | 仅上档给；下档不给；走上限拦截（满槽跳过、三则落点） | 会位移 | RESOLVED（S6 实现） |
| 追加稽古上 / 下档 | **玩家选择**（非概率） | user 2026-11-01（B6 作废 50/50） | `scenario_matsuri.json` `scenario_events[400015106]` + `event_extras` | 训练员选择上 / 下档：上档直前训练属性 +20（`ExtraStatus::LastTrain`）/ SP+20 / 体力-5 / 理事长羁绊+5 / 最低演目 gauge+1 / 解一个 debuff；下档直前训练 +5 / 体力 +5；删除 `rules.kensan.upper_prob`（U40 作废） | 会位移 | RESOLVED（S6 实现）/ TBD（选项策略留 S7） |
| 排练失败 | 完全无效 | user 2026-11-01 / memo 2026-10-31 | `game/train.rs` `settle_training` | 触发失败后事件、不给属性收益；**槽不动、演目 Lv 不升**（推翻旧「Lv 仍 +1 / 槽清零」） | 会位移 | RESOLVED（S5 实现） |
| 吾妻无凸 / R 连续事件 | 未知 | memo | 事件管线 | 官方 UI / 真机 | 会位移 | TBD |
| 通常训练 Lv 上升 | 已实现 | user 2026-10-30 | `game/train.rs` + `game/traits.rs` `train_level` | **基础训练 Lv + 每 4 次 +1**；**合宿中与点演目稽古时不增加**；发布会大成功「全训练 Lv+1」用独立 `train_level_bonus` 叠加 | 会位移 | RESOLVED（S5 实现） |
| 教诲效果等级 | 按教诲自身 Lv 的读法 | user 2026-10-31 / 新文档 §6 | 训练结算 / 教诲分配 | **效果等级 = 所绑定演目的 Lv（1–10）**；**改派后随新演目 Lv 改变** | 会位移 | RESOLVED（口径） |
| 担当分配一次性 | 视频「担当交代 N ターン」 | user 2026-10-31 | `game/stage.rs` | **无周期轮换**；担当分配仍是建局 / NPC 加入时的一次性动作，不是阶段 | 无（阶段设计） | RESOLVED（口径）/ TBD（实现） |
| 合宿结束 Lv+1 范围 | 视频「3 个演目各 +1」 | user 2026-10-31 | 结算 | **全演目 +1**；3 条为样本截断 | 会位移 | RESOLVED（口径） |
| U41 分身等级归属 | 现有三处来源 | 新文档 §22 | 人头布局 / 教诲 | `effect_type=0` 每级都有 vs 视频 Lv5–10，待测 | 会位移 | TBD（待测） |
| U44 囤多演目衰减 | 定性实现 | 新文档 §22 | 训练员估值 | 「所有亮起位置锁人」已定性；量化期望待测 | 会位移 | TBD（待测） |
| U45 吾妻强制召唤 | 不建模 | 新文档 §22 | 人头布局 | 是否以羁绊为前置待测 | 会位移 | TBD（待测） |
| 外出策略（前两年用完友人 / 其他演目推 Lv8） | 未纳入估值 | user 2026-10-31 / video | 训练员估值 | 与「外出早点用 / 不要用外出回体力」并列，待估值后定；不改变机制 | 可能位移（估值） | TBD（策略） |

| 马娘特殊事件记录 | **只登记凯斯奇迹 T65 强制休息**（S3 临时表） | user 2026-11-01 | `game/stage.rs`（临时常量表） | 目标赛回合被固定事件改为**实际执行休息**（`plan.race` 仍为真，不产生比赛事件 / 奖励）；**后续引擎层改造「马娘特殊事件记录」数据结构**以涵盖其他异常事件，改造后改读引擎数据、删除临时表 | 会位移 | 临时（硬编码单条）/ TBD（引擎改造） |
| URA 期是否全变稽古 | **存疑，待确认** | user 2026-11-03 | `game/traits.rs`（`list_actions`）/ `game/stage.rs`（`TurnPlan::ura_period`） | URA 回合 = **72–77**；68–71 是空窗期、不属于 URA。URA 期是否「全部训练变演目排练」待确认，**当前实现按日常机制**（训练攒槽 → 槽满稽古 → 成功 Lv+1）；专属收益来自本番演出、只在 72–77 生效 | 会位移 | TBD |
| URA 演出生效区间 | 已确认：72–77 | user 2026-11-03 | `game/train.rs` `ura_training_active` / `game/distribute.rs` `place_splits` | 体力 +30 / 干劲 +1 / 失败率 -100% / 稽古编成卡 +1 人**只在 URA 回合 72–77**；68–71 空窗期不生效；体力顺序 = 先耗体下降 / 训练扣体 / 失败率下降，再回体（净回体） | 会位移 | RESOLVED（口径） |
| 演目 Lv 满后的槽 / 排练 | 已确认（通用规则） | user 2026-11-03 | `game/mod.rs` / `game/train.rs` | 演目 Lv 到上限（10）后**仍可攒 5 个槽并排练**；排练成功后槽清空、Lv 保持 10 不变（不再升） | 会位移 | RESOLVED |
| URA 期事件 hint 收益 | 通用路径已计入；具体事件数值待确认 | user 2026-11-03 | `game/event.rs` / `game/diag.rs` / `cli/manual.rs` | 事件 `hint_level` 计入 `total_hints`（引擎 `add_value`）；诊断「变化」/ 面板变化新增「灵感±N」；`data/events.json` 的 URA 决赛三条事件目前只有五维 / PT，如真机另有 hint 需补数值 | 会位移（若有新增 hint 数值） | TBD（具体数值） |
| 友人卡门禁 | 仅 **SSR 吾妻**可建局 | user 2026-11-01 | `game/mod.rs`（建局校验） | `card_id` 由 `outing` 表识别 + 稀有度 SSR；**R 吾妻 / 不带友人 / 其他友人卡一律建局报错**；演目 Lv 上限固定 10 | 会位移（不可用组合报错） | RESOLVED（口径）/ S3 已落（校验） |
| 友人出行卡面事件数值 | `809052011–015`（真实卡面事件，基础值入库 + 引擎叠卡面加成） | user 2026-11-01（Q2 按 memo 全量）/ `memo` §11.2 | `data/events.json` `friend_events["1".."5"]` + `game/action.rs` | 每段 = 体力 / 属性 / SP / 羁绊；第 3 段上 / 下双选项；未确认项取 memo 下界 / 首档（段 1 体力 33；段 2/4 干劲 +1、羁绊 +5）；技能灵感按 `hint_level` 记账（快速 Lv2 / 神速 Lv3）；`matsuri_outing_1..5` 占位事件已删 | 会位移（原 STUB → 当前记录值） | RESOLVED（S6 实现）/ TBD（真机复核） |
| 合宿版休息数值 | 普通 30/50/70（`consts.rest_probs`）；合宿版 +40 体 / +1 干劲 | 引擎基础局默认（`is_xiahesu` = 36–39 / 60–63） | `game/action.rs` `do_rest` | memo 无数值；真机复核后替换 | 会位移 | STUB（S4 已实现） |
| 训练失败 / 大失败判定 | 引擎默认公式 + 剧本失败率下降（不屈 / 本番演出） | 引擎 `calc_training_failure_rate` / `training_fail*` 事件 | `game/train.rs` `settle_training` / `game/traits.rs` `calc_training_failure_rate` | 不屈按参与担当阈值（Lv10 体力无关）、本番演出 -100%；合计 ≥100 → 必定成功 | 会位移 | RESOLVED（S5 实现）/ TBD（真机复核） |
| 训练后处理与事件排空 | `AfterTrain` 排空 `unresolved_events`；羁绊按剧本基础值 + 絆教え平摊 + 爱娇一次；灵感 / 加练 / 点击保持；**友人首次 / 点击事件已接真实吾妻数据** | 剧本资产 / memo / 引擎同型 | `game/train.rs` `after_train` / `game/stage.rs` / `game/traits.rs` | 友人点击：首次点击必发，之后按引擎常量 40%（与追加稽古预掷独立） | 有（比赛 / 出行 / 红点 / 点击） | RESOLVED（S6 实现） |
| 合宿期自选比赛 | **允许**（`can_self_race` 满足时） | user 2026-11-01 | `game/traits.rs` `list_actions`（camp 分支） | 无 | 有（动作集） | RESOLVED（口径）/ S4 已落 |
| 治病动作 | 纳入 S4：`BaseAction::do_clinic`（+20 体、清生病位 / 不擅长） | 引擎基础局 / user 2026-11-01 | `game/action.rs` `do_clinic` | 后续事件口径复核 | 会位移 | RESOLVED（S4 实现） |

| 训练上层「上昇量上限」 | `rules.training_upper_cap = 100` + 特化 `status_limit`（按参与聚合）+ 稽古 5 种卡时演目 Lv9/10 `status_limit`；SP 上限同法 + `sp_limit` | user 2026-11-01 / 引擎 §5.8 / act_effect 206 | `data/scenario_matsuri.json` `rules` / `game/train.rs` | 引擎只写「基础上限 100，可被剧本提升」；剧本层叠口径与属性基础上限（2200 组）是否同步上抬待真机 | 会位移 | STUB（S5 占位实现） |
| 演目稽古「全基础 +10」 | 上层乘区之后全五维 flat 加（未使用维也 +10） | user 2026-11-01（选项 A） / ja memo / 参考实现 | `game/train.rs` `training_upper` | 若真机为「下段附加项 / 仅主副非零维」则改口径并重录 | 会位移 | STUB（S5 占位口径） |
| 发布会影响率叠加 | 剧本灵感率与卡面 `hint_prob_increase` 同层加算：`p = 7.5% × (1 + (卡面 + 发布会 + 教え)/100)` | memo 数值 / 推导 | `game/distribute.rs` `distribute_hints` | 倍率 vs 加算待真机；上限 clamp 1.0 | 会位移 | STUB（S5 占位口径） |
| 发布会成功档一次性奖励 / buff | 数据已入资产（memo 中可信度）并真实生效 | memo / Wiki | `data/scenario_matsuri.json` | 成功档数值无实测样本；真机复核后替换 | 会位移 | TBD（成功档数值） |
| 技能灵感 / 粉丝 | **不建模**：发布会 / 本番 / 育成终了 / 出行里的具体技能 Hint Lv 与粉丝数跳过 | 本仓无技能 / 粉丝系统 | — | 接技能系统 / 粉丝机制后补 | 无（只少算收益） | TBD（不建模） |
| 友人点击事件 | **已接真实吾妻事件**：首次点击 → `809052001` 首次事件并转 `BeforeUnlock`；之后每次点击按 `friend_click` 40% 推 `809052032` 训练后事件 | user 2026-11-01（A5 / Q4） | `game/train.rs` `after_train` + `data/events.json` `friend_events` | 与追加稽古预掷**相互独立**；首次事件不在 40% 判定内 | 会位移（羁绊 / 事件收益） | RESOLVED（S6 实现） |
| 追加稽古「随机解 debuff」 | 简化为清「不擅长训练」/ 生病位（无 debuff 系统） | memo（未给细节） | `game/train.rs` `try_kensan` | 真机 debuff 系统 / 实测后替换 | 会位移 | STUB（S5 占位） |

## 代码标记约定

```rust
// STUB: 占位值，见 docs/plan/stubs.md
const SAMPLE_RATE: i32 = 100;
```

- `STUB`：值已存在但未确认，**可以跑**；
- `TBD`：逻辑或数据缺失，**需要后续补齐**（不要用它绕过机制未确认）。
- 标记只用于数值 / 数据占位；**机制未确认不得用 stub 绕过**（应按 S0 闸门先确认机制）。

## 收尾检查

- [ ] 所有 `STUB` / `TBD` 都已在本表登记；
- [ ] 每项都有替换条件与影响面；
- [ ] 替换后已重录基线并在 `glossary.md` 留痕；
- [ ] 代码中搜索 `STUB:` / `TBD:` 与登记表一致。
| 变胖状态（`Ill::FAT`） | 训练 / 排练速度增量归零；每回合 20% 解除；治病 / 合宿版休息解除 | user 2026-11-01 + memo/上游 | `data/scenario_matsuri.json` `rules.fat_recovery_prob` + `game/event.rs` / `game/train.rs` / `game/action.rs` | 事件来源=构造事件「大胃王」33% 档；解除概率为剧本自持暂定值（STUB）；训练后事件 / 休息 / 出行 / 比赛 / 事件仍可加速度 | 会位移 | STUB（概率待真机）/ 已实现 |
| 大胃王（构造事件） | `id 5010`（**不模拟真实 ID：马娘事件 ID 每个马娘不同**，通用模拟用固定构造 ID）/ Random `max_time 3` / 选项 1 体力+10 PT+5；选项 2 体力+30 PT+10 + 33% 变胖 | user 2026-11-01（A17 + 补充：不用真实 ID） | `data/events.json` `uma_events` | 各马娘真实事件 ID 不同，本仓只做通用构造；次数 / 概率 / 数值按用户口径冻结 | 会位移（事件池多一条） | RESOLVED（构造口径） |
| 友人卡事件槽位映射 | `001` first / `032` click / `004` out / `021` newyear / `011–015` 出行 1–5 / `023` end；`002/003/033` R 卡、`022` 未满 5 段结局变体、`031` 随机事件 | user 2026-11-01 核对（按名字 + memo + 温泉同构） | `data/events.json` `friend_events` | 待真机帧复核槽位语义（尤其 `023` 结局 / `004` 出行解锁） | 会位移（若映射错） | TBD（真机复核） |
| 友人事件数值口径 | memo 值按**基础值**入库，结算时引擎叠卡面「事件效果 +30% / 回复量 +60%」 | user 2026-11-01（A13 / B11 按引擎）+ 推断证据（事件⑤ 56=35×1.6 等） | `data/events.json` `friend_events` + 引擎 `apply_friend_bonus` | 若「事件⑤」观测值证明 memo 是终值，则需关闭加成并修正全部友人事件 | 会位移 | STUB（口径推断） |
| 主剧本过场事件 | 属性 ≤5 点，忽略不计；`003/010/020` 仅记录为 Advice 标志、`026` 仅记 hint+1 | user 2026-11-01 | 文档记录 | 如后续实测某过场事件有 >5 点属性再入资产 | 无 | RESOLVED（不建模） |
| 事件⑤（SSR 技能事件） | 不接（`022/031` 均非事件⑤） | user 2026-11-01 | — | 场上有值：体力槽+5 / 体力+56 / 干劲+1 / 根性·智力+13 / SP+26 / 神通 hint / 最低演目 Lv+1；若真机给出事件 ID 再入资产 | 会位移（少算） | TBD |
| 引擎 `no_event_turns` 基线层 | 沿用引擎基线（STUB）；合宿规则层优先 | user 2026-11-01（Q6 按推荐） | `game/event.rs` `random_events_suppressed` | 真机剧本专属「不可发生连续事件回合表」出来后只替换第二层 | 会位移 | STUB |
| 通用事件数值（引擎基础局） | 按 `data/events.json` / `constants.json` 原始值直接模拟（马娘登场 / 新年 / 继承 / 卡连续 / 马娘事件 / 掉心情 / 抽签 / 育成结束） | user 2026-11-01（Q9 / B2 / B12） | `data/events.json` / `data/constants.json` | 引擎手写近似值，非剧本 15 真机口径；后续真机复核 | 会位移 | STUB（口径）/ RESOLVED（接入） |
| `400015105` 教え見直し数值 | 体力 +15 / 干劲 +1（turn 11/35/59） | memo（中可信）；user 2026-11-01「需实测」 | `scenario_matsuri.json` `scenario_events` | 真机实测后替换；mdb 3 行疑似 3 选项不建模 | 会位移 | STUB |
| turn 66 进化技能 | 只记 `hint_level + 1`（不建事件、不占回合） | user 2026-10-30 / 2026-11-01（B9） | `game/event.rs` `apply_evolution_hint` | 金技能 / 具体灵感 Lv 不建模；按「进化 = 该灵感升 1 级」记账 | 会位移（PT） | RESOLVED（记账）/ TBD（真机） |
| 马娘特殊事件（非 T65） | 先不管（只登记凯斯奇迹 T65） | user 2026-11-01（A9） | `game/stage.rs` 临时表 | 引擎「马娘特殊事件记录」改造后统一迁移 | 会位移 | TBD |
| 训练员策略旋钮默认值 | 攻略 / memo 方向的首档（pt_favor 2.0、休息阈值 45、达标保护 3 回合、Senior 合宿前 Lv8、权重默认等） | user 2026-11-02「先按攻略设计旋钮、后续扫参」 | `data/matsuri_config.toml` `[trainer]` / `config.rs` `TrainerConfig::default` | 后续用配置覆盖层扫参（与融合配对 A/B 一起；S9 前只记录冒烟位移） | 会位移（全部决策质量） | STUB（策略参数，待扫参） |
