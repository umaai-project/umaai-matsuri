# stamina / guts 两套卡组退役归档（2026-11-07）

## 原因

用户决定：这两套 build 相对 speed 默认档的残余失分（−1949 / −3270）是**构成性的**
（stamina comp 只有 1 张速卡、guts comp 0 张耐卡）；教诲预设虽已定档，但五维系数复核不通过、
机制旋钮 278 变体 0/6 通过且全部显著为负，参数已调不动。先移除，等放开 comp 做构成搜索、
调参完成后再加入。

## 备份内容

| 文件 | 说明 |
|---|---|
| `stamina_build.csv` | 原 `decks/stamina_build.csv`（主 + 影子行，含定卡注释） |
| `guts_build.csv` | 原 `decks/guts_build.csv` |
| `matsuri_config_before_retire.toml` | 删除前的完整 `data/matsuri_config.toml`（含两套 `[decks.*]` 段，以及已落地的 wisdom `status_weights` 补丁） |
| `smoke_before.txt` | 删除前默认档 seed 61444 冒烟：86053 / PT 11864 |

## 定档成果仍留在 logs/，不随删除丢失

- 定卡：`logs/deckselect/RESULT.md`。stamina 槽2 `30304 → 30300` 樱花千代王
  （独立块 86000–86499 确认 +919.0 ±297.4）；guts 无替换。
- 教诲预设（advice t2/t11/t23/t35）：`logs/advscan/FINAL.md` / `proposed_patch.toml`；
  stamina 复核 +834.4 ±132.0（影子 +794.0）、guts +579.4 ±80.8（影子 +456.3）。
- 五维系数：`logs/statusscan/FINAL.md` —— 两套复核不通过，保持继承 default。
- 机制旋钮：`logs/knobscan/FINAL.md` —— 278 变体、独立块 0/6 通过且显著为负，保持继承 default。

## 加回步骤

1. 把本目录两个 CSV 拷回 `decks/`。
2. 从 `matsuri_config_before_retire.toml` 取 `[decks.stamina_build]` /
   `[decks.guts_build]` 两段（含 `[decks.*.advice]` 子表）插回 `data/matsuri_config.toml`。
3. 按 `AGENTS.md` 同步文档；跑 seed 61444 冒烟，确认默认档仍为 86053 / PT 11864。
4. 若届时 comp / fusion / advice 有变，教诲预设与系数需重扫（本批结论条件化于固定融合 ga-r2 #2）。
