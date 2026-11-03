# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓。

- **开发代号**：`matsuri`（别名：mee-umasim 参考库用 `shuntaisai`，本仓统一用 `matsuri`）
- **形态**：workspace 五 crate（与 `umaai-onsen` 同构）——`matsuri-core`（机制 + 训练员 + 装配，
  宿主稳定面）/ `matsuri-sim`（模拟器与基线裁判）/ `matsuri-ai`（在线决策核心，骨架）/
  `matsuri-cli`（终端入口与手动训练员）/ `matsuri-tools`（测卡 / 复盘，骨架）。
- **依赖**：开发期以 path 引用 `../umaai_next` 的 `umaai-core` / `umasim` / `umaai`；稳定期换
  git tag。不 fork 引擎、不在引擎仓加剧本 crate。
- **数据**：引擎静态数据已拷入 `data/`；剧本资产 `scenario_matsuri.json` 与配置默认层
  `matsuri_config.toml` 由本仓自持，根目录由宿主注入。

## 当前状态

- **S1 建仓完成**：五 crate 改名、依赖路径、引擎静态数据、CI 与文档骨架已落地。
- **S0 机制与用语文档基本收口（2026-10-31 / 2026-11-01 memo 复核）**：`single_mode_15_*` 11 张表与
  `single_mode_training_effect` 已解码；彩圈 +2、gauge 归属、合宿延长 / Lv+1、U18 延长回合、U33 合宿外出、
  友人分身、稽古基础值、**训练值两层口径（主 / 副属性拆分）**、目标赛 +4、`check_point_effect 204`、
  10 月前半 turn 66、排练失败「完全无效」、牵绊教诲 +3 平摊等均已定为口径。
- **S2 数据层完成（2026-11-01）**：`data/scenario_matsuri.json` 16 个顶层键（`master.mdb` 11 张表 +
  `single_mode_training_effect` + memo 口径，映射见 [`docs/plan/intake.md`](docs/plan/intake.md) §4.1）
  + `data/matsuri_config.toml`（融合预设）+ `data.rs` / `config.rs` 重写 + `tests/data.rs` L2；
  四档验收绿（35 测例）。已接入 `umaai-next` 新基础：`data/events.json` 同步赛后奖励与 URA 决赛事件
  （目标赛 `race_career` 覆写 5/72，非目标赛随机 1 项，比赛乘算统一 floor）。演目 / gauge / 教え / 合宿等
  机制在 S3+ 落地，剩余 TBD 见 [`docs/plan/stubs.md`](docs/plan/stubs.md)。
- **S3 局面与阶段机（2026-11-01）**：一回合五阶段 `Begin → Distribute → Train → AfterTrain → End`
  （`StageId` 0/3/1/4/2，只增不改）；回合类型 / 边界（强制比赛、合宿窗口与延长、発表会 / 本番、
  教诲窗口、空窗期 / URA 回合）由 `TurnPlan` 表达；`CampState` 做合宿计数与顺延；演目 / 教诲账本
  （`ActLedger` / `AdviceLedger`）+ `MatsuriView` 全量快照；建局门禁=仅 SSR 吾妻；凯斯奇迹 T65
  强制休息按临时记录落地（后续引擎「马娘特殊事件记录」改造后迁移）。四档绿（34 内联 + 11 L2 +
  5 机制 = 50 测例）。**参考卡组**换为：速·待兼诗歌剧 / 速·乐透心 / 耐·樱花千代王 /
  力·摩耶重炮 / 智·爱慕律动 / 友·SSR 吾妻（满凸）。演目 / gauge / 教え / 発表会 / 合宿结算等机制在 S5+ 落地。
- **S4 前置：剧本人头与名单（2026-11-01）**：新增 `MatsuriPerson`（引擎 `BasePerson` 委托 +
  担当 / 来源 / 可用回合）与 `PersonOrigin`（与引擎 `PersonType` 分工：前者剧本标签、后者引擎语义）；
  建局完成「卡 → 理事长 / 记者 → 编成外神驹（第 1 批 5+余额 1、第 2 批 5）」名单与担当分配
  （link 钉位 / 整位 2 张 / 外派链 / 余额）；**编成检测**双层：禁止项（缺 SSR 吾妻 / 多友人 /
  非 SSR 吾妻友人 / 通用规则）拒绝建局，警告项（link 类型 / 同类 ≥3 / 外派 / 类型 <5 种 /
  人物数极差 ≥2）随 `MatsuriView` 下发。
  四档绿（38 内联 + 11 L2 + 13 机制 = 62 测例）。
- **S4 动作与结算（2026-11-01）**：`Action` 全集（普通训练 / 演目稽古 `ActTrain` / 比赛 / 休息 /
  普通出行 / 友人出行 / 治病）+ 按回合类型的动作集（强制比赛 / 强制休息 / 合宿全稽古 / 日常与 URA 回合）；
  单动作结算：目标赛 / URA / 自选比赛走 `push_race_event`（比赛加成 = 卡面赛后 × 発表会 / 本番永久 buff，乘算；floor 在引擎）、
  普通训练与演目稽古下层公式（稽古切 `training.keiko` 表）+ 失败判定、普通 / 合宿版休息、普通出行、
  吾妻出行 5 段（每段最低演目 Lv+1 + memo 五段事件）；`AfterTrain` 统一结算 `unresolved_events`、
  `apply_event` 补事件羁绊归属、`Distribute` 过渡清零（真实分布留 S5）。四档绿
  （39 内联 + 12 L2 + 22 机制 = 73 测例）；整局冒烟（样例训练员）7690 分 / B、390 阶段。
  演目 Lv / gauge / 発表会 / 训练上层加成 / 事件链留 S5 / S6。
- **S5 机制（2026-11-01）**：机制闭环补齐——①**人头分布**重写（亮起锁担当 / 神驹不在率 50 /
  吾妻排练补位 / 協力・転成・URA 分身 / 每位置 ≤5 / 得意率 + 発表会基线）；②**演目 gauge** 入账 /
  消费（训练参与按担当归集、闪彩 +2；目标赛 +4；休息 / 出行 / 非目标赛 +1；稽古成功清槽 + Lv+1，
  合宿稽古不升不清；三则落点 + 满槽跳过）；③**稽古两档**基础值（`training.keiko.classic` 恢复，
  turn 24 起切档）与**演目 Lv 效果**（全基础 +10 / 友情 / SP 排练 / 灵感 / SP·基础上限）；④**教诲系统**
  （10 种效果 × 绑定演目 Lv × 参与人数叠加、絆 +3 平摊、助力支撑、研鑽追加稽古、不屈、両道、
  turn 2/11/23/35/47/59 窗口、turn 23/47 転成融合、120 排列接口留 S7）；⑤**训练两层公式**（下层 =
  引擎默认公式 + 上层 = 发布会 / 教え / 演目 Lv / 稽古 +10 / 获得量乘区，主 / 副 / SP 拆分 + floor +
  上昇量上限）；⑥**発表会 / 本番 / 合宿 +1 / URA 训练（72–77）/ 育成终了**结算（结果按实际演目 Lv 判定，
  buff / 一次性奖励 / 訓練 Lv+1 / 本番演出 / 全演目 +1 / 终局奖励）；⑦`MatsuriView` 补 S5 资源字段。
  四档绿（**81 内联 + 13 L2 + 25 机制 = 119 测例**）；整局冒烟（样例训练员）**34368 分 / UE9、
  PT 6128、390 阶段**（random 基线 19516 分 / SS+、PT 3985）。
- **S6 事件管线（2026-11-01）**：回合开始事件链落地——①**固定事件**（引擎通用 `story_events` +
  剧本固定 `scenario_events`：`400015104` 駿大祭合宿の申し出 turn 11/35/59、`400015106` 追加の稽古代码触发）；
  ②**随机分布**（支援卡连续事件 / 马娘事件 / 掉心情，分布与数值按引擎 `constants.json` / `events.json`
  原始值；支援卡事件候选只取可用支援卡人头，友人 / 团队不计入）；③**合宿抑制**合并为单一判定
  （第 1 年合宿允许；第 2/3 年**常规窗口**抑制随机事件、**延长段事件轴等同正常回合**（2026-11-03 U19 结项）；
  非合宿回合沿用引擎 `no_event_turns` 基线，STUB）；④**友人链**（吾妻 `809052xxx`：首次点击 → 首次事件 / 之后 40% 训练后事件 / 出行解锁上·下
  → 疲劳回复 flag / 出行 5 段真实卡面事件 / Classic 正月 / 结局；友人事件叠卡面事件效果加成）；
  ⑤**追加稽古事件化**（训练员二选一：上档 = 直前训练 +20 / SP+20 / 体力−5 / 理事长羁绊 +5 /
  最低演目 gauge+1 / 解一个 debuff；下档 +5 / 体力+5；删除 50/50 STUB）；⑥**变胖状态**
  （大胃王事件 33% 附加：训练 / 排练速度增量归零，回合开始 20% 概率解除、治病 / 合宿版休息解除）；
  ⑦turn 66 进化技能只记 `hint_level + 1`；⑧接引擎通用「回合后」事件（turn 48 抽签 / turn 77 育成结束）
  与继承特殊结算。四档绿（**95 内联 + 15 L2 + 27 机制 = 137 测例**）；整局冒烟（样例训练员）
  **44946 分 / UC6、PT 7496、390 阶段**（S6 事件收益接入后的预期位移；S9 前不建基线）。
  策略训练员（S7）已于 2026-11-02 落地；搜索（M2）、技能灵感与粉丝不建模（TBD）留后续。
- **手动面板显示第五轮 + 追加稽古 / 助力支撑显示与判定（2026-11-05）**：①**教诲选择提示补名称**
  （`流镝←101 牵绊`）；②**追加稽古 / 助力支撑可视化**——`ResourceView` 增 `kensan_act` / `kensan_rate` /
  `assist_act` 只读派生字段；资源行标「追加稽古 掷中·演目(概率)」「助力支撑 ×N（触发 演目）」；动作候选
  可兑现时标 `追稽✓ / 助力支撑✓`；训练表加配色图例；掷中未兑现诊断「未选择追加稽古训练」；
  ③**演目槽落点显示**：面板「变化」区新增 `【演目槽】演目 x→y / 清空`，手写档比赛 / 休息 / 出行 /
  友人出行入槽各出 `【演目槽】… 入槽` diag；④**合宿直前事件改挂 `400015104 駿大祭合宿の申し出`**
  （体力+15 / 干劲+1，数值沿用 STUB）；⑤**五维显示值按 >1200 减半**（值与上限都压缩；评分 / 收益 /
  变化量仍用原始值）；⑥**所有事件名亮蓝**；⑦**训练表人物配色**：非彩圈研鑽担当（含 NPC）亮紫、
  稽古列助力担当黄（**研鑽紫色为当轮初版，同日后续改为列标题紫行，见下一条**）。另修
  `value.rs::kensan_bonus` 稽古分支改为参与者口径（与 `try_kensan` 一致）。
  **另修友人 / 理事长记者落位**：⑧**理事长 / 记者不会出现在稽古训练位**（有稽古位时只落非稽古位；
  全稽古回合 = 合宿 / URA / 5 槽全满时本回合完全不出现）；⑨**吾妻只要有稽古训练就必补到某个未满 5 人的
  稽古位**（补位顺序改到锁位后、其余人头前；不判不在、不去普通位；唯一稽古位必定命中），无稽古回合走
  通用不在判定（实现 `practice_train_mask` / `place_guest_outside_practice` + 2 个守门测试）。
  四档绿（**123 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 178 测例**）。
  **基准种子改为 61444**（原 42；落位修正后单 seed 波动过大，换参考 seed）：整局冒烟
  **56210 / UA1、PT 9131**（旧 seed 42：49587 / UB2、PT 8433；S9 前不建基线）。
- **追加稽古呈现调整 + 训练位社交人头唯一性（2026-11-05 补）**：①资源行「追加稽古 掷中·演目(概率)」
  改为亮黄「**可触发追加稽古**」（不写概率 / 演目）；训练表中**参与者含研鑽担当**的列标题下加一行
  紫色「追加稽古」（与触发判定同口径、可多列；比赛回合等无训练位的掷中不提示）；**取消人头紫色**与
  动作后缀「追稽✓」，诊断保留「掷中」措辞；②**每训练位至多 1 名「友人 / 团队 / 理事长 / 记者」**
  （引擎 `Person::is_friend()` 口径；修复自定义放位漏校验导致理事长 / 记者同列，加守门测试）。
  四档绿（**124 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 179 测例**）；
  整局冒烟 **56489 / UA1、PT 9207**；`random` **23010 / UG8、PT 4644**（分布修正后位移；S9 前不建基线）。
- **S7 手写训练员（2026-11-02）**：`MatsuriTrainer` 换成手写策略——①**目标口径 = pt_favor**
  （官方评分 `五维查表 + PT×pt_score_rate`，再乘 `pt_favor_rate × PT`；`1.0` = 官方评分，默认 `2.0`）；
  ②**策略旋钮进剧本配置** `MatsuriConfig [trainer]`（pt_favor / 休息阈值 / 発表会达标保护 /
  Senior 合宿前演目 Lv 目标 / 自选赛开关 / 友人外出尽早用 / 五维权重 / 属性资源权重 / 教诲分配权重；
  覆盖层整段替换 + 载入期范围校验）；③**守门链 + 打分表**（治病 → 强制赛 / 强制休 → 其余候选统一打分：
  官方评分边际 + 体力 / 干劲 / 羁绊 / 灵感 + 演目槽 / Lv / 达标急迫 + 亮起囤积惩罚 + 追加稽古兑现 +
  失败期望；自选赛默认不打）；④**事件选项策略**（组内结果按概率加权求期望 + `ExtraStatus` + 状态增删 +
  友人加成；17 个 `player_select` 决策点全走打分，不再恒取首项）；⑤**教诲分配走 Trainer**：
  T2 Begin 首次授予后 + turn 11/23/35/47/59 每个窗口把 120 个排列合并为一次 `Action::AssignAdvice`
  决策交 Trainer 择优（融合仍走配置预设，`settle_advice_window` 的 `fusion_plan` 留将来替换点）。
  四档绿（**107 内联 + 15 L2 + 29 机制 + 5 CLI 内联 + 1 CLI 冒烟 = 157 测例**）；整局冒烟
  **54683 分 / UB9、PT 9024、390 阶段**（旧样例训练员 40899 / UC、PT 6982；同种子两次逐位一致；
  S9 前不建基线）。
- **S8 手动核对面板（2026-11-01，先行落地 + 四轮显示调整）**：`manual` 档从「摘要 + 编号输入」
  升级为机制核对面板——**开局打印育成配置**（育成马娘 / 卡组含突破与稀有度 / 继承：蓝因子 + 剧本因子 +
  开局继承）；每次动作决策前打印回合结构 / 五维（含引擎上限）/ 体力干劲 / 状态 / **剧本资源表**（演目
  2 字短名 · 槽 x/y · Lv · 教诲→転成 · 担当人物；発表会结果与永久 buff / 助力支撑 / 追加稽古着色）/
  **彩色训练明细表**（每训练一列，列头 = 训练名 Lv，排练时列头 = 演目名 Lv；格内列出该位置全部人物：
  短名、未满 100 的羁绊、Hint 亮黄叹号；人物按类型着色：友人绿 / 彩圈卡亮绿 / 普通支援卡白 / NPC 灰）/
  合宿（仅合宿回合显示计数）/ 友人链（状态·出行段·下一段事件）/ 目标赛与自选赛窗口 / 动作候选（训练成功
  收益预览 + 失败率 + **演目槽增量**，**Inquire 交互选择，非 TTY 退回编号输入**）；事件选择用 **Inquire**，
  事件名亮蓝色，选项组列概率 / 结果档 / 到账数值 / ExtraStatus / 状态增删；每次提示附「自上次以来新结算
  事件」（含 id / 名称 / 选项 / 实际到账）、「上一步后数值变化」（含 **演目 Lv 上升**行）与**教诲分配结果**
  （首次传授 / 転成 / 改派·保持；结果行后紧跟刷新后的剧本资源表）；状态行仅显示已有状态。颜色与表格复用
  `umaai-cli` 呈现原语（非 TTY / `NO_COLOR` 自动退化纯文本）。核心侧新增 `EventRecord` 事件记录（容量 32，
  `MatsuriView.recent_events` 同步）、`preview_training` 与 `preview_action_resources` / `ResourcePreview`
  （克隆局面只读预览，不耗随机流、不改局面）；`[嘉宾]` 统一改 `[NPC]` 且用角色名前两字。
  **剧本机制自 turn 2 起：0–1 回合无担当分配，训练 / 休息 / 出行 / 事件不获取演目槽**
  （`MatsuriGame::scenario_started` 闸门），面板在 0–1 回合标「未分配 / 机制自 T2 开始」；**教诲首次授予
  以 turn 2 Begin（NPC / 友人加入后立即）为准，效果从 T2 的分布 / 训练起生效**（End 的窗口 0 不再重复授予）。
  手动档引擎 `PrintFrontend` 切 `Quiet`，面板为唯一信息源；CLI 冒烟（管道输入跑满整局 + 非 TTY 无 ANSI +
  教诲 / 资源 / 短名 / 无合宿误报断言）。四档绿（**100 内联 + 15 L2 + 27 机制 + 5 CLI 内联 + 1 CLI 冒烟
  = 148 测例**）；样例训练员冒烟 **40899 分 / UC、PT 6982**（T2 闸门 + T2 教诲生效后的位移；S9 前不建基线）。
  S8 其余（在线外壳 / 训练员选择器）待做。
- **S8 View 补全 + diag 诊断（2026-11-02）**：①**`MatsuriView` 升级为回合信息完整快照**（B 方案分节重构，
  落点 `core/src/game/view.rs`）：`turn / resources / uma / board / schedule / friend / camp` 七节 +
  `deck_warnings` / `recent_events`，手动面板已显示的数值 / 人头分布 / 训练位 / 赛程 / 友人链 / 合宿全部
  进快照；名称文本不进（呈现层查静态表）、动作候选与预览不进（决策上下文）。②**手动面板改纯呈现**：
  所有状态从 `view()` 取，`game` 只剩只读预览与建局配置；`view_fields_match_game_at_train_decision`
  在 78 个 Train 决策点逐项守门。③**diag 诊断接线**：复用引擎 `umaai_core::diag!`（统一入口
  `matsuri_core::diag`）+ `log` 依赖 + CLI stderr 日志器消费 `[output] log_level`（默认 `info`，
  覆盖层可设 `off`）；埋点覆盖回合 / 事件链 / 事件结算 / 状态增减 / 动作与变化 / 训练后 / 演目槽 /
  分布 / 教诲 / 発表会 / 合宿 / 终局奖励 / 训练员决策，预览与搜索 rollout 自动静默。
  四档绿（**114 内联 + 15 L2 + 30 机制 + 1 diag + 5 CLI 内联 + 1 CLI 冒烟 = 166 测例**）；整局冒烟
  **54683 分 / UB9、PT 9024（与 S7 逐位一致，未位移）**；整局诊断约 1.6k 行，`DiagGuard` 静默不改变终局局面。
- **URA 阶段口径修正（2026-11-03 / 11-04，用户逐项核对）**：①URA 训练限定效果（体力 +30 / 干劲 +1 / 失败率 -100% /
  稽古编成卡 +1 人）**只在 72–77 生效**；②**68–71 是「剧本结束到 URA 的空窗期」**、不属于 URA——`TurnPlan.interlude`
  + `MatsuriView.turn.interlude`，`ura_period` 改 72–77，面板 / 日志标「空窗期」；③体力顺序 = 先稽古耗体下降 /
  训练扣体 / 失败率下降，再判定回体 → 常为**净回体**；④**演目 Lv 满（10）后仍可攒 5 槽并排练**，排练后槽清空、
  Lv 保持 10；⑤**URA 期全变演目稽古（类似合宿，2026-11-04 确认）**：5 个训练位全部 `ActTrain`（锁担当 / 全灵感 /
  编成卡 +1 人按全排练口径），稽古成功仍按日常 Lv+1 / 清槽（URA 不是合宿，不冻结）；⑥**URA 事件无 Hint（无技能）**——
  「URA 期事件 hint 需要记入」作废，通用事件 `hint_level` 仍计入灵感合计并展示「灵感±N」；⑦72–77 不可友人出行 /
  自选比赛，72/74/76 = 5 稽古 + 休息 / 普通出行 / 治病，73/75/77 只有比赛。四档绿（**119 内联 + 15 L2 + 30 机制
  + 1 diag + 5 CLI 内联 + 1 CLI 冒烟 = 171 测例**）；整局冒烟 54683 / UB9、PT 9024 → 53083 / UB7、PT 8858 →
  **55664 / UA、PT 9136**（URA 全变稽古后位移；S9 前不建基线）。

- **lab 实验仓调优成果合并（2026-11-06，user 拍板「执行合并、接受基线修改、冠军档做默认层」）**：
  ①**训练员策略层**：守门链扩至 ②–⑤（合宿禁令 / 友人外出截止线 / 体力危险线 / 心情崩坏）+ 满体力闲置惩罚
  （恒开，代码层最大单项；主仓默认局面实测 **+9769.8 均分 / t=21.1**）；②**调参层**：`[trainer.dynamic]`
  动态权重（五维追赶 / 体力时间价值 / 稽古节奏）、教诲落位先验 `ROUTE_PRIOR` + `[trainer.advice].route`、
  `status_limit` / `assist`（实测噪声或有害，锁 0）、`search_score` 口径修复 + MC 接线（搜索默认关，关搜索逐位零差）；
  ③**冠军档成为默认层**：`data/matsuri_config.toml` 与 `config.rs::Default` 同步 = lab `ga-r2`
  （`pt_favor_rate 1.7271` / `rest_vital 64` / fusion year3 `501→801,301→901,401→1001` /
  dynamic `status_catchup 1.3553`、`vital_decay 0.2879`、`keiko_tempo 0.486` / `advice.route 500`）；
  ④**幸运体质（生病盾）本地接线**：在既有 `lucky_count` 上补消费（事件 `add_flags` 拦截 + 大失败判定），
  **含变胖**、`bad_trainer` 与 `Ill::BAD_TRAINER` 同置同清；不改引擎 / 协议，离线无授予点 → 正常跑局零差；
  ⑤**CI 修复**：引擎检出补并列软链 + 去 `continue-on-error`、工具链钉 1.99.0、`cargo test --workspace`。
  四档绿（**130 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 1 CLI 冒烟 = 185 测例**）；
  整局冒烟（默认层 = 冠军档）**77538 / LG2、PT 11680**、`random` **26858 / UF6、PT 5101**；
  100 局矩阵（seeds 61444–61543，仓默认局面）**均 73155.0 / min 64851 / max 78080 / ≥LG 24**；
  lab 口径（玉藻 102102 + 卡组 A，seeds 62100–62199）**均 74051.7 / min 61076 / max 79096**（与 lab 冠军留出逐位一致）。
  引擎仓未改（仍 `a8859e0`）；来源 = `Matsuri_Workbench`（lab 实验仓记录 R1–R8）。
- **自选比赛义务区间接线（2026-11-06）**：按引擎通用规则接 `freeRaces`——部分马娘在指定区间内需打满 N 场
  **不低于门槛等级**的比赛（`grade=1` 只放 G1，更低的 G4/OP 被挡）；区间结束 +1 回合未达标 = 育成失败
  （`next()` 终止 + `on_simulation_end` 跳过终局奖励）。训练员新增 `[trainer].free_race_urgency`（默认 1.0）
  紧迫度软加分 + `need ≥ left` 硬保底守门⓪（不受 `self_race` 限制；`self_race` 收窄为「额外多打」）。
  四档绿（**139 内联 + 15 L2 + 31 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟 = 195 测例**）；
  100101 100 局**逐位不变**（均 73155.0）；102102 玉藻 lab 口径 74051.7 → **70876.0（−3175.7）**、无育成失败（预期内）。
- **手写档「决策解释」模式（2026-11-06，user 拍板「复用 manual 完整面板」）**：`matsuri matsuri --explain`
  （或 `MATSURI_EXPLAIN=1`）——`matsuri-cli/src/panel.rs` 抽出 manual 的**完整回合面板**（`BoardPanel`），手写档每回合
  打印面板 + 「【候选打分】」表（与手写打分同源 `explain_action`）+「【决策理由】」；教诲分配 top-10 方案分、
  事件选项期望值。只打印、不改决策（**与不开 explain 逐位一致**；当前 seed 61444 终局 78397 / LG3、PT 11793），默认关闭；
  `manual` 档同步改用共用面板；四档绿 **195 测例**（+核心解释 API、+explain 冒烟）。
- **合宿延长段事件轴 = 正常回合（U19 结项，2026-11-03，user 确认）**：第 2/3 年合宿**延长段**支援卡连续 /
  普通马娘 / 掉心情等随机事件均可触发（**事件轴等同正常回合，仅训练全变演目稽古**）；第 2/3 年**常规 4 回合**
  仍无随机事件。实现 `random_events_suppressed` 合宿层收窄为常规窗口、延长段回落引擎 `no_event_turns` 基线。
  四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；seed 61444 无合宿延长
  ⇒ 冒烟 **77538 / LG2、PT 11680** 逐位不变；109301 凯斯奇迹 T39–41 目标赛触发 2 年延长，T40 / T41
  正常抽到马娘 / 支援卡连续事件、T42 全稽古训练。
- **比赛加成改为乘算（2026-11-03，user 确认）**：支援卡「赛后」与発表会 / 本番 204（30/100）由同表加算改为
  **分别作百分比倍率相乘**：`scale = (100+卡面赛后)/100 × (100+剧本赛后)/100`，引擎在面板乘完后统一 floor 一次
  （floor 位置存疑，先按一次性；222–223 更接近游戏实际）。结算 `do_race` 与训练员 `value.rs::estimated_race_reward` 共用
  `MatsuriGame::race_scale`。测试配卡 55% × 本番大成功 2.0 = 3.10：T71 生涯赛 5/72 → **15/223**（原加算 2.55 → 12/183）。
  四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；
  冒烟 **77538 / LG2、PT 11680 → 78397 / LG3、PT 11793**；`random` **26858 / UF6、PT 5101 → 27150 / UF6、PT 5153**；
  100 局均值待重跑（本批未重录；S9 前不建基线）。
- **狄杜斯「铁心的挑战者」（2026-11-03，user 实测反推）**：生野狄杜斯 106301 / 106302 的特殊状态；出道战后（T12–71）
  对**所有生涯比赛** 五维 **×2.5**（+150%）、PT **×0.46**（属性多、PT 相对少）；与卡面赛后**乘算**、在剧本赛后**乘算之前**。
  自选赛 / T11 / URA 不生效。状态系统未建模，按马娘 ID 硬编码 `MatsuriGame::special_race_stats_bonus` /
  `race_pt_rate`；`MatsuriView.uma.special_race_stats_bonus` + `special_race_pt_rate` 同步、
  manual 面板显示「铁心 五维×2.5 / PT×0.46」。测试配卡：T22/T58 生涯赛 **19/128**；T68/T69/T71（本番大成功后）**38/256**。
  四档绿 **199 测例**（142 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；
  100101 等其余马娘冒烟不变 **78397 / LG3、PT 11793**。
- **溢出与末段稽古旋钮（2026-11-03）**：①`[trainer.overflow]` 按**生效上限**判定近顶，非近顶轴加权，
  主属性近顶时保留副属性 / PT，避免「速溢出 → 力量 / PT 完全放弃」；②`[trainer.endgame]` 窗口 =
  **第三年合宿结束后～T71 的普通回合**，满槽数超过剩余普通回合时优先满槽稽古、未满 Lv 优先补级，
  普通训练加机会成本；**URA 72–77 全稽古不参与**。默认档 `0.02/0.5/0.15` + `250/1.0/80/120`；
  100 seed discovery 上 106301 +245，200 seed holdout 未复现（三马 −33/+7/−7），定位为行为控制旋钮。四档绿 **204 测例**
  （147 内联 + 15 L2 + 32 机制 + 1 diag + 7 CLI 内联 + 2 CLI 冒烟）；seed 61444 冒烟不变
  **78397 / LG3、PT 11793**、`random` **27150 / UF6、PT 5153**。


## 结构

| crate / 文件 | 职责 |
|---|---|
| `crates/matsuri-core/src/lib.rs` | 模块声明 + 对外重导出 + `MAX_TURN` |
| `crates/matsuri-core/src/game/mod.rs` | 局面 `MatsuriGame` + `ActLedger` / `AdviceLedger` 账本 + 演目 gauge / Lv / 発表会 buff 派生 + 训练预览 `TrainingPreview` / 动作资源预览 `ResourcePreview` + `build_game`（含友人卡门禁） |
| `crates/matsuri-core/src/game/view.rs` | 回合信息快照 `MatsuriView`（分节：turn / resources / uma / board / schedule / friend / camp + 编成警告 / 事件记录）+ `view()` |
| `crates/matsuri-core/src/game/diag.rs` | 诊断辅助：关键状态快照 / 变化摘要 / 回合类型摘要（供 `diag!` 埋点，只读） |
| `crates/matsuri-core/src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）+ 回合结构 `TurnPlan` + 合宿进度 `CampState` |
| `crates/matsuri-core/src/game/person.rs` | 剧本人头 `MatsuriPerson` + 来源 `PersonOrigin`（与引擎 `PersonType` 分工见文档） |
| `crates/matsuri-core/src/game/roster.rs` | 建局人头名单 + 担当分配（link 钉位 / 外派链 / NPC 余额） |
| `crates/matsuri-core/src/game/deck.rs` | 编成检测：禁止项 / 警告项（`DeckError` / `DeckWarning` / `DeckReport`） |
| `crates/matsuri-core/src/game/distribute.rs` | 人头分布（亮起锁担当 / 不在率 / 吾妻补位 / 分身）+ 得意率 / 灵感分配；**理事长·记者不落稽古位（全稽古回合不出现）；吾妻有稽古必补未满稽古位；每训练位至多 1 名友人 / 团队 / 理事长 / 记者** |
| `crates/matsuri-core/src/game/event.rs` | 事件管线：回合开始事件链（固定 / 随机分布 / 合宿抑制 / 友人解锁）/ 友人链事件扩展 / 追加稽古事件 / 疲劳回复与变胖 / turn 66 hint 记账 |
| `crates/matsuri-core/src/game/train.rs` | 训练结算：下层引擎公式 + 上层剧本乘区 / 封顶 + 失败率 + 训练后处理（羁绊 / 灵感 / 加练 / 追加稽古事件 / 友人点击 / 变胖速度归零） |
| `crates/matsuri-core/src/game/advice.rs` | 教诲：槽效果聚合（按绑定演目 Lv × 参与人数）/ 融合 / 分配接口 |
| `crates/matsuri-core/src/game/settle.rs` | 回合末结算：発表会 / 本番 / 合宿 +1 / 教诲窗口 / 育成终了 |
| `crates/matsuri-core/src/game/action.rs` | `Action` + `Display` + `ActionEnum` + 比赛 / 休息 / 出行 / 治病结算入口（含演目槽入账） |
| `crates/matsuri-core/src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource` / `FlatSearchGame`，含分布 / 得意率 / 训练值 / 失败率 / 终局覆写） |
| `crates/matsuri-core/src/data.rs` | 剧本资产 schema / 载入 / 校验（`scenario_matsuri.json` 18 节：+ `scenario_events` / `event_extras`；稽古两档 / S5·S6 规则常量） |
| `crates/matsuri-core/src/config.rs` | 剧本配置段（融合 / 転成预设 + 覆盖层） |
| `crates/matsuri-core/src/trainer.rs` | 样例训练员 `MatsuriTrainer` |
| `crates/matsuri-cli/src/main.rs` | 终端前端：bin `matsuri`（跑局；`matsuri` / `random` / `manual` 训练员选择；`--explain` / `MATSURI_EXPLAIN` 开手写档决策解释） |
| `crates/matsuri-cli/src/panel.rs` | 手写档 / 手动档**共用回合面板** `BoardPanel`（2026-11-06 抽出）：状态全部从 `MatsuriView` 取（纯呈现）；育成配置 + 彩色训练明细表（人物着色：彩圈 / 助力黄 / Hint 叹号 / 羁绊；掷中时含研鑽担当的列标题加紫「追加稽古」行）+ 剧本资源与教诲分配结果 + 合宿·友人链·赛程 + 五维与上限显示压缩（>1200 减半）+ 动作槽增量 / 助力支撑标记 + 演目槽变化行 + 事件名亮蓝；`render(..., scores)` 可追加候选打分表 |
| `crates/matsuri-cli/src/explain.rs` | 手写档决策解释包装（`--explain`）：复用 `panel.rs` 面板，面板后追加候选打分表（`explain_action` 同源）与决策理由；教诲分配 top-10、事件选项期望值；**只打印不改决策** |
| `crates/matsuri-cli/src/manual.rs` | 手动核对面板（S8；面板渲染已抽出到 `panel.rs`）：逐决策人机交互（Inquire / 非 TTY 编号输入），教诲分配只支持「保持现状」；事件选项组回显 |
| `crates/matsuri-cli/src/logging.rs` | stderr 日志器：消费 `[output] log_level`，承接引擎 / 剧本 `diag!` 记录（默认 `info`、`off` 静默） |
| `crates/matsuri-sim` | 模拟器与基线裁判（骨架） |
| `crates/matsuri-ai` | 在线决策核心（骨架） |
| `crates/matsuri-tools` | 外围工具（骨架） |
| `data/` | 引擎静态表 + 剧本资产 `scenario_matsuri.json` + 双配置默认层 |
| `docs/` | 本仓文档骨架 |

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/README.md`](docs/README.md) | 本仓文档索引与引擎文档入口 |
| [`docs/plan/scenario_dev_plan.md`](docs/plan/scenario_dev_plan.md) | 开发计划：阶段状态 / 决策记录 / 待决事项 |
| [`docs/plan/intake.md`](docs/plan/intake.md) | 接入包：情报位置 / 机制 / 数值 / 数据表 / 待确认 |
| [`docs/plan/stubs.md`](docs/plan/stubs.md) | 数值占位登记（`STUB` / `TBD`） |
| [`docs/mechanism.md`](docs/mechanism.md) | 机制文档（公式 / 流程 / 实现落点） |
| [`docs/glossary.md`](docs/glossary.md) | 术语单一事实源 + 口径决定记录 |
| [`docs/tests_overview.md`](docs/tests_overview.md) | 测试一览 |
| [`docs/handoff.md`](docs/handoff.md) | 交接提示词 |

## 运行与验收

```powershell
# 数据根是代码级常量 data/（crates/matsuri-cli/src/main.rs 的 DATA_ROOT）
cargo run --release --bin matsuri --            # 样例训练员
cargo run --release --bin matsuri -- random     # 引擎随机基线（对照用）
cargo run --release --bin matsuri -- manual     # 手动核对面板（动作 Inquire；Esc 退出，非 TTY 退回编号输入）
cargo run --release --bin matsuri -- matsuri --explain   # 手写档决策解释：回合面板 + 候选打分 + 决策理由
# 诊断日志级别见 data/ai_config.toml [output] log_level（默认 info；工作目录覆盖层可设 off 静默）

cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
```

- `cargo check` / `clippy` / `test` 通过（**不改引擎任何一行**）。
- 接入新剧本时引擎仓 `git diff` 为空——确需新能力时回引擎仓按「带默认实现、非破坏」加钩子。
- 三条随机轴（局面 / 事件 / 策略）+ 决策流只喂 `Trainer`；阶段自持、对外只投影 `StageId`；
  同一 `master` 下两次运行 / 两支分支逐位一致。
- 回合信息给前端 / 在线外壳走 `MatsuriView`（**完整回合快照**，分节；名称文本由呈现层按静态表解析），
  不进契约 trait。
- 诊断日志统一走 `diag!`（target = `diagnostic`）：级别由 `[output] log_level` 控制（默认 `info`；
  `off` 静默），批量 / 基线入口用 `DiagGuard::suppress()`；工作目录放 `ai_config.toml` 覆盖层即可调级别。

## 依赖三档

```toml
# 1. 开发期：并列目录（当前写法）
umaai-core = { path = "../umaai_next/crates/umaai-core" }
umasim     = { path = "../umaai_next/crates/umasim" }
umaai      = { path = "../umaai_next/crates/umaai" }
umaai-cli  = { path = "../umaai_next/crates/umaai-cli" }   # 仅终端入口需要

# 2. 稳定期：同一 repo、同一 tag
# umaai = { git = "<engine repo>", tag = "engine-v0.1.0" }

# 3. 下游（Web / Android）：自喂帧、无文件监听，并剪掉并行后端
# umaai = { git = "<engine repo>", tag = "engine-v0.1.0", default-features = false }
```
