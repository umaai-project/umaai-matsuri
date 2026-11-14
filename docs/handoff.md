# 2026-11-14 交接：复盘数据分析工具（数据面已通，判据待落）

> 一次性文档：本次交接整份重写。历史进展见 `plan/review_tool_plan_2026-11-13.md`（§13 文件级拆解）
> 与 `changelog.md` 2026-11-13/14 行。

## 1. 当前状态

- **仓**：`umaai-matsuri`（cwd）；引擎 `../umaai_next` 为 path 依赖（本轮**未改引擎**）；
  插件 `../URA_Plugins/SendGameStatusPlugin`（**独立 git 仓**）。
- **提交**：本轮改动待提交（提交信息见下），**未 push**。
- **复盘工具**：`matsuri-tools` 的 `review open` 已支持**局目录 / 局包 zip**、`--game` 选局、
  旧协议守卫（`illEffects` 帧直接报错）；输出 `facts.json` / `brief.md`，不跑搜索、不做估值。
- **插件数据面**：终局帧内嵌 `review` 段；`game{id}.zip` 与帧**同目录扁平**打包；
  收 AI 标记 `ai-decision`；打包后删回合帧与 ai-decision，保留 `thisTurn.json` 与终局帧。
- **AI 标记**：`matsuri-cli online` 在 T77 帧写 `game{id}_ai-decision.json`（只含最终状态）；
  `--replay` 不写。文件存在 = 开过 AI 且陪到局终。
- **真机样本**：`game6324`（132 帧 / T0–T77 / 终局 77932（LG3）/ 160 actions / 0 解析失败）。
  旧包仍留在 `PluginData/SendGameStatusPlugin/records/game6324.zip`（移动被 EPERM，待手动挪平）。

## 2. 本轮已完成（摘要）

1. `review open` 第二切片：zip 输入、`--game`、旧协议守卫；`discover` 改帧文本、`facts` 不再读路径。
2. 插件：`GameStatusOutput` 的 `ReviewMeta/BuildReview/PackGame/CleanupFrames`、
   `GameStatusSend_GameEnd` 的 `payload.review` + 打包触发 + `LoadLog`、`Class1` 的 load 传 `isLoad`。
3. CLI：新增 `ai_trace.rs`（T77 帧写 AI 标记），在线循环接入；`--replay` 排除。
4. 判据整理：B/C 组口径、选择偏差总纲、剧本专属判据（1）–（6）、统计点清单、基线程序设计，
   全部写进 `docs/plan/review_tool_plan_2026-11-13.md` §7。

## 3. 下一步（按优先级）

1. **P0 纯帧统计**：把 §7.2 的逐局字段落进 `review`：B1/B4/B5/B6/B7、C3/C4、E7、
   剧本（1）曲线/节点/稽古次数、（2）出行状态、（4）继承实际值、（5）追加稽古明细、（6）状态资源。
2. **P1 基线程序**：抽 `matsuri-sim` pub `play()`，新增 bin `baseline`，产出
   `benchmark/baselines/acts_friend_<deck>_<uma>.json`（分位表）。
3. **P2 `eval` 单点**：turn+seq、候选表、regret+CI、`recompute/` 缓存；随后 B2/B3/C1/C2 进入收集期。
4. **P3 跨局聚合**：选择偏差分布、追加稽古倾向、同卡组近 N 局自我参照。
5. **插件真机验**：重打插件 zip（ACT）→ 跑一局完整局，确认扁平包、`review.ai`、清理。
6. **技能**：`.trae/skills/matsuri_review/` 六个文件（等 `eval` 可用后再写）。

## 4. 关键落点

| 内容 | 文件 |
|---|---|
| 复盘 bin / 模块 | `crates/matsuri-tools/src/bin/review.rs`、`src/review/{mod,discover,facts,brief}.rs` |
| AI 标记 | `crates/matsuri-cli/src/ai_trace.rs`、`src/online.rs` |
| 计划与判据 | `docs/plan/review_tool_plan_2026-11-13.md`（§7 判据、§13 拆解） |
| 口径决定 | `docs/changelog.md` 2026-11-13/14 |
| 插件 | `../URA_Plugins/SendGameStatusPlugin/{GameStatusOutput,GameStatusSend_GameEnd,Class1}.cs` |
| 局包样例 | `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/records/game6324.zip` |

## 5. 关键命令

```powershell
# 复盘（目录或 zip）
cargo run -q -p matsuri-tools --bin review --offline -- open <局目录|game{id}.zip> --game 6324 --out <目录>

# 剧本仓验收
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo test --workspace --release --offline

# 插件构建（需一次性 danger-full-access：写 obj/bin）
dotnet build .\SendGameStatusPlugin.csproj -c Release --no-restore
```

- `CARGO_TARGET_DIR=F:/UmaAI_Active/.cargo-adv`（target/ 不可写）；
- shell 写不进剧本仓树：文件改动用 file 工具；`cargo fmt`、`git`、插件构建需一次性提权；
- `PluginData` 下移动/删除会被 EPERM 拒绝，需用户手动或批准提权。

## 6. 已知风险 / 边界

1. 真机不可复现随机流：O/E/A 与反事实只做模拟局；不给“上限”。
2. 只支持新协议帧（`charaEffects`）；旧 `illEffects` 帧与无 `review` 段的老包直接报错。
3. AI 估分（`expected`）仅展示，不做“估分 vs 实际”的对照结论；不做在线/重算轨迹一致性验收。
4. 插件状态全内存：`TurnSequence` 重启归 1、`FinalScoreWritten` 一局一份、`loads` 随进程清空。
5. AI 标记只在 **T77 帧到达**时写；AI 在 T76 后退出则无标记（视为“无标记”，不写“没开”）。
6. 旧 `records/game6324.zip` 尚未挪到扁平目录；新包已扁平。

## 7. 待拍板

| # | 问题 | 建议 |
|---|---|---|
| D1 | 局包保留策略（默认不删） | 先不删，观察磁盘 |
| D2 | 是否补 `festa.events[]` | 建议补 |
| D4 | persona 口吻（吾妻/巫女/平实） | 平实起步 |
| D5 | `records/` 是否进 `.gitignore` | 是（AI sidecar 用） |
| D6 | P2 是否本轮做 | 建议只做模拟局，排在技能之后 |
