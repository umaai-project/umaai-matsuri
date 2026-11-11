# 教诲分配扫描状态（每批自动更新）

- 更新时间：**19:46:58**　当前：`FINAL / done`
- 已完成批次：**31**　累计跑局：**7,464,000**　累计跑测墙钟：**6264s**　最近 3 批吞吐：**1,154 局/s**
- 预算：总 6600s，扫描可用 5940s，已用 6264s，剩余 **0s**
- S0 判定：max|Δ| = **742** 分 → **有效，继续扫描**
- 预算决策：局数 **2500** / 套数 **3** (guts_build, stamina_build, wisdom_build) / 轮数 **2**
- 备注：扫描完成

## 当前全局 Top10（配对 Δ vs 参照臂）

| Δ | CI95 | p_adj | 阶段 / 批次 | 变体 |
|---:|---:|---:|---|---|
| +834.4 | ±132.0 | 0.0000 | REVIEW_stamina / main | `rv__04_stamina__r2_t2_p076` |
| +826.3 | ±122.7 | 0.0000 | REVIEW_stamina / main | `rv__05_stamina__r2_t2_p077` |
| +817.4 | ±137.8 | 0.0000 | REVIEW_stamina / main | `rv__03_stamina__r2_t2_p073` |
| +802.4 | ±146.0 | 0.0000 | S1_stamina_r2 / t2 | `stamina__r2_t2_p073` |
| +798.7 | ±145.2 | 0.0000 | S1_stamina_r2 / t2 | `stamina__r2_t2_p076` |
| +794.0 | ±132.3 | 0.0000 | REVIEW_stamina / shadow | `rv__04_stamina__r2_t2_p076` |
| +792.2 | ±145.1 | 0.0000 | S1_stamina_r2 / t2 | `stamina__r2_t2_p077` |
| +784.7 | ±145.3 | 0.0000 | S1_stamina_r2 / t2 | `stamina__r2_t2_p072` |
| +783.7 | ±141.7 | 0.0000 | S1_stamina_r2 / t2 | `stamina__r2_t2_p028` |
| +779.2 | ±138.5 | 0.0000 | REVIEW_stamina / shadow | `rv__03_stamina__r2_t2_p073` |

## 最近 8 批

| 阶段 | 批次 | 行 | 局/行 | 墙钟 | 吞吐 | 当批 best Δ |
|---|---|---:|---:|---:|---:|---:|
| S1_wisdom_r2 | t23 | 121 | 2500 | 248.7s | 1,216 | +404.0 |
| S1_wisdom_r2 | t35 | 121 | 2500 | 240.6s | 1,257 | +404.0 |
| REVIEW_guts | main | 4 | 5000 | 15.6s | 1,282 | +579.4 |
| REVIEW_guts | shadow | 4 | 5000 | 14.6s | 1,370 | +460.3 |
| REVIEW_stamina | main | 4 | 5000 | 15.2s | 1,316 | +834.4 |
| REVIEW_stamina | shadow | 4 | 5000 | 19.7s | 1,015 | +794.0 |
| REVIEW_wisdom | main | 4 | 5000 | 16.6s | 1,205 | +282.9 |
| REVIEW_wisdom | shadow | 4 | 5000 | 16.1s | 1,242 | +346.8 |

## 阶段产物

- `progress.jsonl`：每批一行机读记录；`STAGE_*.md`：阶段结束报告。
- 初筛 `--correct none`（按 Δ 排序），复核 `--correct holm`（判定）。
- 固定融合 ga-r2 #2；行名精确等于 variant 档名；`--matsuri-config` 显式载入。
