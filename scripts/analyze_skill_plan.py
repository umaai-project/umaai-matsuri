#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""分析真机局包中的 skillplan（技能插件最优方案）与模拟评分口径。

用途
----
给定真机局包（zip / 局目录 / skillplan.json），读取：
  - game{id}_skillplan.json：技能插件在终局算出的最优购买方案
    （plan.predictedTotalScore 即「最优方案可达的最终评分」）；
  - game{id}_finalScore.json：终局帧（五维、剩余/已花技能点、总灵感）。
对这两份数据做评分对账，并支持用不同模拟公式「离线重算」终局分。

离线重算
--------
不修改、不运行 Rust 模拟：只用终局帧里的数据按指定公式重算，
毫秒级得到误差表。内置口径（现用/去510/680/不乘切/游戏2.0），
也可用 --sim 追加自定义口径：
    --sim "标签:固定分:每PT分:切因子"
    - 切因子可写 auto（按帧内切者/小切状态）、none（1.0）或数值（如 1.04）
示例：
    --sim "去510:0:1.88:auto"
    --sim "680不乘切:680:1.88:none"

用法
----
    python scripts/analyze_skill_plan.py <局包.zip|局目录|skillplan.json> [...]
        [--pt-mode remaining|cumulative] [--sim ...] [--json out.json]
"""
from __future__ import annotations

import argparse
import json
import sys
import zipfile
from collections import defaultdict
from pathlib import Path

# hint 等级折扣（%）：hint L1-L5 = 10/20/30/35/40，来自真机价格逐行核对
HINT_DISCOUNT = {0: 0, 1: 10, 2: 20, 3: 30, 4: 35, 5: 40}
XIAO_QIE_DISCOUNT = 4   # 小切：技能价格 -4%
QIE_ZHE_DISCOUNT = 10   # 切者：技能价格 -10%


def status_point(status: int) -> int:
    """五维原始属性 -> 终局评价分（复刻 umaai-core::score::status_point）。"""
    if status <= 0:
        return 0
    step = status if status <= 1200 else 1200 + (status - 1200) // 2
    if step <= 1200:
        low = [5, 8, 10, 13, 16, 18, 21, 24, 26, 28, 29, 30,
               31, 33, 34, 35, 39, 41, 42, 43, 52, 55, 66, 68]
        rem, score = step + 1, 0
        for band, rate in enumerate(low):
            if rem <= 0:
                break
            count = min(rem, 51 if band + 1 == len(low) else 50)
            score += count * rate
            rem -= count
        return score // 10
    if step <= 2000:
        rem, score, band = step - 1200, 38418, 0
        while rem > 0:
            width = 9 if band == 0 else (11 if band == 79 else 10)
            count = min(rem, width)
            score += count * ((7910 + 131 * band) // 100)
            rem -= count
            band += 1
        return score // 10
    delta = step - 2000
    bands, rem = delta // 25, delta % 25
    return 14280 + (183 * delta + 25 * bands * (bands - 1) // 2 + bands * rem + 1) // 10


def load_json(path: Path) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


BUILTIN_VARIANTS = [
    ("现用 510+1.88x切", 510, 1.88, "auto"),
    ("去510 1.88x切", 0, 1.88, "auto"),
    ("680+1.88x切", 680, 1.88, "auto"),
    ("510+1.88 不乘切", 510, 1.88, "none"),
    ("680+1.88 不乘切", 680, 1.88, "none"),
    ("510+2.0x切(游戏内)", 510, 2.0, "auto"),
]


def parse_variant(label: str) -> tuple[str, int, float, str]:
    parts = label.split(":")
    if len(parts) != 4:
        raise SystemExit(f"--sim 需要 4 段（标签:固定分:每PT分:切因子）: {label}")
    name, fixed, rate, qie = parts
    return name, int(fixed), float(rate), qie


def qie_factor(spec: str, xiao: bool, zhe: bool) -> float:
    if spec == "auto":
        return 1.10 if zhe else (1.04 if xiao else 1.0)
    if spec == "none":
        return 1.0
    return float(spec)


def analyze(plan: dict, final: dict, variants_spec, pt_mode: str) -> dict:
    pl = plan["plan"]
    cond = plan.get("conditions", {})
    xiao = bool(cond.get("isXiaoQie"))
    zhe = bool(cond.get("isQieZhe"))

    five = [min(s, lim) for s, lim in zip(final["fiveStatus"], final["fiveStatusLimit"])]
    five_pts = [status_point(v) for v in five]
    five_sum = sum(five_pts)

    pt_left = final["skillPt"]
    pt_spent = final.get("skillPtSpent", 0)
    hints = final.get("totalHints", 0)
    pt = pt_left if pt_mode == "remaining" else pt_left + pt_spent
    total_pt = int(pt + 6.5 * hints)  # floor(skill_pt + 6.5*H)

    skills = pl.get("skills", [])
    evolutions = pl.get("evolutions", [])
    sum_score = sum(s["score"] for s in skills)
    sum_cost = sum(s["cost"] for s in skills)
    sum_base = sum(s["baseCost"] for s in skills)
    evo_gain = sum(e["scoreGain"] for e in evolutions)
    predicted = pl["predictedTotalScore"]
    baseline = predicted - five_sum - sum_score

    variants = {}
    for name, fixed, rate, qie in variants_spec:
        q = qie_factor(qie, xiao, zhe)
        variants[name] = five_sum + fixed + int(total_pt * rate * q)

    # 价格公式校验：cost = floor(baseCost * (100 - hintDiscount - qieDiscount) / 100)
    qie_disc = XIAO_QIE_DISCOUNT if xiao else (QIE_ZHE_DISCOUNT if zhe else 0)
    price_hit = price_total = 0
    price_bad = []
    for s in skills:
        if s["cost"] > s["baseCost"]:
            continue  # 组内前置/紫技能组：cost 为组价，不参与单价校验
        price_total += 1
        want = s["baseCost"] * (100 - HINT_DISCOUNT.get(s["hintLevel"], 0) - qie_disc) // 100
        if want == s["cost"]:
            price_hit += 1
        else:
            price_bad.append((s["skillId"], s["baseCost"], s["hintLevel"], s["cost"], want))

    by_hint = defaultdict(lambda: {"n": 0, "base": 0, "cost": 0, "score": 0})
    for s in skills:
        g = by_hint[s["hintLevel"]]
        g["n"] += 1
        g["base"] += s["baseCost"]
        g["cost"] += s["cost"]
        g["score"] += s["score"]

    return {
        "chara_id": final.get("single_mode_chara_id"),
        "uma_id": final.get("umaId"),
        "star": final.get("umaStar"),
        "qie": "qiezhe" if zhe else ("xiaoqie" if xiao else "none"),
        "pt_mode": pt_mode,
        "five": five,
        "five_pts": five_pts,
        "five_sum": five_sum,
        "hints": hints,
        "pt_left": pt_left,
        "pt_spent": pt_spent,
        "pt": pt,
        "total_pt": total_pt,
        "plan": {
            "status": pl.get("status"),
            "predicted": predicted,
            "rank": pl.get("rank"),
            "baseline": baseline,
            "n_skills": len(skills),
            "sum_score": sum_score,
            "sum_cost": sum_cost,
            "sum_base": sum_base,
            "evo_gain": evo_gain,
            "used": pl.get("usedSkillPoints"),
            "remain": pl.get("remainingSkillPoints"),
        },
        "variants": variants,
        "variant_err": {k: predicted - v for k, v in variants.items()},
        "purchase_score": predicted - five_sum - baseline,
        "purchase_per_pt": (predicted - five_sum - baseline) / total_pt if total_pt else 0,
        "skill_per_pt": (predicted - five_sum) / total_pt if total_pt else 0,
        "price_check": {"match": price_hit, "total": price_total, "bad": price_bad},
        "by_hint": {k: dict(v) for k, v in sorted(by_hint.items())},
        "group_priced": sum(1 for s in skills if s["cost"] > s["baseCost"]),
    }


def fmt_report(r: dict) -> str:
    p = r["plan"]
    lines = []
    lines.append(f"## {r['chara_id']}（马娘 {r['uma_id']} / {r['star']}星 / {r['qie']} / PT口径={r['pt_mode']}）")
    lines.append("")
    lines.append(f"- 五维 {r['five']} -> 分 {r['five_pts']} = **{r['five_sum']}**")
    lines.append(f"- 技能点 剩余 {r['pt_left']} + 已花 {r['pt_spent']}；灵感 H={r['hints']}；"
                 f"计分 PT = floor({r['pt']} + 6.5*{r['hints']}) = **{r['total_pt']}**")
    lines.append(f"- 技能插件：status={p['status']} rank={p['rank']} 技能 {p['n_skills']} 个；"
                 f"预算 {p['used']} 用 {p['sum_cost']} 剩 {p['remain']}")
    lines.append(f"- 技能评分 合计={p['sum_score']}（含进化增益 {p['evo_gain']}，基线 {p['baseline']}）")
    lines.append(f"- **最优预测总分 = {p['predicted']}**；购买技能分 = {r['purchase_score']}；"
                 f"购买分/PT = {r['purchase_per_pt']:.3f}；全技能分/PT = {r['skill_per_pt']:.3f}")
    lines.append("")
    lines.append("| 模拟口径 | 模拟总分 | 误差(真值-模拟) |")
    lines.append("|---|---|---|")
    for k, v in r["variants"].items():
        lines.append(f"| {k} | {v} | {r['variant_err'][k]:+d} |")
    lines.append("")
    pc = r["price_check"]
    lines.append(f"- 价格公式校验（cost<=base 的单价行）：{pc['match']}/{pc['total']} 命中")
    if pc["bad"]:
        lines.append(f"  - 不命中：{pc['bad'][:6]}")
    lines.append(f"- 组内前置/高价行（cost>base，不参与单价校验）：{r['group_priced']} 行")
    lines.append("")
    lines.append("| hint等级 | 行数 | 合计base | 合计cost | 合计score |")
    lines.append("|---|---|---|---|---|")
    for lv, g in r["by_hint"].items():
        lines.append(f"| L{lv} | {g['n']} | {g['base']} | {g['cost']} | {g['score']} |")
    lines.append("")
    return "\n".join(lines)


def load_zip(zpath: Path):
    out = []
    with zipfile.ZipFile(zpath) as z:
        names = [n for n in z.namelist() if n.endswith("_skillplan.json")]
        if not names:
            raise SystemExit(f"zip 内没有 skillplan: {zpath}")
        for n in names:
            gid = Path(n).name.split("_")[0]
            final_name = f"{gid}_finalScore.json"
            if final_name not in z.namelist():
                print(f"[跳过] {zpath}:{n}：zip 内无 {final_name}", file=sys.stderr)
                continue
            out.append((json.loads(z.read(n).decode("utf-8")),
                        json.loads(z.read(final_name).decode("utf-8"))))
    return out


def collect(paths):
    items = []
    for raw in paths:
        p = Path(raw)
        if p.suffix.lower() == ".zip":
            items.extend(load_zip(p))
            continue
        if p.is_file():
            if "skillplan" not in p.name:
                raise SystemExit(f"不是 skillplan 文件: {p}")
            gid = p.name.split("_")[0]
            final = p.parent / f"{gid}_finalScore.json"
            if not final.exists():
                print(f"[跳过] {p}：找不到 finalScore", file=sys.stderr)
                continue
            items.append((load_json(p), load_json(final)))
            continue
        plans = sorted(p.glob("game*_skillplan.json"))
        if not plans:
            raise SystemExit(f"目录内没有 game*_skillplan.json: {p}")
        for plan in plans:
            gid = plan.name.split("_")[0]
            final = p / f"{gid}_finalScore.json"
            if not final.exists():
                print(f"[跳过] {plan}：找不到 finalScore", file=sys.stderr)
                continue
            items.append((load_json(plan), load_json(final)))
    return items


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description="分析技能插件最优方案与模拟评分口径（离线重算）")
    ap.add_argument("inputs", nargs="+", help="局包 zip / 局目录 / skillplan.json")
    ap.add_argument("--pt-mode", choices=["remaining", "cumulative"], default="remaining",
                    help="计分 PT 是否含已花技能点（默认 remaining=与现用模拟一致）")
    ap.add_argument("--sim", action="append", default=[], metavar="标签:固定分:每PT分:切因子",
                    help="追加自定义离线口径；切因子 auto/none/数值，可重复")
    ap.add_argument("--json", help="把结果写入 JSON")
    args = ap.parse_args(argv)

    variants = list(BUILTIN_VARIANTS) + [parse_variant(s) for s in args.sim]
    results = [analyze(plan, final, variants, args.pt_mode) for plan, final in collect(args.inputs)]

    for r in results:
        print(fmt_report(r))

    names = [v[0] for v in variants]
    print("\n## 汇总（误差 = 真值 - 模拟，负值 = 模拟偏高）\n")
    header = "| 局 | 五维分 | 计分PT | 最优预测 | " + " | ".join(names) + " |"
    sep = "|---" * (4 + len(names)) + "|"
    print(header)
    print(sep)
    for r in results:
        cells = [str(r["chara_id"]), str(r["five_sum"]), str(r["total_pt"]), str(r["plan"]["predicted"])]
        cells += [f"{r['variants'][n]} ({r['variant_err'][n]:+d})" for n in names]
        print("| " + " | ".join(cells) + " |")

    if args.json:
        out = Path(args.json)
        out.parent.mkdir(parents=True, exist_ok=True)
        with open(out, "w", encoding="utf-8") as f:
            json.dump(results, f, ensure_ascii=False, indent=2)
        print(f"\n已写入 {args.json}")


if __name__ == "__main__":
    main(sys.argv[1:])
