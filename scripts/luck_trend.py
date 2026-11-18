#!/usr/bin/env python3
"""逐回合期望终局分形状分析：读 `bench luck --per-turn` 的输出，出均值曲线、显示分残差、局部斜率与拟合比较。

用法：
    python3 scripts/luck_trend.py --glob 'benchmark/_local/luck_trend_20261117/per_turn_b*.txt' --bonus 40
可选：--svg out.svg 画 2x2 图；--boot 2000 bootstrap 次数；--csv per_turn_mean.csv 落逐回合均值表。
口径：按局重采样（cluster bootstrap），避免把同一局的 65 个点当独立样本。
"""

import argparse
import glob
import sys

import numpy as np

MAX_TURN = 77


def parse(paths):
    games, rows, batches = {}, {}, []
    for path in paths:
        here = []
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split(",")
                if parts[0] == "T" and len(parts) == 6:
                    seed, turn = int(parts[1]), int(parts[2])
                    rows.setdefault(seed, {})[turn] = (float(parts[3]), float(parts[4]), int(parts[5]))
                elif len(parts) == 11 and parts[0].isdigit():
                    here.append(int(parts[0]))
                    games[int(parts[0])] = {
                        "first": float(parts[2]),
                        "last": float(parts[4]),
                        "delta": float(parts[5]),
                        "b": float(parts[6]),
                        "reg": float(parts[7]),
                        "score": int(parts[9]),
                    }
        batches.append((path.rsplit("/", 1)[-1], sorted(set(here))))
    return games, rows, batches


def ols(x, y):
    x, y = np.asarray(x, float), np.asarray(y, float)
    keep = np.isfinite(x) & np.isfinite(y)
    if keep.sum() < 2:
        return None
    x, y = x[keep], y[keep]
    xm, ym = x.mean(), y.mean()
    sxx = float(((x - xm) ** 2).sum())
    if sxx <= 0:
        return None
    b = float(((x - xm) * (y - ym)).sum()) / sxx
    return b, float(ym - b * xm)


def ci(values, level=95.0):
    lo, hi = np.percentile(values, [(100 - level) / 2, 100 - (100 - level) / 2])
    return float(lo), float(hi)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--glob", nargs="+", required=True)
    ap.add_argument("--bonus", type=float, default=40.0)
    ap.add_argument("--boot", type=int, default=2000)
    ap.add_argument("--svg", default=None)
    ap.add_argument("--csv", default=None)
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    files = sorted({p for pattern in args.glob for p in glob.glob(pattern)})
    if not files:
        sys.exit("没有匹配文件")
    games, rows, batches = parse(files)
    if not rows:
        sys.exit("没有逐回合行（确认用了 --per-turn）")

    seeds = sorted(rows)
    turns = np.array(sorted({t for s in rows for t in rows[s]}), float)
    E = np.full((len(seeds), len(turns)), np.nan)
    S = np.full((len(seeds), len(turns)), np.nan)
    C = np.full((len(seeds), len(turns)), np.nan)
    for i, seed in enumerate(seeds):
        for t, (e, s, c) in rows[seed].items():
            j = int(np.searchsorted(turns, t))
            E[i, j], S[i, j], C[i, j] = e, s, c

    mu = np.nanmean(E, axis=0)
    n = np.sum(np.isfinite(E), axis=0)
    sd = np.nanstd(E, axis=0, ddof=1)
    se = sd / np.sqrt(n)
    ci_t = 1.96 * se
    smu = np.nanmean(S, axis=0)
    cmu = np.nanmean(C, axis=0)
    mc_se = smu / np.sqrt(np.maximum(2048.0 / cmu, 1.0))

    print(f"# 文件 {len(files)}: {', '.join(files)}")
    print(f"# 局数 n={len(seeds)} 回合点={len(turns)} 覆盖 n(t)∈[{int(n.min())},{int(n.max())}]")
    per_game_span = [len(rows[s]) for s in seeds]
    print(f"# 每局点数 mean={np.mean(per_game_span):.1f} min={min(per_game_span)} max={max(per_game_span)}")
    print(f"# 终局实际分 mean={np.mean([g['score'] for g in games.values()]):.1f} "
          f"sd={np.std([g['score'] for g in games.values()], ddof=1):.1f} "
          f"(有终局行的局 {len(games)})")

    print("\n## 逐回合均值（全表写入 csv；下表抽样）")
    rng = np.random.default_rng(args.seed)
    boot_mu = np.empty((args.boot, len(turns)))
    for k in range(args.boot):
        idx = rng.integers(0, len(seeds), len(seeds))
        boot_mu[k] = np.nanmean(E[idx], axis=0)
    mu_ci = np.percentile(boot_mu, [2.5, 97.5], axis=0)

    print("| t | n | mean E | sd | ci95 | mean stdev | stdev/√(n/候选)† |")
    print("|---|---|---|---|---|---|---|")
    for j, t in enumerate(turns):
        if int(t) % 4 == 0 or t in (1, 76):
            print(f"| {int(t)} | {int(n[j])} | {mu[j]:.1f} | {sd[j]:.1f} | ±{ci_t[j]:.1f} | {smu[j]:.1f} | {mc_se[j]:.1f} |")

    # 斜率估计
    d = np.array([games[s]["delta"] for s in seeds if s in games])
    bg = np.array([games[s]["b"] for s in seeds if s in games])
    br = np.array([games[s]["reg"] for s in seeds if s in games])
    def mean_ci(v):
        m, s = float(np.mean(v)), float(np.std(v, ddof=1))
        q = 1.96 * s / np.sqrt(len(v))
        return m, q
    m_d, q_d = mean_ci(d)
    m_bg, q_bg = mean_ci(bg)
    m_br, q_br = mean_ci(br)
    span = turns[-1] - turns[0]
    fit = ols(turns, mu)
    b_lin, a_lin = fit
    print(f"\n## 斜率")
    print(f"- 端点 b_gap = {m_bg:.2f} ± {q_bg:.2f}（n={len(bg)}）")
    print(f"- 逐局 OLS b_reg = {m_br:.2f} ± {q_br:.2f}")
    print(f"- 均值曲线 OLS b_lin = {b_lin:.2f}（点估计，无 CI）")
    print(f"- 端到端 ΔE = {m_d:.1f} ± {q_d:.1f}；span={int(span)} 回合")

    # bootstrap：均值曲线线性 / 二次 / 分段
    boot_b, boot_q, boot_r, boot_seg = [], [], [], []
    for k in range(args.boot):
        x = turns
        y = boot_mu[k]
        f = ols(x, y)
        if f:
            boot_b.append(f[0])
        q = np.polyfit(x, y, 2)
        boot_q.append(q[0])
        r = y - (y[0] + args.bonus * (x - x[0]))
        boot_r.append(r)
        segs = []
        for lo, hi in ((0, 22), (23, 46), (47, 76)):
            m = (x >= lo) & (x <= hi)
            fs = ols(x[m], y[m])
            segs.append(fs[0] if fs else np.nan)
        boot_seg.append(segs)
    boot_r = np.array(boot_r)
    boot_seg = np.array(boot_seg)

    q2 = np.polyfit(turns, mu, 2)[0]
    lo_b, hi_b = ci(boot_b)
    lo_q, hi_q = ci(boot_q)
    peak_t = float(turns[np.argmin(mu)])
    print(f"\n## 形状项")
    print(f"- 二次项 c = {q2:.4f}（CI95 [{lo_q:.4f}, {hi_q:.4f}]，>0 凹向上 / <0 凹向下）")
    print(f"  等效端点偏差 c·(span/2)² = {q2 * (span / 2) ** 2:.0f} 分")
    print(f"- 均值曲线线性斜率 b_lin CI95 [{lo_b:.2f}, {hi_b:.2f}]")
    print(f"- 均值曲线最低点 t={peak_t:.0f}（E={np.nanmin(mu):.1f}）")

    print("\n## 显示分残差（bonus=" + str(args.bonus) + "，r(t)=meanE(t)-meanE(0)-bonus·t）")
    print("| t | r | ci95 | 显著 |")
    print("|---|---|---|---|")
    for j, t in enumerate(turns):
        r = mu[j] - (mu[0] + args.bonus * (turns[j] - turns[0]))
        lo, hi = np.percentile(boot_r[:, j], [2.5, 97.5])
        flag = "yes" if (lo > 0 or hi < 0) else ""
        if int(t) % 4 == 0 or t == 76 or flag:
            print(f"| {int(t)} | {r:.0f} | [{lo:.0f}, {hi:.0f}] | {flag} |")
    r_end = mu[-1] - (mu[0] + args.bonus * span)
    lo_e, hi_e = np.percentile(boot_r[:, -1], [2.5, 97.5])
    print(f"- 末点残差 r(76) = {r_end:.0f}（CI95 [{lo_e:.0f}, {hi_e:.0f}]）")
    sig = [(int(turns[j]), float(mu[j] - (mu[0] + args.bonus * turns[j])), float(np.percentile(boot_r[:, j], 2.5)), float(np.percentile(boot_r[:, j], 97.5))) for j in range(len(turns)) if (np.percentile(boot_r[:, j], 2.5) > 0 or np.percentile(boot_r[:, j], 97.5) < 0)]
    print(f"- 显著偏离回合数 = {len(sig)}" + (f"：{[(t, round(r)) for t, r, l, h in sig][:12]}..." if len(sig) > 12 else "" if not sig else f"：{[(t, round(r)) for t, r, l, h in sig]}"))

    print("\n## 稳健复核（10% 截尾均值，看重尾是否主导形状）")

    def trim_mean(y, frac=0.1):
        y = y[np.isfinite(y)]
        if len(y) < 5:
            return float(np.mean(y)) if len(y) else np.nan
        k = int(len(y) * frac)
        return float(np.sort(y)[k:len(y) - k].mean())

    tmu = np.array([trim_mean(E[:, j]) for j in range(len(turns))])
    tr = tmu - (tmu[0] + args.bonus * (turns - turns[0]))
    nb = min(args.boot, 300)
    boot_tr = np.empty((nb, len(turns)))
    for k in range(nb):
        idx = rng.integers(0, len(seeds), len(seeds))
        for j in range(len(turns)):
            boot_tr[k, j] = trim_mean(E[idx, j])
    boot_tr = boot_tr - (boot_tr[:, :1] + args.bonus * (turns - turns[0])[None, :])
    print("| t | r(均值) | r(截尾) | ci95(截尾) |")
    print("|---|---|---|---|")
    for j, t in enumerate(turns):
        if int(t) % 8 == 0 or t == 76 or (np.nanmin(boot_tr[:, j]) > 0 or np.nanmax(boot_tr[:, j]) < 0):
            lo_t, hi_t = ci(boot_tr[:, j])
            print(f"| {int(t)} | {mu[j] - (mu[0] + args.bonus * turns[j]):.0f} | {tr[j]:.0f} | [{lo_t:.0f}, {hi_t:.0f}] |")
    peak = int(np.nanargmax(tr))
    print(f"- 截尾残差峰 t={int(turns[peak])} 值 {tr[peak]:.0f}（均值残差同点 {mu[peak] - (mu[0] + args.bonus * turns[peak]):.0f}）")

    print("\n## 分带斜率（每带 ΔE/Δt，供 bonus 分档表参考）")
    print("| 带 | 端点 t | ΔE/回合 | 带内 ΔE | ci95 |")
    print("|---|---|---|---|---|")
    for lo, hi in ((0, 12), (13, 24), (25, 36), (37, 48), (49, 60), (61, 76)):
        jlo = min(int(np.searchsorted(turns, lo)), len(turns) - 1)
        jhi = min(int(np.searchsorted(turns, hi)), len(turns) - 1)
        dt = turns[jhi] - turns[jlo]
        if dt <= 0:
            continue
        slope = (mu[jhi] - mu[jlo]) / dt
        bs = (boot_mu[:, jhi] - boot_mu[:, jlo]) / dt
        lo_c, hi_c = ci(bs)
        print(f"| [{lo},{hi}] | {int(turns[jlo])}→{int(turns[jhi])} | {slope:.1f} | {mu[jhi] - mu[jlo]:+.0f} | [{lo_c:.1f}, {hi_c:.1f}] |")

    print("\n## 最优常数 bonus（过原点拟合 b* = argmin_b RMS_t[meanE(t)-meanE(0)-b·t]）")
    grid = np.arange(20.0, 60.0, 0.02)
    _t0 = mu[0]

    def rms_curve(curve):
        y = curve - curve[0]
        res = y[None, :] - grid[:, None] * turns[None, :]
        return np.sqrt(np.nanmean(res ** 2, axis=1))

    rms_g = rms_curve(mu)
    kb = int(np.argmin(rms_g))
    bstar = float(grid[kb])
    boot_b = []
    for k in range(args.boot):
        rg = rms_curve(boot_mu[k])
        if np.all(np.isfinite(rg)):
            boot_b.append(float(grid[int(np.argmin(rg))]))
    lo_b, hi_b = ci(np.array(boot_b))
    print(f"- b* = {bstar:.2f}（CI95 [{lo_b:.2f}, {hi_b:.2f}]）；残差 RMS={rms_g[kb]:.1f}，max|r|={np.nanmax(np.abs(mu - _t0 - bstar * turns)):.0f}")
    print(f"- 对照（同一 RMS 口径）：b=36.9（端点弦斜率）RMS={float(np.sqrt(np.nanmean((mu - _t0 - 36.9 * turns) ** 2))):.1f}；"
          f"b=40（现配置）RMS={float(np.sqrt(np.nanmean((mu - _t0 - 40.0 * turns) ** 2))):.1f}；"
          f"b=42.6（带截距 OLS）RMS={float(np.sqrt(np.nanmean((mu - _t0 - 42.6 * turns) ** 2))):.1f}")
    print("- 口径说明：带截距 OLS 会让拟合线在 t=0 偏离 meanE(0)，而显示分以 meanE(0) 为锚，故 bonus 标定应用过原点 RMS。")

    print("\n## 分段斜率")
    names = ["[0,22]", "[23,46]", "[47,76]"]
    seg_mean = np.array([mu[(turns >= lo) & (turns <= hi)] for lo, hi in ((0, 22), (23, 46), (47, 76))], dtype=object)
    seg_pt = []
    for lo, hi in ((0, 22), (23, 46), (47, 76)):
        m = (turns >= lo) & (turns <= hi)
        f = ols(turns[m], mu[m])
        seg_pt.append(f[0] if f else np.nan)
    for i, name in enumerate(names):
        lo, hi = ci(boot_seg[:, i])
        print(f"- {name}: {seg_pt[i]:.2f}（CI95 [{lo:.2f}, {hi:.2f}]）")
    for i, j in ((0, 1), (1, 2), (0, 2)):
        diff = boot_seg[:, i] - boot_seg[:, j]
        lo, hi = ci(diff)
        print(f"- {names[i]} − {names[j]}: {seg_pt[i] - seg_pt[j]:+.2f}（CI95 [{lo:+.2f}, {hi:+.2f}]）{' 显著' if lo > 0 or hi < 0 else ''}")

    # 局部斜率
    print("\n## 局部斜率（滑窗 OLS，window 宽度 w 回合）")
    print("| t | b(w=6) | b(w=10) | b(w=10) ci95 |")
    print("|---|---|---|---|")
    for j, t in enumerate(turns):
        row = []
        for w in (6, 10):
            m = np.abs(turns - t) <= w
            f = ols(turns[m], mu[m])
            row.append(f[0] if f else np.nan)
        m = np.abs(turns - t) <= 10
        bs = []
        for k in range(min(args.boot, 400)):
            f = ols(turns[m], boot_mu[k][m])
            if f:
                bs.append(f[0])
        lo, hi = ci(bs) if bs else (np.nan, np.nan)
        if int(t) % 6 == 0 or t == 76:
            print(f"| {int(t)} | {row[0]:.1f} | {row[1]:.1f} | [{lo:.1f}, {hi:.1f}] |")

    # 波动结构
    step = []
    for i, seed in enumerate(seeds):
        ts = sorted(rows[seed])
        for a, b2 in zip(ts, ts[1:]):
            de = (rows[seed][b2][0] - rows[seed][a][0]) / (b2 - a)
            step.append(de)
    step = np.array(step)
    print("\n## 波动结构")
    print(f"- 逐局单步斜率（相邻决策点）mean={step.mean():.1f} sd={step.std(ddof=1):.1f} se={step.std(ddof=1)/np.sqrt(len(step)):.2f}")
    print(f"- 交叉局 sd(E) 曲线：t=0 {sd[0]:.1f} / t=38 {sd[int(np.argmin(np.abs(turns-38)))]:.1f} / t=76 {sd[-1]:.1f}")
    print("- 单点搜索 MC 噪声须另测（bench root 固定根多次搜索）：N=2048 时 t=0/24 单候选均值 sd ≈ 100 分，"
          "远小于交叉局 sd，回合间差异主要来自局面分化。")

    print("\n## 按批稳定性")

    def curve(ss):
        ss = [s for s in ss if s in rows]
        out = np.full(len(turns), np.nan)
        for j, t in enumerate(turns):
            vals = [rows[s][t][0] for s in ss if t in rows[s]]
            if vals:
                out[j] = float(np.mean(vals))
        return out, len(ss)

    for label, ss in batches:
        c, m = curve(ss)
        f = ols(turns, c)
        if not np.isfinite(c[0]) or not np.isfinite(c[-1]) or not f:
            print(f"- {label}: 跳过（不完整，n={m}）")
            continue
        sc = [games[s]["score"] for s in ss if s in games]
        sc_txt = f" score={np.mean(sc):.0f}" if sc else ""
        print(f"- {label}: n={m} E0={c[0]:.1f} Elast={c[-1]:.1f} ΔE={c[-1] - c[0]:+.1f} b_lin={f[0]:.2f}{sc_txt}")

    if len(seeds) >= 8:
        bs = np.array([games[s]["reg"] for s in seeds if s in games])
        nh = len(bs) // 2
        if nh >= 4:
            nd = min(args.boot, 400)
            diffs = np.array([
                bs[rng.integers(0, nh, nh)].mean() - bs[rng.integers(nh, 2 * nh, 2 * nh - nh)].mean()
                for _ in range(nd)
            ])
            lo_d, hi_d = ci(diffs)
            print(f"- 前半/后半（按 seed 序）b_reg 差 = {bs[:nh].mean() - bs[nh:2 * nh].mean():+.2f}（CI95 [{lo_d:+.2f}, {hi_d:+.2f}]）")

    print("\n## 期望分与终局实际分")
    paired = [(games[s]["first"], games[s]["last"], games[s]["score"]) for s in seeds if s in games]
    if paired:
        first = np.array([p[0] for p in paired])
        last = np.array([p[1] for p in paired])
        score = np.array([p[2] for p in paired])
        for name, est in (("E(0)", first), ("E(76)", last)):
            gap = score - est
            se_g = float(gap.std(ddof=1) / np.sqrt(len(gap)))
            print(f"- {name} vs 终局实际分：gap={gap.mean():+.1f} ± {1.96 * se_g:.1f}（sd={gap.std(ddof=1):.1f}, n={len(gap)}）")

    if args.csv:
        with open(args.csv, "w", encoding="utf-8") as f:
            f.write("turn,n,mean,sd,se,ci95,stdev,cand,mc_se\n")
            for j, t in enumerate(turns):
                f.write(f"{int(t)},{int(n[j])},{mu[j]:.3f},{sd[j]:.3f},{se[j]:.3f},{ci_t[j]:.3f},{smu[j]:.3f},{cmu[j]:.2f},{mc_se[j]:.3f}\n")
        print(f"\n# 已写 {args.csv}")

    if args.svg:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        from matplotlib import font_manager as _fm

        _fm.fontManager.addfont("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc")
        plt.rcParams["font.family"] = ["Noto Sans CJK JP"]
        plt.rcParams["axes.unicode_minus"] = False

        fig, axes = plt.subplots(2, 2, figsize=(11, 8))
        ax = axes[0][0]
        ax.fill_between(turns, mu_ci[0], mu_ci[1], alpha=0.25, color="tab:blue", label="均值 95% CI")
        ax.plot(turns, mu, color="tab:blue", lw=1.6, label="均值 E(t)")
        ax.plot(turns, a_lin + b_lin * turns, "--", color="tab:red", lw=1.2, label=f"线性拟合 b={b_lin:.1f}")
        ax.plot(turns, mu[0] + args.bonus * (turns - turns[0]), ls=(0, (3, 2)), color="0.35", lw=0.9,
                label=f"luck_turn_bonus={args.bonus:g} 趋势线")
        ax.set_title("期望终局分 E(t) 随回合变化")
        ax.set_xlabel("回合 t"); ax.set_ylabel("期望终局分（分）"); ax.legend(fontsize=8); ax.grid(alpha=0.3)

        ax = axes[0][1]
        disp = mu + (MAX_TURN - turns) * args.bonus
        dl = boot_mu + (MAX_TURN - turns) * args.bonus
        ax.fill_between(turns, np.percentile(dl, 2.5, axis=0), np.percentile(dl, 97.5, axis=0), alpha=0.25, color="tab:green")
        ax.plot(turns, disp, color="tab:green", lw=1.6, label=f"显示分（bonus={args.bonus:g}）")
        ax.axhline(disp[0], color="grey", ls=":", lw=1, label="初始显示分基准")
        ax.set_title("显示分：水平即线性模型成立")
        ax.set_xlabel("回合 t"); ax.set_ylabel("显示分（分）"); ax.legend(fontsize=8); ax.grid(alpha=0.3)

        ax = axes[1][0]
        for w in (6, 10):
            arr = []
            for j in range(len(turns)):
                m = np.abs(turns - turns[j]) <= w
                f = ols(turns[m], mu[m])
                arr.append(f[0] if f else np.nan)
            ax.plot(turns, arr, lw=1.2, label=f"滑窗斜率 w={w}")
        ax.axhline(args.bonus, color="grey", ls="--", lw=1, label=f"luck_turn_bonus={args.bonus:g}")
        ax.set_title("瞬时斜率 b(t)")
        ax.set_xlabel("回合 t"); ax.set_ylabel("分/回合"); ax.legend(fontsize=8); ax.grid(alpha=0.3)

        ax = axes[1][1]
        rr = mu - (mu[0] + args.bonus * (turns - turns[0]))
        ax.fill_between(turns, np.percentile(boot_r, 2.5, axis=0), np.percentile(boot_r, 97.5, axis=0), alpha=0.25, color="tab:orange", label="残差 95% CI")
        ax.plot(turns, rr, color="tab:orange", lw=1.6, label="残差 r(t)")
        ax.axhline(0.0, color="grey", ls="--", lw=1)
        ax.set_title(f"相对线性 bonus={args.bonus:g} 的残差")
        ax.set_xlabel("回合 t"); ax.set_ylabel("偏差（分）"); ax.legend(fontsize=8); ax.grid(alpha=0.3)
        fig.tight_layout()
        fmt = "png" if args.svg.lower().endswith(".png") else "svg"
        fig.savefig(args.svg, format=fmt, dpi=140 if fmt == "png" else None)
        print(f"# 已写 {args.svg}")


if __name__ == "__main__":
    main()
