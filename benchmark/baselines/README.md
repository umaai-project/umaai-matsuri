# 基线（变更探测器）

一档一个 `*.json`，记录**固定局面条件 + 固定种子块**下的均值。`bench baseline` 自动读取 `*.json`，仅比较分数与 PT 均值（随机器负载波动的耗时不参与判定）。

录制口径以 `data/constants.json` 的 `pt_score_rate` 与 `data/matsuri_config.toml` 默认档 `pt_favor_rate`（及各 JSON 内 `config` 快照）为准；任一口径变更须重录基线。

## 档位

| 文件 | 局面条件 |
|---|---|
| `a-normal.json` | 普通马娘 `105902`，无局外 buff（`qie=none`、`shield=0`，对应仓内默认档） |
| `b-ideal.json` | 狄杜斯 `106302`，小切 + 心情盾 2（`qie=xiaoqie`、`shield=2`） |

每条记录按训练员与 `search_n` 分开：手写档 100 局，搜索档（`N=8192`）当前 10 局；搜索档补满 100 局后同步更新 `games` 与均值。

## 复跑与判定

```bash
# 全部档、两种训练员
./target/release/bench baseline

# 只跑理想档的手写项
./target/release/bench baseline --name b-ideal --trainer hand

# 缩短局数（判定容差按合并标准误自动放宽）
./target/release/bench baseline --games 20
```

- **判定规则**：`|Δ| ≤ max(3 × 合并标准误, 1% × 基线均值)` 视为通过，容差由 `--sigma`（默认 3）与 `--rel`（默认 0.01）控制，任一档超限即非零退出。
- **重录方式**：输出表带 `score_sd` / `pt_sd`（样本标准差，n−1 口径），当前按表内 `*_mean` / `*_sd` 手动更新对应 JSON（待 `--update` 实现）。

## 更新纪律

- 偏差超限时先排查机制或口径位移，确认合理后再重录基线，保持默认容差门禁。
- 重录后在 [`docs/changelog.md`](../docs/changelog.md) 的决定记录写明位移原因与影响面；档间差与基线快照见 [`docs/tests_overview.md`](../docs/tests_overview.md) §4。
