# 命令与取数

## 入口

**运行期直接跑 skill 自带二进制**，不依赖 cargo / Rust / 项目源码：

```powershell
<skill>\bin\review.exe <子命令> ... --data <skill>\data
```

`--data` 指向 skill 自带的 `data/`，传了之后不会回读项目源码路径，整目录可复制到任意位置使用。
`bin/review.exe` 或 `data/` 缺失时，由分发者重建一次（需要 cargo）：
`powershell -NoProfile -File scripts/build_review_skill.ps1 -WithData`。

## 子命令

| 子命令 | 参数 | 产物 |
|---|---|---|
| `open` | `<局包\|目录> [--game id] [--out 目录] [--data 根]` | `facts.json`、`brief.md` |
| `eval` | `<局包> --turn N [--seq S] [--n 1024] [--swap 动作名] [--all] [--force] [--game id] [--out] [--data]` | `recompute/turn{NN}_{seq}.json`；`--swap` 落 `recompute/swap/` |
| `infer` | `<局包> [--what advice,fusion,skill,kensan,all] [--turn N] [--json] [--game] [--out] [--data]` | `infer.json` |
| `curve` | `<局包> [--from A] [--to B] [--n 1024] [--force] [--game] [--out] [--data]` | `curve.json` |
| `whatif` | `<局包> --turn N --swap 动作名[,动作名…] [--horizon 8\|0] [--k 100] [--game] [--out] [--data]` | `whatif.json` |
| `report` | `<局包> [--narrative md] [--game] [--out] [--data]` | `digest.json`、`report.html` |

约定：
- `--out` 缺省 = 局包同级；复盘统一指向 `<工作区>/game{id}/`。
- `--data` 缺省 = 仓内 `data/`，正常不用传。
- `eval --all` 逐决策面各一点（6327 实测 121 点）；`--n` 建议 256。
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
| 事件选项怎么选 | 暂不比对（波动大，已停用），只陈述 `brief §3` 的触发位置 |
| 某个字段没有 | 标 `unavailable`，不猜 |

## 取数片段（Node，避免 PowerShell 中文乱码）

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
<skill>\bin\review.exe open game6327.zip --data <skill>\data --out game6327

# 2) 全量选择偏差（逐决策面）
<skill>\bin\review.exe eval game6327.zip --all --n 256 --data <skill>\data --out game6327

# 3) 某教诲点换一排列的终局差
<skill>\bin\review.exe eval game6327.zip --turn 35 --seq 3 --swap "速←特化 耐←助力 力←体力 根←灵感 智←研习" --data <skill>\data --out game6327

# 4) 某手换掉后的下游走向 / 终局
<skill>\bin\review.exe whatif game6327.zip --turn 12 --swap "休息" --k 100 --data <skill>\data --out game6327
<skill>\bin\review.exe whatif game6327.zip --turn 12 --swap "休息" --horizon 0 --k 10 --data <skill>\data --out game6327
```
