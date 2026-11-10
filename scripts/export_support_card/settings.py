"""export_support_card 的配置加载与路径解析。

配置全部落在同目录的 config.toml，本模块负责把它读成一个强类型对象，
并按优先级定位游戏的 master.mdb 与翻译数据。
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent
DEFAULT_CONFIG = BASE_DIR / "config.toml"
ENV_MDB = "UMAMUSUME_MASTER_MDB"
GAME_MDB_RELATIVE = Path("AppData") / "LocalLow" / "Cygames" / "umamusume" / "master" / "master.mdb"


class ConfigError(RuntimeError):
    """配置缺失或取值非法。"""


def _expand(value: str) -> Path:
    """展开环境变量 / ~，相对路径按脚本目录解析。"""
    raw = os.path.expandvars(os.path.expanduser(str(value)))
    path = Path(raw)
    return path if path.is_absolute() else (BASE_DIR / path).resolve()


def default_game_mdb() -> Path | None:
    """Windows 下游戏的默认库位置（DMM / Steam 版一致）。"""
    home = os.environ.get("USERPROFILE") or os.environ.get("HOME")
    if not home:
        return None
    return Path(home) / GAME_MDB_RELATIVE


@dataclass
class Config:
    config_path: Path
    master_mdb: str
    fallback_script_dir: bool
    translate_file: Path | None
    translate_cache: Path
    output_dir: Path
    copy_translate: bool
    short_types: list[str]
    direct_keys: list[str]
    direct_merge_cards: list[int]
    hint_values: list[list[int]]
    initial_keys: dict[str, int]
    bonus_keys: dict[str, int]
    race_condition_ids: dict[int, str] = field(default_factory=dict)
    race_condition_grades: dict[int, int] = field(default_factory=dict)
    special_races: dict[int, dict] = field(default_factory=dict)


def _require(table: dict, key: str, section: str):
    if key not in table:
        raise ConfigError(f"配置缺少 [{section}] {key}（{DEFAULT_CONFIG.name}）")
    return table[key]


def _as_int_list(values, what: str) -> list[int]:
    try:
        return [int(v) for v in values]
    except (TypeError, ValueError) as exc:
        raise ConfigError(f"{what} 必须是整数数组：{values!r}") from exc


def load_config(path: str | os.PathLike | None = None) -> Config:
    config_path = Path(path).resolve() if path else DEFAULT_CONFIG
    if not config_path.exists():
        raise ConfigError(f"找不到配置文件：{config_path}")
    with config_path.open("rb") as f:
        raw = tomllib.load(f)

    paths = raw.get("paths", {})
    output = raw.get("output", {})
    support_card = raw.get("support_card", {})

    short_types = list(_require(support_card, "short_types", "support_card"))
    if len(short_types) < 8:
        raise ConfigError(f"[support_card] short_types 至少需要 8 项（下标 0-7），当前 {len(short_types)} 项")

    hint_values = [list(row) for row in _require(support_card, "hint_values", "support_card")]
    if len(hint_values) < 5 or any(len(row) != 6 for row in hint_values[:5]):
        raise ConfigError("[support_card] hint_values 需要 5 行，每行 6 项")

    race_condition_ids: dict[int, str] = {}
    race_condition_grades: dict[int, int] = {}
    for entry in raw.get("race_condition", []):
        cond_id = int(_require(entry, "id", "race_condition"))
        race_condition_ids[cond_id] = str(entry.get("name", cond_id))
        race_condition_grades[cond_id] = int(entry.get("grade", 1))

    special_races: dict[int, dict] = {}
    for entry in raw.get("special_races", []):
        chara_id = int(_require(entry, "chara_id", "special_races"))
        item: dict = {}
        if "races" in entry:
            item["races"] = _as_int_list(entry["races"], f"special_races {chara_id} races")
        if "free_races" in entry:
            free_races = []
            for free in entry["free_races"]:
                free_races.append({
                    "startTurn": int(_require(free, "start_turn", f"special_races {chara_id} free_races")),
                    "endTurn": int(_require(free, "end_turn", f"special_races {chara_id} free_races")),
                    "count": int(free.get("count", 1)),
                })
            item["freeRaces"] = free_races
        if "note" in entry:
            item["note"] = str(entry["note"])
        special_races[chara_id] = item

    translate_file_raw = str(paths.get("translate_file", "") or "").strip()
    return Config(
        config_path=config_path,
        master_mdb=str(paths.get("master_mdb", "") or "").strip(),
        fallback_script_dir=bool(paths.get("fallback_script_dir", True)),
        translate_file=_expand(translate_file_raw) if translate_file_raw else None,
        translate_cache=_expand(str(paths.get("translate_cache", "text_data_dict.json"))),
        output_dir=_expand(str(output.get("dir", "."))),
        copy_translate=bool(output.get("copy_translate", True)),
        short_types=short_types,
        direct_keys=[str(key) for key in _require(support_card, "direct_keys", "support_card")],
        direct_merge_cards=_as_int_list(_require(support_card, "direct_merge_cards", "support_card"),
                                        "direct_merge_cards"),
        hint_values=hint_values,
        initial_keys={str(k): int(v) for k, v in _require(support_card, "initial_keys", "support_card").items()},
        bonus_keys={str(k): int(v) for k, v in _require(support_card, "bonus_keys", "support_card").items()},
        race_condition_ids=race_condition_ids,
        race_condition_grades=race_condition_grades,
        special_races=special_races,
    )


def _as_mdb_file(candidate: Path) -> Path:
    return candidate / "master.mdb" if candidate.is_dir() else candidate


def mdb_candidates(cfg: Config, cli_path: str | None = None) -> list[tuple[str, Path]]:
    """按优先级列出所有候选库路径（标签, 路径）。"""
    items: list[tuple[str, Path]] = []
    if cli_path:
        items.append(("命令行 --mdb", _as_mdb_file(Path(cli_path).expanduser())))
    if cfg.master_mdb:
        items.append((f"配置 paths.master_mdb", _as_mdb_file(_expand(cfg.master_mdb))))
    if os.environ.get(ENV_MDB):
        items.append((f"环境变量 {ENV_MDB}", _as_mdb_file(_expand(os.environ[ENV_MDB]))))
    game_mdb = default_game_mdb()
    if game_mdb:
        items.append(("游戏默认目录", game_mdb))
    if cfg.fallback_script_dir:
        items.append(("脚本目录兜底", BASE_DIR / "master.mdb"))
    return items


def resolve_master_mdb(cfg: Config, cli_path: str | None = None) -> tuple[Path, str]:
    """返回 (库文件路径, 来源说明)；全部落空时抛 ConfigError 并列出尝试过的路径。"""
    if cli_path:
        explicit = _as_mdb_file(Path(cli_path).expanduser())
        if not explicit.is_file():
            raise ConfigError(f"--mdb 指定的文件不存在：{explicit}")
    candidates = mdb_candidates(cfg, cli_path)
    for label, candidate in candidates:
        if candidate.is_file():
            return candidate, label
    tried = "\n".join(f"  {label}: {candidate}" for label, candidate in candidates)
    raise ConfigError(
        "找不到 master.mdb，已尝试：\n"
        f"{tried}\n"
        f"请在 {cfg.config_path} 的 [paths] master_mdb 里写明实际路径，"
        f"或用 --mdb 指定，或设置环境变量 {ENV_MDB}。"
    )


def describe_mdb(path: Path) -> str:
    """库文件的一行摘要（大小 + 修改时间），便于确认用的是哪一版。"""
    import datetime

    stat = path.stat()
    mtime = datetime.datetime.fromtimestamp(stat.st_mtime).strftime("%Y-%m-%d %H:%M")
    return f"{path}  ({stat.st_size / 1024 / 1024:.1f} MB, {mtime})"
