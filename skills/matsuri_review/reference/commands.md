# 命令与取数

## 入口

**直接跑 skill 自带二进制**，不依赖 cargo / Rust / 项目源码：

```powershell
<skill>\bin\review.exe <子命令> ... --data <skill>\data
```

`--data` 指向 skill 自带的 `data/`，传了之后不会回读项目源码路径，整目录可复制到任意位置使用。
不传 `--data` 时按 exe 位置找 `data/`（同级或上级）；发布副本建议始终显式传 `--data`。

本目录若存在 `reference/dev.md`（开发态辅助文档），构建/分发说明见该文档；发布包不含此文件。

## 子命令

| 子命令 | 参数 | 产物 |
|---|---|---|
| `open` | `<局包\|目录> [--game id] [--out 目录] [--data 根]` | `facts.json`、`brief.md` |
| `eval` | `<局包> --turn N [--seq S] [--n 1024] [--swap 动作名] [--all] [--force] [--game id] [--out] [--data]` | `recompute/turn{NN}_{seq}.json`；`--swap` 落 `recompute/swap/` |
| `infer` | `<局包> [--what advice,fusion,skill,kensan,all] [--turn N] [--json] [--game] [--out] [--data]` | `infer.json` |
| `curve` | `<局包> [--from A] [--to B] [--n 1024] [--force] [--game] [--out] [--data]` | `curve.json` |
| `whatif` | `<局包> --turn N --swap 动作名[,动作名…] [--horizon 8\|0] [--k 100] [--game] [--out] [--data]` | `whatif.json` |
| `report` | `<局包> [--narrative md] [--game] [--out] [--data]` | `digest.json`、`report.html`（末尾含更优回合表） |
| `show` | `<局包> --turn N --seq S [--game] [--data]` | 标准输出 JSON（原始帧玩家可见状态） |

约定：
- `--out` 缺省 = 局包同级；复盘统一指向 `<工作区>/game{id}/`。
- `--data` 建议显式传 `<skill>\data`；缺省按 exe 位置解析（见上），发布副本不要依赖缺省。
- 选择偏差日常 `eval --all --n 2048`，最终 2–3 个点用 `--n 8192`（在线同口径）复核；逐决策面各一点。叙事只展开 `regret ≥ 100`（100–200 用「X 小优」，200 以上才明确表述）。UCB 下 `paired_n` 1024–8192，CI 按配对样本解释。
- 多决策面单点必须 `--seq`；具体面名会在报错里列出。

## 追问路由

| 用户问 | 动作 |
|---|---|
| 这局打得怎么样 | `open` → 读 `brief.md` → 六问叙事 |
| 哪回合最亏 | `eval --all` → 取 `regret > ci95` 且 `ci95` 最小 |
| 这一手选对了吗 | `eval --turn N [--seq S]` → 看 `regret ± ci95` 与命中 |
| 换一手会差多少分 | `eval --turn N [--seq S] --swap <动作名>` |
| 换一手之后会怎么走 | `whatif --turn N --swap <动作名> --k 100`（`--horizon 0` 看终局） |
| 教诲 / 転成选错了吗 | `infer --what advice,fusion`；再 `eval --turn N --seq <教诲帧>` |
| 出行 / 演目节奏 | 读 `brief §8` 陈述节点；无基线分位，不判快慢 |
| 某回合当时是什么状态 | `show --turn N --seq S` → 按训练位 / 数值表整理（仅显著点） |
| 事件选项怎么选 | 暂不比对（波动大，已停用），只陈述 `brief §3` 的触发位置 |
| 某个字段没有 | 标 `unavailable`，不猜 |

## 取数片段（Node，避免 PowerShell 中文乱码；没有 node 时直接 Read JSON）

读 eval 点：

```powershell
node -e "const fs=require('fs');const j=JSON.parse(fs.readFileSync(process.argv[1],'utf8'));console.log(JSON.stringify({turn:j.turn,seq:j.seq,kind:j.kind,regret:j.regret,ci95:j.ci95,hit:j.hit,chosen:j.chosen&&j.chosen.description,best:j.best&&j.best.description},null,1))" <out>/recompute/turn12_0.json
```

读 whatif 终局分与首个分叉：

```powershell
node -e "const fs=require('fs');const j=JSON.parse(fs.readFileSync(process.argv[1],'utf8'));console.log(JSON.stringify({turn:j.turn,source:j.actual_source,actual:j.actual.final_score,alts:j.alternates.map(a=>({label:a.label,final:a.final_score})),diverge:j.diverge_turns},null,1))" <out>/whatif.json
```

读 digest 的 evals：

```powershell
node -e "const fs=require('fs');const j=JSON.parse(fs.readFileSync(process.argv[1],'utf8'));for(const e of j.evals) console.log('T'+e.turn+' seq'+e.seq,e.kind,'regret',e.regret,'ci',e.ci95,e.hit)" <out>/digest.json
```

## 常见组合

```powershell
# 1) 出事实并读 brief
<skill>\bin\review.exe open game{id}.zip --data <skill>\data --out game{id}

# 2) 全量选择偏差（逐决策面）
<skill>\bin\review.exe eval game{id}.zip --all --n 2048 --data <skill>\data --out game{id}

# 3) 某教诲点换一排列的终局差
<skill>\bin\review.exe eval game{id}.zip --turn 35 --seq 3 --swap "速←特化 耐←助力 力←体力 根←灵感 智←研习" --data <skill>\data --out game{id}

# 4) 某手换掉后的下游走向 / 终局
<skill>\bin\review.exe whatif game{id}.zip --turn 12 --swap "休息" --k 100 --data <skill>\data --out game{id}
<skill>\bin\review.exe whatif game{id}.zip --turn 12 --swap "休息" --horizon 0 --k 10 --data <skill>\data --out game{id}
```
