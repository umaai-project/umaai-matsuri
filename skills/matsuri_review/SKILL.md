---
name: matsuri_review
description: 骏大祭（matsuri）单局复盘。对真机局包 game{id}.zip 运行 review 工具（open / eval / infer / curve / report / whatif），先读 brief.md 掌握事实，再按需重算选择偏差与反事实，输出叙事归因（总体、走势、门禁、选择偏差、教诲与転成、换手后果）。用户说「复盘这局 / 分析 gameXXXX / 这局打得怎么样 / 这手换掉会怎样」时使用。
---

# matsuri_review · 骏大祭单局复盘

> 计算下沉到 Rust（`review` bin），判断力留给本 skill。
> 主路径：`open` 出事实 → **一次 Read `brief.md`** → 按需 `eval` / `whatif` → 写叙事。
> 口径速查见 [reference/metrics_glossary.md](reference/metrics_glossary.md)；
> 已知坑位见 [reference/pitfalls.md](reference/pitfalls.md)；
> 命令与追问路由见 [reference/commands.md](reference/commands.md)。

## 职责边界

**只做三件事**（计算与判定在 bin 里，不在此层重复）：

1. **叙述化结论**——把 `brief.md` 的事实转成带回合号与数值的人话；
2. **按需定向重算**——只对用户追问的回合跑 `eval` / `whatif`，不整局重算；
3. **回答追问**——具体回合 / 决策 / 换手后果。

**不做**：手算帧、给玩家水平评级、跨玩家比较、把 `pending` 当事实、把三个评分量混着说（见口径）。

## 入口

- **运行期**：`<skill>\bin\review.exe <子命令> ... --data <skill>\data`
  （预构建，**不依赖 cargo 与 Rust 工具链**）。`data/` 是 skill 自带的游戏数据副本；
  显式传 `--data` 后不读取本项目源码的任何路径，skill 可以整体复制到任意目录运行。
- **重建二进制**（仅分发者需要，需要 cargo）：`powershell -NoProfile -File scripts/build_review_skill.ps1 -WithData`。

## 工作流

1. **定位局包**（三级优先）：

   | 优先 | 来源 |
   |---|---|
   | ① | 会话中拖入 / 上传的文件 |
   | ② | 用户明确给的路径 |
   | ③ | `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/` 下最新 `game*.zip`（每次现取，不缓存文件名） |

   ③ 的目录只按本机默认位置举例；局包可以从任意位置传入。

2. **出事实**（零重算）：

   ```powershell
   <skill>\bin\review.exe open <局包.zip> --data <skill>\data --out <工作区>/game{id}
   ```

   产物：`facts.json`、`brief.md`。

3. **读 `brief.md`**（约 10KB，一次 Read 读完，**不派子代理**）：

   `§0` 覆盖与数据健康 → `§1` 总体 → `§2` 教诲与転成 → `§3` 追加稽古 →
   `§4` 实际训练 → `§5` 状态波动 → `§6` 合宿延长 → `§8` 剧本专属 → `§7` 检查项 → 末尾 `pending`。

   `brief` 只给事实与检查项，**不下叙事结论**；`pending` 是「能算未算」，不是事实。

4. **按需重算**（只对追问的回合）：

   | 追问 | 命令 |
   |---|---|
   | 哪回合最亏 | `eval --all`（逐决策面）→ 取 `regret > ci95` 且 `ci95` 最小的点 |
   | 这一手值多少 | `eval --turn N [--seq S]`（多决策面必须 `--seq`） |
   | 换一手会差多少分 | `eval --turn N [--seq S] --swap <动作名>` |
   | 换一手之后会怎么走 | `whatif --turn N --swap <动作名> [--horizon 8\|0] [--k 100]` |
   | 教诲 / 転成 / 技能 / 追加稽古 | `infer --what advice,fusion,skill,kensan` |
   | T(n) 走势与形态 | `curve [--from A --to B]` |

5. **写叙事**，必要时 `report` 出网页版：

   ```powershell
   <skill>\bin\review.exe report <局包.zip> --data <skill>\data --out <工作区>/game{id} [--narrative <叙述.md>]
   ```

## 归因框架（六问）

1. **总体**：终局分与档位（口径：**好局 ≥ LG20（88,500）**、**非常优秀 ≥ LF（91,400）**，见 `metrics_glossary` §10）、门禁是否触发（`B1` 発表会 / `B6` 体力线 / `B7` 干劲 / `C3` 训练失败 / `C4` 资源）、演目 Lv 与目标差距。
2. **走势**：`score` 逐回合的峰值与阴跌段（`§1` / `§5`），校准「哪几段在掉」。
3. **选择偏差**：`eval --all` 显著点 → 逐点说明「第几选、损失多少、五维/PT 取舍」；跨零的点一律写「命中（噪声内）」。
4. **教诲 / 転成**：`§2` 的落位与 `infer` 的反推；必要时 `eval --turn N --seq 3` 看该点 `regret`。
5. **资源与节奏**：出行段数、合宿延长、体力低点、干劲掉落与恢复、自选赛区间。
6. **反事实**：只对 1–2 个关键回合跑 `whatif`，回答「换这一手，之后几回合/终局差多少」。

## 输出纪律

- 正向先行，每条结论带**回合号 + 数值**，结尾落一个下一局可执行的动作；
- 不评级、不跨玩家比较、不写「失误」——命中即命中，显著才展开；
- `recomputed` 必须标 `N`，不同 `N` 不混比；
- 三个评分量分开说（见 [reference/metrics_glossary.md](reference/metrics_glossary.md) §1）：
  `brief` 的 `score` 是**局面评分**、`eval.t_n` 是**期望终局分**、`regret` 是**决策差**；
- 教诲回合的帧分「训练前 / 训练后 / 教诲分配」三个时点，评分跳变先确认是不是训练收益（见 pitfalls）；
- **内置标准不暴露**：好局线 / 非常优秀线 / 继承预期都是内部判据——过线时给一句正面表述，
  **未过线则完全不提**；`brief` 只给继承数字（如「T30 270」），**低于/高于预期由你判断后再说**，
  标准数值不进叙事；
- **技能点未花不是判据**：局末预结算、直接生成局包时 `skillPtSpent=0` 是正常状态，
  不要写成「浪费点」或「损失」；
- **五维一律显示值**：工具的 `brief` / `whatif` 已统一把真实值转显示值（超 1200 按
  `1200 + (值 − 1200) / 2`，原本就是显示值的不再减半）；**模型不要再二次减半**，也不写原始值；
- **事件选项暂不比对**：不同选项的期望差波动大，`eval` / `whatif` 都不处理事件面；
  追问「这个事件该选哪个」时说明暂不支持，只陈述 `brief §3` 的事实（触发位置）；
- **`review.ai.present` 只作内部判据**：不在叙事里出现「AI 陪跑局 / 这是 AI 打的」这类表述。
