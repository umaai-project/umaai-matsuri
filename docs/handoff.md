# 交接提示词

> 用法：上下文过长时，新窗口只读本文件 + 下列关键文件即可续接；只写「代码 / git 状态里恢复不了」的信息。
> 当前任务：**umaai-matsuri 的 S2 数据层实施**。方案已与用户拍板，新窗口**可直接开工**，不用再等确认；
> 完成后向用户汇报 diff，**不自动 commit / push**。

## 一、已完成（本会话）

1. **S1 验收复核**：`cargo fmt --all --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` /
   `cargo test --workspace --release --offline` 三档实测绿（10 单测）。S1 无需返工；唯一尾巴：
   CI 缺 `cargo check` 档（S2 顺手补）。
2. **日文 memo 更新复核**（`../mee-umasim/data/shuntaisai_memo.md`，HEAD `49f5e0b7` 2026-10-31）：
   新增**普通训练 Lv1–5 基础值全表**（Lv1 与 `master.mdb` 逐值一致，Lv2–5 由此补齐，
   10-31 修正了力量 Lv4/Lv5、根性 Lv5）；演目稽古 Junior/Classic 两档与现有 docs 一致。
3. **B 类文档修正已回写（用户拍板「按日文 memo 修正」）**：
   - **B1 牵绊教诲 `+3`**：改为**平摊、不按参与担当人数叠加**；与训练绊合算，
     参加友人 +7 / 参加非友人 +10 / 不参加 +3；**愛嬌在合计值上再 +2**（+9 / +12 / +5）。
     参加绊基础：友人 +4 / 普通卡 +7 / 不参加 0；愛嬌 +2 只加一次。
   - **B2 合宿版休息**：只在**原本夏合宿期间（Classic/Senior 7–8 月）**；**延长回合休息 = 普通休息**；
     第 2/3 年外出仍不可（含延长段）。
   - **B3 比赛奖励**：只建模**基础值**；有馬記念等特殊比赛自带额外数值**不建模**；
     memo 的「有馬 全ステ 21 / SP 200」观察转为 **U46 验证点**（第三年底目标赛的剧本加成 / レスボ）。
   - **B4 排练失败**：**完全无效**（触发失败后事件、无属性收益、**槽不动、演目 Lv 不升**）；
     推翻旧「失败仍 +1 / 槽清零」。
   - 已改文件（未提交）：`docs/glossary.md`、`docs/mechanism.md`、`docs/matsuri_memo_cn.md`、
     `docs/plan/intake.md`、`docs/plan/stubs.md`、`docs/plan/scenario_dev_plan.md`（§5.3 汇总了 B1–B4）。

## 二、参考源纪律（重要）

- **`../matsuri-ref-01` 是实验性库，实现与 memo 多有错误，参考度低**：不得作为机制 / 数值 / 实现依据；
  不读它的 `source/` `raw15/` `out/` 代码与 JSON。
- 数据只用**外部 `master.mdb`**（工作区 `../UmaAI/master.mdb`，md5 `ef481f95…`，不入仓）的原始表，
  由本仓自己的查询提取；机制口径以本仓 `docs/` + 日文 memo 为准。

## 三、下一步 = S2 数据层（方案已确认）

### 3.1 资产 `data/scenario_matsuri.json`（语义化 schema，`deny_unknown_fields` + 载入期校验）

| 节 | 内容 |
|---|---|
| `limits` | 五维基础上限 2200/1850/1800/1700/1800、绝对上限 2500×5、`support_num=5`、`max_skill_point` |
| `acts` | 5 演目：`act_type→command_id(5101–5105)→train(0–4)→npc_chara_id`（`act_info`+`act_member`） |
| `training.normal` | **5 训练 × Lv1–5 × [速耐力根智 SP 体力]**：Lv1 取 `master.mdb`，Lv2–5 取 memo 2026-10-31（力量 Lv4=`[0,7,12,…]`、Lv5=`[0,8,13,…]`，根性 Lv5 主=16） |
| `training.keiko` | 演目稽古 Junior / Classic 两档 × 5 行（Junior=mdb 5101–5105；Classic=memo §3.2） |
| `act_effects` | Lv1–10 类型化：全基础+10、友情%、SP练习%、Hint 事件/全Hint、SP 上限、基础上限、`201` 隐藏值 30(TBD)（66 行） |
| `advices` | 10 教诲 × Lv1–10 类型化效果（牵绊 flat+3/主基础%、得意率、灵感率、助力、体力、特化、研鑽、协力、不屈、両道）（153 行） |
| `fusion` | 5 个基础教诲 → 融合升级效果（9 行） |
| `check_points` | 発表会永久 buff：schedule 0–3 × 成功/大成功 → 友情/得意/灵感/比赛加成（20 行聚合成 7 条） |
| `check_point_rewards` | **新增**：発表会/本番一次性奖励（成功 + 大成功两档）+ 育成终了（全大成功 / 全成功）；成功档值见 memo 10-31（第2回成功 全能力+5/SP150/粉丝2000；本番成功 +10/SP200/粉丝5000+序盤巧者Lv1+彼方Lv1；育成终了全成功 +15/SP150/彼方Lv3） |
| `finals_act` | 体力+30 / 干劲+1 / 失败率−100% / 追加 1 人（4 行） |
| `outing` | 2 张友人卡 × 5 段 → 演目 Lv+1（10 行） |
| `schedule` | turn 23/47/67，`need_act_num=5`，Lv 3/5/6（**1基→0基**） |
| `advice_windows` | turn 2/11/23/35/47/59（**−1 基**） |
| `camp` | 3 窗口 (12–15)/(36–39)/(60–63) + Lv+1 固定结算点 15/39/63 |
| `rules` | gauge 上限 5；入账 +1/+2/+4/+1；训练Lv 每 4 次 +1；普通训练 5 级；演目 Lv 上限 6（R/无）/10（SSR 吾妻）；**絆基础值**（参加友人 4 / 普通卡 7 / 不参加 0 / 教え +3 / 愛嬌 +2） |
| `race_rewards` | 基础值表：目标赛 五维+5/SP72；非目标 GI +10/SP45；GIII +8/SP35；URA 预选/准决/决 +10、SP 40/60/80（**只基础值**） |

- 数据源行数核对：`act_info` 5 / `act_member` 5 / `advice` 10 / `advice_effect` 153 / `advice_recommend` 6 /
  `check_point_effect` 20 / `finals_act_effect` 4 / `fusion_effect` 9 / `outing_effect` 10 / `schedule` 3 /
  `act_effect` 66 / `single_mode_training_effect`(scenario 15) 56。
- 生成方式：**一次性提取脚本，不入仓**；provenance 记 `docs/plan/intake.md` 新增 §4.1「资产 ↔ mdb 表映射」。

### 3.2 配置 `data/matsuri_config.toml`（默认层 + 覆盖层，未知键严格报错）

```toml
[[fusion.year2]]  # turn 23：2! 配对
base = 101
target = 601
# … 101+601 / 201+701
[[fusion.year3]]  # turn 47：剩余 3 个
base = 401
target = 801
# … 401+801 / 301+901 / 501+1001
```

校验：year2 恰 2 组、year3 恰 3 组；base 只用 101–501 且不重复；year2 target ∈ {601,701}、
year3 ∈ {801,901,1001}，各用一次。训练员 / 搜索参数留 S7 / M2，不预置。

### 3.3 代码改动

| 文件 | 动作 |
|---|---|
| `crates/matsuri-core/src/data.rs` | 重写 schema + 载入/校验 + 内联单测（约 350–450 行） |
| `crates/matsuri-core/src/config.rs` | 重写为融合预设 + 覆盖层 + 单测 |
| `crates/matsuri-core/src/game/mod.rs` | `MatsuriView` 清空为占位（S3 填 gauge/Lv/教诲槽）；`do_train` 接真实训练表 |
| `crates/matsuri-cli/src/manual.rs` | 去掉 `sample_resource` 展示行 |
| `data/scenario_matsuri.json` / `data/matsuri_config.toml` | 全量替换 |
| `crates/matsuri-core/tests/data.rs`（新建） | L2：从仓根 `data/` 载入资产 + 配置，断言关键数值 |
| `.github/workflows/ci.yml` | 补 `cargo check` 档（S1 尾巴） |

### 3.4 同时要回写的 A 类 memo 增量（memo 10-31，S2 一并落 docs）

1. **训练公式主/副拆分**：主属性 `×(1+友情)×(1+训练效果)×(1+主获得量)`；副属性
   `×(1+友情)×(1+训练效果+副获得量)`（副获得量与训练效果同层加算）；下段=通常通り；稽古主/副基础各 +10。
   → `mechanism.md` §3.1、`matsuri_memo_cn.md` §3.3。
2. **普通训练 Lv1–5 表** → `intake.md` §3.2（现在只有 Lv1「代表值」）。
3. **一次性奖励成功档**（第2回成功 / 本番成功 / 育成终了全成功）→ `intake.md` §3.6（现在写「待样本」）。
4. **研鑽「追加の稽古」判定**：**1 回合 1 次共通掷骰，p = r(绑定演目Lv)，不按人数加算**；掷中则该演目
   担当参与的训练位都出图标。推翻 U39 现口径 `p = min(100%, r×N)` → `stubs.md` / `memo_cn` §4.4/§14.5。
5. **R 连续事件第 1 回**：耐力 +4、根性 +4、绊 +5 → `memo_cn` §11.2（现 TODO）。
6. **攻略增量**（策略参考，不改机制）：Junior 合宿可外出、终盘外出完走→不屈连打、根性 Lv5 止め 等
   → `memo_cn` §16 / 训练员笔记。

### 3.5 测试与验收

- 单测：解析成功 / 缺字段 / 未知键 / 训练表 5×5 / acts 1–5 连续 / act_effects 10 级 / advice 10×10 与白名单 /
  fusion 5 组 / check_points 7 条单调 / schedule 升序 / camp 一致 / limits / rules；config：默认层 / 覆盖层 /
  未知键 / 融合配对非法拒绝。
- L2：真实资产 + 配置载入，抽查关键值（schedule、windows、稽古两档、融合预设、gauge 上限等）。
- 四档全绿（`fmt` / `check` / `clippy -D warnings` / `test --release --offline`）+ 引擎仓代码零改。
- 文档同步：`README.md` 状态、`docs/plan/{intake,stubs,scenario_dev_plan}.md`、
  `docs/{mechanism,glossary,tests_overview}.md`；S2 勾 ☑。

## 四、关键命令 / 约束

```powershell
# cwd = 仓根；同一 target 一次只跑一个 cargo；一律 --offline（~/.cargo 只读，cargo add/fetch 需先请用户提权）
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --release --offline
```

- 引擎仓为并列目录 `../umaai_next`（path 依赖）；**不改引擎**（现状代码等价、`git diff --ignore-all-space` 为空，
  只剩行尾噪音与 `docs/notes/` 文档差异）。
- 测试 cwd 在各 crate 目录，数据根用 `matsuri_core::testing::data_dir()` 取仓根。
- 提交纪律：**不自动 commit / push**；完成后先汇报 diff，由用户确认后统一更新文档再提交。
- 工作树既有改动：`AGENTS.md`（用户新增规则 5）、`data/events.json`（仅 CRLF→LF 行尾、内容等价，非本会话）——
  提交准备时再决定去留。

## 五、未决 / 风险

- U46（有馬→第三年底目标赛剧本加成验证）TBD；其余 U02 / U19 / U20 / U27 / U29 / U40 / U41 / U44 / U45 等按临时口径。
- U39 在 S2 按 A 类第 4 条更新（memo 已给新判定，仍可待真机复核）。
- 普通训练 Lv2–5 来自日文 memo（「中」可信度），待真机确认；替换后重录基线并记 glossary。
- 成功档一次性奖励同样来自 memo（中可信度）。
- 训练值上层乘区归属仍可按真机复核；演目稽古 Senior 第三档暂定没有（U02）。
