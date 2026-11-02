#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""从 master.mdb 生成 cardDB 单卡记录（复刻 xf8410/umaai-rs 的 export_support_card 逻辑）。

用途：官方 cardDB 缺新卡时（如 30322 重炮），按同一套规则从 mdb 补一条，
并可用 --verify 复算已有卡、与现有 cardDB 逐字段比对（证明规则复现无误）。

用法：
  python tools/gen_carddb_entry.py --verify              # 复算 + 比对（不写文件）
  python tools/gen_carddb_entry.py --diff 30289 30323    # 打印指定卡的比对明细
  python tools/gen_carddb_entry.py --emit 30322          # 输出 30322 的 JSON 记录
  python tools/gen_carddb_entry.py --add 30322           # 插入 data/cardDB.json（备份后写）
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MDB = os.environ.get("MASTER_MDB", r"D:/Downloads/master.mdb")
CARDB = os.path.join(ROOT, "data", "cardDB.json")

# support_card_effect_table.type -> cardDB 字段（复刻 effectTypeToStr 的名字映射 + mergeCardEffect）
# 说明：3..7 副属性、30 技能点走 bonus[][which]；9..13 初始属性走 initialBonus[][which]
STAT_BONUS = {3: 0, 4: 1, 5: 2, 6: 3, 7: 4, 30: 5}
STAT_INITIAL = {9: 0, 10: 1, 11: 2, 12: 3, 13: 4, 32: 5}
# DirectKeys（xf8410 main.py 里按名字直取，名字来自 effectTypeToStr）
DIRECT_ADD = {
    1: "youQing",
    2: "ganJing",
    8: "xunLian",
    14: "initialJiBan",
    15: "saiHou",
    18: "hintProbIncrease",
    19: "deYiLv",
    25: "eventRecoveryAmountUp",
    26: "eventEffectUp",
    27: "failRateDrop",
    28: "vitalCostDrop",
    31: "wizVitalBonus",
}
# 乘算类：友情/得意率/失败率/体力消耗
MULT_YOUQING = 1
MULT_DEYI = 19
MULT_FAIL = 27
MULT_VITAL = 28

# xf8410 里 DirectMergeCards（直接合并面板、不设固有的特判卡）
DIRECT_MERGE_CARDS = {30155, 30171, 30128}

HINT_VALUES = [
    [6, 0, 2, 0, 0, 0],
    [0, 6, 0, 2, 0, 0],
    [0, 2, 6, 0, 0, 0],
    [1, 0, 1, 6, 0, 0],
    [2, 0, 0, 0, 6, 0],
]

# _effectTypeExHandlers 里 type=103 的中文摘要模板
SUMMARY_103 = "编入支援卡种类>={}时，训练效果提高 ({})"


def card_type_index(support_card_type: int, command_id: int) -> int:
    """复刻 umadb._get_support_card_type：训练卡按 command_id 再细分，返回 0 基下标。

    101 速度 / 102 力量 / 103 根性 / 105 耐力 / 106 智力；104 未处理 → 速度。
    support_card_type 2=友人(5)、3=团队(6)。
    """
    if support_card_type == 1:
        return {101: 0, 102: 2, 103: 3, 105: 1, 106: 4}.get(command_id, 0)
    if support_card_type == 2:
        return 5
    if support_card_type == 3:
        return 6
    return 0


def parse_effect_row(row: list[int]) -> list[int]:
    """11 档（lv0,5,10,...,50）线性插值补全；-1 表示该档位未定义。"""
    stack = [(i, v) for i, v in enumerate(row) if v != -1]
    ret = [0] * 11
    if not stack:
        return ret
    prev_index, prev = stack[-1]
    for i in range(10, -1, -1):
        if not stack:
            ret[i] = 0
            continue
        index, value = stack[-1]
        if i > prev_index:
            ret[i] = prev
        elif i == index:
            ret[i] = value
            prev_index, prev = stack.pop()
        else:
            ret[i] = int(value + (prev - value) * (i - index) / (prev_index - index))
    return ret


def _entry_defaults() -> dict:
    return {
        "filled": True,
        "bonus": [0, 0, 0, 0, 0, 0],
        "initialBonus": [0, 0, 0, 0, 0, 0],
        "hintBonus": [0, 0, 0, 0, 0, 5],  # xf8410 原脚本注释 deprecated
        "hintLevel": 0,
    }


def merge_card_effect(base: dict, pairs: list[tuple[int, int]]) -> dict:
    """复刻 mergeCardEffect：把固有词条按乘/加规则并进面板。"""
    ret = {k: (list(v) if isinstance(v, list) else v) for k, v in base.items()}
    for key, value in pairs:
        if key == 41:  # 神鹰特判：全属性 bonus +1
            for j in range(5):
                ret["bonus"][j] = base["bonus"][j] + 1
        elif key == MULT_YOUQING:
            ret["youQing"] = (100.0 + value) * (100 + base.get("youQing", 0)) / 100.0 - 100
        elif key == 2:
            ret["ganJing"] = base.get("ganJing", 0) + value
        elif key == MULT_DEYI:
            ret["deYiLv"] = (100.0 + value) * (100 + base.get("deYiLv", 0)) / 100.0 - 100
        elif key == 8:
            ret["xunLian"] = base.get("xunLian", 0) + value
        elif key == 15:
            ret["saiHou"] = base.get("saiHou", 0) + value
        elif key == MULT_FAIL:
            ret["failRateDrop"] = 100 - (100 - base.get("failRateDrop", 0)) * (100 - value) / 100
        elif key == MULT_VITAL:
            ret["vitalCostDrop"] = 100 - (100 - base.get("vitalCostDrop", 0)) * (100 - value) / 100
        elif key == 31:
            ret["wizVitalBonus"] = base.get("wizVitalBonus", 0) + value
        elif 3 <= key <= 7:
            which = key - 3
            ret["bonus"][which] = base["bonus"][which] + value
        elif key == 30:
            ret["bonus"][5] = base["bonus"][5] + value
        elif 9 <= key <= 13:
            which = key - 9
            ret["initialBonus"][which] = base["initialBonus"][which] + value
        elif key == 14:
            ret["initialJiBan"] = base.get("initialJiBan", 0) + value
        elif key == 17:
            ret["hintBonus"][5] += value * 5
        elif key == 18:
            pass  # hint 率：不处理
        else:
            pass
    return ret


def _unique_effects(param6: list[int]) -> tuple[int | None, list[int], list[tuple[int, int]]]:
    """解析固有 12 参数（两组各 6）-> (CY 固有 type, [(基础面板词条, 数值)])。

    复刻 parseUniqueEffectRow：只有 params[0] >= 100 才登记「固有」，
    其余（0 < type <= 31）都会并进基础面板（mergeCardEffect）。
    101/102/103 这几种由 AI 侧按固有处理，不并入面板。
    """
    cy_type: int | None = None
    cy_params: list[int] = []
    pairs: list[tuple[int, int]] = []
    param6 = (list(param6) + [0] * 12)[:12]
    for g in range(2):
        params = param6[g * 6 : (g + 1) * 6]
        index = 0
        while index < 6:
            if params[index] == 0:
                break
            start = index
            head = params[index]
            if head >= 100:
                cy_type = head
                cy_params = list(params)
                if head in (102, 103, 104):
                    pairs.append((8, params[index + 2]))
                index += 6
            elif 0 < head <= 31:
                pairs.append((head, params[index + 1]))
                index += 2
            else:
                index += 2
            if index == start:
                break
    return cy_type, cy_params, pairs


def build_entry(con: sqlite3.Connection, card_id: int) -> dict:
    cur = con.cursor()
    cur.execute(
        "select chara_id, rarity, effect_table_id, unique_effect_id, support_card_type, command_id "
        "from support_card_data where id=?",
        (card_id,),
    )
    row = cur.fetchone()
    if not row:
        raise SystemExit(f"mdb 里没有 support_card_data.id={card_id}")
    chara_id, rarity, effect_id, unique_id, sct, command_id = row

    cur.execute(
        "select type, init, limit_lv5, limit_lv10, limit_lv15, limit_lv20, limit_lv25, "
        "limit_lv30, limit_lv35, limit_lv40, limit_lv45, limit_lv50 "
        "from support_card_effect_table where id=?",
        (effect_id,),
    )
    eff_rows = cur.fetchall()

    # xf8410: effect_range = 3 + rarity，取 5 档（SSR=6 → lv30..lv50）
    preset = 3 + rarity
    by_type: dict[int, list[int]] = {}
    for r in eff_rows:
        by_type[r[0]] = parse_effect_row(list(r[1:]))[preset : preset + 5]

    card_value = []
    for i in range(5):
        d = _entry_defaults()
        for tp, vals in by_type.items():
            v = vals[i]
            if tp in DIRECT_ADD:
                d[DIRECT_ADD[tp]] = v
            elif tp in STAT_BONUS:
                d["bonus"][STAT_BONUS[tp]] = v
            elif tp in STAT_INITIAL:
                d["initialBonus"][STAT_INITIAL[tp]] = v
            elif tp == 17:
                d["hintBonus"][5] += 5 * v
                d["hintLevel"] = v + 1
        card_value.append(d)

    # 无 hint 技能的训练卡：hintBonus 换成固定属性表（xf8410 HintValues 特判）
    cur.execute(
        "select count(*) from single_mode_hint_gain where support_card_id=? and hint_gain_type=0",
        (card_id,),
    )
    has_hint_skill = cur.fetchone()[0] > 0
    ctype = card_type_index(sct, command_id)
    if not has_hint_skill and ctype < 5:
        for i in range(5):
            card_value[i]["hintBonus"] = list(HINT_VALUES[ctype])
            card_value[i]["hintLevel"] = 0

    entry = {
        "cardId": card_id,
        "charaId": chara_id,
        "cardName": "",
        "fullName": "",
        "rarity": rarity,
        "cardType": card_type_index(sct, command_id),
        "cardValue": card_value,
        "uniqueEffectType": 0,
        "uniqueEffectSummary": "",
        "uniqueEffectParam": [],
    }

    # 固有
    if unique_id:
        cur.execute(
            "select type_0, value_0, value_0_1, value_0_2, value_0_3, value_0_4, "
            "type_1, value_1, value_1_1, value_1_2, value_1_3, value_1_4 "
            "from support_card_unique_effect where id=? order by lv limit 1",
            (unique_id,),
        )
        u = cur.fetchone()
        if u and u[0]:
            param6 = [x for x in u]
            from_cy, cy_params, pairs = _unique_effects(param6)
            if card_id in DIRECT_MERGE_CARDS or from_cy is None:
                # 无固有 / 特判卡：直接并进面板
                card_value = [merge_card_effect(card_value[i], pairs) for i in range(5)]
                entry["cardValue"] = card_value
                entry["uniqueEffectType"] = 0
            elif from_cy == 101:
                jiban = cy_params[1]
                entry["uniqueEffectType"] = 1 if jiban < 100 else 2
                entry["uniqueEffectParam"] = cy_params
                entry["uniqueEffectSummary"] = f"羁绊>={jiban}时"
            elif from_cy == 102:
                entry["uniqueEffectType"] = 3
                entry["uniqueEffectParam"] = cy_params
                entry["uniqueEffectSummary"] = f"羁绊>={cy_params[1]}且不是擅长训练时，训练效果提高({cy_params[2]})"
            elif from_cy == 103:
                # xf8410 特判：改成 AI 的 21 号固有，并重排参数（param[2] 恒为 8 = 训练词条）
                p = list(cy_params)
                p[3] = p[2]
                p[2] = 8
                entry["uniqueEffectType"] = 21
                entry["uniqueEffectParam"] = p
                entry["uniqueEffectSummary"] = SUMMARY_103.format(cy_params[1], cy_params[2])
            else:
                entry["uniqueEffectType"] = from_cy - 100 if from_cy <= 120 else from_cy - 99
                entry["uniqueEffectParam"] = cy_params
                entry["uniqueEffectSummary"] = f"固有 {from_cy}"
    return entry


def _diff_entry(expected: dict, actual: dict, keys: list[str]) -> list[str]:
    out = []
    for k in keys:
        if expected.get(k) != actual.get(k):
            out.append(f"  {k}:\n    mdb   = {expected.get(k)}\n    cardDB= {actual.get(k)}")
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--verify", action="store_true")
    ap.add_argument("--diff", nargs="*", type=int)
    ap.add_argument("--emit", type=int)
    ap.add_argument("--add", type=int)
    args = ap.parse_args()

    if not os.path.exists(MDB):
        print(f"找不到 master.mdb: {MDB}", file=sys.stderr)
        return 2
    con = sqlite3.connect(MDB)
    with open(CARDB, encoding="utf-8") as f:
        carddb = json.load(f)

    if args.emit:
        print(json.dumps(build_entry(con, args.emit), ensure_ascii=False, indent=1))
        return 0

    if args.add:
        entry = build_entry(con, args.add)
        if os.path.exists(CARDB):
            shutil.copyfile(CARDB, CARDB + ".bak")
        carddb[str(args.add)] = entry
        with open(CARDB, "w", encoding="utf-8") as f:
            json.dump(carddb, f, ensure_ascii=False, indent=1)
        print(f"已写入 {CARDB}（{len(carddb)} 张），备份 .bak")
        return 0

    # 比对：默认只比数值相关字段（名称/摘要来自翻译，不比）
    num_keys = ["cardId", "charaId", "rarity", "cardType", "cardValue",
                "uniqueEffectType", "uniqueEffectParam"]
    targets = args.diff if args.diff else [int(k) for k in carddb.keys()]
    ok = bad = skip = 0
    for cid in targets:
        if str(cid) not in carddb:
            skip += 1
            continue
        try:
            exp = build_entry(con, cid)
        except SystemExit:
            skip += 1
            continue
        diffs = _diff_entry(exp, carddb[str(cid)], num_keys)
        if diffs:
            bad += 1
            if bad <= 6 or args.diff:
                print(f"[DIFF] {cid}")
                print("\n".join(diffs))
        else:
            ok += 1
    print(f"\n一致 {ok} / 不一致 {bad} / 跳过 {skip}（共 {len(targets)} 张）")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
