# 五维训练系数扫描结果（固定融合 ga-r2 #2）

## 执行概要

- 基准 = 教诲预设落地后的当前生产档；只扫 stamina / guts / wisdom 三套（speed 默认档不动）。
- 阶段 0 诊断 10k 局 × 4 套；粗扫 5 维 × 4 档（0.5/0.75/1.25/1.5×）+ 构成反推点，坐标下降 2 轮；共 126 个变体。
- 13 批 / 490,000 局 / 跑测墙钟 395s（6.6 分钟）。
- 初筛 95000 块 2500 局 `--correct none`；复核 105000 块 5000 局 `--correct holm` + 影子卡组同向。

## 结论

- 初筛最优 Δ = **+736.3 ±159.0**（`wi__r2_a1_f3`）。
- 独立块复核通过 **3 / 9** 组。

### 每套最优（复核 Δ 最大）

| build | 复核 Δ | CI95 | p_adj | status_weights |
|---|---:|---:|---:|---|
| wisdom_build | **+630.6** | ±145.4 | 0.0000 | `[0.808125, 1.111613, 0.629025, 0.150000, 1.310850]` |

- **stamina / guts 复核 0/6 通过**：初筛里的正值（guts r2 +92.0）与最不负值（stamina r1 −178.3）
  在独立块上消失或影子卡组反向，判为噪声 → 两档 `status_weights` 保持继承 default，不写差异。
  阶段 0 诊断里它们的残余失分（相对 speed 默认档 stamina −1949、guts −3270）不是单靠五维系数能补的，
  下一步应看 `overflow` / `endgame` / `dynamic` 或联合精修。
- 补丁端到端验证：`tomllib` 解析通过、`cardtest` 载入 rc=0、wisdom 的 advice 段仍在、
  default 的 `status_weights` 未变、**默认档 seed 61444 仍 86053 / PT 11864**；
  wisdom 主卡组 10 局分数 80848.5 → 82303.9（仅证明补丁生效，10 局样本不足以估计效应量）。

### 建议补丁

- 见 `proposed_patch.toml`；**本轮未写入 `data/matsuri_config.toml`**（改生产档需同步文档并单独确认）。

## 限制

- 结论条件化于固定融合 ga-r2 #2 与 2026-11-07 落地的教诲预设；若扫 fusion 或再改 advice，本批系数需重扫。
- `status_weights` 与 `dynamic` / `overflow` 连乘耦合（`value.rs:753-756`），坐标下降只保证局部最优；联合精修留待方案 §4.2 阶段 6。
- `cardtest` 强制手写档，结论不外推到搜索档 / 在线。

## 产物

- `STAGE_DIAG.md` / `STAGE_REVIEW.md` / `STATUS.md` / `progress.jsonl`；
- `rank_all.csv` 全排名、`proposed_patch.toml` 建议补丁、`best_*_r*.json` 坐标基座。

