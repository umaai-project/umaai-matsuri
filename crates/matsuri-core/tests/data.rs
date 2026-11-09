//! L2 数据层集成测试：从仓根 `data/` 载入真实剧本资产与配置，抽查关键数值
//!
//! 单元测试守 schema / 校验逻辑（`data.rs` / `config.rs` 内联），本文件守**仓内资产**：
//! 数据一旦被误改（尤其 mdb 重导出错）就在这里失败，而不是等跑局才发现。

use anyhow::Result;
use matsuri_core::MatsuriConfig;
use matsuri_core::data::MatsuriData;
use matsuri_core::testing::data_dir;

/// 载入仓内剧本资产（含完整校验）
#[test]
fn load_repo_data() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    assert_eq!(data.scenario_id, 15);
    println!(
        "资产载入成功：演目 {} / 教诲 {} / 発表会 buff {} / 一次性奖励 {}",
        data.acts.len(),
        data.advices.len(),
        data.check_points.len(),
        data.check_point_rewards.len()
    );
    Ok(())
}

/// 属性上限与编成位数
#[test]
fn limits() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    assert_eq!(
        data.limits.five_status_limit_base,
        [3200, 2500, 2400, 2200, 2400]
    );
    assert_eq!(data.limits.five_status_limit_upper, [3800; 5]);
    assert_eq!(data.limits.support_num, 5);
    assert_eq!(data.limits.max_skill_point, 99999);
    Ok(())
}

/// 建局上限取剧本资产，而不是引擎通用兜底表 `constants.json`
#[test]
fn newgame_limit_from_scenario_asset() {
    use matsuri_core::testing::sample_game;
    use umasim::game::Game;

    let data = MatsuriData::load(data_dir()).expect("资产载入失败");
    let lim = sample_game().uma().five_status_limit;
    // 开局继承增量只落在速度位：blue_count=[18,0,0,0,0] → floor(18 × 5.33) = 95
    assert_eq!(lim[0], data.limits.five_status_limit_base[0] + 95);
    assert_eq!(lim[1..], data.limits.five_status_limit_base[1..]);
}

/// 演目 ↔ 命令 ↔ 训练位 ↔ 神驹（`act_info` + `act_member` + `single_mode_training`）
#[test]
fn acts() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    let expect = [
        (1, 5101, 0, 1009_u32), // 流鏑馬 / 速度 / ダイワスカーレット
        (2, 5102, 1, 1096_u32), // 神旗争奪戦 / 耐力 / ノーリーズン
        (3, 5103, 2, 1024_u32), // 納舞 / 力量 / マヤノトップガン
        (4, 5104, 3, 1029_u32), // 大曳神輿 / 根性 / ユキノビジン
        (5, 5105, 4, 1128_u32), // 飛駆 / 智力 / ブラストワンピース
    ];
    for (act, (act_type, command_id, train, npc)) in data.acts.iter().zip(expect) {
        assert_eq!(
            (act.act_type, act.command_id, act.train, act.npc_chara_id),
            (act_type, command_id, train, npc)
        );
    }
    Ok(())
}

/// 普通训练 / 稽古基础值（Lv1 = mdb；Lv2–5 = memo；稽古 Junior / Classic 两档）
#[test]
fn training_table() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    let normal = &data.training.normal;
    assert_eq!(normal.len(), 5);
    for levels in normal {
        assert_eq!(levels.len(), 5, "普通训练应满 5 级");
    }
    assert_eq!(normal[0][0], [12, 0, 2, 0, 0, 7, -20], "速度 Lv1");
    assert_eq!(normal[0][4], [16, 0, 3, 0, 0, 7, -25], "速度 Lv5");
    assert_eq!(normal[1][0], [0, 10, 0, 3, 0, 7, -20], "耐力 Lv1");
    assert_eq!(normal[2][3], [0, 7, 12, 0, 0, 7, -25], "力量 Lv4");
    assert_eq!(normal[2][4], [0, 8, 13, 0, 0, 7, -25], "力量 Lv5");
    assert_eq!(normal[3][4], [3, 0, 2, 16, 0, 7, -25], "根性 Lv5 主值 16");
    assert_eq!(normal[4][0], [2, 0, 0, 0, 8, 6, 5], "智力 Lv1（回体 +5）");
    let junior = &data.training.keiko.junior;
    assert_eq!(junior[0], [16, 0, 3, 0, 0, 9, -25], "Junior 流鏑馬");
    assert_eq!(junior[3], [3, 0, 2, 15, 0, 10, -25], "Junior 大曳神輿");
    assert_eq!(junior[4], [3, 0, 0, 0, 12, 8, 5], "Junior 飛駆");
    let classic = &data.training.keiko.classic;
    assert_eq!(classic[0], [21, 0, 3, 0, 0, 10, -25], "Classic 流鏑馬");
    assert_eq!(classic[1], [0, 20, 0, 8, 0, 12, -25], "Classic 神旗争奪戦");
    assert_eq!(classic[2], [0, 8, 20, 0, 0, 12, -25], "Classic 納舞");
    assert_eq!(classic[3], [3, 0, 2, 23, 0, 12, -25], "Classic 大曳神輿");
    assert_eq!(classic[4], [4, 0, 0, 0, 14, 9, 5], "Classic 飛駆");
    Ok(())
}

/// 演目 Lv 效果表（Lv8 跳档 / Lv9 基础上限 / Lv10 頂点）
#[test]
fn act_effects() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    assert_eq!(data.act_effects.len(), 10);
    let lv1 = &data.act_effects[0];
    assert_eq!(lv1.all_status_bonus, 10);
    assert_eq!(lv1.friendship, 2);
    assert_eq!(lv1.sp_training, 20);
    assert!(lv1.hint_event && !lv1.all_hint);
    assert_eq!((lv1.sp_limit, lv1.status_limit), (0, 0));
    let lv8 = &data.act_effects[7];
    assert_eq!((lv8.friendship, lv8.sp_training), (10, 100), "Lv8 跳档");
    assert_eq!(lv8.sp_limit, 100);
    let lv9 = &data.act_effects[8];
    assert_eq!(lv9.status_limit, 20);
    let lv10 = &data.act_effects[9];
    assert_eq!(
        (
            lv10.friendship,
            lv10.sp_training,
            lv10.sp_limit,
            lv10.status_limit
        ),
        (15, 170, 120, 25)
    );
    Ok(())
}

/// 教诲逐级数值（絆平摊 / 不屈阈值 / 両道）
#[test]
fn advices() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    // 絆：+3 平摊，稽古主基础 Lv1 = 3% / Lv10 = 15%
    let advice = &data.advices[0];
    assert_eq!(advice.id, 101);
    let matsuri_core::data::AdviceEffect::Friendship { levels } = &advice.effect else {
        panic!("advice 101 应为絆の教え");
    };
    assert!(levels.iter().all(|level| level.gauge == 3));
    assert_eq!(
        (levels[0].keiko_main_bonus, levels[9].keiko_main_bonus),
        (3, 15)
    );
    // 不屈：体力阈值 45 → 0，Lv8 起给稽古效果
    let unyielding = &data.advices[8];
    assert_eq!(unyielding.id, 901);
    let matsuri_core::data::AdviceEffect::Unyielding { levels } = &unyielding.effect else {
        panic!("advice 901 应为不屈の教え");
    };
    assert_eq!(levels[0].fail_vital_threshold, 45);
    assert_eq!(levels[9].fail_vital_threshold, 0);
    assert_eq!(
        (
            levels[7].keiko_training_effect,
            levels[9].keiko_training_effect
        ),
        (1, 5)
    );
    // 両道：副基础 25% / 稽古效果 15%
    let dual = &data.advices[9];
    assert_eq!(dual.id, 1001);
    let matsuri_core::data::AdviceEffect::Dual { levels } = &dual.effect else {
        panic!("advice 1001 应为両道の教え");
    };
    assert_eq!(
        (levels[9].sub_gain, levels[9].keiko_training_effect),
        (25, 15)
    );
    Ok(())
}

/// 転成升级效果 + 発表会永久 buff + 一次性奖励
#[test]
fn fusion_and_check_points() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    assert_eq!(data.fusion.len(), 5);
    assert_eq!(data.fusion[0].advice_id, 101);
    let check_point = data
        .check_points
        .iter()
        .find(|buff| {
            buff.schedule == 3 && buff.result == matsuri_core::data::CheckPointResult::GreatSuccess
        })
        .expect("应有一行本番大成功 buff");
    assert_eq!(
        (
            check_point.friendship,
            check_point.deyilv,
            check_point.hint_rate,
            check_point.race_bonus
        ),
        (20, 80, 150, 100)
    );
    let reward = data
        .check_point_rewards
        .iter()
        .find(|reward| {
            reward.schedule == Some(3) && reward.result == matsuri_core::data::RewardResult::Success
        })
        .expect("应有一行本番成功一次性奖励");
    assert_eq!((reward.status, reward.skill_pt), (10, 200));
    assert_eq!(
        reward
            .hints
            .iter()
            .map(|hint| (hint.skill.as_str(), hint.level))
            .collect::<Vec<_>>(),
        vec![("序盤巧者", 1), ("彼方の呼び声", 1)]
    );
    let career_end = data
        .career_end_rewards
        .iter()
        .find(|reward| reward.result == matsuri_core::data::RewardResult::GreatSuccess)
        .expect("应有一行育成终了全大成功奖励");
    assert!(career_end.schedule.is_none());
    assert_eq!((career_end.status, career_end.skill_pt), (30, 350));
    Ok(())
}

/// 赛程 / 教诲窗口 / 合宿 / 规则常量 / 出行
#[test]
fn schedule_camp_rules_and_outing() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    let schedule: Vec<(i32, i32)> = data
        .schedule
        .iter()
        .map(|item| (item.turn, item.need_act_level))
        .collect();
    assert_eq!(schedule, vec![(23, 3), (47, 5), (67, 6)]);
    assert!(data.schedule.iter().all(|item| item.need_act_num == 5));
    assert_eq!(data.advice_windows, vec![2, 11, 23, 35, 47, 59]);
    let camp_ends: Vec<i32> = data.camp.windows.iter().map(|window| window.end).collect();
    assert_eq!(camp_ends, vec![15, 39, 63]);
    assert_eq!(data.camp.level_up_turns, camp_ends);
    assert_eq!(data.rules.act_gauge_max, 5);
    assert_eq!(
        (
            data.rules.act_gauge_gain.member,
            data.rules.act_gauge_gain.friendship,
            data.rules.act_gauge_gain.target_race,
            data.rules.act_gauge_gain.other
        ),
        (1, 2, 4, 1)
    );
    assert_eq!(data.rules.train_level_up_count, 4);
    assert_eq!(
        (
            data.rules.act_level_max,
            data.rules.act_level_max_with_friend
        ),
        (6, 10)
    );
    assert_eq!(
        (
            data.rules.friendship_gain.friend,
            data.rules.friendship_gain.card,
            data.rules.friendship_gain.absent,
            data.rules.friendship_gain.lesson,
            data.rules.friendship_gain.charm
        ),
        (4, 7, 0, 3, 2)
    );
    let outing_cards: Vec<u32> = data.outing.iter().map(|out| out.card_id).collect();
    assert!(outing_cards.contains(&30323));
    assert!(outing_cards.contains(&10149));
    assert!(data.outing.iter().all(|out| out.add_level == 1));
    Ok(())
}

/// 引擎静态数据（仓根 `data/events.json`）：比赛奖励已按 `umaai-next` 新基础同步
///
/// 目标赛覆写 5/72；非目标赛用引擎的随机 1 项（`extra_status`）；URA 决赛走独立事件。
/// 引擎进一步调整这些数值时，本用例是剧本仓的第一道「数据未同步」报警。
#[test]
fn engine_events_race_rewards() -> Result<()> {
    use umaai_core::Data;
    use umaai_core::gamedata::ExtraStatus;

    let data = Data::init(data_dir())?;
    let career = data.system_event("race_career")?;
    assert_eq!(
        career.choices[0][0].value.status_pt,
        [5, 5, 5, 5, 5, 72],
        "目标赛覆写 5/72"
    );
    for (key, skill_pt) in [
        ("race_ura_qualifier", 40),
        ("race_ura_semifinal", 60),
        ("race_ura_final", 80),
    ] {
        let event = data.system_event(key)?;
        assert_eq!(
            event.choices[0][0].value.status_pt,
            [10, 10, 10, 10, 10, skill_pt],
            "{key}"
        );
    }
    for (key, value) in [
        ("race_g1", 10),
        ("race_g2", 8),
        ("race_g3", 8),
        ("race_g4", 5),
    ] {
        let event = data.system_event(key)?;
        assert_eq!(
            event.choices[0][0].extra_status,
            Some(ExtraStatus::Random { count: 1, value }),
            "{key} 应为随机 1 项 +{value}"
        );
    }
    println!("引擎事件数据同步：目标赛 5/72、URA 决赛 40/60/80、非目标随机 1 项");
    Ok(())
}

/// 规则常量：上层上限 / 助力支撑；追加稽古数值与变胖解除概率在事件 / 资产侧
#[test]
fn rules_s5_s6() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    let rules = &data.rules;
    assert_eq!(rules.training_upper_cap, 100);
    assert_eq!(
        (rules.assist_support.count, rules.assist_support.effect),
        (3, 5)
    );
    assert_eq!(rules.fat_recovery_prob, 15, "变胖每回合解除概率（暂定值）");
    println!("S5/S6 规则常量：上层上限 100 / 助力支撑 3×5% / 变胖解除 15%");
    Ok(())
}

/// 本番演出 4 行
#[test]
fn finals_act() -> Result<()> {
    let data = MatsuriData::load(data_dir())?;
    assert_eq!(
        (
            data.finals_act.vital,
            data.finals_act.motivation,
            data.finals_act.fail_rate_drop,
            data.finals_act.extra_member
        ),
        (30, 1, 100, 1)
    );
    Ok(())
}

/// 仓内 `data/events.json`：吾妻（`809052xxx`）友人链事件全部就位、无温泉残留
///
/// 槽位口径：`001`=首次 / `032`=训练后 / `004`=出行解锁 / `021`=正月 /
/// `011–015`=出行 1–5 / `023`=结局；`002/003/033`=R 卡、`022`=未满 5 段结局变体（不接）、
/// `031`=随机友人卡事件（忽略）。
#[test]
fn engine_events_friend_chain() -> Result<()> {
    use umaai_core::Data;

    let data = Data::init(data_dir())?;
    let keys = [
        "first", "click", "out", "newyear", "1", "2", "3", "4", "5", "end",
    ];
    assert_eq!(data.events.friend_events.len(), keys.len());
    for (key, expect_id) in [
        ("first", 809052001_u32),
        ("click", 809052032),
        ("out", 809052004),
        ("newyear", 809052021),
        ("1", 809052011),
        ("2", 809052012),
        ("3", 809052013),
        ("4", 809052014),
        ("5", 809052015),
        ("end", 809052023),
    ] {
        let event = data
            .events
            .friend_events
            .get(key)
            .unwrap_or_else(|| panic!("friend_events[{key}] 应存在"));
        assert_eq!(event.id, expect_id, "friend_events[{key}] 的事件 id");
        assert!(
            (809052001..=809052999).contains(&event.id),
            "友人事件应是吾妻 809052xxx，实际 {}（不应残留温泉 80905xxx）",
            event.id
        );
    }
    // 出行解锁（上 / 下双选项）：上 = 体力 +25，下 = 弧を描いて Lv5（hint_level 5）
    let out = &data.events.friend_events["out"];
    assert!(out.player_select, "出行解锁应有上 / 下选项");
    assert_eq!(out.choices.len(), 2);
    assert_eq!(
        (
            out.choices[0][0].value.vital,
            out.choices[0][0].value.friendship
        ),
        (25, 5)
    );
    assert_eq!(out.choices[1][0].value.hint_level, 5);
    // 出行第 3 段也是上 / 下双选项
    let outing3 = &data.events.friend_events["3"];
    assert!(outing3.player_select);
    assert_eq!(outing3.choices[0][0].value.vital, 50);
    // 结局 = 外出 5 次档（速 / 根 / 智 +15、SP +50，按 memo 基础值）
    let end = &data.events.friend_events["end"];
    assert_eq!(end.choices[0][0].value.status_pt, [15, 0, 0, 15, 15, 50]);
    // 训练后事件 = 根性 +4 / SP +4 / 羁绊 +5
    let click = &data.events.friend_events["click"];
    assert_eq!(
        (
            click.choices[0][0].value.status_pt[3],
            click.choices[0][0].value.status_pt[5]
        ),
        (4, 4)
    );
    println!("吾妻友人链事件：10 个槽位全就位、无温泉残留、出行 5 段 + 结局数值与 memo 一致");
    Ok(())
}

/// 仓内 `data/events.json`：构造事件「大胃王」
///
/// 选择 1：体力 +10 / PT +5；选择 2：体力 +30 / PT +10，33% 追加「变胖」（`Ill::FAT`）。
#[test]
fn engine_events_big_eater() -> Result<()> {
    use umaai_core::gamedata::TriggerType;
    use umaai_core::{Data, Ill};

    let data = Data::init(data_dir())?;
    let event = data
        .events
        .uma_events
        .iter()
        .find(|event| event.id == 5010)
        .expect("马娘事件池应含构造事件 5010（大胃王）");
    assert_eq!(event.name, "大胃王");
    assert!(event.player_select);
    assert!(
        matches!(event.trigger, TriggerType::Random { max_time: 3 }),
        "大胃王触发次数应为 3"
    );
    assert_eq!(event.choices.len(), 2);
    assert_eq!(
        (
            event.choices[0][0].value.vital,
            event.choices[0][0].value.status_pt[5]
        ),
        (10, 5)
    );
    let second = &event.choices[1];
    assert_eq!((second[0].prob, second[1].prob), (33, 67));
    assert_eq!(second[0].value.vital, 30);
    assert_eq!(second[0].value.status_pt[5], 10);
    let flags = second[0].add_flags.as_ref().expect("33% 档应附带变胖状态");
    assert!(
        flags.ill.contains(Ill::FAT),
        "33% 档应附 Ill::FAT（变胖）：{:?}",
        flags.ill
    );
    assert!(second[1].add_flags.is_none(), "67% 档无状态");
    println!("大胃王：二选一 + 33% 变胖，max_time = 3");
    Ok(())
}

/// 剧本固定事件（`scenario_matsuri.json`）：`400015104` / `400015106` + 事件额外效果
#[test]
fn scenario_events_asset() -> Result<()> {
    use umaai_core::gamedata::TriggerType;

    let data = MatsuriData::load(data_dir())?;
    assert_eq!(data.scenario_events.len(), 2, "剧本固定事件应只有 2 条");
    let review = data
        .scenario_events
        .iter()
        .find(|event| event.id == 400015104)
        .expect("应有駿大祭合宿の申し出事件");
    assert_eq!(review.name, "駿大祭合宿の申し出", "合宿直前友人事件名");
    assert!(matches!(review.trigger, TriggerType::Fixed { ref turns } if turns == &[11, 35, 59]));
    assert_eq!(review.choices[0][0].value.vital, 15);
    assert_eq!(review.choices[0][0].value.motivation, 1);
    let kensan = data
        .scenario_events
        .iter()
        .find(|event| event.id == 400015106)
        .expect("应有追加の稽古事件");
    assert!(kensan.player_select, "追加稽古应由训练员选上 / 下档");
    assert_eq!(kensan.choices.len(), 2);
    // 上档：直前训练 +20（LastTrain）、SP+20、体力-5、理事长羁绊+5
    let upper = &kensan.choices[0][0];
    assert_eq!(upper.value.vital, -5);
    assert_eq!(upper.value.status_pt[5], 20);
    assert_eq!(upper.value.friendship, 5);
    assert!(matches!(
        upper.extra_status,
        Some(umaai_core::gamedata::ExtraStatus::LastTrain { value: 20 })
    ));
    // 下档：直前训练 +5、体力 +5
    let lower = &kensan.choices[1][0];
    assert_eq!(lower.value.vital, 5);
    assert!(matches!(
        lower.extra_status,
        Some(umaai_core::gamedata::ExtraStatus::LastTrain { value: 5 })
    ));
    // 事件额外效果：上档最低演目 gauge+1 + 解一个 debuff
    assert_eq!(data.event_extras.len(), 1);
    let extra = &data.event_extras[0];
    assert_eq!((extra.event_id, extra.choice), (400015106, 0));
    assert_eq!(extra.act_gauge_lowest, 1);
    assert!(extra.clear_debuff);
    println!("剧本固定事件：駿大祭合宿の申し出 + 追加稽古（上下档 + 事件扩展）");
    Ok(())
}

/// 剧本配置默认层（融合预设）
#[test]
fn load_repo_config() -> Result<()> {
    let cfg = MatsuriConfig::load(data_dir().join("matsuri_config.toml"), None)?;
    let default = MatsuriConfig::default();
    assert_eq!(cfg.pre, default.pre);
    assert_eq!(cfg.search, default.search);
    assert_eq!(cfg.deck, default.deck, "载入后 deck 预置为 [decks.default]");
    assert_eq!(
        cfg.decks.default_params(),
        default.decks.default_params(),
        "[decks.default] 必须与内置兜底一致"
    );
    for name in ["stamina_build", "wisdom_build"] {
        assert!(
            cfg.decks.resolve_name(name).is_some(),
            "默认层应带 {name} 档"
        );
    }
    assert_eq!(cfg.decks.default_params().fusion.year2.len(), 2);
    assert_eq!(cfg.decks.default_params().fusion.year3.len(), 3);
    println!(
        "配置载入成功：year2 {:?} / year3 {:?}",
        cfg.decks.default_params().fusion.year2,
        cfg.decks.default_params().fusion.year3
    );
    Ok(())
}
