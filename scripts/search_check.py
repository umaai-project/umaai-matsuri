#!/usr/bin/env python3
"""搜索档（FlatSearch）配对验收：同场景、同种子，现行档 vs 候选片段，逐局跑 `bench game`。

用法（仓根执行，依赖 numpy（经 ga_trainer）；先 `cargo build --release -p matsuri-tools --bin bench`）：
    python scripts/search_check.py --out logs/ga/search3 \
        --check speed=decks/speed_build.csv,logs/ga/run2/ok_g0269.toml \
        --check stamina=decks/stamina_build.csv,logs/ga/run3/run3_stamina/ok_g0109.toml \
        [--search-n 8192] [--seed 30000000] [--pairs 400] [--workers 8] [--rayon 1] [--max-minutes 20]

口径：
- 每个 check = 夹具（主卡组 + `*_shadow` 影子卡组）+ 候选片段；现行档 = 不指定档名、按构成自动选
  （与玩家实际一致）；候选 = 片段转成 variant、`--deck-profile` 选。
- 第 i 对用种子 seed + i，场景按 i 轮转：马娘（`--umas`，去掉卡组里带本人卡的）× 卡组（主 / 影子）
  × 种马（ga_trainer.SIRES），各维度边际均衡；同一对的两臂同种子同场景（CRN）。
- 换卡 / 种马走 cwd 覆盖层 `[sim]`，马娘走 `--uma`；按种子轮转各 check，限时截断时各 check 对数相近。
- 逐局追加 `games.csv`（`--sync` 同步、重跑即续跑）；`summary.json` = 每个 check 的总体配对 Δ / SE / t
  与按马娘 / 卡组 / 种马拆分的 Δ。引擎并行与串行结果一致，`--workers × --rayon` 只影响吞吐。
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import time
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ga_trainer as ga  # noqa: E402

EXE_NAME = "bench.exe" if sys.platform == "win32" else "bench"
DEFAULT_EXE = ga.ROOT / "target" / "release" / EXE_NAME
FIELDS = ["check", "arm", "seed", "uma", "deck", "sire", "score", "pt", "turn", "elapsed_us"]
ARMS = ("current", "new")
DEFAULT_UMAS = "105902,102102,100101,112901,114101,106301"


@dataclass(frozen=True)
class Check:
    """一项配对验收：名字 + 卡组（主 / 影子）+ 候选 variant 档名。"""

    name: str
    decks: dict[str, ga.Deck]
    profile: str
    umas: list[int]


@dataclass(frozen=True)
class Scene:
    """一对局面的固定条件。"""

    uma: int
    deck: str
    sire: str


def scene_of(index: int, umas: list[int], decks: list[str]) -> Scene:
    """第 index 对的场景：马娘最快轮转，其次卡组，再次种马（混合进制，边际均衡）。"""
    sires = list(ga.SIRES)
    return Scene(umas[index % len(umas)], decks[(index // len(umas)) % len(decks)],
                 sires[(index // (len(umas) * len(decks))) % len(sires)])


def allowed_umas(umas: list[int], decks: list[ga.Deck]) -> list[int]:
    """去掉卡组里带本人卡的马娘（编成规则：担当马娘本人不能入卡组；角色 id = 马娘 id // 100）。"""
    with ga.CARD_DB.open(encoding="utf-8") as fh:
        db = json.load(fh)
    charas = {db[str(int(card) // 10)]["charaId"] for deck in decks for card in deck.cards.split(",")}
    return [uma for uma in umas if uma // 100 not in charas]


def prepare(args, out: Path, umas: list[int]) -> tuple[Path, list[Check]]:
    """写配置（默认层 + 各候选 variant）与每个 check × 卡组 × 种马的覆盖层目录。"""
    parts, checks = [], []
    for spec in args.check:
        name, _, paths = spec.partition("=")
        fixture, _, snippet = paths.partition(",")
        if not (name and fixture and snippet) or any(c.name == name for c in checks):
            sys.exit(f"--check 格式为 名字=夹具.csv,片段.toml 且名字不重复：{spec}")
        main, shadow = ga.load_decks(Path(fixture))
        if shadow is None:
            sys.exit(f"{fixture} 没有 *_shadow 影子行")
        decks = {"main": main, "shadow": shadow}
        profile = f"chk_{name}"
        parts.append(ga.deck_toml(profile, ga.load_point(Path(snippet)), main.comp))
        for kind, deck in decks.items():
            for sire, (blue, extra) in ga.SIRES.items():
                cwd = out / f"{name}_{kind}_{sire}"
                cwd.mkdir(parents=True, exist_ok=True)
                (cwd / "ai_config.toml").write_text(
                    f"[sim]\ncards = [{deck.cards.replace(',', ', ')}]\nblue_count = {blue}\nextra_count = {extra}\n",
                    encoding="utf-8")
        usable = allowed_umas(umas, list(decks.values()))
        if usable != umas:
            print(f"# {name}：卡组含本人卡，排除马娘 {sorted(set(umas) - set(usable))}", flush=True)
        checks.append(Check(name, decks, profile, usable))
    cfg = out / "check_cfg.toml"
    cfg.write_text(ga.BASE_CONFIG.read_text(encoding="utf-8") + "".join(parts), encoding="utf-8")
    return cfg, checks


def play(args, out: Path, cfg: Path, check: Check, arm: str, seed: int, scene: Scene) -> dict:
    """跑一局，返回 games.csv 的一行。"""
    cmd = [str(args.exe), "game", "--matsuri-config", str(cfg), "--search-n", str(args.search_n),
           "--games", "1", "--warmup", "0", "--seed", str(seed), "--uma", str(scene.uma)]
    if arm == "new":
        cmd += ["--deck-profile", check.profile]
    proc = subprocess.run(cmd, cwd=out / f"{check.name}_{scene.deck}_{scene.sire}",
                          env=dict(os.environ, RAYON_NUM_THREADS=str(args.rayon)),
                          capture_output=True, text=True, encoding="utf-8",
                          creationflags=ga.BELOW_NORMAL if sys.platform == "win32" else 0)
    line = next((l for l in proc.stdout.splitlines() if l.startswith(f"{seed},")), None)
    if proc.returncode != 0 or line is None:
        raise RuntimeError(f"bench 失败（{check.name}/{arm}/{seed}/{scene}）：\n{proc.stdout}\n{proc.stderr}")
    _, score, pt, turn, elapsed = line.split(",")
    return {"check": check.name, "arm": arm, "seed": seed, "uma": scene.uma, "deck": scene.deck,
            "sire": scene.sire, "score": int(score), "pt": int(pt), "turn": int(turn), "elapsed_us": int(elapsed)}


def read_rows(path: Path) -> list[dict]:
    """读已完成的局（续跑用）。"""
    if not path.exists():
        return []
    text = {"check", "arm", "deck", "sire"}
    with path.open(encoding="utf-8") as fh:
        return [{k: (v if k in text else int(v)) for k, v in row.items()} for row in csv.DictReader(fh)]


def paired(diff: list[int]) -> dict:
    """一组配对差的均值 / SE / t。"""
    mean = statistics.mean(diff)
    se = statistics.stdev(diff) / math.sqrt(len(diff)) if len(diff) > 1 else 0.0
    return {"n": len(diff), "delta": mean, "se": se, "t": mean / se if se > 0 else 0.0}


def summarize(rows: list[dict], checks: list[Check]) -> dict:
    """每个 check：总体配对 Δ（new − current）+ 按马娘 / 卡组 / 种马拆分。"""
    report = {}
    for check in checks:
        by: dict[int, dict[str, dict]] = {}
        for row in rows:
            if row["check"] == check.name:
                by.setdefault(row["seed"], {})[row["arm"]] = row
        pairs = [v for v in by.values() if len(v) == 2]
        if len(pairs) < 2:
            report[check.name] = {"n": len(pairs)}
            continue
        diff = [p["new"]["score"] - p["current"]["score"] for p in pairs]
        entry = paired(diff) | {
            "wins": sum(d > 0 for d in diff), "losses": sum(d < 0 for d in diff),
            "current": statistics.mean(p["current"]["score"] for p in pairs),
            "new": statistics.mean(p["new"]["score"] for p in pairs),
            "pt_delta": statistics.mean(p["new"]["pt"] - p["current"]["pt"] for p in pairs),
            "secs_per_game": statistics.mean(r["elapsed_us"] for p in pairs for r in p.values()) / 1e6,
        }
        for key in ("uma", "deck", "sire"):
            groups: dict[str, list[int]] = {}
            for p, d in zip(pairs, diff):
                groups.setdefault(str(p["new"][key]), []).append(d)
            entry[f"by_{key}"] = {k: paired(v) for k, v in sorted(groups.items())}
        report[check.name] = entry
    return report


def sync(args, out: Path, names: list[str]) -> None:
    """复制结果到 `--sync`。"""
    if args.sync is not None:
        args.sync.mkdir(parents=True, exist_ok=True)
        for name in names:
            if (out / name).exists():
                shutil.copy2(out / name, args.sync / name)


def show(report: dict) -> None:
    """打印总体表 + 拆分。"""
    print(f"{'check':8s} {'对数':>4s} | {'现行':>8s} {'候选':>8s} | {'配对 Δ':>16s} | {'t':>6s} | 胜/负 | PT Δ | 秒/局")
    for name, r in report.items():
        if "delta" not in r:
            print(f"{name:8s} {r['n']:4d} | 配对不足")
            continue
        print(f"{name:8s} {r['n']:4d} | {r['current']:8.0f} {r['new']:8.0f} | {r['delta']:+8.1f} ± {r['se']:5.1f} | "
              f"{r['t']:6.2f} | {r['wins']}/{r['losses']} | {r['pt_delta']:+.0f} | {r['secs_per_game']:.0f}")
        for key in ("uma", "deck", "sire"):
            cells = "  ".join(f"{k} {v['delta']:+.0f}(n{v['n']})" for k, v in r[f"by_{key}"].items())
            print(f"    {key:5s} {cells}")


def main() -> None:
    """命令行入口。"""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--check", action="append", required=True, help="名字=夹具.csv,片段.toml（可多次）")
    parser.add_argument("--sync", type=Path, default=None)
    parser.add_argument("--exe", type=Path, default=DEFAULT_EXE)
    parser.add_argument("--umas", default=DEFAULT_UMAS, help="轮转的马娘（逗号分隔）")
    parser.add_argument("--search-n", type=int, default=8192, help="与线上默认一致")
    parser.add_argument("--seed", type=int, default=30_000_000)
    parser.add_argument("--pairs", type=int, default=400, help="每个 check 的配对数上限")
    parser.add_argument("--workers", type=int, default=8, help="并发 bench 进程数")
    parser.add_argument("--rayon", type=int, default=1, help="每个进程的搜索线程数")
    parser.add_argument("--max-minutes", type=float, default=20.0, help="到时不再派新局（在跑的局跑完）")
    args = parser.parse_args()
    if not args.exe.exists():
        sys.exit(f"找不到 {args.exe}；先 cargo build --release -p matsuri-tools --bin bench")
    umas = [int(u) for u in args.umas.split(",")]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if args.sync is not None and not (out / "games.csv").exists() and (args.sync / "games.csv").exists():
        shutil.copy2(args.sync / "games.csv", out / "games.csv")
    cfg, checks = prepare(args, out, umas)
    rows = read_rows(out / "games.csv")
    done = {(r["check"], r["arm"], r["seed"]) for r in rows}
    jobs = [(c, arm, args.seed + i, scene_of(i, c.umas, ["main", "shadow"]))
            for i in range(args.pairs) for c in checks for arm in ARMS if (c.name, arm, args.seed + i) not in done]
    print(f"# search_n={args.search_n} checks={[c.name for c in checks]} umas={umas} 已有 {len(rows)} 局，"
          f"待跑 {len(jobs)} 局 workers={args.workers} rayon={args.rayon} 限时 {args.max_minutes} 分", flush=True)
    games = out / "games.csv"
    if not games.exists():
        games.write_text(",".join(FIELDS) + "\n", encoding="utf-8")
    started, queue, running = time.time(), iter(jobs), {}
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        def submit() -> None:
            while len(running) < args.workers and (time.time() - started) / 60 < args.max_minutes:
                job = next(queue, None)
                if job is None:
                    return
                running[pool.submit(play, args, out, cfg, *job)] = job
        submit()
        while running:
            finished, _ = wait(running, return_when=FIRST_COMPLETED)
            for future in finished:
                running.pop(future)
                row = future.result()
                rows.append(row)
                with games.open("a", encoding="utf-8") as fh:
                    fh.write(",".join(str(row[k]) for k in FIELDS) + "\n")
                print(f"{(time.time() - started) / 60:5.1f} 分  {row['check']:8s} {row['arm']:7s} {row['seed']}  "
                      f"{row['uma']} {row['deck']:6s} {row['sire']}  {row['score']}  {row['elapsed_us'] / 1e6:.0f}s",
                      flush=True)
            sync(args, out, ["games.csv"])
            submit()
    report = summarize(rows, checks)
    (out / "summary.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    sync(args, out, ["games.csv", "summary.json"])
    show(report)


if __name__ == "__main__":
    main()
