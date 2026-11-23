# 2026-11-23 评审状态复核

## 1. 复核范围与基线

本报告重新核对 review 目录现有评审记录、当前文档真源与源码，不直接沿用旧报告的提交号、路径或结论。

- 剧本仓 HEAD：86bd946。
- 引擎子仓 HEAD：ae3cab1。
- 工作树存在用户未提交修改；本次只读复核，未修改生产代码。
- 初次复核未运行 cargo test（旧 target 的锁被拒绝访问）；后续随机流专项使用工作区 temp 的独立 target，实跑证据单列于 §8，其余条目不据此宣称实跑。
- 旧报告引用的 docs/plan/... 等路径已迁移或删除，相关结论按当前实际文件重新判断。

状态含义：已修复、部分修复、仍存在、待验证、已取消/不纳入。

## 2. 已修复或已收口

| 来源问题 | 当前结论 | 当前证据 |
|---|---|---|
| derive_seed 纯 XOR 低位碰撞 | 已修复：派生算法改逐段加法，BaseGame 的 TurnFixed 也已带显式非零标签 | umaai_next/crates/umaai-core/src/rng.rs；umaai_next/crates/umasim/src/game/base/mod.rs |
| MAX_SCORE 高分右尾截断 | 已修复 | umaai_next/crates/umasim/src/search/result.rs 已改样本存储，并有 110000 分测试 |
| 在线事件搜索因 event_queue 为空而旁路 | 已修复 | crates/matsuri-ai/src/session.rs 注入当前事件并按阶段续跑 |
| 在线教诲帧错误落 Stage::Train | 已修复 | crates/matsuri-ai/src/online.rs 按 advice window 判 Begin/End |
| 在线融合帧与预设重复落地 | 已修复 | crates/matsuri-ai/src/session.rs、crates/matsuri-core/src/game/settle.rs |
| preview_training_layers 漏判变胖 | 已修复 | crates/matsuri-core/src/game/train.rs，已有预览/结算对齐测试 |
| 技能灵感字段统一与每卡上限 | 已修复 | cardDB.skillCount、引擎 SupportCardData.skill_count、删除 card_skills.json |
| 规则流派生散落为手写数组 | 已修复：收敛到 `StreamTag::derive(master, turn)`，形状在其中固化 | umaai_next/crates/umaai-core/src/rng.rs；base/mod.rs、basic.rs、umaai/session.rs、模板、matsuri-core/stage.rs |
| 锁位分支跳过随机导致 CRN 游标错位 | **已修复**（方案 A）：落位恒定 1 字、意向槽不可行才用同一个字回退；同结构对照的入口/不在/终游标归零，跨板仍分叉属结构差异（§8） | crates/matsuri-core/src/game/distribute.rs；引擎 `distribute_person` |
| 目标赛 +4 涨槽并列规则 | 已修复 | 旧评审已记录 6da1e88 修复，当前不再列为现存 Bug |
| 融合/状态位类型化与主要结算链 | 已落地 | game/advice.rs、game/settle.rs、数据层 deny_unknown_fields |
| 复盘局包动作/教诲/融合差分反推 MVP | 部分已落地 | matsuri-tools/src/review；事件动作和完整质量门禁仍有边界 |
| 跨局聚合、AI sidecar 记录层 | 已取消，不纳入待办 | docs/todo.md 已明确取消 |

## 3. 引擎仓遗留问题

### 3.1 P1：直接影响搜索或通用规则正确性

1. **已关闭（后续复验，§8）**：BasicGame 和剧本模板的 apply_root_action 已改走策略流；Matsuri 原有路径无需修改。
2. BaseGame 事件状态位契约未闭合：BaseGame::apply_event 仍主要结算数值，update_flags 由具体 Game 分散补调。
3. 支援卡连续事件候选仍有固定 0..6 假设，缺少按实际卡候选的数据驱动守门。
4. do_hint 多次发放和非法训练位边界不足，缺少 hint_count>1 接近容量与非法 train_type 回归。
5. FlatSearch UCB 预算语义仍不稳：search_n=0 在 UCB 路径静默按 1；按单候选 max_planned 停机；计划数与成功样本数分裂。
6. 失败 rollout 仍可参与正常排名，未将候选标记为 invalid/degraded。
7. max_depth/叶估尚未接线：入口只拒绝不支持的深度，rollout 循环没有截断叶估。
8. stats.rs 的 percentile、零方差 paired、非有限值和小样本 CI 仍缺守门。
9. RaceMask(u64) 无法覆盖 URA 回合 75/77，free_race 等级 0 仍可能被当作有效比赛位。
10. 引擎数据校验不足：概率范围/和、赛程重复、rank/card 字段和 free-race 覆盖范围未完整校验。
11. 协议事件解析不完整：R/SR 支援卡、Story 内层多分支及 extras/buff 仍有丢失。
12. 更新器事务不持久：安装/回滚状态只在内存，启动阶段仍可能清理 .old；路径、symlink、大小写冲突和本地写盘错误域仍未收口。

### 3.2 P2：架构与通用能力

- 通用 Session 仍吞原始 JSON、路由类型有限，并由帧驱动副作用推进。
- BaseGame 的 RuleStreams 仍由具体 Game 自持策略流，通用托管契约未统一。
- 协议还原仍不能完整承载 person 级 is_hint 与非卡人头字段。
- Successive Halving/Racing 仍停留在 docs/search_sh.md 设计稿，未落地为可选引擎方案。

## 4. 剧本仓遗留问题

### 4.1 P1：在线可信性与搜索统计

1. 失败帧提前登记 seen：matsuri-ai/src/session.rs 在验算/搜索完成前插入 digest，失败重投会被当作重复帧。
2. 搜索结果没有新鲜度失效门禁：没有 generation、run_id、state_hash，旧搜索结果可能在新帧后继续发布。
3. 缺失训练等级会让永久 bonus 下降：game/restore.rs 缺等级时继承上一帧有效总等级并反解 bonus，3→4 等反例仍未锁定。
4. 训练验算忽略 failure rate：matsuri-ai/src/output.rs 的 TrainingDeviation::is_match 只比较七项收益 delta。
5. 缺失 campRemain/assistSupportLeft 被当成 0：restore.rs 使用 unwrap_or(0)，与协议中 None=未知的约定冲突。
6. TrainingFrame lower/upper 长度不校验：frame.rs 的 zip 会静默截断非 7 项数据。
7. rollout 健康度只有观测，没有有效性门禁：planned/succeeded/failed 已记录，但失败样本仍可能改变候选排名。
8. 搜索摘要 E/σ 不是同一随机量：weighted_baseline、pooled_stdev 和 PT 回扣不能代表选中动作的官方终局分布。
9. 恢复层静默丢弃未知人头但无 typed quality 门禁：diag 可留痕，自动执行仍无法按 Degraded/Unknown 阻断。
10. 同回合多个决策面共用同一 decision RNG：局包 7084 的事件、训练、教诲三面已复现；详见 §8。

### 4.2 P2：维护和策略上限

- AtomicBool keiko_settling 仍作为训练表上下文 side-channel，尚未改为显式参数。
- 融合组合仍由 decks.fusion 预设决定，AI 不搜索 fusion pair；在线对账已修复，但策略能力未扩展。
- 教诲仍按手写分从 120 剪到 Top-16，value.rs 仍用全育成 horizon；没有全 120 vs Top-16 召回率/遗漏收益实验。
- T65 强制休息仍是 stage.rs 临时表，待引擎特殊事件记录能力。
- targetId 旧注释仍声称可用 targetId-1 映射卡位，与当前实现冲突。
- hint 折算公式、hint_efficiency=0.92 和多次 hint 容量边界仍需真机/独立小程序验证。
- bad_trainer 与全量 charaEffects 清除一致性缺专门回归。
- ADR-0010 建议的 type13 + 词条33 cardDB 守门测试未见。

## 5. 复盘工具链遗留

- report 分数轨迹仍需改为 T(n) 期望终局分主图；facts.score 只作属性参考。
- report --narrative 仍需决定 Markdown 渲染或明确纯文本输出。
- 在线常驻 luck 写入仍待从 T0 开始的完整局实测。
- regret/CI/what-if 已有 MVP，但事件面不可判、候选分布和完整分环节归因仍不完整。
- 跨局聚合已取消，不再列为待修复项。

## 6. 当前优先级建议

### P1：先处理

- decision RNG 按决策面加盐。
- 失败帧 seen 生命周期与业务重试。
- 搜索结果 generation/state_hash 新鲜度门禁。
- 缺失训练等级时永久 bonus 不下降。
- None 与有效 0 的协议字段语义分离。
- failure_rate 和七项收益同时参与验算。
- rollout 失败结果进入 invalid/degraded 门禁。
- FlatSearch UCB/SH 预算与不等样本问题。
- 训练帧七项长度守门。
- 恢复质量标记与自动执行门禁。

### P2：再处理

- RaceMask/free-race 数据契约。
- BaseGame 事件状态位与 RNG 轴契约。
- AtomicBool 显式上下文化。
- 融合策略是否进入 AI 决策。
- 教诲 Top-16/horizon 对照实验。
- T65 特殊事件迁移。
- E/σ 统计口径统一。
- Hint 折算和容量边界验证。

### P3：最后处理

- targetId 注释冲突。
- R/SR 事件和 Story extras。
- 更新器持久事务。
- report T(n)、narrative、在线 luck 实测。
- type13 + 词条33 守门测试与复盘文案收口。

## 7. 复核原则

- 参考实现能否复用，必须检查代码中的剧本门限和仓库归属，不能只信文档描述。
- 计划文档中的“必须补”先按最小改动面收敛；能由已有帧或字段推出的内容，不自动升级为必须新增插件端点。
- 设计稿、源码内联测试和已有提交记录，不能等同于本轮实际运行测试；报告必须显式区分证据等级。

## 8. 随机流第一步：离线 Release 配对实验

### 根动作：对照正常阶段，不比较终局均分

- BasicGame：普通休息，体力 0、策略游标从 1 开始，8 个规则主种子 × 决策流提前抽 0 或 5 次。修复前 16/16 消耗决策流，修复后 0/16；修复后全部局面与显式策略流参考局相同。缺策略流时拒绝，不回退到决策流。
- 场景模板：训练和休息目前都是确定性动作。训练与正常阶段全局面相等，休息体力 50 → 70；两者均保留非零策略游标及外部决策流，缺策略流均拒绝。不能把“没有抽数”当作随机训练的隔离证据。
- Matsuri：开局推进到真实 Train 阶段，体力设为 25，固定速训练及事件首选项。规则种子 0..127，搜索根相对正常阶段多预抽 5 个决策字。64 次成功、23 次普通失败、41 次大失败；128/128 的动作结算及 AfterTrain 后全局面一致（含 RNG、事件记录；共 11 个队列事件）。

### 锁位 CRN：同一块板能补齐，换板不保证能补齐

每个场景取规则种子 0..1023。两侧均调用真实分配入口；另从同一 RNG 副本逐人重放，要求与未插桩入口的最终整板及 RNG 相同。不在判定、首抽槽与拒绝原因是副本重建值，不是生产日志。每臂同输入复跑，10,240 对 A/A 的分布与终态 RNG 全部一致。

| 场景（每行 1,024 对） | 任一观测边界游标不同 | 分配结束游标不同 | 观测边界相同、整板不同 |
|---|---:|---:|---:|
| T0 人工亮一槽，隔离吾妻及访客 | 565 | 474 | 459 |
| T20 无锁 → 单演目锁位 | 1,024 | 805 | 0 |
| T20 一个 → 两个锁定演目 | 635 | 557 | 389 |
| T20 无锁 → 五演目全稽古 | 1,024 | 861 | 0 |
| T20 人工满槽，新增锁位预放五人 | 970 | 892 | 54 |
| 合计 | 4,218 | 3,589 | 902 |

- 相同占位板、同一 RNG 入口的单人对照（空板、满槽、自身重复、社交冲突）：1,024/1,024 的 burn 与引擎普通分配终游标相同。整轮差异不是简单的“少抽一次”。
- 可复现的常规反例：T20、seed 0，无锁与单锁分支的 p1 均从游标 6 首抽槽 0。无锁接受，游标到 8；锁位板已经有 p1，拒绝后重抽槽 2，游标到 9。共同未锁的 p3 实际落槽 0 / 2；整轮终游标 28 / 27。
- 5,120 对里有 629 对中途游标分叉、最后重新相等；仅看终游标会漏掉。比较两侧共同未锁且有对应抽签轨迹的卡与神驹（21,504 人次），有 2,263 次不在结果不同、2,273 次拒绝次数不同、11,983 次最终占位不同。不同锁态本应改变部分合法槽位；不能把每次占位差异都归为随机流错误，也不能把这些人工等权场景比例当作实战发生率。

### 在线决策：同回合不同决策面已分面派生（已修复）

**修复前**（三面共用 `decision_rng(turn)`）：局包 7084 原始帧、不改字段，T47 的事件（830289002）、训练、教诲阶段键分别为 0、1、2，却读到相同的决策流首字 8802492840419419720；真实 FlatSearch 内核回调（只观察输入种子，不拿占位分数解释选择）收到的前四个 rollout 种子也逐项相同。T48 首字 899928688232075080，与它不同。固定 T47 训练局面、只换决策流，推荐从 #4 飞悬稽古翻到 #2 纳舞稽古（每候选 4 次，只说明抽样敏感，不说明哪个更优）。

**修复**：新增 `matsuri_ai::online::decision_seed(master, game, route)`，按 `derive_seed(master, [turn, PROBE_TAG, 决策面, 面内标识])` 派生——训练 1、事件 2 + 事件 id、教诲 3 + 窗口；`session.rs` 三处决策点与 `review eval` 共用该入口。同帧同局面仍逐位可复现；同回合同一面（反复进出训练屏）仍共享随机骨架，便于「同回合换动作」的反事实对照。帧字节（含仅供验算的训练预览）不参与派生，`G13` 不受影响。

**修复后实测**（同批原始帧，每候选 4 次）：三面种子分别为 `0x3888a7b2d8da1695`（事件）/ `0x132cd541b6a4d824`（训练）/ `0x4eb0f55c0bbf4c9b`（教诲），前四个 rollout 种子不再重合；T48 训练为 `0x0ac431dc8a11d15d`。独立会话重放同帧仍逐位一致，重送同帧仍返回 `Skipped`。

**影响**：在线决策的抽样入口整体换代（旧口径 `[turn, PROBE]`），`review eval` 的旧缓存按 seed 自动失效；离线 `bench` / 基线不走这条入口，数值不变。

### 引擎派生、TurnFixed 迁移与同次搜索的 CRN

历史实现（提交 0a2ffdc 之前）先异或所有分量再做一次 SplitMix64；交换分量顺序、追加零、同时翻转 base 和一个分量的相同位，三类输入在旧式中等值。当前逐段加法派生区分了这批反例；不是全域无碰撞证明。

BasicGame 实测 master=42、T0，TurnFixed 主种子为 `0x57e1faba65107204`，匹配不带 tag 的公式；显式标签候选为 `0xf24323fdb7f69720`。fork 与同种子 reset 对齐。固定 4 个初始 master × T0/T1/T2/T10，仅在真实分配前替换 TurnFixed，其他轴仍用原主种子：显式 tag 对照 16/16 改变人头分布，历史 XOR 对照也是 16/16。标签迁移会移动已测固定种子基线；不等于当前分配存在偏差。

均匀 FlatSearch 真实回调：7 候选 × 4 次，28 次回调，所有候选同序号的 seed 均等于同一张 CRN 表对应值。保留此配对与隔离跨决策面的搜索入口并不冲突。

`DECISION_TAG`：在本次核查的两仓生产 Rust 源码未找到该常量的消费者。FlatSearch 实收根来自入口 RNG 的一个字，而线上入口用 Probe 标签。只改未被消费的常量与更改线上正在使用的派生公式不是同一件事。没有更改标签或重录基线。


### 锁位 CRN：落位改为「同一字定意向槽 + 可行槽回退」（已实施）

根因不是“补得不够”，而是**消费次数依赖板**：落位原本“接受即停”，抽几次取决于第几次抽到可进的槽；锁位预放改变了板，`burn_locked_person` 又按已被改过的板重放，次数必然对不上。

改法（`Game::distribute_person`）：

1. 每次落位**恒定消费 1 个字**；不在判定仍是 1 个字。
2. 先按固定权重把字映射成「意向槽」（与板无关，跨分支同一个字给出同一个意向槽）。
3. 意向槽恰好不可行（已满 / 已有本人 / 同槽已有友人）时，用**同一个字**在可行槽权重上重映射。
4. 于是“手气差导致有空位却不出现”消失：只要还有可行槽，就必定落位。

`burn_locked_person` 同步降为「不在 1 字 + 落位 1 字」，且完全不读分布；吾妻补位、访客落位、分身的选位也从 `random_range` 改成恒定 1 字的等权选取（`random_range` 的字数随长度与拒绝采样变化，会把后续人头挤位）。

同结构对照（两侧都含稽古或都不含，规则路径一致）：

| 场景 | entry_diff | absent_diff | end_diff |
|---|---:|---:|---:|
| T0 单演目 | 1553 → **0** | 246 → **0** | 474 → **0** |
| T20 单 → 双锁 | 2491 → **0** | 397 → **0** | 557 → **0** |
| T20 合成满槽 | 3251 → **0** | 533 → **0** | 892 → **0** |

落位差异仍存在（366 / 1072 人次），那是「锁位人头已占槽」的合法状态差。

仍分叉的两个场景是**结构不同**的那类：无锁 → 单锁、无锁 → 全稽古。它们让友人补位在「必到稽古位」与「普通不在判定」之间切换、访客在「无普通位」与「有普通位」之间切换。这是规则差异，不是实现噪声；要继续闭合得改规则本身。

成本（`bench parts`，20000 次）：`distribute_all` 498→622 / 491→636 / 543→569 / 587→618 ns（T2/T5/T20/T40），约 +5–29%，**早期回合偏高**（板较空时原实现重抽少）。比“固定抽满 10 次”那版的 +27–41% 好。

语义变化（有意）：不再有“10 次都没抽中 → 不出现”，只要存在可行槽就必定落位。原来那条「锁位不改变其他人头落位」的断言不再成立（可行槽变化本来就会改变后续落位），改为更强且可验证的判据 `fallback_only_when_intended_slot_blocked`：

- 对照取两个**都含稽古**的板（只差多亮一个演目），保证友人 / 访客走同一路径、消费严格对齐；
- 断言同一个字在两支给出**相同意向槽**；
- 只有意向槽在某一支不可行时才允许回退；两侧都可行时落位必须相同；
- 实测 64 个规则种子、347 次有效对照，其中 23 次回退；把回退改成无条件触发后该用例立即失败（变异验证有效）。

引擎 L4 基线与剧本基线因此位移，按 C 批次与 TurnFixed 标签一起重录；在此之前 `umaai-tests --test baseline` 会红。


### 落位改动后的基线位移实测

`bench baseline`（手写档 100 局，seed0 73200）与引擎 L4 复跑结果：

| 档 / 训练员 | 分数 旧→新 | Δ | 容差 | PT 旧→新 | Δ | 容差 | 判定 |
|---|---:|---:|---:|---:|---:|---:|---|
| a-normal 手写（100 局） | 82193.1→82548.8 | +355.6 | ±1169.1 | 11850.6→12202.8 | +352.2 | ±167.6 | DRIFT（PT） |
| a-normal 搜索（10 局） | 86508.1→86438.2 | −69.9 | ±2106.6 | 12456.7→12896.6 | +439.9 | ±366.8 | DRIFT（PT） |
| b-ideal 手写（100 局） | 86922.5→87773.4 | +850.9 | ±869.2 | 12418.6→12818.9 | +400.3 | ±156.9 | DRIFT（分数 + PT） |
| b-ideal 搜索（10 局） | 89734.2→89973.9 | +239.7 | ±1341.9 | 13369.4→13504.1 | +134.7 | ±391.9 | **OK** |

分数 sd：手写 2755.7→2378.2 / 1954.8→1632.5；搜索 1570.2→1229.1 / 1000.2→738.3（四组全部下降）。

搜索档单次耗时：a-normal 442.1 s、b-ideal 402.1 s（各 10 局，约 40–44 s/局）。

引擎 L4（`umaai_next/crates/umaai-tests/baselines/`）：

- `basic_full_run`：score 16387→16367、pt 3075→3072，五维有 ±5–10 的小位移；
- `basic_search_root`：`means[0..6]` 全部上移 +400…+660；
- `basic_seed_sweep`：`score_sum` 128356→130372，各项 ±(100–1500)。

两个档的分数 sd 都明显下降（2756→2378、1955→1633），与“落位不再因手气差丢人、每回合人数更稳定”一致。

**尚未覆盖**：基线 JSON 未改写——按 C 批次与 `TurnFixed` 标签一起重录，避免同一批数据位移两次。

读法：四个档的 PT 全部上移（+135 ~ +440），方向一致；分数只有 b-ideal 手写档超出容差。搜索档两组分数差异都在容差内，b-ideal 搜索档整体判 OK。四组 sd 同步下降，与「落位不再因手气差丢人、每回合人数更稳定」一致。


### TurnFixed 显式标签迁移（已实施）

`BaseGame::reset_rule_streams` 的回合固定流由 `derive_seed(master, [turn])` 改为 `derive_seed(master, [turn, TURN_FIXED_TAG])`，与事件流形成对称的两条带标签规则流。常量与 `StreamTag::TurnFixed` 的映射本来就有，缺的只是调用口径。

迁移会改掉每个回合的 TurnFixed 主种子，因此全部分布重掷。手写档复跑（100 局，seed0 73200）：

| 档 | 分数 | Δ | PT | Δ |
|---|---:|---:|---:|---:|
| a-normal | 82420.6（上一版 82548.8） | +227.5 | 12175.9 | +325.3 |
| b-ideal | 87303.7（上一版 87773.4） | +381.2 | 12751.3 | +332.7 |

引擎 L4 同样再位移：`basic_full_run` score 16387→16916，`basic_search_root` 的 `best_action` 由 `NormalOuting` 变为 `Train(1)`。这些数字与落位改动叠加，**只作为重录输入，不作机制收益解读**。

### 复跑

在剧本仓根运行以下命令。若默认构建目录不可写，在各自的 `cargo test` 后加 `--target-dir <可写临时目录>`；同一 target 串行执行。在线用例默认读仓根旁的 `../Backup/game7084`；可用 `MATSURI_RNG_FRAME_DIR` 指向含 `game7084_turn47.json`、`game7084_turn47_2.json`、`game7084_turn47_4.json`、`game7084_turn48_2.json` 的目录。缺原始帧会明确报错，不会退成合成帧。

```powershell
cargo test --release --offline -p matsuri-core --test rng_train_probe -- --nocapture
cargo test --release --offline -p matsuri-core --lib game::distribute::tests::crn_ -- --include-ignored --nocapture --test-threads=1
cargo test --release --offline -p matsuri-ai --lib session::tests::online_decision_rng_probe -- --ignored --exact --nocapture
cargo test --release --offline --manifest-path umaai_next/Cargo.toml -p umaai-tests --test rule_streams --test search_smoke
cargo test --release --offline --manifest-path umaai_next/Cargo.toml -p umaai-tests --test rng_seed_probe -- --nocapture
cargo test --release --offline --manifest-path umaai_next/templates/umaai-scenario/Cargo.toml -p scenario-core
```


