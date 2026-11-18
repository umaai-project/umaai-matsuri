# -*- coding: utf-8 -*-
"""逐回合波动图：x=回合，y=波动"""
import glob, numpy as np
from matplotlib import pyplot as plt, font_manager as fm

fm.fontManager.addfont('/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc')
plt.rcParams['font.family'] = ['Noto Sans CJK JP']
plt.rcParams['axes.unicode_minus'] = False

files = sorted(glob.glob('benchmark/_local/luck_trend_20261117/per_turn_b*.txt'))
rows = {}
for f in files:
    for ln in open(f):
        p = ln.strip().split(',')
        if len(p) != 6 or p[0] != 'T':
            continue
        seed, t, e = int(p[1]), int(p[2]), float(p[3])
        rows.setdefault(seed, {})[t] = e

seeds = sorted(rows)
turns = np.array(sorted({t for s in rows for t in rows[s]}), float)
E = np.array([[rows[s].get(int(t), np.nan) for t in turns] for s in seeds])
n = np.sum(np.isfinite(E), axis=0)
sd = np.nanstd(E, axis=0, ddof=1)
q = np.nanpercentile(E, [5, 25, 50, 75, 95], axis=0)

fig, axs = plt.subplots(2, 1, figsize=(11, 8), sharex=True,
                        gridspec_kw={'height_ratios': [1.0, 1.2], 'hspace': 0.18})

ax = axs[0]
ax.plot(turns, sd, color='#c0392b', lw=2, label='跨局标准差 sd(E)')
i38, i76 = int(np.argmin(np.abs(turns - 38))), len(turns) - 1
ax.scatter([turns[0], turns[i38], turns[-1]], [sd[0], sd[i38], sd[-1]],
           color='#c0392b', zorder=5)
for tt, vv, txt, off in ((turns[0], sd[0], f't=0: {sd[0]:.0f}', (10, 6)),
                         (turns[i38], sd[i38], f't=38: {sd[i38]:.0f}', (10, 6)),
                         (turns[-1], sd[-1], f't=76: {sd[-1]:.0f}', (-78, 6))):
    ax.annotate(txt, (tt, vv), textcoords='offset points', xytext=off, fontsize=9)
ax.text(0.985, 0.06, 'sd(t) 随分化线性增长：t=0 仅 71（几乎无分化），t=76 达 1644（≈终局实际分 sd 1505）',
        transform=ax.transAxes, ha='right', fontsize=8.5, color='#555')
ax.axhspan(44, 73, color='#27ae60', alpha=0.25,
           label='搜索 MC 噪声 44–73（N=2048 报告值）')
ax.set_ylabel('跨局标准差（分）')
ax.set_title('搜索期期望终局分 E(t) 的逐回合波动（300 局，N=2048，speed_build 基线）', fontsize=12)
ax.grid(alpha=0.3)
ax.legend(loc='upper left', fontsize=9)

ax = axs[1]
ax.fill_between(turns, q[0], q[4], color='#2980b9', alpha=0.18, label='p5–p95')
ax.fill_between(turns, q[1], q[3], color='#2980b9', alpha=0.35, label='p25–p75')
ax.plot(turns, q[2], color='#2980b9', lw=2, label='中位数')
ax.plot(turns, np.nanmean(E, axis=0), color='#e67e22', lw=1.4, ls='--', label='均值')
ax.plot(turns, np.nanmean(E, axis=0)[0] + 40.0 * turns, ls=(0, (3, 2)), color='0.35', lw=0.9,
        label='luck_turn_bonus=40 趋势线（E(0)+40·t）')
peak = int(np.nanargmax(np.nanmean(E, axis=0)))
ax.annotate(f'均值见顶 t={int(turns[peak])}', (turns[peak], np.nanmean(E, axis=0)[peak]),
            textcoords='offset points', xytext=(-70, 14), fontsize=9,
            arrowprops=dict(arrowstyle='->', lw=0.8, color='#555'))
ax.set_xlabel('回合 t')
ax.set_ylabel('期望终局分 E(t)（分）')
ax.grid(alpha=0.3)
ax.legend(loc='upper left', fontsize=9)

out = 'benchmark/_local/luck_trend_20261117/volatility_n300.png'
fig.savefig(out, dpi=140, bbox_inches='tight')
print('wrote', out)
print('sd: t0 %.1f  t24 %.1f  t38 %.1f  t56 %.1f  t69 %.1f  t76 %.1f'
      % tuple(sd[[int(np.argmin(np.abs(turns - x))) for x in (0, 24, 38, 56, 69, 76)]]))
print('p5-p95 宽度: t0 %.0f  t38 %.0f  t76 %.0f'
      % tuple((q[4] - q[0])[[int(np.argmin(np.abs(turns - x))) for x in (0, 38, 76)]]))
