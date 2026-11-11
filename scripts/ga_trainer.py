#!/usr/bin/env python3
"""手写训练员的 CMA-ES 联合搜索（官方评分口径；多马娘 × 多种马综合；一次调一个卡组档）。

用法（仓根执行，依赖 numpy；先 `cargo build --release -p matsuri-tools --bin cardtest`）：
    python scripts/ga_trainer.py search  --out benchmark/_local/ga/run3 [--decks benchmark/decks/stamina_build.csv]
        [--profile stamina_build] [--start snippet.toml] [--seed-offset 10000000] [--max-minutes 100]
    python scripts/ga_trainer.py confirm --out benchmark/_local/ga/run3 （同上参数）[--games 2500]

口径：
- `--decks` 夹具第一行 = 被调卡组，`*_shadow` 行 = 影子卡组（只在验收时看换卡后是否同向）；
  `--profile` = 产出片段的档名（`default` 只用于默认构成 21101，其余构成各写独立档，互不影响）。
- 场景 = 马娘 × 种马（蓝因子 / 剧本因子，各自一个 cwd 覆盖层目录）；适应度 = 场景均值，
  每场景扣罚 `--friend-penalty × (1 − 友人出行走满 5 段的局占比)`（硬需求：每局走完）。
- 搜索空间：36 个连续 / 整数旋钮 + pt_favor_rate + 两个布尔开关（阈值 0.5）
  + 融合（5 个随机键 → 基础教诲排列 → 601/701/801/901/1001）+ 6 个窗口的教诲落位（各 5 个随机键）。
- 每代第一行是起点（`--start`，缺省 = 默认层）作参照；同代所有候选同种子（CRN）。
- 种子三段零重叠（再整体加 `--seed-offset`，不同轮次互不复用）：
  选择 5,000,000 + 代号 × 局数 / 复核 6,000,000 / 验收 7,000,000。
- 每代把状态与日志同步到 `--sync`（例如 Drive），`--resume` 从断点续跑；`--max-minutes` 限时。
- 晋级规则（看结果前固定）：验收只认复核分最高者；门禁 = 对现值与对起点都 z > 2.58、
  友人走满率不低于两者 − `--friend-tol`、影子卡组对两者 Δ 均为正。
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import shutil
import subprocess
import sys
import time
import tomllib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
BASE_CONFIG = ROOT / "data" / "matsuri_config.toml"
EXE_NAME = "cardtest.exe" if sys.platform == "win32" else "cardtest"
DEFAULT_EXE = ROOT / "target" / "release" / EXE_NAME
DEFAULT_DECKS = ROOT / "benchmark" / "decks" / "speed_build.csv"
CARD_DB = ROOT / "data" / "cardDB.json"
REF = "current"  # 不匹配任何档名 → cardtest 按卡组构成选档（= 该卡组在默认层的现行档）
START = "ga_start"
SEED_SELECT = 5_000_000
SEED_RECHECK = 6_000_000
SEED_CONFIRM = 7_000_000
UMAS = [105902, 102102, 100101]
SIRES = {  # 种马：蓝因子 [速耐力根智] + 剧本因子 [速耐力根智 + 技能点]
    "b18_root": ([18, 0, 0, 0, 0], [10, 10, 120, 140, 0, 100]),
    "b18_none": ([18, 0, 0, 0, 0], [10, 10, 120, 0, 0, 100]),
    "b99_root": ([9, 9, 0, 0, 0], [10, 10, 120, 140, 0, 100]),
    "b99_none": ([9, 9, 0, 0, 0], [10, 10, 120, 0, 0, 100]),
}
BASES = [101, 201, 301, 401, 501]
FUSION_TARGETS = [601, 701, 801, 901, 1001]
WINDOWS = ["t2", "t11", "t23", "t35", "t47", "t59"]
SYNC_FILES = ["knobs.json", "gens.jsonl", "recheck.jsonl", "state.json", "state.npz", "final.json"]
BELOW_NORMAL = 0x00004000


@dataclass(frozen=True)
class Knob:
    """一个训练员旋钮：TOML 路径（段, 键, 数组下标）+ 搜索区间 + 类型（float / int / bool）。"""

    section: str
    key: str
    index: int | None
    lo: float
    hi: float
    kind: str = "float"

    @property
    def label(self) -> str:
        """日志用短名。"""
        name = f"{self.section}.{self.key}" if self.section else self.key
        return f"{name}[{self.index}]" if self.index is not None else name


# 区间：有校验上界取校验上界（如 endgame.keiko ≤ 1000）；其余为 ga-r2 现值的约 2–6 倍
KNOBS: list[Knob] = [
    Knob("", "pt_favor_rate", None, 1.0, 3.0),
    Knob("", "self_race", None, 0, 1, "bool"),
    Knob("", "friend_outing_early", None, 0, 1, "bool"),
    Knob("", "rest_vital", None, 20, 90, "int"),
    Knob("", "checkpoint_margin", None, 0, 20, "int"),
    Knob("", "act_level_target", None, 0, 10, "int"),
    Knob("", "free_race_urgency", None, 0.0, 4.0),
    *[Knob("", "status_weights", i, 0.0, hi) for i, hi in enumerate([2.0, 3.0, 2.5, 1.5, 6.0])],
    Knob("weights", "vital", None, 0.0, 9.0),
    Knob("weights", "motivation", None, 0.0, 70.0),
    Knob("weights", "friendship", None, 0.0, 3.4),
    Knob("weights", "hint", None, 0.0, 7.0),
    Knob("weights", "gauge", None, 0.0, 84.0),
    Knob("weights", "act_level", None, 0.0, 78.0),
    Knob("weights", "checkpoint", None, 0.0, 3300.0),
    Knob("weights", "fat", None, 0.0, 325.0),
    Knob("advice", "card", None, 0.0, 4.3),
    Knob("advice", "npc", None, 0.0, 1.3),
    Knob("advice", "train", None, 0.0, 1.0),
    Knob("advice", "keiko", None, 0.0, 4.6),
    Knob("advice", "level", None, 0.0, 1.0),
    Knob("advice", "milestone", None, 0.0, 1.0),
    Knob("advice", "route", None, 0.0, 1500.0),
    Knob("dynamic", "status_catchup", None, 0.0, 4.0),
    Knob("dynamic", "act_catchup", None, 0.0, 2.0),
    Knob("dynamic", "vital_decay", None, 0.0, 1.0),
    Knob("dynamic", "vital_idle", None, 0.0, 4.0),
    Knob("dynamic", "keiko_tempo", None, 0.0, 4.0),
    Knob("overflow", "margin", None, 0.0, 0.5),
    Knob("overflow", "tilt", None, 0.0, 4.0),
    Knob("overflow", "keep", None, 0.0, 2.0),
    Knob("endgame", "keiko", None, 0.0, 1000.0),
    Knob("endgame", "scarcity", None, 0.0, 4.0),
    Knob("endgame", "level", None, 0.0, 500.0),
    Knob("endgame", "normal_penalty", None, 0.0, 1000.0),
]
N_KNOBS = len(KNOBS)
DIM = N_KNOBS + len(BASES) * (1 + len(WINDOWS))


@dataclass
class Point:
    """物理空间里的一个完整参数点：旋钮值 + 融合排列 + 各窗口教诲落位。"""

    knobs: list[float]
    fusion: list[int]
    advice: dict[str, list[int]]

    def to_json(self) -> dict:
        """日志 / 断点用的可序列化形式。"""
        return {"knobs": {k.label: v for k, v in zip(KNOBS, self.knobs)},
                "fusion": self.fusion, "advice": self.advice}

    @staticmethod
    def from_json(data: dict) -> "Point":
        """`to_json` 的逆。"""
        return Point([float(data["knobs"][k.label]) for k in KNOBS], list(data["fusion"]),
                     {w: list(r) for w, r in data["advice"].items()})


@dataclass(frozen=True)
class Deck:
    """测量夹具的一行：行名 + 6 张卡（idrank 逗号串，含友人）+ 构成 [速, 耐, 力, 根, 智]。"""

    name: str
    cards: str
    comp: list[int]

    @property
    def code(self) -> str:
        """构成码（如 12101）。"""
        return "".join(map(str, self.comp))


def load_decks(path: Path) -> tuple[Deck, Deck | None]:
    """读 `benchmark/decks/<档>.csv`：第一行 = 主卡组，首个 `*_shadow` 行 = 影子卡组（可无）。

    构成按 cardDB 现算，与 comp 列不符直接报错（防止片段写错构成、档挂到别的卡组上）。
    """
    with CARD_DB.open(encoding="utf-8") as fh:
        db = json.load(fh)
    with path.open(encoding="utf-8") as fh:
        rows = list(csv.DictReader(line for line in fh if not line.startswith("#")))

    def deck(row: dict) -> Deck:
        cards = [row[f"slot{i}"] for i in range(5)] + [row["friend"]]
        comp = [0] * 5
        for card in cards:
            kind = db[str(int(card) // 10)]["cardType"]
            if kind < 5:
                comp[kind] += 1
        found = Deck(row["name"], ",".join(cards), comp)
        if row["comp"] != found.code:
            sys.exit(f"{path}: {row['name']} 的 comp 列 {row['comp']} ≠ 按卡算出的 {found.code}")
        return found

    main = deck(rows[0])
    shadow = next((deck(r) for r in rows[1:] if r["name"].endswith("_shadow")), None)
    if shadow is not None and shadow.comp != main.comp:
        sys.exit(f"{path}: 影子卡组构成 {shadow.code} ≠ 主卡组 {main.code}")
    return main, shadow


def fixed_keys() -> dict[str, dict]:
    """默认层 trainer 里不在搜索空间的键：原样写进片段，让新档自足、不随 default 以后的改动漂移。"""
    with BASE_CONFIG.open("rb") as fh:
        trainer = tomllib.load(fh)["decks"]["default"]["trainer"]
    covered = {(k.section, k.key) for k in KNOBS}
    found: dict[str, dict] = {}
    for key, value in trainer.items():
        if isinstance(value, dict):
            for sub, item in value.items():
                if (key, sub) not in covered:
                    found.setdefault(key, {})[sub] = item
        elif ("", key) not in covered:
            found.setdefault("", {})[key] = value
    return found


FIXED = fixed_keys()


def merge(base: dict, over: dict) -> dict:
    """TOML 表深合并（与 deck_config 的继承口径一致：表递归、其余整体覆盖）。"""
    out = dict(base)
    for key, value in over.items():
        out[key] = merge(out[key], value) if isinstance(value, dict) and isinstance(out.get(key), dict) else value
    return out


def load_point(start: Path | None) -> Point:
    """默认层 `[decks.default]`，再叠加片段（`confirm` 产出的 `[decks.<档>.*]`，只含一个档）。"""
    with BASE_CONFIG.open("rb") as fh:
        deck = tomllib.load(fh)["decks"]["default"]
    if start is not None:
        with start.open("rb") as fh:
            tables = tomllib.load(fh)["decks"]
        if len(tables) != 1:
            sys.exit(f"{start}: 片段必须只含一个 [decks.<档>] 表（实际 {list(tables)}）")
        deck = merge(deck, next(iter(tables.values())))
    trainer = deck["trainer"]
    knobs = []
    for knob in KNOBS:
        table = trainer[knob.section] if knob.section else trainer
        value = table[knob.key]
        knobs.append(float(value[knob.index] if knob.index is not None else value))
    fusion_map = {pair["target"]: pair["base"] for pair in deck["fusion"]["year2"] + deck["fusion"]["year3"]}
    fusion = [fusion_map[target] for target in FUSION_TARGETS]
    advice = {w: list(deck["advice"][w]) for w in WINDOWS}
    return Point(knobs, fusion, advice)


def perm_keys(order: list[int]) -> list[float]:
    """排列 → 随机键（键按 BASES 顺序；位置 0..4 映射到 0.3..0.7，留出搜索余量）。"""
    return [0.3 + 0.1 * order.index(base) for base in BASES]


def keys_perm(keys: np.ndarray) -> list[int]:
    """随机键 → 排列（按键升序排 BASES）。"""
    return [BASES[i] for i in np.argsort(keys, kind="stable")]


def encode(point: Point) -> np.ndarray:
    """物理点 → 归一化向量 [0,1]^DIM。"""
    z = []
    for knob, value in zip(KNOBS, point.knobs):
        z.append(0.75 if value >= 0.5 else 0.25) if knob.kind == "bool" else z.append((value - knob.lo) / (knob.hi - knob.lo))
    z += perm_keys(point.fusion)
    for window in WINDOWS:
        z += perm_keys(point.advice[window])
    return np.array(z)


def decode(z: np.ndarray) -> Point:
    """归一化向量 → 物理点（截断、整数取整、布尔阈值 0.5、随机键排序）。"""
    z = np.clip(z, 0.0, 1.0)
    knobs = []
    for knob, value in zip(KNOBS, z[:N_KNOBS]):
        if knob.kind == "bool":
            knobs.append(1.0 if value >= 0.5 else 0.0)
        else:
            phys = knob.lo + value * (knob.hi - knob.lo)
            knobs.append(float(round(phys)) if knob.kind == "int" else float(phys))
    rest = z[N_KNOBS:].reshape(1 + len(WINDOWS), len(BASES))
    advice = {w: keys_perm(rest[i + 1]) for i, w in enumerate(WINDOWS)}
    return Point(knobs, keys_perm(rest[0]), advice)


def fmt(knob: Knob, value: float) -> str:
    """TOML 字面量。"""
    if knob.kind == "bool":
        return "true" if value >= 0.5 else "false"
    return str(int(value)) if knob.kind == "int" else repr(round(float(value), 6))


def deck_toml(name: str, point: Point, comp: list[int], variant: bool = True) -> str:
    """一个点 → TOML 段：variant 档（扫参用）或贴回配置的完整片段。

    片段：`default` 只写子表（贴回默认层）；其余档带表头与 comp（新增 / 整段替换该档）。
    搜索空间外的 trainer 键按默认层现值写全（见 [`fixed_keys`]）。
    """
    head = f"decks.{name}"
    if variant:
        lines = [f"\n[{head}]", f"comp = {comp}", "variant = true"]
    else:
        lines = [] if name == "default" else [f"[{head}]", f"comp = {comp}"]
    pairs = [f"{{ base = {b}, target = {t} }}" for b, t in zip(point.fusion, FUSION_TARGETS)]
    lines += [f"\n[{head}.fusion]", f"year2 = [{', '.join(pairs[:2])}]", f"year3 = [{', '.join(pairs[2:])}]"]
    lines.append(f"\n[{head}.advice]")
    lines += [f"{w} = [{', '.join(map(str, point.advice[w]))}]" for w in WINDOWS]
    sections: dict[str, list[str]] = {}
    weights = [0.0] * 5
    for knob, value in zip(KNOBS, point.knobs):
        if knob.key == "status_weights":
            weights[knob.index] = float(value)
        else:
            sections.setdefault(knob.section, []).append(f"{knob.key} = {fmt(knob, value)}")
    for section, items in FIXED.items():
        sections.setdefault(section, []).extend(f"{key} = {json.dumps(value)}" for key, value in items.items())
    lines.append(f"\n[{head}.trainer]")
    lines += sections.pop("", [])
    lines.append("status_weights = [" + ", ".join(repr(round(v, 6)) for v in weights) + "]")
    for section, items in sections.items():
        lines.append(f"\n[{head}.trainer.{section}]")
        lines += items
    return "\n".join(lines) + "\n"


def write_sires(out: Path) -> dict[str, Path]:
    """每套种马一个覆盖层目录（cwd 下同名 `ai_config.toml` 只改蓝因子 / 剧本因子）。"""
    dirs = {}
    for name, (blue, extra) in SIRES.items():
        path = out / f"sire_{name}"
        path.mkdir(parents=True, exist_ok=True)
        (path / "ai_config.toml").write_text(
            f"[sim]\nblue_count = {blue}\nextra_count = {extra}\n", encoding="utf-8")
        dirs[name] = path
    return dirs


def scenarios() -> list[tuple[int, str]]:
    """全部场景（马娘, 种马）。"""
    return [(uma, sire) for uma in UMAS for sire in SIRES]


def evaluate(args, out: Path, tag: str, rows: list[tuple[str, Point | None]], games: int, seed: int,
             ref: str, deck: Deck, correct: str = "none") -> dict[tuple[int, str], dict[str, dict[str, float]]]:
    """一批点在全部场景下各用 `deck` 跑一次 cardtest（第一行 = ref；名为 REF 的行不写档 = 现行档）。

    返回 {场景: {名: 指标}}。
    """
    out = out.resolve()  # cardtest 的 cwd 是覆盖层目录，路径必须是绝对的
    cfg = out / f"{tag}_cfg.toml"
    decks = out / f"{tag}_decks.csv"
    text = BASE_CONFIG.read_text(encoding="utf-8") + "".join(
        deck_toml(n, p, deck.comp) for n, p in rows if n != REF)
    cfg.write_text(text, encoding="utf-8")
    lines = ["name,comp,pick,slot0,slot1,slot2,slot3,slot4,friend"]
    names = [ref] + [n for n, _ in rows if n != ref]
    lines += [f"{n},{deck.code},ga,{deck.cards}" for n in names]
    decks.write_text("\n".join(lines) + "\n", encoding="utf-8")
    threads = max(1, args.threads // args.jobs)

    def one(scene: tuple[int, str]) -> tuple[tuple[int, str], dict]:
        uma, sire = scene
        csv_path = out / f"{tag}_{uma}_{sire}.csv"
        cmd = [str(args.exe), "run", "--decks", str(decks), "--matsuri-config", str(cfg),
               "--games", str(games), "--seed", str(seed), "--threads", str(threads),
               "--uma", str(uma), "--correct", correct, "--out", str(csv_path)]
        proc = subprocess.run(cmd, cwd=args.sire_dirs[sire], capture_output=True, text=True,
                              encoding="utf-8", creationflags=BELOW_NORMAL if sys.platform == "win32" else 0)
        if proc.returncode != 0:
            raise RuntimeError(f"cardtest 失败（{uma}/{sire}）：\n{proc.stdout}\n{proc.stderr}")
        keys = ("score_mean", "delta", "ci95", "p_adj", "pt_mean", "friend_full", "friend_mean",
                "five0", "five1", "five2", "five3", "five4")
        with csv_path.open(encoding="utf-8") as fh:
            return scene, {row["name"]: {k: float(row[k]) for k in keys} for row in csv.DictReader(fh)}

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        return dict(pool.map(one, scenarios()))


def fitness(args, res: dict, name: str) -> float:
    """场景均分 − 友人未走满扣罚。"""
    return float(np.mean([r[name]["score_mean"] - args.friend_penalty * (1 - r[name]["friend_full"])
                          for r in res.values()]))


def summary(args, res: dict, name: str) -> dict:
    """对 ref 的综合配对 Δ（场景均值；SE 由各场景 ci95 反推、场景间独立）+ 友人 / 五维 / PT 均值。"""
    rows = [r[name] for r in res.values()]
    delta = float(np.mean([r["delta"] for r in rows]))
    se = math.sqrt(sum((r["ci95"] / 1.96) ** 2 for r in rows)) / len(rows)
    return {
        "delta": delta, "se": se, "fitness": fitness(args, res, name),
        "friend_full": float(np.mean([r["friend_full"] for r in rows])),
        "pt": float(np.mean([r["pt_mean"] for r in rows])),
        "five": [float(np.mean([r[f"five{i}"] for r in rows])) for i in range(5)],
    }


class Cma:
    """标准 CMA-ES（Hansen 2016 教程式；最大化；可存取断点）。"""

    def __init__(self, mean: np.ndarray, sigma: float, lam: int, rng: np.random.Generator):
        n = len(mean)
        self.n, self.lam, self.mean, self.sigma, self.rng = n, lam, mean.copy(), sigma, rng
        self.mu = lam // 2
        w = np.log(self.mu + 0.5) - np.log(np.arange(1, self.mu + 1))
        self.w = w / w.sum()
        self.mueff = 1.0 / np.sum(self.w**2)
        self.cc = (4 + self.mueff / n) / (n + 4 + 2 * self.mueff / n)
        self.cs = (self.mueff + 2) / (n + self.mueff + 5)
        self.c1 = 2 / ((n + 1.3) ** 2 + self.mueff)
        self.cmu = min(1 - self.c1, 2 * (self.mueff - 2 + 1 / self.mueff) / ((n + 2) ** 2 + self.mueff))
        self.damps = 1 + 2 * max(0.0, math.sqrt((self.mueff - 1) / (n + 1)) - 1) + self.cs
        self.chi_n = math.sqrt(n) * (1 - 1 / (4 * n) + 1 / (21 * n * n))
        self.pc, self.ps = np.zeros(n), np.zeros(n)
        self.C, self.B, self.D = np.eye(n), np.eye(n), np.ones(n)
        self.gen = 0

    def ask(self) -> list[np.ndarray]:
        """采样 λ 个点并截断到 [0,1]（修复后的点同时用于更新）。"""
        return [np.clip(self.mean + self.sigma * (self.B @ (self.D * self.rng.standard_normal(self.n))), 0.0, 1.0)
                for _ in range(self.lam)]

    def tell(self, xs: list[np.ndarray], fits: list[float]) -> None:
        """按适应度降序更新均值 / 进化路径 / 协方差 / 步长。"""
        sel = np.array([xs[i] for i in np.argsort(fits)[::-1][: self.mu]])
        old = self.mean
        self.mean = self.w @ sel
        yw = (self.mean - old) / self.sigma
        inv_sqrt = self.B @ np.diag(1 / self.D) @ self.B.T
        self.ps = (1 - self.cs) * self.ps + math.sqrt(self.cs * (2 - self.cs) * self.mueff) * (inv_sqrt @ yw)
        self.gen += 1
        norm = np.linalg.norm(self.ps) / math.sqrt(1 - (1 - self.cs) ** (2 * self.gen))
        hsig = float(norm / self.chi_n < 1.4 + 2 / (self.n + 1))
        self.pc = (1 - self.cc) * self.pc + hsig * math.sqrt(self.cc * (2 - self.cc) * self.mueff) * yw
        ys = (sel - old) / self.sigma
        self.C = ((1 - self.c1 - self.cmu) * self.C
                  + self.c1 * (np.outer(self.pc, self.pc) + (1 - hsig) * self.cc * (2 - self.cc) * self.C)
                  + self.cmu * ((ys.T * self.w) @ ys))
        self.sigma *= math.exp((self.cs / self.damps) * (np.linalg.norm(self.ps) / self.chi_n - 1))
        self.C = (self.C + self.C.T) / 2
        eig, self.B = np.linalg.eigh(self.C)
        self.D = np.sqrt(np.maximum(eig, 1e-20))

    def save(self, out: Path) -> None:
        """断点：数组进 npz，标量与随机数状态进 json。"""
        np.savez(out / "state.npz", mean=self.mean, pc=self.pc, ps=self.ps, C=self.C, B=self.B, D=self.D)
        (out / "state.json").write_text(json.dumps(
            {"sigma": self.sigma, "gen": self.gen, "lam": self.lam, "rng": self.rng.bit_generator.state}),
            encoding="utf-8")

    def load(self, out: Path) -> None:
        """读回 `save` 的断点。"""
        arrays = np.load(out / "state.npz")
        self.mean, self.pc, self.ps = arrays["mean"], arrays["pc"], arrays["ps"]
        self.C, self.B, self.D = arrays["C"], arrays["B"], arrays["D"]
        meta = json.loads((out / "state.json").read_text(encoding="utf-8"))
        self.sigma, self.gen = meta["sigma"], meta["gen"]
        self.rng.bit_generator.state = meta["rng"]


def sync(args, out: Path) -> None:
    """把断点与日志复制到 `--sync`（缺省不同步）。"""
    if args.sync is None:
        return
    args.sync.mkdir(parents=True, exist_ok=True)
    for name in SYNC_FILES:
        if (out / name).exists():
            shutil.copy2(out / name, args.sync / name)


def restore(args, out: Path) -> None:
    """续跑：工作目录缺断点时从 `--sync` 取回。"""
    if args.sync is not None and not (out / "state.json").exists():
        for name in SYNC_FILES:
            if (args.sync / name).exists():
                shutil.copy2(args.sync / name, out / name)


def log(path: Path, record: dict) -> None:
    """追加一行 JSON 日志。"""
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, ensure_ascii=False) + "\n")


def recheck(args, out: Path, gen: int, point: Point, start: Point) -> None:
    """把当前均值点放到固定复核块上与起点配对重测，并落日志。"""
    res = evaluate(args, out, "recheck", [(START, start), ("ga_mean", point)],
                   args.recheck_games, SEED_RECHECK + args.seed_offset, ref=START, deck=args.deck)
    s = summary(args, res, "ga_mean")
    s0 = summary(args, res, START)
    log(out / "recheck.jsonl", {"gen": gen, **s, "fitness_gain": s["fitness"] - s0["fitness"],
                                "start_friend_full": s0["friend_full"], "point": point.to_json()})
    print(f"  recheck mean  Δ={s['delta']:+8.1f} ± {s['se']:.1f}  适应度增益 {s['fitness'] - s0['fitness']:+8.1f}  "
          f"友人走满 {s['friend_full']:.3f}（起点 {s0['friend_full']:.3f}）  根 {s['five'][3]:.0f}", flush=True)


def cmd_search(args) -> None:
    """CMA-ES 主循环：每代换种子块；每 recheck_every 代与收尾时把均值点放到复核块重测。"""
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    args.sire_dirs = write_sires(out)
    start = load_point(args.start)
    cma = Cma(encode(start), args.sigma, args.lam, np.random.default_rng(args.rng_seed))
    if args.resume:
        restore(args, out)
        if (out / "state.json").exists():
            cma.load(out)
            print(f"# 续跑：从第 {cma.gen} 代开始", flush=True)
    if not (out / "knobs.json").exists():
        (out / "knobs.json").write_text(json.dumps(
            {"start": start.to_json(), "dim": DIM, "umas": UMAS, "sires": SIRES,
             "args": {k: str(v) for k, v in vars(args).items() if k != "sire_dirs"}},
            ensure_ascii=False, indent=1), encoding="utf-8")
    started = time.time()
    print(f"# 档 {args.profile} 卡组 {args.deck.name}（{args.deck.code}）dim={DIM} λ={args.lam} "
          f"场景={len(scenarios())} 每场景 {args.games} 局 jobs={args.jobs} threads={args.threads} "
          f"种子偏移 {args.seed_offset}", flush=True)
    while cma.gen < args.gens:
        gen = cma.gen
        zs = cma.ask()
        points = [(f"ga_g{gen:04d}_i{i:02d}", decode(z)) for i, z in enumerate(zs)]
        tick = time.time()
        res = evaluate(args, out, "gen", [(START, start)] + points, args.games,
                       SEED_SELECT + args.seed_offset + gen * args.games, ref=START, deck=args.deck)
        fits = [fitness(args, res, name) for name, _ in points]
        base = fitness(args, res, START)
        cma.tell(zs, fits)
        top = int(np.argmax(fits))
        log(out / "gens.jsonl", {"gen": gen, "sigma": cma.sigma, "start_fit": base,
                                 "best_gain": fits[top] - base, "median_gain": float(np.median(fits)) - base,
                                 "secs": time.time() - tick})
        print(f"gen {gen:4d}  σ={cma.sigma:.3f}  best {fits[top] - base:+8.1f}  median {np.median(fits) - base:+8.1f}  "
              f"{time.time() - tick:5.1f}s/代  累计 {(time.time() - started) / 60:5.1f} 分", flush=True)
        timeout = (time.time() - started) / 60 >= args.max_minutes
        if cma.gen % args.recheck_every == 0 or cma.gen >= args.gens or timeout:
            recheck(args, out, gen, decode(cma.mean), start)
        cma.save(out)
        sync(args, out)
        if timeout:
            print(f"# 到达限时 {args.max_minutes} 分钟，停在第 {cma.gen} 代（--resume 可续跑）", flush=True)
            break
    (out / "final.json").write_text(json.dumps(
        {"gen": cma.gen, "sigma": cma.sigma, "mean": decode(cma.mean).to_json()},
        ensure_ascii=False, indent=1), encoding="utf-8")
    sync(args, out)


def confirm_pass(args, out: Path, tag: str, rows: list[tuple[str, Point | None]], deck: Deck) -> dict:
    """验收块上用 `deck` 跑两遍（对现值 / 对起点配对），返回 {ref: {名: summary}}。"""
    report = {}
    for ref in (REF, START):
        res = evaluate(args, out, f"{tag}_vs_{ref}", rows, args.games, SEED_CONFIRM + args.seed_offset,
                       ref=ref, deck=deck, correct="holm")
        report[ref] = {name: summary(args, res, name) for name, _ in rows}
    return report


def z(row: dict) -> float:
    """配对 Δ 的 z 值（SE 为 0 时记 0）。"""
    return row["delta"] / row["se"] if row["se"] > 0 else 0.0


def cmd_confirm(args) -> None:
    """验收：复核增益最高的 top 个点在独立验收块上分别对 现值 与 起点 配对，再看影子卡组。

    晋级规则（看结果前固定）：只有第一个（复核增益最高者）是门禁，其余仅供参考；门禁同时要求：
    对现值与对起点 Δ 都 z > 2.58；友人走满率 ≥ max(现值, 起点) − `--friend-tol`（硬需求）；
    有影子卡组时，影子上对现值与对起点 Δ 都为正（换卡后同向）。
    """
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    restore(args, out)
    args.sire_dirs = write_sires(out)
    start = load_point(args.start)
    with (out / "recheck.jsonl").open(encoding="utf-8") as fh:
        rechecks = sorted((json.loads(l) for l in fh if l.strip()), key=lambda r: r["fitness_gain"], reverse=True)
    chosen = [(f"ok_g{r['gen']:04d}", Point.from_json(r["point"]), r) for r in rechecks[: args.top]]
    rows = [(REF, None), (START, start)] + [(n, p) for n, p, _ in chosen]
    report = {"main": confirm_pass(args, out, "confirm", rows, args.deck)}
    if args.shadow is not None:
        report["shadow"] = confirm_pass(args, out, "shadow", rows, args.shadow)
    main, shadow = report["main"], report.get("shadow")
    ref_full, start_full = main[REF][REF]["friend_full"], main[REF][START]["friend_full"]
    print(f"# 档 {args.profile} 卡组 {args.deck.name}（{args.deck.code}）影子 "
          f"{args.shadow.name if args.shadow else '无'}")
    print(f"{'候选':12s} {'复核增益':>8s} | {'对现值 Δ':>16s} | {'对起点 Δ':>16s} | 友人走满 | 根 | 影子 对现值 / 对起点")
    for rank, (name, point, rc) in enumerate(chosen):
        if rank == 1:
            print("--- 以下仅供参考（不参与晋级）")
        a, b = main[REF][name], main[START][name]
        side = f"{shadow[REF][name]['delta']:+7.1f} / {shadow[START][name]['delta']:+7.1f}" if shadow else "—"
        print(f"{name:12s} {rc['fitness_gain']:+8.1f} | {a['delta']:+8.1f} ± {a['se']:5.1f} | "
              f"{b['delta']:+8.1f} ± {b['se']:5.1f} | {a['friend_full']:.3f} | {a['five'][3]:.0f} | {side}")
        if rank == 0:
            full_ok = a["friend_full"] >= max(ref_full, start_full) - args.friend_tol
            shadow_ok = shadow is None or (shadow[REF][name]["delta"] > 0 and shadow[START][name]["delta"] > 0)
            passed = z(a) > 2.58 and z(b) > 2.58 and full_ok and shadow_ok
            report["gate"] = {"name": name, "passed": passed, "z_ref": z(a), "z_start": z(b),
                              "ref_friend_full": ref_full, "start_friend_full": start_full,
                              "shadow_ok": shadow_ok}
            print(f"  门禁：{'通过' if passed else '未通过'}（z 对现值 {z(a):.2f} / 对起点 {z(b):.2f}；"
                  f"走满 {a['friend_full']:.4f} vs max(现值 {ref_full:.4f}, 起点 {start_full:.4f}) − "
                  f"{args.friend_tol}；影子同向 {'是' if shadow_ok else '否'}）")
        (out / f"{name}.toml").write_text(deck_toml(args.profile, point, args.deck.comp, variant=False),
                                          encoding="utf-8")
    s0 = main[REF][START]
    side = f"{shadow[REF][START]['delta']:+7.1f}" if shadow else "—"
    print(f"{'起点':12s} {'':8s} | {s0['delta']:+8.1f} ± {s0['se']:5.1f} | {'':16s} | {s0['friend_full']:.3f} | "
          f"{s0['five'][3]:.0f} | {side}（现值友人走满 {ref_full:.3f}）")
    (out / "confirm.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    if args.sync is not None:
        for path in [out / "confirm.json", *out.glob("ok_g*.toml")]:
            shutil.copy2(path, args.sync / path.name)


def check_profile(args) -> None:
    """档名必须与卡组构成对得上、不与同构成的其他档并存；参照行名不得撞上真实档名。"""
    with BASE_CONFIG.open("rb") as fh:
        decks = tomllib.load(fh)["decks"]
    for name in (REF, START):
        if name in decks:
            sys.exit(f"参照行名 {name} 与配置里的档重名")
    comp, name = args.deck.comp, args.profile
    if name == "default":
        if decks["default"]["comp"] != comp:
            sys.exit(f"default 只用于默认构成 {decks['default']['comp']}；{args.deck.code} 请用 --profile 写独立档")
        return
    if name in decks and decks[name].get("comp") != comp:
        sys.exit(f"档 {name} 的 comp {decks[name].get('comp')} ≠ 卡组构成 {comp}")
    clash = [n for n, d in decks.items() if n not in ("default", name) and not d.get("variant") and d.get("comp") == comp]
    if clash:
        sys.exit(f"构成 {args.deck.code} 已有档 {clash}；请用 --profile {clash[0]}")


def main() -> None:
    """命令行入口。"""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("search", "confirm"):
        p = sub.add_parser(name)
        p.add_argument("--out", type=Path, required=True)
        p.add_argument("--start", type=Path, default=None,
                       help="起点片段（只含一个 [decks.<档>]，叠加在 default 上）；缺省 = 默认层 default")
        p.add_argument("--sync", type=Path, default=None, help="断点 / 日志同步目录（如 Drive）")
        p.add_argument("--exe", type=Path, default=DEFAULT_EXE)
        p.add_argument("--decks", type=Path, default=DEFAULT_DECKS,
                       help="卡组夹具：第一行 = 被调卡组，*_shadow 行 = 影子卡组")
        p.add_argument("--profile", default="default", help="产出片段的档名（default 只用于默认构成）")
        p.add_argument("--seed-offset", type=int, default=0, help="三段种子整体平移（不同轮次互不复用）")
        p.add_argument("--threads", type=int, default=8, help="总线程数（按 --jobs 平分）")
        p.add_argument("--jobs", type=int, default=1, help="并发跑的场景数")
        p.add_argument("--friend-penalty", type=float, default=100000.0,
                       help="每场景扣罚系数：1%% 的局没走满友人 → −1000（硬需求）")
        p.add_argument("--friend-tol", type=float, default=0.002, help="门禁：走满率不得低于现值超过此值")
    s = sub.choices["search"]
    s.add_argument("--gens", type=int, default=100000)
    s.add_argument("--max-minutes", type=float, default=100.0)
    s.add_argument("--lam", type=int, default=48)
    s.add_argument("--sigma", type=float, default=0.1)
    s.add_argument("--games", type=int, default=250, help="每候选每场景局数")
    s.add_argument("--recheck-every", type=int, default=10)
    s.add_argument("--recheck-games", type=int, default=500)
    s.add_argument("--rng-seed", type=int, default=20261108)
    s.add_argument("--resume", action="store_true")
    c = sub.choices["confirm"]
    c.add_argument("--games", type=int, default=2500)
    c.add_argument("--top", type=int, default=5)
    args = parser.parse_args()
    if not args.exe.exists():
        sys.exit(f"找不到 {args.exe}；先 cargo build --release -p matsuri-tools --bin cardtest")
    args.deck, args.shadow = load_decks(args.decks)
    check_profile(args)
    (cmd_search if args.cmd == "search" else cmd_confirm)(args)


if __name__ == "__main__":
    main()
