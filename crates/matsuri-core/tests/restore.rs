//! 还原层集成：`restore` / `export_festa` 的 round-trip、卡位与分布映射、编成门禁
//!
//! 口径：用仓内真实资产建一局、用手写训练员推进到**决策点**（`Train` 阶段），
//! 再把「帧该给的东西」（剧本段 + 帧人头 + 协议 id 分布）喂回 `restore`，
//! 断言还原态与离线态逐项一致。这一层不依赖真机帧，先把「帧字段 ↔ 状态字段」钉死。

use std::sync::Arc;

use anyhow::{Result, anyhow};
use matsuri_core::data::CheckPointResult;
use matsuri_core::game::festa::{FestaPerson, FestaState};
use matsuri_core::game::restore::RestoreInput;
use matsuri_core::testing::sample_game_with_uma;
use matsuri_core::{MatsuriConfig, MatsuriGame, MatsuriTrainer, PersonOrigin, Stage};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::BasePersonStatus;
use umaai_core::Data;
use umasim::game::Game;

/// 仓内默认测试马娘（与 `testing::sample_game` 同源）
const UMA_ID: u32 = 105902;
/// 还原验证用的决策点回合（合宿已过两轮、発表会已结算两场、神驹两批都在）
const TARGET_TURN: i32 = 60;

/// 推进到 `target` 回合的 `Train` 阶段（= 真机 `command` 帧的时刻）
fn drive_to_train(game: &mut MatsuriGame, target: i32) -> Result<()> {
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    // 4 = `Stage::AfterTrain` 的 `StageId`；先跑到目标的 Distribute，再走一步到 Train
    while !(game.turn() == target && game.stage().get() == 3) {
        assert!(game.turn() <= target, "局面已越过目标回合 {target}");
        game.run_stage(&trainer, &mut decision)?;
        game.next();
    }
    game.run_stage(&trainer, &mut decision)?; // Distribute 结算
    game.next(); // → Train（决策点）
    assert_eq!(game.stage().get(), 1, "应停在 Train 阶段");
    Ok(())
}

/// 由局面卡组造「帧人头」（`baseGame.persons`：卡组顺序、含 `charaId` 与羁绊）
fn frame_persons(game: &MatsuriGame) -> Vec<BasePersonStatus> {
    game.deck
        .iter()
        .map(|card| BasePersonStatus {
            person_type: if card.card_type >= 5 { 1 } else { 2 },
            chara_id: card.data.chara_id,
            friendship: card.friendship,
            is_hint: false,
        })
        .collect()
}

/// 把离线分布（`person_index`）翻成协议 id，模拟真机帧的 `personDistribution`
fn protocol_distribution(game: &MatsuriGame) -> Vec<Vec<i32>> {
    let rows: Vec<Vec<i32>> = game
        .base
        .distribution
        .iter()
        .map(|row| {
            row.iter()
                .map(|index| {
                    let person = &game.persons[*index as usize];
                    match person.origin {
                        PersonOrigin::Card | PersonOrigin::Friend => *index,
                        PersonOrigin::Yayoi => 6,
                        PersonOrigin::Reporter => 7,
                        PersonOrigin::Guest => person.chara_id() as i32,
                    }
                })
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 5, "分布恒 5 个训练位");
    rows
}

/// 分布语义：每行的人头描述（origin + chara_id），用于跨 index 方案比对
fn describe_distribution(game: &MatsuriGame) -> Vec<Vec<(String, u32)>> {
    game.base
        .distribution
        .iter()
        .map(|row| {
            row.iter()
                .filter(|index| **index >= 0)
                .map(|index| {
                    let person = &game.persons[*index as usize];
                    (format!("{:?}", person.origin), person.chara_id())
                })
                .collect()
        })
        .collect()
}

/// 每演目担当人数（卡 / 神驹分开计）
fn act_headcount(game: &MatsuriGame) -> ([usize; 5], [usize; 5]) {
    let mut cards = [0usize; 5];
    let mut guests = [0usize; 5];
    for person in &game.persons {
        let Some(act) = person.act else { continue };
        match person.origin {
            PersonOrigin::Card => cards[act] += 1,
            PersonOrigin::Guest => guests[act] += 1,
            _ => {}
        }
    }
    (cards, guests)
}

/// 主用例：决策点 round-trip 逐项一致
#[test]
fn restore_round_trip_at_decision_point() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;

    let data = Arc::clone(&game.data);
    let config = MatsuriConfig::default();
    // 先把导出的剧本段与协议分布取出来（之后 base 会被 move 给 restore）
    let festa = game.export_festa();
    let raw_dist = protocol_distribution(&game);
    let persons = frame_persons(&game);
    let levels: [i32; 5] = std::array::from_fn(|train| game.train_level(train) as i32);
    let before_dist = describe_distribution(&game);
    let (before_cards, before_guests) = act_headcount(&game);
    let before_plan = game.plan;
    let before_warnings = game.deck_warnings.clone();

    let mut base = game.base.clone();
    // 真机帧的分布是协议 id，底座里也放协议 id（`to_base_game` 就是这么干的）
    base.distribution = raw_dist;
    let restored = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: Some((0, 0)),
        },
        data,
        config,
    )?;

    // 演目 / 教诲 / 合宿 / 発表会 / 资源旗标
    assert_eq!(restored.acts, game.acts, "演目槽与 Lv");
    assert_eq!(restored.advice, game.advice, "教诲槽 / 転成 / 已授予");
    assert_eq!(restored.camp, game.camp, "合宿进度");
    assert_eq!(restored.check_points, game.check_points, "発表会结果");
    assert_eq!(restored.finals_done, game.finals_done, "本番是否已结算");
    assert_eq!(
        restored.assist_support_left, game.assist_support_left,
        "助力支撑"
    );
    assert_eq!(restored.kensan_pending, game.kensan_pending, "追加稽古");
    assert_eq!(restored.fatigue_recovery, game.fatigue_recovery, "疲劳回复");

    // 训练等级：不变式是「与原局面同等级」（不要求 bonus 逐位相同：夹取后可能多解）
    for (train, level) in levels.iter().enumerate() {
        assert_eq!(
            restored.train_level(train),
            game.train_level(train),
            "第 {train} 号训练位等级"
        );
        assert_eq!(
            restored.train_level(train) as i32,
            *level,
            "导出的 tlt 应与还原后等级自洽"
        );
    }

    // 人头结构：卡位担当逐位相同、每演目人数相同、神驹集合相同
    for index in 0..game.deck.len() {
        assert_eq!(
            restored.persons[index].act, game.persons[index].act,
            "第 {index} 张卡的担当"
        );
        assert_eq!(
            restored.persons[index].chara_id(),
            game.persons[index].chara_id(),
            "第 {index} 张卡的身份"
        );
    }
    let (after_cards, after_guests) = act_headcount(&restored);
    assert_eq!(after_cards, before_cards, "每演目担当卡数");
    assert_eq!(after_guests, before_guests, "每演目担当神驹数");
    assert!(
        restored
            .persons
            .iter()
            .any(|person| person.origin == PersonOrigin::Yayoi),
        "理事长必须在场（帧只给羁绊，人头按规则建）"
    );
    assert!(
        restored
            .persons
            .iter()
            .any(|person| person.origin == PersonOrigin::Reporter),
        "记者必须在场"
    );

    // 分布：语义一致（人头集合与顺序），下标可不同（神驹排序不同）
    assert_eq!(
        describe_distribution(&restored),
        before_dist,
        "每训练位站的人"
    );

    // 派生字段
    assert_eq!(restored.plan, before_plan, "回合结构应逐字段一致");
    assert_eq!(restored.forced_rest, game.forced_rest, "强制休息");
    assert_eq!(restored.career_failed, game.career_failed, "育成失败");
    assert_eq!(restored.stage, Stage::Train, "入口阶段");
    assert_eq!(restored.deck_warnings, before_warnings, "编成警告");

    // 二次导出必须与首次导出等价（导出是还原的逆）
    let exported_again = restored.export_festa();
    assert_eq!(exported_again.acts, festa.acts, "演目导出可逆");
    assert_eq!(
        exported_again.camp_remain, festa.camp_remain,
        "合宿导出可逆"
    );
    assert_eq!(
        exported_again.assist_support_left, festa.assist_support_left,
        "助力导出可逆"
    );
    assert_eq!(
        exported_again.check_points, festa.check_points,
        "発表会导出可逆"
    );
    assert_eq!(
        exported_again.train_level_total, festa.train_level_total,
        "训练等级导出可逆"
    );
    Ok(())
}

/// T0 帧：剧本未启动（`persons` 的 `actType` 全 0）——名单保持建局结果，不被清空
#[test]
fn restore_keeps_build_roster_before_scenario_start() -> Result<()> {
    let game = sample_game_with_uma(UMA_ID);
    assert_eq!(game.turn(), 0, "建局在 T0");
    let data = Arc::clone(&game.data);
    // 真机 T0：整份名单都在，但 `actType` 全 0（含 101–115 占位与理事长 / 记者）
    let festa = FestaState {
        acts: Vec::new(),
        persons: (1..=6)
            .map(|id| FestaPerson {
                target_id: id,
                act_type: 0,
                origin: if id == 1 { "friend" } else { "card" }.to_string(),
            })
            .collect(),
        ..FestaState::default()
    };
    let restored = MatsuriGame::restore(
        RestoreInput {
            base: game.base.clone(),
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &frame_persons(&game),
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )?;
    assert_eq!(
        restored.persons.len(),
        game.persons.len(),
        "T0 名单与建局一致"
    );
    for (after, before) in restored.persons.iter().zip(game.persons.iter()) {
        assert_eq!(after.act, before.act, "T0 不做担当覆盖");
    }
    assert!(!restored.advice.acquired, "T0 尚未获得教诲");
    Ok(())
}

/// 帧等级整组为 0（合宿期 `command` / `load` / `special` 帧）→ 沿用上一帧等级
#[test]
fn restore_carries_level_baseline_when_frame_zero() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;
    let data = Arc::clone(&game.data);
    let levels: [i32; 5] = std::array::from_fn(|train| game.train_level(train) as i32);
    let mut festa = game.export_festa();
    festa.train_level_total = Some([0; 5]);

    let mut base = game.base.clone();
    base.distribution = Vec::new();
    let restored = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &frame_persons(&game),
            prev_train_levels: Some(levels),
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )?;
    for (train, level) in levels.iter().enumerate() {
        assert_eq!(
            restored.train_level(train) as i32,
            *level,
            "第 {train} 号训练位等级应沿用上一帧基准"
        );
    }
    Ok(())
}

/// 入口阶段：`isAfterTraining` 的帧还原成 `AfterTrain`
#[test]
fn restore_after_training_stage() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;
    let data = Arc::clone(&game.data);
    let festa = game.export_festa();
    let persons = frame_persons(&game);
    let mut base = game.base.clone();
    base.distribution = Vec::new();
    let restored = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::AfterTrain,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )?;
    assert_eq!(restored.stage, Stage::AfterTrain);
    assert_eq!(restored.stage().get(), 4, "AfterTrain 的 StageId");
    Ok(())
}

/// 编成门禁：帧卡组没带 SSR 吾妻 → 拒绝还原并报错
#[test]
fn restore_rejects_deck_without_ssr_friend() -> Result<()> {
    let game = sample_game_with_uma(UMA_ID);
    let data = Arc::clone(&game.data);
    let festa = game.export_festa();
    let persons = frame_persons(&game);
    // 把友人卡换成第二张普通卡（同卡重复）：卡组仍是 6 张，但友人位空缺
    // —— `BaseGame::new` 不查剧本规则，门禁在 `init_persons` → `inspect_deck` 里
    let mut base = game.base.clone();
    let friend_index = base
        .deck
        .iter()
        .position(|card| card.card_type >= 5)
        .ok_or_else(|| anyhow!("测试卡组应带友人卡"))?;
    let replacement = base.deck[0].clone();
    base.deck[friend_index] = replacement;
    let err = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )
    .expect_err("没带 SSR 吾妻必须拒绝还原");
    let message = format!("{err:#}");
    assert!(
        message.contains("吾妻"),
        "报错要指出缺 SSR 吾妻，实际：{message}"
    );
    Ok(())
}

/// 神驹校验：帧里出现无法解名的 `targetId` → 拒绝还原
#[test]
fn restore_rejects_unknown_guest() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;
    let data = Arc::clone(&game.data);
    let mut festa = game.export_festa();
    festa.persons.push(FestaPerson {
        target_id: 9999,
        act_type: 1,
        origin: "guest".to_string(),
    });
    let persons = frame_persons(&game);
    let mut base = game.base.clone();
    base.distribution = Vec::new();
    let err = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )
    .expect_err("未知神驹必须拒绝还原");
    assert!(format!("{err:#}").contains("9999"), "报错要带上出问题的 id");
    Ok(())
}

/// 卡位顺序自检：帧人头与卡组对不上 → 拒绝还原（不让错位羁绊进决策）
#[test]
fn restore_rejects_roster_order_mismatch() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;
    let data = Arc::clone(&game.data);
    let festa = game.export_festa();
    let mut persons = frame_persons(&game);
    persons.swap(0, 1);
    if persons[0].chara_id == persons[1].chara_id {
        persons[0].chara_id = 9052;
        persons[1].chara_id = 1001;
    }
    let mut base = game.base.clone();
    base.distribution = Vec::new();
    let err = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )
    .expect_err("人头顺序错位必须拒绝还原");
    assert!(format!("{err:#}").contains("顺序"), "报错要指出顺序不一致");
    Ok(())
}

/// 分布映射：`101–115` 占位被丢弃、`6/7` 落到理事长 / 记者、`>=1000` 落到神驹人头
#[test]
fn distribution_maps_protocol_ids() -> Result<()> {
    let mut game = sample_game_with_uma(UMA_ID);
    drive_to_train(&mut game, TARGET_TURN)?;
    let data = Arc::clone(&game.data);
    let festa = game.export_festa();
    let persons = frame_persons(&game);
    let guest = game
        .persons
        .iter()
        .find(|person| person.origin == PersonOrigin::Guest)
        .ok_or_else(|| anyhow!("测试局面应有编成外神驹"))?
        .chara_id();
    let mut base = game.base.clone();
    base.distribution = vec![
        vec![0, 6, 7, 101, -1],
        vec![guest as i32, -1, -1, -1, -1],
        vec![-1, -1, -1, -1, -1],
        vec![-1, -1, -1, -1, -1],
        vec![-1, -1, -1, -1, -1],
    ];
    let restored = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: Some((30, 40)),
        },
        data,
        MatsuriConfig::default(),
    )?;

    let first = &restored.base.distribution[0];
    assert_eq!(first[0], 0, "卡位 id 0 → 卡组下标 0");
    assert_eq!(
        restored.persons[first[1] as usize].origin,
        PersonOrigin::Yayoi,
        "id 6 → 理事长人头"
    );
    assert_eq!(
        restored.persons[first[2] as usize].origin,
        PersonOrigin::Reporter,
        "id 7 → 记者人头"
    );
    assert_eq!(first.len(), 3, "101 占位与 -1 空位都被丢弃");
    let yayoi = restored
        .persons
        .iter()
        .find(|person| person.origin == PersonOrigin::Yayoi)
        .ok_or_else(|| anyhow!("理事长应在场"))?;
    assert_eq!(yayoi.friendship(), 30, "理事长羁绊按帧写回");

    let second = &restored.base.distribution[1];
    let mapped = &restored.persons[second[0] as usize];
    assert_eq!(mapped.origin, PersonOrigin::Guest);
    assert_eq!(mapped.chara_id(), guest, "id ≥ 1000 → 神驹人头");
    Ok(())
}

/// 発表会结果：`resultType` 0/1/2 → NotReached / Success / GreatSuccess，且 `finals_done` 随之
#[test]
fn restore_maps_check_point_results() -> Result<()> {
    let game = sample_game_with_uma(UMA_ID);
    let data = Arc::clone(&game.data);
    let persons = frame_persons(&game);
    let festa = FestaState {
        check_points: Some(vec![
            matsuri_core::game::festa::FestaCheckPoint {
                schedule_id: 2,
                result_type: 2,
            },
            matsuri_core::game::festa::FestaCheckPoint {
                schedule_id: 1,
                result_type: 1,
            },
        ]),
        ..FestaState::default()
    };
    let mut base = game.base.clone();
    base.distribution = Vec::new();
    let restored = MatsuriGame::restore(
        RestoreInput {
            base,
            festa: &festa,
            stage: Stage::Train,
            frame_persons: &persons,
            prev_train_levels: None,
            noncard_friendship: None,
        },
        data,
        MatsuriConfig::default(),
    )?;
    // 数组顺序不保证：按 `scheduleId` 落位
    assert_eq!(restored.check_points[0], CheckPointResult::Success);
    assert_eq!(restored.check_points[1], CheckPointResult::GreatSuccess);
    assert_eq!(restored.check_points[2], CheckPointResult::NotReached);
    assert!(!restored.finals_done, "本番未结算");
    Ok(())
}

/// 真实数据初始化（`Data::init` 幂等，与其它测试共享）
#[test]
fn data_root_is_ready() {
    Data::init(matsuri_core::testing::data_dir()).expect("引擎静态数据初始化");
}
