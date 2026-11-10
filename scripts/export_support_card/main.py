#encoding: utf-8
"""支援卡 / 马娘数据导出。

数据来源：游戏目录下的 master.mdb（自动检测，见 config.toml [paths]）；
所有可调数据（卡类型简称、词条映射、生涯比赛特判等）都在 config.toml 里。
"""

import argparse
import codecs
import re
import shutil
from pathlib import Path

try:
    import jsons
except ImportError:  # pragma: no cover - 环境缺依赖时给一句人话
    raise SystemExit("缺少依赖 jsons，请先执行：pip install jsons requests")

from umadb import *
from umamodels import *
from support_card_helper import *
import net_utils  # noqa: F401  （translators 间接使用，保留原有导入面）
import translators
from settings import Config, ConfigError, describe_mdb, load_config, resolve_master_mdb

# 翻译数据在产物目录里的固定文件名（引擎的 ai_data 白名单就是这个名字）
TRANSLATE_OUTPUT_NAME = "text_data_dict.json"


def short_name(card, short_types):
    return re.sub(r"\[.*\]", "[%s]" % short_types[card.type], card.original_name)


def file_name(card):
    return re.sub(r"[\.\*\?\/\\\&\$\:]", "", card.original_name) + ".json"


def mergeCardEffect(cardValue, effect):
    ret = dict(cardValue)

    for i in range(0, int(len(effect)/2)):
        key, value = effect[i*2], effect[i*2+1]
        if key == 41: # 神鹰特判
            for j in range(0, 5):
                ret["bonus"][j] = cardValue["bonus"][j] + 1
        elif key == 1:  # 友情
            ret["youQing"] = (100.0 + value) * (100 + cardValue.get("youQing", 0)) / 100.0 - 100
        elif key == 2: # 干劲
            ret["ganJing"] = cardValue.get("ganJing", 0) + value
        elif key == 19:    # 得意率
            ret["deYiLv"] = (100.0 + value) * (100 + cardValue.get("deYiLv", 0)) / 100.0 - 100
        elif key == 8: # 训练
            ret["xunLian"] = cardValue.get("xunLian", 0) + value
        elif key == 15:    # 赛后
            ret["saiHou"] = cardValue.get("saiHou", 0) + value
        elif key == 27:    # 失败率-减乘
            ret["failRateDrop"] = 100 - (100 - cardValue.get("failRateDrop", 0)) * (100 - value) / 100
        elif key == 28:    # 体力消耗-减乘
            ret["vitalCostDrop"] = 100 - (100 - cardValue.get("vitalCostDrop", 0)) * (100 - value) / 100
        elif key == 31:    # 回体增加-诗歌剧
            ret["wizVitalBonus"] = cardValue.get("wizVitalBonus", 0) + value
        elif key >= 3 and key <= 7:   # 副属性
            which = key-3
            ret["bonus"][which] = cardValue["bonus"][which] + value
        elif key == 30:    # 技能点
            ret["bonus"][5] = cardValue["bonus"][5] + value
        elif key >= 9 and key <= 13:  # 初始属性
            which = key-9
            ret["initialBonus"][which] = cardValue["initialBonus"][which] + value
        elif key == 14:    # 初始羁绊
            ret["initialJiBan"] = cardValue.get("initialJiBan", 0) + value
        elif key == 17:   # hint等级
            ret["hintBonus"][5] += value * 5
        elif key == 18: # hint率：不处理
            pass
        else:
            print(f"未知数值词条 {key} = {value}")
#             // 忽略的属性
#         case (int)UniqueEffectType::RaceFanUp:   // 粉丝
#         case (int)UniqueEffectType::SkillTipsEventRateUp:    // Hint率
    return ret


def prepareUniqueEffect(card, ueffect, cfg: Config):
    utype = 0
    useParam = False
    if (not ueffect["type"]) or (card["cardId"] in cfg.direct_merge_cards):
        # 默认固有或者特判直接合并的卡（智太阳神）
        utype = 0
        for which in range(0, 5):
            card["cardValue"][which] = mergeCardEffect(card["cardValue"][which], ueffect["effect"])
    elif ueffect["type"] == 101:
        # 把CY的固有编号转换为AI的
        jiBan = ueffect["uniqueParams"][1]
        if jiBan < 100:
            utype = 1
        elif jiBan == 100:
            utype = 2
        else:
            print(f"新的羁绊型固有: {card['cardName']} - {jiBan}")
        useParam = True
    else:
        # 其他固有，只复制参数和映射ID
        if ueffect["type"] == 102:
            utype = 3
            useParam = True
        elif ueffect["type"] == 103:
            utype = 21   # 改为21号固有，参见文档
            ueffect["uniqueParams"][3] = ueffect["uniqueParams"][2]
            ueffect["uniqueParams"][2] = 8  # 调整参数顺序，方便AI处理
            useParam = True
        else:
            if ueffect["type"] <= 120:
                utype = ueffect["type"] - 100    # 直接映射
            else:
                utype = ueffect["type"] - 99     # 121 -> 22
            useParam = True

    card["uniqueEffectType"] = utype
    card["uniqueEffectSummary"] = ueffect["summary"]
    if useParam:
        card["uniqueEffectParam"] = ueffect["uniqueParams"]


# 支援卡解析
def parseSupportCard(cfg: Config, card_list, trans):
    result = {}
    for card in card_list:
        # 调用github TLG翻译
        trans_card = trans.translate_support_card(card)
        print(trans_card.name)

        # 处理效果
        effect = {}
        unique_effect = None
        effect_range = 3 + card.rarity

        if card.unique_effect:
            unique_effect = parseUniqueEffectRow(card.unique_effect)

        if card.effect_row_dict:
            for tp, row in card.effect_row_dict.items():
                type_name = effectTypeToStr(tp)
                type_data = parseEffectRow(row)
                effect[type_name] = type_data[effect_range : effect_range+5]

        # 整理
        trans_card.unique_effect = unique_effect
        trans_card.effect_row_dict = effect
        trans_card.original_name = trans_card.name
        trans_card.name = short_name(trans_card, cfg.short_types)

        # 转换成UmaSim格式
        ucard = dict(
            cardId = int(trans_card.id),
            charaId = trans_card.chara_id,
            cardName = trans_card.name,
            fullName = trans_card.original_name,
            rarity = trans_card.rarity.value,
        # cardSkill = list(map(lambda x: int(x), trans_card.train_skill_list)),
            cardType = trans_card.type.value - 1, # 从0开始
            cardValue = []
            #uniqueEffect = trans_card.unique_effect
        )
        for i in range(0, 5):
            d = dict(
                filled = True,
                bonus = [0, 0, 0, 0, 0, 0],
                initialBonus = [0, 0, 0, 0, 0, 0],
                hintBonus = [0, 0, 0, 0, 0, 5], # deprecated
                hintLevel = 0
            )
            for key, value in trans_card.effect_row_dict.items():
                if key in cfg.direct_keys:
                    d[key] = value[i]
                elif key == "hintLvUp":
                    d["hintBonus"][5] += 5 * value[i]
                    d["hintLevel"] = value[i] + 1
                elif '初期' in key:
                    d['initialBonus'][cfg.initial_keys[key]] = value[i]
                elif 'ボーナス' in key:
                    d['bonus'][cfg.bonus_keys[key]] = value[i]
            ucard["cardValue"].append(d)
        # 对UniqueEffect字段进行进一步预处理
        if trans_card.unique_effect:
            prepareUniqueEffect(ucard, trans_card.unique_effect, cfg)
        if len(trans_card.train_skill_list) == 0 and ucard["cardType"] < 5:
            # 无技能的卡，把hint技能点换为属性
            for i in range(0, 5):
                ucard["cardValue"][i]["hintBonus"] = cfg.hint_values[ucard["cardType"]]
                ucard["cardValue"][i]["hintLevel"] = 0
            print(f"hintBonus = {cfg.hint_values[ucard['cardType']]}")
        result[int(trans_card.id)] = ucard
    return result


# 马娘解析
def parseUma(cfg: Config, uma_list, trans):
    result = {}
    for uma in uma_list:
        # 调用github TLG翻译
        trans_uma = trans.translate_chara_card(uma)
        print(trans_uma.id, trans_uma.name)

        fiveStatusBonus = [
            uma.talent_info.speed,
            uma.talent_info.stamina,
            uma.talent_info.power,
            uma.talent_info.guts,
            uma.talent_info.wiz
        ]
        fiveStatusInitial = [
            uma.status_set[5].speed,
            uma.status_set[5].stamina,
            uma.status_set[5].power,
            uma.status_set[5].guts,
            uma.status_set[5].wiz
        ]

        # 生涯比赛
        chara_id = int(int(uma.id) / 100)
        races = []
        freeRaces = []
        raceNote = None
        last_race = -1
        for race in uma.route_races:
            if race.condition_type == 1:
                # 固定比赛
                if race.turn not in races:
                    if race.determine_race != 0:
                        if chara_id not in cfg.special_races:
                            print(f"************ 回合 {race.turn} 有特殊生涯比赛，需要手动添加")
                    else:
                        races.append(race.turn - 1)
            elif race.condition_type == 2:
                # 自选比赛
                grade = cfg.race_condition_ids[race.condition_id]
                rank = race.condition_value_1
                count = race.condition_value_2
                print(f"  回合 {last_race+1} -> {race.turn} 在 {count} 场 {grade} 中取得 {rank} 名以内")
                freeRaces.append({
                    "startTurn": last_race,
                    "endTurn": race.turn-1,
                    "count": count,
                    "grade": cfg.race_condition_grades[race.condition_id]
                })
            elif race.condition_type == 3:
                # 粉丝数
                print(f"  回合 {last_race+1} -> {race.turn} 取得 {race.condition_value_1} 粉丝")
                freeRaces.append({
                    "startTurn": last_race,
                    "endTurn": race.turn-1,
                    "count": 1
                })
            last_race = race.turn

        # 特殊生涯比赛
        if chara_id in cfg.special_races:
            entry = cfg.special_races[chara_id]
            if "races" in entry:
                races.extend(entry["races"])
            if "freeRaces" in entry:
                freeRaces.extend(entry["freeRaces"])
            if "note" in entry:
                print("** 特殊生涯比赛: " + entry["note"])
                raceNote = entry["note"]

        races.sort()
        freeRaces.sort(key=lambda x: x["startTurn"])

        # 返回
        result[uma.id] = dict(
            bonusData=[],
            fiveStatusBonus=fiveStatusBonus,
            fiveStatusInitial=fiveStatusInitial,
            freeRaces=freeRaces,
            gameId=int(uma.id),
            name=trans_uma.name,
            preferRaces=[],
            preferReds=[],
            races=races,
            raceNote=raceNote,
            star=5
        )
    return result


def write_json(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with codecs.open(path, 'w', encoding='utf-8') as f:
        f.write(jsons.dumps(data, strip_nulls=True, jdkwargs=dict(ensure_ascii=False, indent=2, skipkeys=True)))
        f.write("\n")
    print(f"已写出 {path}（{path.stat().st_size / 1024:.0f} KB）")


def copy_translate_data(cfg: Config, output_dir: Path):
    """把实际使用的翻译数据复制到产物目录（引擎也读这份）。"""
    if not cfg.copy_translate:
        print("按配置跳过翻译数据复制（[output] copy_translate = false）")
        return
    source = next((p for p in (cfg.translate_file, cfg.translate_cache) if p and p.is_file()), None)
    if source is None:
        print("没有可复制的翻译数据，跳过")
        return
    target = (output_dir / TRANSLATE_OUTPUT_NAME).resolve()
    if target == source.resolve():
        print(f"翻译数据已在产物目录：{target}")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    print(f"已复制翻译数据 {source} -> {target}（{target.stat().st_size / 1024 / 1024:.1f} MB）")


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description="从游戏的 master.mdb 导出 cardDB.json / umaDB.json")
    parser.add_argument("--config", help="配置文件路径（默认：脚本目录下的 config.toml）")
    parser.add_argument("--mdb", help="直接指定 master.mdb（优先级最高）")
    parser.add_argument("--output", help="产物目录（覆盖配置里的 [output] dir）")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    try:
        cfg = load_config(args.config)
        mdb_path, mdb_label = resolve_master_mdb(cfg, args.mdb)
    except ConfigError as exc:
        raise SystemExit(str(exc))

    print(f"[db] 使用 master.mdb（{mdb_label}）：{describe_mdb(mdb_path)}")
    if cfg.translate_file and cfg.translate_file.is_file():
        print(f"[i18n] 翻译数据：{cfg.translate_file}")
    else:
        print(f"[i18n] 配置的翻译文件不可用，将联网下载到 {cfg.translate_cache}")
    output_dir = Path(args.output).resolve() if args.output else cfg.output_dir

    db = Umadb(mdb_path)
    trans = translators.TrainersLegendTranslator(None, translate_file=cfg.translate_file,
                                                 cache_file=cfg.translate_cache)

    card_list = db.get_all_support_card_data()
    uma_list = db.get_all_character_card_data()

    cardDB = parseSupportCard(cfg, card_list, trans)
    umaDB = parseUma(cfg, uma_list, trans)

    write_json(output_dir / "cardDB.json", cardDB)
    write_json(output_dir / "umaDB.json", umaDB)
    copy_translate_data(cfg, output_dir)


if __name__ == "__main__":
    main()
