//! L2 数据层集成测试：从仓根 `data/` 载入真实剧本资产与配置，抽查关键数值
//!
//! 单元测试守 schema / 校验逻辑（`data.rs` / `config.rs` 内联），本文件守**仓内资产**：
//! 数据一旦被误改（尤其 mdb 重导出错）就在这里失败，而不是等 S3+ 跑局才发现。

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
        [2200, 1850, 1800, 1700, 1800]
    );
    assert_eq!(data.limits.five_status_limit_upper, [2500; 5]);
    assert_eq!(data.limits.support_num, 5);
    assert_eq!(data.limits.max_skill_point, 99999);
    Ok(())
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

/// 普通训练 / 稽古基础值（Lv1 = mdb；Lv2–5 = memo 2026-10-31；稽古当前只落 Junior 档）
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

/// 剧本配置默认层（融合预设）
#[test]
fn load_repo_config() -> Result<()> {
    let cfg = MatsuriConfig::load(data_dir().join("matsuri_config.toml"), None)?;
    assert_eq!(cfg, MatsuriConfig::default());
    assert_eq!(cfg.fusion.year2.len(), 2);
    assert_eq!(cfg.fusion.year3.len(), 3);
    println!(
        "配置载入成功：year2 {:?} / year3 {:?}",
        cfg.fusion.year2, cfg.fusion.year3
    );
    Ok(())
}
