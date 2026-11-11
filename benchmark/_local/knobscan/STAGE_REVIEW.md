# 耐 / 根机制旋钮独立块复核

- 5000 局（seed 120000），`--correct holm`；判定 = 主卡组 CI 不跨零 + 影子卡组同向。

## stamina_build

| 变体 | 主 Δ | CI95 | p_adj | 影子 Δ | 同向 | 判定 |
|---|---:|---:|---:|---:|:---:|---|
| `rv__00_st__rf_j_drop_end` | -299.3 | ±134.6 | 0.0000 | -396.2 | 是 | **不通过** |
| `rv__01_st__rf_j_all` | -298.4 | ±125.8 | 0.0000 | -395.4 | 是 | **不通过** |
| `rv__02_st__rf_j_drop_ovf` | -303.1 | ±140.2 | 0.0000 | -400.0 | 是 | **不通过** |

## guts_build

| 变体 | 主 Δ | CI95 | p_adj | 影子 Δ | 同向 | 判定 |
|---|---:|---:|---:|---:|:---:|---|
| `rv__03_gu__rf_j_all` | -361.1 | ±88.0 | 0.0000 | -285.2 | 是 | **不通过** |
| `rv__04_gu__rf_j_drop_end` | -370.6 | ±84.4 | 0.0000 | -290.9 | 是 | **不通过** |
| `rv__05_gu__rf_j_drop_ovf` | -392.0 | ±78.4 | 0.0000 | -338.7 | 是 | **不通过** |

## 变体出处

- `rv__00_st__rf_j_drop_end`：stamina_build / 初筛 Δ -342.1 / `st__rf_j_drop_end`
- `rv__01_st__rf_j_all`：stamina_build / 初筛 Δ -345.0 / `st__rf_j_all`
- `rv__02_st__rf_j_drop_ovf`：stamina_build / 初筛 Δ -354.1 / `st__rf_j_drop_ovf`
- `rv__03_gu__rf_j_all`：guts_build / 初筛 Δ -285.6 / `gu__rf_j_all`
- `rv__04_gu__rf_j_drop_end`：guts_build / 初筛 Δ -291.1 / `gu__rf_j_drop_end`
- `rv__05_gu__rf_j_drop_ovf`：guts_build / 初筛 Δ -303.8 / `gu__rf_j_drop_ovf`

**通过 0 / 6 组**
