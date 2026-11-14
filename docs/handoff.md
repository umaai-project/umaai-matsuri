# 2026-11-14 交接：复盘判据口径定稿，下一步开工 infer / tuning / P0

> 一次性文档，本次整份重写。历史进展见 `plan/review_tool_plan_2026-11-13.md` 与 `changelog.md` 2026-11-13/14 行。

## 1. 当前状态

- 仓 `umaai-matsuri`（cwd）；引擎 `../umaai_next` 为 path 依赖（本轮**未改引擎**）；插件 `../URA_Plugins/SendGameStatusPlugin`（独立仓）。
- 已提交（**未 push**）：剧本仓 `f8d44ba`（review zip/--game/旧守卫 + AI 标记 + 判据文档）、插件仓 `6271d31`（扁平包 + review 段 + AI 标记 + 版本裁剪）。
- **工作树未提交**：`crates/matsuri-tools/src/bin/rhythm.rs`、`crates/matsuri-tools/Cargo.toml`（注册 bin）、`benchmark/_local/acts_friend/summary.md`（入库；18 个 CSV 被 gitignore）、`docs/handoff.md`、`docs/plan/review_tool_plan_2026-11-13.md`（判据口径更新）。
- `rhythm` 基线已跑完：2 档 × N=2048 × 50 局 + 手写同种子 50 局 + N=8192 × 10 局（uma 105902、qie=none、shield=0、seed0=73200）。

## 2. 已定口径（不要再翻旧结论）

- 引擎零改动；复盘按需重算；不做 AI 估分对照、不做在线/重算轨迹一致性；旧 `illEffects` 帧与无 `review` 段老包不兼容。
- 局包扁平：`game{id}.zip` 与帧同目录（帧 + finalScore + ai-decision）；旧 `records/` 只读兼容。
- 判据级别：B1 gate；B4 info（合宿延长回合）；B6/B7/C3/C4 gate；B5 **暂缓（用户：不查）**；B8–B10 不检测。
- 选择偏差（B2/B3/C1/C2）：实际 vs 一选估分差 + 配对 CI；跨零=同等优解，显著才分析。
- 判据 (2) 出行：只两条 warn——T23 前 <4 段、T36 前未走满 5 段；**解锁时点不作判据**（羁绊随机）。
- 判据 (3)：只做 swings 分解 + 阴跌段陈述；**不做属性 gap / 超未达预期**（无每维增量基线）。
- 判据 (4)：只出实际值 + 与可配置标准（默认 400/次）对照；**不做分位/verdict**（无继承分布基线）。
- 判据 (5)：以事件 `400015106` 为准；位置 = `kensan_act`（未転成 701 时为 None，不会触发）。
- 判据 (6)：切者=随机好运、小切=局外口径不算运气；盾/体质用“计数−事件对齐”，不判失效。

## 3. 下一步（按顺序开工）

1. **`review infer` 参数化反推入口**（skill 按需调用）：
   `review infer <局目录|zip> --what advice|fusion|skill|kensan|all [--turn N] [--json] [--out <dir>]`
   - 只读帧+资产，不重算；教诲/転成/追加稽古=high，学技能=medium；
   - 输出 `infer.json` + 摘要；与 `open` 的 `pending` 互补（infer 给纯帧能定的，pending 给要算力的）。
2. **`review/tuning.rs`**：继承标准（默认 400/次）、分位阈值若以后恢复也放这里；CLI `--inherit-expect` 覆盖；不进 `matsuri_config`（避免内置默认锁）。
3. **P0 逐局字段**进 `facts.json`：数据健康 4 条 + B1/B4/B6/B7 + C3/C4 + E7 + 剧本 (1) 曲线/节点/稽古次数/卡位、(2) 出行状态、(3) swings、(4) 继承实际值、(5) 追加稽古明细、(6) 状态资源。
4. 之后：`baseline` 正式 bin（抽 `matsuri-sim` pub `play()`，只采 Lv/出行/追加稽古）→ `eval` 单点（regret+CI+缓存）→ curve/report → 跨局聚合 → 技能六个文件 → 插件重打 zip 真机验。

## 4. 基线结论（可直接用）

- 出行：搜索档把 5 段推迟（speed +9.0 回合、wisdom +7.0），终局分 +2706/+2707（46–49/50 上升），**Lv 总和不变**——时间置换不是少拿；原因：rollout 教师固定手写档（search.rs `with_rollout_trainer`），早/晚差被抹到 ~100 分（噪声级）。
- 走满率：T36 前 speed 手写 50/50、搜索 45/50；wisdom 手写 50/50、搜索 37/50。T23 前 ≥4 段：手写 44–47/50、搜索 28–34/50。
- 演目 Lv：Lv6 几乎全压在 T39（第二合宿结算后）；根位两档 0 张卡，Lv8/10 结构性未达，只记录。
- 归因闭合：稽古 52% / 合宿 36% / 出行 11–12% / other=0。

## 5. 数据缺口（已确认、不要重复找）

- 帧无比赛名次/胜负 → 只能判“必赛未跑”，且 B5 已暂缓。
- 帧无友情/彩圈/大成功数量；只有 `baseGame.training[cmd].failureRate/lower/upper` 预览 → swings 只能写“超出预览”。
- 每维每回合增量分位、继承分布：**无法获得，不作判据**（用户拍板）。
- 在线 JSON 的候选 five/pt 被 `json_payload` 丢弃 → 建议侧只能靠 `eval` 重算。
- `raceHistory` 是“已比赛回合列表”（含自选赛、含输赢），不是“只记跑赢”。

## 6. 关键落点

| 内容 | 文件 |
|---|---|
| 复盘 bin / 模块 | `crates/matsuri-tools/src/bin/review.rs`、`src/review/{mod,discover,facts,brief}.rs` |
| 节奏采集 bin | `crates/matsuri-tools/src/bin/rhythm.rs` |
| 本轮基线结论 | `benchmark/_local/acts_friend/summary.md`（入库） |
| 判据与计划 | `docs/plan/review_tool_plan_2026-11-13.md` §7 |
| AI 标记 | `crates/matsuri-cli/src/ai_trace.rs`、`src/online.rs` |
| 插件 | `../URA_Plugins/SendGameStatusPlugin/{GameStatusOutput,GameStatusSend_GameEnd,Class1}.cs` |
| 局包样例 | `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/game6324.zip` |

## 7. 命令

    $env:CARGO_TARGET_DIR='F:/UmaAI_Active/.cargo-adv'
    cargo fmt --all --check
    cargo check --workspace --all-targets --offline
    cargo test --workspace --release --offline

    # 复盘（目录或 zip）
    cargo run -q -p matsuri-tools --bin review --offline -- open <局目录|game{id}.zip> --game 6324 --out <目录>

    # 节奏基线
    F:/UmaAI_Active/.cargo-adv/release/rhythm.exe --deck speed_build --search-n 2048 --games 50

- 本机 pwsh 写剧本仓树会被 ACL 拒（os error 5）：跑测用 `--out <工作区临时目录>`，file 工具可写；
- 同一 target 目录一次只跑一个 cargo；
- 插件构建需一次性 danger-full-access（写 obj/bin）。

## 8. 风险 / 边界

1. N=2048 ≠ 线上 8192；方向一致但走满更早。
2. 模拟解锁 = 羁绊判定，与真机口径不同，只记录不判。
3. 6324 是 AI 陪跑局（解锁 T8、各段 10/11/13/19/22、走满 T22），不能作手动节奏证据；6323 未点友人。
4. 根位无卡，Lv8/10 未达是结构性的。
5. 旧 `records/game6324.zip` 已挪到扁平目录（2026-11-14 晚确认）。

## 9. 待拍板

| # | 问题 | 建议 |
|---|---|---|
| D1 | 局包保留策略（默认不删） | 先观察磁盘 |
| D2 | 是否补 `festa.events[]` | 建议补 |
| D4 | persona 口吻 | 平实起步 |
| D5 | `records/` 是否进 `.gitignore` | 是 |
| D6 | P2 反事实是否本轮做 | 排技能之后 |
| D7 | 是否跑“关掉 rollout 早出行偏好”的 A/B | 先 20 局看方向 |
| D8 | rhythm 是否补“出行落点当时 Lv”+ five_status | five_status 已决定不采；落点 Lv 可选 |
| D9 | 搜索档是否加出行截止线 | 等 D7 |
