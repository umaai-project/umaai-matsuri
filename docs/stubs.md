# 数值占位与待核登记（STUB / TBD）

> 机制已确认、数值未到或待真机复核的统一登记处。**占位不阻塞开发**，但必须可整体替换、可盘点。
> 规则：
> - 占位集中放在剧本资产 / 配置 / 常量表，代码里不散落魔法数；
> - 代码标记统一为 `STUB:`（临时占位）与 `TBD:`（待补逻辑/数值），并在此表登记；
> - 替换为确认值后：重录基线，并在 [`changelog.md`](changelog.md) 的决定记录写明位移原因与影响面；
> - 机制口径以 [`mechanism.md`](mechanism.md) 为准；功能性开发待办见 [`todo.md`](todo.md)。

## 登记表

| 项 | 当前占位值 | 来源 | 存放位置 | 替换条件 | 基线影响 | 状态 |
|---|---|---|---|---|---|---|
| `training.normal` Lv2–5 | 5 训练 × Lv2–5 × 7 值 | `memo` 2026-10-31 | `data/scenario_matsuri.json` / `data.rs` | 已入仓（Lv1=mdb），Lv2–5 为 memo 中可信度，待真机复核 | 会位移 | TBD（Lv2–5 复核） |
| `check_point_rewards` 成功档 | 発表会 / 本番成功档数值 | `memo` 2026-10-31 | `data/scenario_matsuri.json` / `data.rs` | 已入仓并真实生效，成功档无实测样本，真机复核后替换 | 会位移 | TBD（成功档复核） |
| `act_effects[].sp_training_hidden` | 保留原值 `30`（`act_effect 201.v1`） | `master.mdb` | `data/scenario_matsuri.json` / `data.rs` | 全等级恒定，隐藏参数语义待真机 | 可能位移 | TBD |
| 随机非 link 角色取人 | 从 `chara_id` 1001 起按序取第一个 | user 2026-11-01 | `game/roster.rs` | 离线确定性简化，在线可换真机池 | 无（外观身份） | STUB |
| 神驹第 2 批人选 | 随机非 link（turn 47 大成功 +5） | `memo` / user | `game/roster.rs` | `act_member` 只有 5 行，待真机/剧情名单替换 | 无（外观身份） | STUB |
| NPC 不在率 | `absent_weight = 50`（≈ 9.1%） | user 2026-10-30 | `game/traits.rs` `absent_weight` | 备选 50 / 100 / 200，待真机统计（U29） | 会位移 | STUB |
| 特殊状态「铁心的挑战者」（status 31） | 按马娘 ID 106301 / 106302 硬编码 | user 2026-11-03 | `game/mod.rs` `special_race_stats_bonus` | 状态系统落地后改读 status 31 | 会位移（狄杜斯） | STUB |
| 第三年底目标赛剧本加成（U46） | 未实现 | `memo` 2026-10-31 | 比赛结算 | 核实 Senior 11 月前半 / 12 月后半目标赛是否吃レスボ | 会位移 | TBD（待真机） |
| Senior 每演目 +1 担当 | `memo` 已答，待旁证逐元素核对 | `memo` | 人头布局 | 对照 `evaluation_info_array` 25→30 核对后收口 | 会位移 | TBD（复核） |
| 追加稽古发生率 | `p = r(演目 Lv)`，1 回合 1 次共通掷骰 | `memo` 2026-10-31 | `data/scenario_matsuri.json` | 研钻 Lv1–10 概率待真机复核 | 会位移 | TBD（真机复核） |
| 吾妻无凸 / R 连续事件 | 未知 | `memo` | 事件管线 | 真机观察后补齐 | 会位移 | TBD |
| U41 分身等级归属 | 分身按占位计、不复制面板等级 | 新文档 §22 | `game/distribute.rs` | 核实 `effect_type=0` 全级 vs Lv5–10 差异 | 会位移 | TBD（待测） |
| U44 囤多演目衰减 | 定性实现「所有亮起锁人」 | 新文档 §22 | 训练员估值 | 量化期望衰减后校准 | 会位移 | TBD（待测） |
| U45 吾妻强制召唤 | 不建模 | 新文档 §22 | 人头布局 | 核实是否以羁绊为前置 | 会位移 | TBD（待测） |
| 外出策略（前两年用完友人 / 推 Lv8） | 未纳入估值 | user / video | 训练员估值 | 估值验证后决定是否纳入 | 可能位移（估值） | TBD（策略） |
| 马娘特殊事件记录（含 T65 迁移） | 只硬编码凯斯奇迹 T65 强制休息 | user 2026-11-01 | `game/stage.rs` 临时表 | 引擎改造「马娘特殊事件记录」后迁移并删临时表 | 会位移 | 临时 / TBD |
| 普通训练 Lv 失败是否计数 | 成功才计数（`floor(count/4)+1`） | 引擎同型 | `game/train.rs` | 真机复核训练失败是否影响计数 | 会位移 | TBD（真机复核） |
| 合宿版休息数值 | +40 体 / +1 干劲 | 引擎基础局默认 | `game/action.rs` `do_rest` | 真机复核后替换 | 会位移 | STUB |
| 训练失败率与不屈阈值 | 引擎默认 + 不屈/本番扣减 | 引擎 / `memo` | `game/train.rs` / `traits.rs` | 真机复核（Lv10 体力无关） | 会位移 | TBD（真机复核） |
| 训练上层「上昇量上限」与属性基础上限 | `rules.training_upper_cap = 100` 等 | user / 引擎 §5.8 | `scenario_matsuri.json` / `game/train.rs` | 核实属性基础上限（2200 组）是否同步上抬 | 会位移 | STUB |
| 演目稽古「全基础 +10」 | 上层乘区之后全五维 flat 加 | user 2026-11-01 | `game/train.rs` `training_upper` | 若真机为下段附加项则改口径 | 会位移 | STUB |
| 发布会影响率叠加 | `7.5% × (1 + (卡面+发布会+教え)/100)` | `memo` / 推导 | `game/distribute.rs` `distribute_hints` | 乘算 vs 加算待真机复核 | 会位移 | STUB |
| 技能灵感 / 粉丝 | 不建模（只计 `hint_level` 总账） | 本仓边界 | — | 接技能 / 粉丝系统后补 | 无（少算收益） | TBD（不建模） |
| 追加稽古「随机解 debuff」 | 简化为清「不擅长」/ 生病位 | `memo` | `game/train.rs` `try_kensan` | 真机 debuff 系统明确后替换 | 会位移 | STUB |
| 变胖状态解除率 | 每回合 15% 解除（暂定） | user / 上游 | `scenario_matsuri.json` `rules.fat_recovery_prob` | 15% 档待真机复核（25% 档配对噪声内） | 近零 | STUB |
| 友人卡事件槽位与数值口径 | `001/032/004/021/011–015/023`；基础值叠卡面加成 | user / `memo` | `data/events.json` `friend_events` | 真机帧复核槽位语义与是否为终值 | 会位移 | STUB / TBD |
| 事件⑤（SSR 技能事件） | 不接（`022/031` 均非事件⑤） | user 2026-11-01 | — | 真机给出事件 ID 后入资产 | 会位移（少算） | TBD |
| 引擎 `no_event_turns` 基线层 | 沿用引擎基线 + 49 局标定补 0-based 1/2/12/68（合宿规则层优先） | user 2026-11-01 / 2026-11-20 | `game/event.rs` | 真机剧本专属不可连续事件表（按卡独立段）出来后替换 | 会位移 | STUB |
| 通用事件数值（引擎基础局） | 直接沿用 `events.json` / `constants.json` | 引擎近似值 | `data/events.json` / `constants.json` | 真机复核非剧本 15 口径事件 | 会位移 | STUB |
| `400015104` 合宿の申し出数值 | 体力 +15 / 干劲 +1（T11/35/59） | `memo` / user | `scenario_matsuri.json` | 真机实测确认第 1 年是否 +10 及事件归属 | 会位移 | STUB |
| 在线 `resultType` 0 / 1 取值（G3） | `2` 为大成功；失败/普通档未采样 | `online_protocol.md` | `game/restore.rs` | 真机采样失败/普通档発表会帧核对 | 仅在线还原 | TBD（真机采样） |
| 在线 `fatigueRecovery` 消费时点（G8） | 帧直读状态 45 | `online_protocol.md` | `game/restore.rs` | 验证与引擎一次性 flag 等价性 | 仅在线还原 | TBD（真机验证） |
| 在线延长段 `campRemain` 递减（G9） | 未采样到延长段连续帧 | `online_protocol.md` | `game/restore.rs` | 采样延长段连续帧核实 `camp.counted` | 仅在线还原 | TBD（真机采样） |
| 育成终了部分大成功混合档（U14） | 按全成功档处理 | `memo` 2026-10-31 | `game/settle.rs` | 真机打出「部分大成功」收尾样本核对 | 会位移 | TBD（真机复核） |
| `scenario_data_array` `15001 = 46`（U26） | 未建模（疑似五演目 Lv 之和） | 上游协议 | — | 不同演目 Lv 结局对比该值 | 无（仅校验） | TBD |
| turn 47 非大成功是否 +5 人（U27） | 按「大成功才加」实现 | 推演 | `game/settle.rs` | Classic 汇报会走成功档对照 `evaluation_info_array` | 会位移 | TBD（真机复核） |
| 第 6 名 NPC 全并列 tie-break（U32） | 按「人数最少 → 属性序」实现 | 推演 | `game/roster.rs` | `1/1/1/1/1` 卡组 + 吾妻实测全并列落点 | 会位移 | TBD（真机复核） |

## 代码标记约定

```rust
// STUB: 占位值，见 docs/stubs.md
const SAMPLE_RATE: i32 = 100;
```

- `STUB`：值已存在但未确认，**可以跑**；`TBD`：逻辑或数据缺失，**需要后续补齐**。
- 标记只用于数值 / 数据占位；**机制未确认不得用 stub 绕过**。

## 收尾检查

- [ ] 所有 `STUB` / `TBD` 都已在本表登记；
- [ ] 每项都有替换条件与影响面；
- [ ] 替换后已重录基线并在 `changelog.md` 留痕；
- [ ] 代码中搜索 `STUB:` / `TBD:` 与登记表一致。
