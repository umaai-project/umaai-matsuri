"""导出每张支援卡的灵感技能池（方案：分桶 Hint 探针的数据源）。

数据来源：游戏 master.mdb 的 single_mode_hint_gain（hint_gain_type = 0），
按 support_card_id 聚合成 `{卡ID: [技能ID...]}`，写到 `<仓根>/data/card_skills.json`。

用法（仓根）：
    python scripts/export_card_skills.py [master.mdb] [输出路径]
缺省 master.mdb = %USERPROFILE%/AppData/LocalLow/Cygames/umamusume/master/master.mdb。
"""

from __future__ import annotations

import json
import os
import sqlite3
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_MDB = (
    Path(os.environ.get("USERPROFILE", ""))
    / "AppData/LocalLow/Cygames/umamusume/master/master.mdb"
)


def main() -> int:
    mdb = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_MDB
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else REPO / "data" / "card_skills.json"
    if not mdb.is_file():
        print(f"找不到 master.mdb：{mdb}", file=sys.stderr)
        return 1
    conn = sqlite3.connect(f"file:{mdb}?mode=ro", uri=True)
    rows = conn.execute(
        "SELECT support_card_id, hint_value_1 FROM single_mode_hint_gain "
        "WHERE hint_gain_type = 0 ORDER BY support_card_id, id"
    ).fetchall()
    conn.close()
    cards: dict[str, int] = {}
    for card, _skill in rows:
        cards[str(card)] = cards.get(str(card), 0) + 1
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(cards, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    counts = sorted(len(v) for v in cards.values())
    print(f"cards={len(cards)} skills={len(rows)} -> {out}")
    if counts:
        print(
            f"skills/card: min={counts[0]} p50={counts[len(counts) // 2]} max={counts[-1]}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
