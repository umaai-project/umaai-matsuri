# 耐 / 根机制旋钮扫描状态（每批自动更新）

- 更新时间：**21:03:31**　当前：`FINAL / done`
- 已完成批次：**12**　累计跑局：**795,000**　累计跑测墙钟：**626s**　最近 3 批吞吐：**1,175 局/s**
- 基准 = 教诲预设落地 + 五维系数结论后的当前生产档（stamina/guts 仍继承 default 的 status_weights）。
- 固定融合 ga-r2 #2；手写档 `search_n=0`；16 旋钮 × 2 档 + 坐标下降 2 轮。
- 初筛 110000 块 2500 局 `--correct none`；复核 120000 块 5000 局 `--correct holm` + 影子同向。
- 备注：扫描完成

## 当前全局 Top10（配对 Δ vs 各自当前生产档）

| Δ | CI95 | p_adj | 阶段 / 批次 | 变体 |
|---:|---:|---:|---|---|
| -285.2 | ±77.6 | 0.0000 | REVIEW_gu / shadow | `rv__03_gu__rf_j_all` |
| -285.6 | ±102.0 | 0.0000 | REFINE_gu_joint / all | `gu__rf_j_all` |
| -290.9 | ±74.3 | 0.0000 | REVIEW_gu / shadow | `rv__04_gu__rf_j_drop_end` |
| -291.1 | ±101.9 | 0.0000 | REFINE_gu_joint / all | `gu__rf_j_drop_end` |
| -298.4 | ±125.8 | 0.0000 | REVIEW_st / main | `rv__01_st__rf_j_all` |
| -299.3 | ±134.6 | 0.0000 | REVIEW_st / main | `rv__00_st__rf_j_drop_end` |
| -303.1 | ±140.2 | 0.0000 | REVIEW_st / main | `rv__02_st__rf_j_drop_ovf` |
| -303.8 | ±101.9 | 0.0000 | REFINE_gu_joint / all | `gu__rf_j_drop_ovf` |
| -305.0 | ±105.0 | 0.0000 | REFINE_gu_joint / all | `gu__rf_j_drop_dyn` |
| -338.7 | ±69.1 | 0.0000 | REVIEW_gu / shadow | `rv__05_gu__rf_j_drop_ovf` |

## 最近 8 批

| 阶段 | 批次 | 行 | 局/行 | 墙钟 | 吞吐 | best Δ |
|---|---|---:|---:|---:|---:|---:|
| REFINE_st_grid | all | 65 | 2500 | 118.3s | 1,374 | -495.9 |
| REFINE_st_joint | all | 10 | 2500 | 19.3s | 1,295 | -342.1 |
| REFINE_gu_grid | all | 65 | 2500 | 117.6s | 1,382 | -361.8 |
| REFINE_gu_joint | all | 10 | 2500 | 19.6s | 1,276 | -285.6 |
| REVIEW_st | main | 4 | 5000 | 14.5s | 1,379 | -298.4 |
| REVIEW_st | shadow | 4 | 5000 | 21.0s | 952 | -395.4 |
| REVIEW_gu | main | 4 | 5000 | 15.3s | 1,307 | -361.1 |
| REVIEW_gu | shadow | 4 | 5000 | 15.8s | 1,266 | -285.2 |
