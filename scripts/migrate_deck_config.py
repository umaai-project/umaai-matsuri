#!/usr/bin/env python3
"""把旧版 `matsuri_config.toml`（顶层 `[fusion]` + `[trainer]`）迁移到 `[decks.default.*]`。

用法：
    python3 scripts/migrate_deck_config.py old.toml > new.toml
    python3 scripts/migrate_deck_config.py old.toml --out data/matsuri_config.toml
    python3 scripts/migrate_deck_config.py --dir lab/config --out-dir lab/config-new
    python3 scripts/migrate_deck_config.py old.toml --comp 1,2,1,0,1

口径（见 docs/plan/deck_config.md）：
- 旧顶层 `[fusion]` / `[trainer]` 变成 `[decks.default.fusion]` / `[decks.default.trainer]`；
- 旧配置没有 `[advice]` 时补当前内置默认落位（101–501 六窗口）；
- `[pre]` / `[search]` 原样保留；
- 只用于一次性迁移，输出请人工复核；不覆盖原文件（除非显式 `--out`）。
"""

from __future__ import annotations

import argparse
import sys
import tomllib

# 原 value.rs::ROUTE_PRIOR（默认档 = speed_build）
DEFAULT_ADVICE = {
    "t2": [201, 101, 501, 301, 401],
    "t11": [501, 301, 101, 201, 401],
    "t23": [101, 501, 401, 301, 201],
    "t35": [101, 301, 501, 401, 201],
    "t47": [501, 101, 401, 301, 201],
    "t59": [501, 401, 101, 301, 201],
}


def fmt_value(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, list):
        return "[" + ", ".join(fmt_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{key} = {fmt_value(item)}" for key, item in value.items()) + " }"
    raise TypeError(f"不支持的 TOML 值: {type(value)!r}")


def dump_table(path: str, table: dict, out: list[str]) -> None:
    """先写标量 / 数组，再递归写子表；path 为空表示文件根。"""
    scalars = {key: value for key, value in table.items() if not isinstance(value, dict)}
    nested = {key: value for key, value in table.items() if isinstance(value, dict)}
    if path and (scalars or not nested):
        out.append(f"[{path}]")
        for key, value in scalars.items():
            out.append(f"{key} = {fmt_value(value)}")
        out.append("")
    for key, value in nested.items():
        child = f"{path}.{key}" if path else key
        dump_table(child, value, out)


def migrate(data: dict, comp: list[int], note: str) -> dict:
    legacy_fusion = data.get("fusion")
    legacy_trainer = data.get("trainer")
    if legacy_fusion is None or legacy_trainer is None:
        raise SystemExit("旧配置缺少 [fusion] 或 [trainer]；已经是新 schema？")
    if "decks" in data:
        raise SystemExit("配置里已有 [decks.*]，不需要迁移")

    year2 = legacy_fusion.get("year2")
    year3 = legacy_fusion.get("year3")
    if not year2 or not year3:
        raise SystemExit("旧配置 [fusion] 缺少 year2 / year3")

    advice = data.get("advice") or DEFAULT_ADVICE
    default = {
        "comp": comp,
        "note": note,
        "fusion": {"year2": year2, "year3": year3},
        "advice": advice,
        "trainer": legacy_trainer,
    }
    migrated = {}
    for key in ("pre", "search"):
        if key in data:
            migrated[key] = data[key]
    migrated["decks"] = {"default": default}
    return migrated


def main() -> None:
    parser = argparse.ArgumentParser(description="迁移旧 matsuri_config.toml 到 [decks.default.*]")
    parser.add_argument("path", nargs="?", help="旧配置文件路径（单文件模式）")
    parser.add_argument("--dir", help="批量迁移目录下的 *.toml")
    parser.add_argument("--out-dir", help="批量输出目录（缺省同目录 <名>.migrated.toml）")
    parser.add_argument("--out", help="输出路径（缺省 stdout）")
    parser.add_argument("--comp", default="2,1,1,0,1", help="default 档 comp，缺省 2,1,1,0,1")
    parser.add_argument("--note", default="legacy matsuri_config.toml 迁移档", help="default 档 note")
    args = parser.parse_args()

    comp = [int(item) for item in args.comp.split(",") if item.strip()]
    if len(comp) != 5 or sum(comp) != 5:
        raise SystemExit("--comp 必须是 5 个非负整数且合计 5")

    def convert(source: str) -> str:
        with open(source, "rb") as handle:
            data = tomllib.load(handle)
        migrated = migrate(data, comp, args.note)
        lines: list[str] = [
            "# 由 scripts/migrate_deck_config.py 从旧版 matsuri_config.toml 迁移；请人工复核。",
            "",
        ]
        dump_table("", migrated, lines)
        text = "\n".join(lines).rstrip() + "\n"
        # 语法自检：迁移后的文本必须还能被 TOML 解析
        tomllib.loads(text)
        return text

    if args.dir:
        from pathlib import Path as FilePath

        sources = sorted(FilePath(args.dir).glob("*.toml"))
        if not sources:
            raise SystemExit(f"{args.dir} 下没有 *.toml")
        out_dir = FilePath(args.out_dir) if args.out_dir else None
        if out_dir:
            out_dir.mkdir(parents=True, exist_ok=True)
        for source in sources:
            try:
                text = convert(str(source))
            except SystemExit as err:
                print(f"跳过 {source.name}: {err}", file=sys.stderr)
                continue
            target = out_dir / source.name if out_dir else source.with_name(source.stem + ".migrated.toml")
            target.write_text(text, encoding="utf-8")
            print(f"已写入 {target}", file=sys.stderr)
        return

    if not args.path:
        raise SystemExit("单文件模式需要 path；或用 --dir 批量迁移")
    text = convert(args.path)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
        print(f"已写入 {args.out}", file=sys.stderr)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
