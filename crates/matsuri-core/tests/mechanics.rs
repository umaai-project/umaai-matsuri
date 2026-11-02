//! S3 机制集成：仓内真实资产建一局，逐阶段跑满 78 回合，守回合结构（`TurnPlan`）/ 赛程 /
//! 合宿窗口与延长 / 発表会 / 本番 / URA 收尾 / 马娘特殊事件（强制休息）/ `MatsuriView` 账本。
//!
//! 训练员用样例 `MatsuriTrainer`：本阶段只关心回合边界，不关心具体选了什么动作。
//! 数据根统一经 `matsuri_core::testing::data_dir()` 取仓根 `data/`（引擎静态表 + 剧本资产）。

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use matsuri_core::data::CheckPointResult;
use matsuri_core::game::deck::inspect_deck;
use matsuri_core::testing::data_dir;
use matsuri_core::{
    Action, DeckError, DeckReport, DeckWarning, MAX_TURN, MatsuriConfig, MatsuriData, MatsuriGame,
    MatsuriPerson, MatsuriTrainer, MatsuriView, PersonOrigin, TurnPlan,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::Config;
use umaai_core::Data;
use umaai_core::gamedata::EventChoice;
use umasim::game::{FriendOutState, Game, Person, SupportCard, Trainer};

/// 5 段友人出行的演目 Lv 期望：每段 Lv+1（三则落点）→ 槽+1（满槽跳过，三则落点）
fn expected_outing_levels(level: [i32; 5], gauge: [i32; 5], gauge_max: i32) -> [i32; 5] {
    let mut level = level;
    let mut gauge = gauge;
    for _ in 0..5 {
        let act = (0..5)
            .min_by_key(|act| (level[*act], gauge[*act], *act as i32))
            .expect("5 演目非空");
        level[act] += 1;
        if let Some(next) = (0..5)
            .filter(|act| gauge[*act] < gauge_max)
            .min_by_key(|act| (level[*act], gauge[*act], *act as i32))
        {
            gauge[next] += 1;
        }
    }
    level
}

/// 按三则落点 / 满槽跳过推进 `count` 次演目槽，返回各演目增量
fn expected_gauge_up(level: [i32; 5], gauge: [i32; 5], count: i32, gauge_max: i32) -> [i32; 5] {
    let mut guage = gauge;
    let mut gained = [0; 5];
    for _ in 0..count {
        let Some(act) = (0..5)
            .filter(|act| guage[*act] < gauge_max)
            .min_by_key(|act| (level[*act], guage[*act], *act as i32))
        else {
            break;
        };
        guage[act] += 1;
        gained[act] += 1;
    }
    gained
}

/// 单回合观测（`Begin` 阶段后的回合结构与资源快照）
struct TurnObs {
    turn: i32,
    plan: TurnPlan,
    view: MatsuriView,
}

/// 仓内资产与配置（一次载入，多处复用）
struct Repo {
    cfg: Config,
    data: Arc<MatsuriData>,
}

impl Repo {
    /// 载入仓内配置 / 引擎静态数据 / 剧本资产
    fn load() -> Result<Self> {
        let root = data_dir();
        let cfg = Config::load_with_overlay_file(&root, ".").context("载入配置失败")?;
        Data::init(&root)?;
        let data = Arc::new(MatsuriData::load(&root).context("载入剧本数据失败")?);
        Ok(Self { cfg, data })
    }

    /// 按指定马娘建一局，并注入固定主种子
    fn game(&self, uma: u32) -> Result<MatsuriGame> {
        self.game_with(uma, &self.cfg.sim.cards)
    }

    /// 按指定马娘 + 自定义卡组建一局
    fn game_with(&self, uma: u32, deck: &[u32]) -> Result<MatsuriGame> {
        let mut game = MatsuriGame::newgame(
            uma,
            deck,
            self.cfg.inherit(),
            self.cfg.race_grades()?,
            Arc::clone(&self.data),
        )?;
        game.set_rule_master(42);
        Ok(game)
    }
}

/// 逐阶段跑完整局：每回合 `Begin` 阶段跑完回调一次，并记录观测
fn sweep(
    mut game: MatsuriGame,
    mut on_begin: impl FnMut(i32, &mut MatsuriGame) -> Result<()>,
) -> Result<Vec<TurnObs>> {
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    let mut obs = Vec::with_capacity((MAX_TURN + 1) as usize);
    for turn in 0..=MAX_TURN {
        for stage_idx in 0..5 {
            game.run_stage(&trainer, &mut decision)?;
            if stage_idx == 0 {
                on_begin(turn, &mut game)?;
                obs.push(TurnObs {
                    turn,
                    plan: game.plan,
                    view: game.view(),
                });
            }
            let has_next = game.next();
            let expect_next = stage_idx < 4 || turn < MAX_TURN;
            assert_eq!(
                has_next, expect_next,
                "阶段推进异常：turn {turn} stage_idx {stage_idx}"
            );
        }
    }
    Ok(obs)
}

/// 本回合应处的合宿档：`(年次, 是否原窗口内)`；不在合宿窗口为 `None`
fn camp_expect(turn: i32) -> Option<(usize, bool)> {
    match turn {
        12..=15 => Some((0, true)),
        36..=39 => Some((1, true)),
        60..=63 => Some((2, true)),
        _ => None,
    }
}

/// 回合结构 / 赛程 / 発表会 / URA 收尾 / View 账本初值
#[test]
fn turn_structure() -> Result<()> {
    let repo = Repo::load()?;
    let obs = sweep(repo.game(repo.cfg.sim.uma)?, |turn, game| {
        if turn == 11 {
            assert!(game.plan.race, "turn 11 是目标赛回合（生涯赛程）");
        }
        if turn == 65 {
            assert!(
                !game.forced_rest,
                "特别周没有特殊事件记录，T65 不应被改成强制休息"
            );
        }
        Ok(())
    })?;
    for o in &obs {
        let turn = o.turn;
        let plan = o.plan;
        let check_point = match turn {
            23 => Some(0),
            47 => Some(1),
            67 => Some(2),
            _ => None,
        };
        assert_eq!(
            plan.check_point, check_point,
            "turn {turn} 発表会 / 本番场次"
        );
        assert_eq!(plan.finals(), turn == 67, "turn {turn} 本番标记");
        assert_eq!(o.view.race, plan.race, "turn {turn} View 比赛标记");
        assert_eq!(o.view.finals, plan.finals(), "turn {turn} View 本番标记");
        assert_eq!(
            o.view.check_point, plan.check_point,
            "turn {turn} View 発表会场次"
        );
        assert_eq!(
            o.view.advice_window,
            plan.advice_window.is_some(),
            "turn {turn} View 教诲窗口"
        );
        assert_eq!(plan.ura_period, turn >= 68, "turn {turn} URA 收尾期");
        assert_eq!(
            plan.ura_race,
            matches!(turn, 73 | 75 | 77),
            "turn {turn} URA 决赛"
        );
        assert_eq!(
            plan.advice_window.is_some(),
            matches!(turn, 2 | 11 | 23 | 35 | 47 | 59),
            "turn {turn} 教诲窗口"
        );
        assert_eq!(
            plan.camp_level_up,
            matches!(turn, 15 | 39 | 63),
            "turn {turn} 合宿 Lv+1 结算点"
        );
        match camp_expect(turn) {
            Some((year, true)) => {
                assert!(plan.camp_zone, "turn {turn} 应在合宿区");
                assert_eq!(plan.camp_year, Some(year), "turn {turn} 合宿年次");
                assert!(
                    plan.camp_regular && !plan.camp_extended,
                    "turn {turn} 原窗口内"
                );
                assert_eq!(plan.camp_rest, year >= 1, "turn {turn} 合宿版休息档");
                assert!(o.view.camp_active && o.view.camp_training);
                let start = [12, 36, 60][year];
                assert_eq!(
                    o.view.camp_counted as i32,
                    turn - start,
                    "turn {turn} 已计入合宿回合数"
                );
            }
            _ => {
                assert!(!plan.camp_zone, "turn {turn} 不应在合宿区");
                assert_eq!(plan.camp_year, None, "turn {turn} 合宿年次");
                assert!(!o.view.camp_active);
            }
        }
    }
    // View 账本初值：演目 Lv 从 1 起、gauge / 教诲槽清零、発表会未结算
    let first = &obs[0];
    assert_eq!(first.turn, 0);
    assert_eq!(first.view.act_level, [1; 5]);
    assert_eq!(first.view.act_gauge, [0; 5]);
    assert_eq!(
        first.view.act_level_max, 10,
        "参考卡组带 SSR 吾妻 → 演目 Lv 上限 10"
    );
    assert_eq!(first.view.advice_slots, [0; 5]);
    assert_eq!(first.view.advice_fused, [0; 5]);
    assert!(!first.view.race && !first.view.finals);
    assert_eq!(first.view.check_point, None);
    assert!(!first.view.advice_window);
    assert!(!first.view.forced_rest);
    assert_eq!(first.view.check_points, [CheckPointResult::NotReached; 3]);
    assert!(!first.view.finals_done);
    assert!(!first.view.ura_period);
    println!("回合结构：78 回合 / 発表会 3 场 / 合宿 3 次 / 教诲窗口 6 个；View 初值 OK");
    Ok(())
}

/// 合宿延长：原窗口最后一回合被强制休息占用时不计入 4 回合，合宿顺延一个回合
#[test]
fn camp_forced_rest_extends() -> Result<()> {
    let repo = Repo::load()?;
    let obs = sweep(repo.game(repo.cfg.sim.uma)?, |turn, game| {
        if turn == 39 {
            // 模拟事件管线在本回合设置的强制休息占用（S3 只做排除判定）
            game.forced_rest = true;
        }
        Ok(())
    })?;
    let at = |turn: i32| &obs[turn as usize];
    for turn in 36..=38 {
        let o = at(turn);
        assert!(
            o.plan.camp_zone && o.plan.camp_regular && !o.plan.camp_extended,
            "turn {turn} 应为原窗口合宿"
        );
    }
    let occupied = at(39);
    assert!(occupied.plan.camp_zone && occupied.plan.camp_regular);
    assert_eq!(occupied.plan.camp_year, Some(1));
    assert!(occupied.plan.camp_level_up, "Lv+1 仍固定在原窗口结算点结算");
    let extended = at(40);
    assert!(extended.plan.camp_zone && extended.plan.camp_extended);
    assert_eq!(extended.plan.camp_year, Some(1));
    assert!(!extended.plan.camp_rest, "延长回合休息为普通休息");
    assert_eq!(extended.view.camp_counted, 3, "被强制休息占用的回合不计入");
    assert!(extended.view.camp_training);
    let after = at(41);
    assert!(!after.plan.camp_zone, "计满 4 个非比赛回合后合宿结束");
    assert_eq!(after.view.camp_counted, 0);
    println!("合宿延长：turn 39 强制休息 → 顺延到 turn 40，turn 41 结束");
    Ok(())
}

/// 马娘特殊事件：凯斯奇迹 T65 仍是目标赛回合，但实际执行休息（临时记录）
#[test]
fn keis_miracle_t65_forced_rest() -> Result<()> {
    let repo = Repo::load()?;
    let obs = sweep(repo.game(109301)?, |turn, game| {
        let expected = turn == 65;
        assert_eq!(
            game.forced_rest, expected,
            "turn {turn} 凯斯奇迹强制休息标记"
        );
        if turn == 65 {
            assert!(game.plan.race, "T65 仍按目标赛回合分类（赛程标记不变）");
            assert!(game.view().forced_rest);
        }
        Ok(())
    })?;
    let t65 = &obs[65];
    assert!(t65.plan.race && t65.view.forced_rest);
    assert!(!t65.plan.camp_zone, "T65 在合宿窗口外（样例赛程）");
    println!("凯斯奇迹 T65：目标赛回合分类保留 + 强制休息标记；T64 / T66 无标记");
    Ok(())
}

/// 友人卡门禁：只有 SSR 吾妻可建局；R 吾妻 / 其他友人 / 不带友人一律报错
#[test]
fn friend_must_be_ssr_azuma() -> Result<()> {
    let repo = Repo::load()?;
    let cards = repo.cfg.sim.cards.clone();
    let last = cards.len() - 1;
    let grades = repo.cfg.race_grades()?;
    let build = |deck: &[u32]| {
        MatsuriGame::newgame(
            repo.cfg.sim.uma,
            deck,
            repo.cfg.inherit(),
            grades,
            Arc::clone(&repo.data),
        )
    };
    // R 吾妻（10149 rank4）
    let mut r_deck = cards.clone();
    r_deck[last] = 101494;
    assert!(build(&r_deck).is_err(), "R 吾妻应报错");
    // 其他 SSR 友人卡（塔可布莱恩 30257 rank4）
    let mut other_deck = cards.clone();
    other_deck[last] = 302574;
    assert!(build(&other_deck).is_err(), "非吾妻友人卡应报错");
    // 不带友人
    assert!(build(&cards[..last]).is_err(), "不带友人应报错");
    // 参考卡组（SSR 吾妻）可建局
    let game = build(&cards)?;
    assert_eq!(game.act_level_cap(), 10);
    println!("友人卡门禁：R 吾妻 / 其他友人 / 不带友人报错；SSR 吾妻可建局（Lv 上限 10）");
    Ok(())
}

/// 人头名单与担当分配：前缀同序 / 同 chara 唯一 / link 钉位 / 余额 NPC / 第 2 批
#[test]
fn roster_and_act_assignment() -> Result<()> {
    let repo = Repo::load()?;
    let game = repo.game(repo.cfg.sim.uma)?;
    // 6 卡 + 理事长 + 记者 + 第 1 批 6（5 神驹 + 余额 1）+ 第 2 批 5
    assert_eq!(game.persons.len(), 6 + 1 + 1 + 6 + 5);
    // persons 前缀与 deck 逐槽同序（引擎布局契约：热路径按下标直取卡组）
    for (person, card) in game.persons.iter().zip(&game.base.deck) {
        assert_eq!(person.card_id(), Some(card.card_id));
    }
    // 同 chara_id 只允许一个人头（分身只在 distribution 里重复下标）
    let mut seen = HashSet::new();
    for person in &game.persons {
        assert!(
            seen.insert(person.chara_id()),
            "出现重复 chara_id: {}",
            person.chara_id()
        );
    }
    // 担当落位
    let mut cards = [0usize; 5];
    let mut first_batch = [0usize; 5];
    let mut second_batch = [0usize; 5];
    for person in &game.persons {
        match person.origin {
            PersonOrigin::Card => {
                let act = person.act.expect("支援卡应有担当");
                cards[act] += 1;
            }
            PersonOrigin::Guest => {
                let act = person.act.expect("编成外神驹应有担当");
                if person.needs_great_success {
                    second_batch[act] += 1;
                } else {
                    first_batch[act] += 1;
                }
            }
            _ => assert!(person.act.is_none(), "{:?} 不应有担当", person.origin),
        }
    }
    assert!(
        cards.iter().all(|&count| count <= 2),
        "整位最多 2 张卡: {cards:?}"
    );
    assert_eq!(cards, [2, 1, 1, 0, 1], "参考卡组落位：速2/耐1/力1/毅0/智1");
    assert_eq!(
        first_batch,
        [1, 1, 1, 2, 1],
        "5 神驹各 1 + 余额给最少位（毅力）"
    );
    assert_eq!(second_batch, [1, 1, 1, 1, 1], "第 2 批每演目 +1");
    // 参考卡组的力卡=摩耶重炮（chara 1024，力演目 link）：与固定神驹同角色 → 换成随机非 link
    let act2_first: Vec<u32> = game
        .persons
        .iter()
        .filter(|person| {
            person.origin == PersonOrigin::Guest
                && !person.needs_great_success
                && person.act == Some(2)
        })
        .map(MatsuriPerson::chara_id)
        .collect();
    assert_eq!(act2_first.len(), 1);
    assert_ne!(
        act2_first[0], 1024,
        "同 chara 已被编成卡占用，应换成随机非 link"
    );
    // 随机补人的神驹（余额 / 第 2 批）不能是剧本 link 角色，也不能是育成马娘
    let link_chars: HashSet<u32> = game
        .data
        .acts
        .iter()
        .map(|act| act.npc_chara_id)
        .chain(std::iter::once(9052))
        .collect();
    let trained = game.uma().chara_id();
    for person in &game.persons {
        if person.origin != PersonOrigin::Guest || person.chara_id() == 1024 {
            continue;
        }
        assert_ne!(person.chara_id(), trained, "神驹不能与育成马娘同角色");
    }
    for person in &game.persons {
        if person.origin == PersonOrigin::Guest && person.needs_great_success {
            assert!(
                !link_chars.contains(&person.chara_id()),
                "随机补人不能是剧本 link 角色"
            );
        }
    }
    println!(
        "人头名单：{} 人 / 卡落位 {cards:?} / 第1批 {first_batch:?} / 第2批 {second_batch:?} / 警告 {deck_warnings:?}",
        game.persons.len(),
        deck_warnings = game.deck_warnings
    );
    Ok(())
}

/// 人头可用回合：吾妻与第 1 批神驹 turn 2 起、第 2 批需大成功、理事长 / 记者按引擎默认
#[test]
fn person_availability_by_turn() -> Result<()> {
    let repo = Repo::load()?;
    let _ = sweep(repo.game(repo.cfg.sim.uma)?, |turn, game| {
        let avail = |origin: PersonOrigin| -> Vec<bool> {
            game.persons
                .iter()
                .enumerate()
                .filter(|(_, person)| person.origin == origin)
                .map(|(index, _)| game.person_is_available(index))
                .collect()
        };
        match turn {
            0 => {
                assert!(avail(PersonOrigin::Card).iter().all(|ok| *ok));
                assert!(
                    avail(PersonOrigin::Friend).iter().all(|ok| !*ok),
                    "吾妻 turn 2 前不可用"
                );
                assert!(
                    avail(PersonOrigin::Guest).iter().all(|ok| !*ok),
                    "神驹 turn 2 前不可用"
                );
                assert!(
                    avail(PersonOrigin::Reporter).iter().all(|ok| !*ok),
                    "记者 turn 13 前不可用"
                );
                assert!(avail(PersonOrigin::Yayoi).iter().all(|ok| *ok));
            }
            2 => {
                assert!(avail(PersonOrigin::Friend).iter().all(|ok| *ok));
                let first: Vec<bool> = game
                    .persons
                    .iter()
                    .enumerate()
                    .filter(|(_, person)| {
                        person.origin == PersonOrigin::Guest && !person.needs_great_success
                    })
                    .map(|(index, _)| game.person_is_available(index))
                    .collect();
                assert!(first.iter().all(|ok| *ok), "第 1 批神驹 turn 2 起可用");
                let second: Vec<bool> = game
                    .persons
                    .iter()
                    .enumerate()
                    .filter(|(_, person)| {
                        person.origin == PersonOrigin::Guest && person.needs_great_success
                    })
                    .map(|(index, _)| game.person_is_available(index))
                    .collect();
                assert!(
                    second.iter().all(|ok| !*ok),
                    "第 2 批需 Classic 発表会大成功"
                );
            }
            13 => {
                assert!(
                    avail(PersonOrigin::Reporter).iter().all(|ok| *ok),
                    "记者 turn 13 起可用"
                );
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(())
}

/// 育成马娘本身是剧本 link 角色时：该演目的固定神驹由随机非 link 角色顶替
#[test]
fn trained_uma_link_replaces_fixed_guest() -> Result<()> {
    let repo = Repo::load()?;
    // ダイワスカーレット（chara 1009）= 流鏑馬（act 0）的剧本 link 角色
    let game = repo.game(100901)?;
    assert_eq!(game.uma().chara_id(), 1009);
    let act0_first: Vec<u32> = game
        .persons
        .iter()
        .filter(|person| {
            person.origin == PersonOrigin::Guest
                && !person.needs_great_success
                && person.act == Some(0)
        })
        .map(MatsuriPerson::chara_id)
        .collect();
    assert_eq!(act0_first.len(), 1);
    assert_ne!(
        act0_first[0], 1009,
        "育成马娘是同演目 link 角色 → 换成随机非 link 顶替"
    );
    let mut seen = HashSet::new();
    for person in &game.persons {
        assert!(seen.insert(person.chara_id()), "出现重复 chara_id");
    }
    println!(
        "育成马娘=link 角色：act0 固定神驹已换人（chara {}）",
        act0_first[0]
    );
    Ok(())
}

/// 编成门禁：团队卡 / 同角色卡 / 普通卡数量不对都要报错
#[test]
fn deck_rules_rejected() -> Result<()> {
    let repo = Repo::load()?;
    let cards = repo.cfg.sim.cards.clone();
    let grades = repo.cfg.race_grades()?;
    let build = |deck: &[u32]| {
        MatsuriGame::newgame(
            repo.cfg.sim.uma,
            deck,
            repo.cfg.inherit(),
            grades,
            Arc::clone(&repo.data),
        )
    };
    // 团队卡（card_type 6）→ 报错
    let mut team = cards.clone();
    team[0] = 300674;
    assert!(build(&team).is_err(), "团队卡应报错");
    // 同角色两张卡（待兼诗歌剧 30317 / 20069 同 chara）→ 报错
    let mut duplicate = cards.clone();
    duplicate[1] = 200694;
    assert!(build(&duplicate).is_err(), "同角色支援卡应报错");
    // 普通支援卡不足 5 张 → 报错
    let mut fewer = cards.clone();
    fewer.remove(0);
    assert!(build(&fewer).is_err(), "普通支援卡数量不对应报错");
    println!("编成门禁：团队卡 / 同角色卡 / 数量不对均报错");
    Ok(())
}

/// 编成自查：禁止项（聚合）与「合理情况」不产生警告
#[test]
fn deck_inspection() -> Result<()> {
    let repo = Repo::load()?;
    let reference = repo.cfg.sim.cards.clone();
    let inspect = |ids: &[u32]| -> Result<DeckReport> {
        let deck: Vec<SupportCard> = ids
            .iter()
            .map(|idrank| SupportCard::new(*idrank))
            .collect::<Result<Vec<_>>>()?;
        inspect_deck(&deck, &repo.data, repo.cfg.sim.uma / 100)
    };
    // 参考卡组：禁止项为空；link=固定演目（力重炮）属合理情况，不警告
    let report = inspect(&reference)?;
    assert!(report.is_ok(), "参考卡组应通过：{}", report.error_message());
    assert!(
        report.warnings.is_empty(),
        "参考卡组不应有警告: {:?}",
        report.warnings
    );
    // R 吾妻（10149 rank4）→ 不是 SSR 吾妻
    let last = reference.len() - 1;
    let mut r_azuma = reference.clone();
    r_azuma[last] = 101494;
    assert!(
        inspect(&r_azuma)?
            .errors
            .contains(&DeckError::FriendCardNotSsrAzuma { card_id: 10149 })
    );
    // 没带友人（5 张）→ MissingSsrAzuma + 通用规则（必须 6 张）
    let no_friend = reference[..last].to_vec();
    let errors = inspect(&no_friend)?.errors;
    assert!(errors.contains(&DeckError::MissingSsrAzuma));
    assert!(errors.iter().any(|error| matches!(
        error,
        DeckError::GeneralRuleViolation { reasons } if reasons.iter().any(|reason| reason.contains("必须 6 张"))
    )));
    // 团队卡（7 张）→ 通用规则（张数 / 卡类型）
    let mut team = reference.clone();
    team.push(300674);
    assert!(inspect(&team)?.errors.iter().any(|error| matches!(
        error,
        DeckError::GeneralRuleViolation { reasons }
            if reasons.iter().any(|reason| reason.contains("卡类型不合法"))
    )));
    // 同角色支援卡（待兼诗歌剧 30317 / 20069 同 chara）→ 通用规则（重复人物）
    let mut duplicate = reference.clone();
    duplicate[1] = 200694;
    assert!(inspect(&duplicate)?.errors.iter().any(|error| matches!(
        error,
        DeckError::GeneralRuleViolation { reasons }
            if reasons.iter().any(|reason| reason.contains("存在重复人物"))
    )));
    // 两张友人卡（SSR 吾妻 + 塔可布莱恩）
    let mut two_friends = reference.clone();
    two_friends.push(302574);
    assert!(
        inspect(&two_friends)?
            .errors
            .contains(&DeckError::MultipleFriendCards { count: 2 })
    );
    // 包含担当马娘本人：inspect 的 trained_chara 传 1062（待兼诗歌剧）
    let deck: Vec<SupportCard> = reference
        .iter()
        .map(|idrank| SupportCard::new(*idrank))
        .collect::<Result<Vec<_>>>()?;
    assert!(
        inspect_deck(&deck, &repo.data, 1062)?
            .errors
            .iter()
            .any(|error| matches!(
                error,
                DeckError::GeneralRuleViolation { reasons }
                    if reasons.iter().any(|reason| reason.contains("包含担当马娘本人"))
            ))
    );
    println!(
        "编成自查：禁止项（缺吾妻 / R 吾妻 / 团队卡 / 重复人物 / 多友人 / 含担当马娘）均能报出"
    );
    Ok(())
}

/// 编成警告：类型集中（5 速）→ W2/W3/W4
#[test]
fn deck_warnings_type_concentration() -> Result<()> {
    let repo = Repo::load()?;
    // 5 张速度卡（不同角色、非 link）+ SSR 吾妻
    let deck = [303174, 303114, 302534, 302654, 302754, 303234];
    let game = repo.game_with(repo.cfg.sim.uma, &deck)?;
    assert!(
        game.deck_warnings
            .contains(&DeckWarning::TypeConcentration {
                card_type: 0,
                count: 5
            }),
        "5 张同类卡应警告: {:?}",
        game.deck_warnings
    );
    let dispatched = game
        .deck_warnings
        .iter()
        .filter(|warning| matches!(warning, DeckWarning::CardDispatched { .. }))
        .count();
    assert!(
        dispatched >= 2,
        "5 张同类必然外派: {:?}",
        game.deck_warnings
    );
    assert!(
        game.deck_warnings
            .contains(&DeckWarning::FewTypeKinds { kinds: 2 }),
        "5 速 + 友人 = 2 种类型应警告: {:?}",
        game.deck_warnings
    );
    // 各演目人物数 3/2/2/3/1 → 极差 2 → W10
    assert!(
        game.deck_warnings
            .contains(&DeckWarning::UnevenActPersons { min: 1, max: 3 }),
        "人物数极差 ≥2 应警告: {:?}",
        game.deck_warnings
    );
    // View 同步下发警告
    assert_eq!(game.view().deck_warnings, game.deck_warnings);
    println!("编成警告（5 速）：{:?}", game.deck_warnings);
    Ok(())
}

/// 编成警告：link 卡类型与固定演目不一致（速重炮钉在力量演目）
#[test]
fn deck_warnings_link_type_mismatch() -> Result<()> {
    let repo = Repo::load()?;
    // 把力重炮（30322）换成速重炮（30072）：同为摩耶重炮（chara 1024，力量演目 link）
    let deck = [303174, 303114, 303004, 300724, 303204, 303234];
    let game = repo.game_with(repo.cfg.sim.uma, &deck)?;
    assert!(
        game.deck_warnings.contains(&DeckWarning::LinkTypeMismatch {
            card_id: 30072,
            card_type: 0,
            act: 2,
        }),
        "速重炮钉力量演目应警告: {:?}",
        game.deck_warnings
    );
    println!("编成警告（link 类型不一致）：{:?}", game.deck_warnings);
    Ok(())
}

/// 编成警告：各演目「卡+NPC」人物数极差 ≥2（本局 3/2/2/3/1）
#[test]
fn deck_warning_uneven_persons() -> Result<()> {
    let repo = Repo::load()?;
    // 4 速 + 力重炮（link，= 力量演目固定神驹同角色 → 顶替）+ 吾妻
    // 卡落位：速 2 / 毅力 2 / 力量 1；NPC：5 神驹 + 余额给耐力/智力最少位
    // → 人物数约 3/2/2/4/1，极差 ≥2 → W10
    let deck = [303174, 303114, 302534, 302654, 303224, 303234];
    let game = repo.game_with(repo.cfg.sim.uma, &deck)?;
    assert!(
        game.deck_warnings
            .iter()
            .any(|warning| matches!(warning, DeckWarning::UnevenActPersons { .. })),
        "人物数极差 ≥2 应警告: {:?}",
        game.deck_warnings
    );
    println!("编成警告（各演目人物数极差 ≥2）：{:?}", game.deck_warnings);
    Ok(())
}

/// `MatsuriView` 值快照可序列化往返（宿主传输格式由剧本提供）
#[test]
fn view_snapshot_roundtrip() -> Result<()> {
    let repo = Repo::load()?;
    let game = repo.game(repo.cfg.sim.uma)?;
    let view = game.view();
    let text = serde_json::to_string(&view).context("序列化 View 失败")?;
    let back: MatsuriView = serde_json::from_str(&text).context("反序列化 View 失败")?;
    assert_eq!(back, view);
    println!("View 快照往返 OK：{text}");
    Ok(())
}

// ---------------------------------------------------------------------------
// S4：动作集与单动作结算
// ---------------------------------------------------------------------------

/// 测试用训练员：动作固定、事件选项固定（不成立时直接报错，避免静默选别的动作）
struct ScriptedTrainer {
    action: Action,
    choice: usize,
}

impl Trainer<MatsuriGame> for ScriptedTrainer {
    fn select_action(
        &self,
        _game: &MatsuriGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        actions
            .iter()
            .position(|action| action == &self.action)
            .ok_or_else(|| anyhow!("候选动作缺少 {:?}", self.action))
    }

    fn select_choice(
        &self,
        _game: &MatsuriGame,
        _choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        Ok(self.choice)
    }
}

/// 局面数值快照（测单动作增量用）
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stats {
    five: [i32; 5],
    pt: i32,
    vital: i32,
    motivation: i32,
}

impl Stats {
    fn of(game: &MatsuriGame) -> Self {
        let uma = game.uma();
        Self {
            five: uma.five_status,
            pt: uma.skill_pt,
            vital: uma.vital,
            motivation: uma.motivation,
        }
    }
}

/// 事件已结算次数（按事件 id）
fn event_count(game: &MatsuriGame, id: u32) -> u32 {
    *game.base.events.get(&id).unwrap_or(&0)
}

/// 用样例训练员把局面推进到 `target` 回合的 **Distribute 阶段**：
/// 该回合 `Begin` 已执行（`plan` / `forced_rest` 已派生），`Train` 尚未决策。
fn drive_to_turn(game: &mut MatsuriGame, target: i32) -> Result<()> {
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    while !(game.turn() == target && game.stage().get() == 3) {
        assert!(game.turn() <= target, "局面已越过目标回合 {target}");
        game.run_stage(&trainer, &mut decision)?;
        game.next();
    }
    Ok(())
}

/// 从当前阶段（Begin / Distribute）起用脚本训练员跑完本回合，并推进到下一回合 Begin
fn finish_turn(game: &mut MatsuriGame, trainer: &impl Trainer<MatsuriGame>) -> Result<()> {
    let mut decision = StdRng::seed_from_u64(11);
    loop {
        let stage = game.stage().get();
        game.run_stage(trainer, &mut decision)?;
        game.next();
        if stage == 2 {
            // 刚执行完 End 阶段
            break;
        }
    }
    Ok(())
}

/// 一回合的动作集按回合类型分叉：普通 / 合宿年 1 / 合宿年 2·3 / 目标赛 / 强制休息 / 自选比赛 / URA
#[test]
fn action_set_by_turn_type() -> Result<()> {
    let repo = Repo::load()?;

    // 普通回合（turn 2）：5 普通训练 + 休息 + 普通出行；无比赛 / 稽古 / 友人出行 / 治病
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 2)?;
    let actions = game.list_actions()?;
    let trains: Vec<u8> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Train(train) => Some(*train),
            _ => None,
        })
        .collect();
    assert_eq!(trains, vec![0, 1, 2, 3, 4], "普通回合 5 个训练位");
    assert!(actions.contains(&Action::Rest));
    assert!(actions.contains(&Action::NormalOuting));
    assert!(!actions.contains(&Action::Race), "turn 2 未进自选比赛窗口");
    assert!(!actions.contains(&Action::FriendOuting), "吾妻未解锁");
    assert!(!actions.contains(&Action::Clinic));
    assert!(!actions.iter().any(|a| matches!(a, Action::ActTrain(_))));

    // 第 1 年合宿（turn 12）：5 稽古 + 休息 + 出行（第 1 年可出行）
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 12)?;
    let actions = game.list_actions()?;
    let acts: Vec<u8> = actions
        .iter()
        .filter_map(|action| match action {
            Action::ActTrain(act) => Some(*act),
            _ => None,
        })
        .collect();
    assert_eq!(acts, vec![0, 1, 2, 3, 4], "合宿回合 5 个训练位全变稽古");
    assert!(actions.contains(&Action::Rest));
    assert!(actions.contains(&Action::NormalOuting), "第 1 年合宿可出行");
    assert!(!actions.iter().any(|a| matches!(a, Action::Train(_))));

    // 第 2 年合宿（turn 36）：5 稽古 + 休息；第 2/3 年合宿不可出行
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 36)?;
    let actions = game.list_actions()?;
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::ActTrain(_)))
            .count(),
        5
    );
    assert!(actions.contains(&Action::Rest));
    assert!(!actions.contains(&Action::NormalOuting));
    assert!(!actions.contains(&Action::FriendOuting));

    // 目标赛回合（turn 11）：只有比赛
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 11)?;
    assert!(game.plan.race, "turn 11 是目标赛回合");
    assert_eq!(game.list_actions()?, vec![Action::Race]);

    // 凯斯奇迹 T65：目标赛被强制休息占用 → 只有休息
    let mut game = repo.game(109301)?;
    drive_to_turn(&mut game, 65)?;
    assert!(game.plan.race && game.forced_rest);
    assert_eq!(game.list_actions()?, vec![Action::Rest]);

    // 自选比赛窗口：13–71 中等级非 0 的回合含比赛动作
    let grades = repo.cfg.race_grades()?;
    let race_turn = (16..=35)
        .find(|turn| grades[*turn as usize] > 0)
        .context("样例赛程在 16–35 应至少有一个可自选比赛回合")?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, race_turn)?;
    let actions = game.list_actions()?;
    assert!(
        actions.contains(&Action::Race),
        "turn {race_turn} 应可自选比赛"
    );

    // URA 决赛（turn 73）：只有比赛
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 73)?;
    assert_eq!(game.list_actions()?, vec![Action::Race]);

    println!(
        "动作集：普通 / 合宿年 1 / 合宿年 2 / 目标赛 / 强制休息 / 自选比赛（turn {race_turn}）/ URA 全部分支正确"
    );
    Ok(())
}

/// 目标赛结算：比赛事件入队并在 `AfterTrain` 入账；五维与 SP 都吃比赛加成（floor）
#[test]
fn target_race_reward_applied() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 11)?;
    assert!(game.plan.race, "turn 11 是目标赛回合");
    let scale = (100 + game.uma().race_bonus) as f64 / 100.0;
    let career_id = Data::get()?.system_event("race_career")?.id;
    let before = Stats::of(&game);
    let races_before = event_count(&game, career_id);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Race,
            choice: 0,
        },
    )?;
    let after = Stats::of(&game);
    for axis in 0..5 {
        let expect = (5.0 * scale).floor() as i32;
        assert_eq!(
            after.five[axis] - before.five[axis],
            expect,
            "五维 axis {axis} 应为 floor(5 × {scale:.2})"
        );
    }
    assert_eq!(
        after.pt - before.pt,
        (72.0 * scale).floor() as i32,
        "目标赛 SP 应为 floor(72 × {scale:.2})"
    );
    assert_eq!(event_count(&game, career_id), races_before + 1);
    println!(
        "目标赛奖励：race_bonus={} → 五维 +{} / SP +{}",
        game.uma().race_bonus,
        (5.0 * scale).floor() as i32,
        (72.0 * scale).floor() as i32
    );
    Ok(())
}

/// 自选比赛结算：非目标赛随机 1 项属性 + 面板 SP，两者都吃比赛加成
#[test]
fn self_race_reward_applied() -> Result<()> {
    use umaai_core::gamedata::ExtraStatus;

    let repo = Repo::load()?;
    let grades = repo.cfg.race_grades()?;
    let race_turn = (16..=35)
        .find(|turn| grades[*turn as usize] > 0)
        .context("样例赛程在 16–35 应至少有一个可自选比赛回合")?;
    let grade = grades[race_turn as usize];
    let event = Data::get()?
        .system_event(&format!("race_g{grade}"))?
        .clone();
    let panel = &event.choices[0][0].value;
    let Some(ExtraStatus::Random { value, .. }) = event.choices[0][0].extra_status else {
        return Err(anyhow!("race_g{grade} 应为随机 1 项额外属性"));
    };
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, race_turn)?;
    let scale = (100 + game.uma().race_bonus) as f64 / 100.0;
    let before = Stats::of(&game);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Race,
            choice: 0,
        },
    )?;
    let after = Stats::of(&game);
    let delta: Vec<i32> = (0..5)
        .map(|axis| after.five[axis] - before.five[axis])
        .collect();
    let expect_extra = (f64::from(value) * scale).floor() as i32;
    assert_eq!(
        delta.iter().filter(|value| **value != 0).count(),
        1,
        "非目标赛只给 1 项属性: {delta:?}"
    );
    assert_eq!(delta.iter().sum::<i32>(), expect_extra);
    assert_eq!(
        after.pt - before.pt,
        (f64::from(panel.status_pt[5]) * scale).floor() as i32,
        "自选比赛 SP 吃比赛加成"
    );
    println!(
        "自选比赛 G{grade}（turn {race_turn}）：随机 1 项 +{expect_extra} / SP +{}",
        after.pt - before.pt
    );
    Ok(())
}

/// 强制休息回合（凯斯奇迹 T65）：实际执行休息，不产生比赛事件 / 奖励
#[test]
fn forced_rest_turn_does_not_race() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(109301)?;
    drive_to_turn(&mut game, 65)?;
    assert!(game.plan.race && game.forced_rest);
    let career_id = Data::get()?.system_event("race_career")?.id;
    let before = Stats::of(&game);
    let races_before = event_count(&game, career_id);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Rest,
            choice: 0,
        },
    )?;
    assert_eq!(
        event_count(&game, career_id),
        races_before,
        "强制休息不产生比赛事件"
    );
    let after = Stats::of(&game);
    assert_eq!(after.five, before.five, "强制休息不改五维");
    assert!(
        after.vital >= before.vital,
        "休息回体：{} -> {}",
        before.vital,
        after.vital
    );
    println!(
        "凯斯奇迹 T65 强制休息：无比赛事件、五维不变、体力 {} -> {}",
        before.vital, after.vital
    );
    Ok(())
}

/// 训练失败：不给属性收益 / 技能点；体力 0 时连续训练应出现失败（失败事件入账）
#[test]
fn training_failure_has_no_gain() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    let fail_ids = [
        Data::get()?.system_event("training_fail")?.id,
        Data::get()?.system_event("training_fail_low")?.id,
    ];
    let mut found = false;
    for turn in 20..=24 {
        drive_to_turn(&mut game, turn)?;
        game.uma_mut().vital = 0;
        let actions = game.list_actions()?;
        // 槽满时该位变演目稽古；失败口径两者相同
        let action = if actions.contains(&Action::Train(0)) {
            Action::Train(0)
        } else {
            Action::ActTrain(0)
        };
        let before = Stats::of(&game);
        let fail_before: u32 = fail_ids.iter().map(|id| event_count(&game, *id)).sum();
        finish_turn(&mut game, &ScriptedTrainer { action, choice: 0 })?;
        let fail_after: u32 = fail_ids.iter().map(|id| event_count(&game, *id)).sum();
        if fail_after > fail_before {
            let after = Stats::of(&game);
            for axis in 0..5 {
                assert!(
                    after.five[axis] <= before.five[axis],
                    "失败不给属性收益（axis {axis}：{} -> {}）",
                    before.five[axis],
                    after.five[axis]
                );
            }
            assert_eq!(after.pt, before.pt, "失败不给技能点");
            found = true;
            break;
        }
    }
    assert!(found, "体力 0 连续训练应至少出现一次训练失败");
    println!("训练失败：触发失败事件、无属性 / SP 收益");
    Ok(())
}

/// 休息：普通休息按 30/50/70 分档，合宿版（原夏合宿窗口）固定 +40 体 / +1 干劲
#[test]
fn rest_normal_and_camp_versions() -> Result<()> {
    let repo = Repo::load()?;
    // 普通休息（turn 20，体力 0 起）
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    game.uma_mut().vital = 0;
    let before = Stats::of(&game);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Rest,
            choice: 0,
        },
    )?;
    let after = Stats::of(&game);
    let delta = after.vital - before.vital;
    assert!(
        [30, 50, 70].contains(&delta),
        "普通休息回体应为 30/50/70 之一：{delta}"
    );
    assert_eq!(after.motivation, before.motivation, "普通休息不改干劲");

    // 合宿版休息（turn 36，原窗口内）
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 36)?;
    game.uma_mut().vital = 0;
    let before = Stats::of(&game);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Rest,
            choice: 0,
        },
    )?;
    let after = Stats::of(&game);
    assert_eq!(after.vital - before.vital, 40, "合宿版休息 +40 体");
    assert_eq!(
        after.motivation,
        (before.motivation + 1).min(5),
        "合宿版休息 +1 干劲"
    );
    println!(
        "休息：普通档 +{delta} / 合宿版 +40 体 +1 干劲（体力 {} -> {}）",
        before.vital, after.vital
    );
    Ok(())
}

/// 出行：普通出行结算通用事件；友吾妻出行 5 段推进 + 每段最低演目 Lv+1 + 段事件数值
#[test]
fn outing_actions() -> Result<()> {
    let repo = Repo::load()?;
    // 普通出行
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    // 干劲压到 1：三种普通出行事件都必然带来增量（否则 KTV 档只加干劲会被上限吃掉）
    game.uma_mut().motivation = 1;
    let before = Stats::of(&game);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::NormalOuting,
            choice: 0,
        },
    )?;
    let outing_ids: Vec<u32> = ["normal_outing_1", "normal_outing_2", "normal_outing_3"]
        .iter()
        .map(|key| {
            Data::get()
                .expect("数据已初始化")
                .system_event(key)
                .expect("事件存在")
                .id
        })
        .collect();
    let hits = outing_ids
        .iter()
        .filter(|id| event_count(&game, **id) > 0)
        .count();
    assert_eq!(hits, 1, "普通出行应恰好结算 1 个通用出行事件");
    assert_ne!(Stats::of(&game), before, "普通出行应带来数值变化");

    // 友人出行：手动解锁（真实解锁事件的「上 / 下」选择留事件链单测），连续 5 段
    // 从 turn 27 起：跨过 turn 24 的正月事件、避开本局目标赛（turn 26 / 33），
    // 使本段 delta 只含出行 5 段（期间随机事件只会额外增加）
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 27)?;
    game.base.friend.out_state = FriendOutState::AfterUnlock;
    let friend_index = game
        .persons()
        .iter()
        .position(|person| person.card_id() == Some(30323))
        .context("SSR 吾妻人头应存在")?;
    let friendship_before = game.persons()[friend_index].friendship();
    let before = Stats::of(&game);
    let level_before = game.view().act_level;
    let gauge_before = game.view().act_gauge;
    for step in 0..5 {
        assert!(
            game.list_actions()?.contains(&Action::FriendOuting),
            "第 {} 段应可友人出行",
            step + 1
        );
        finish_turn(
            &mut game,
            &ScriptedTrainer {
                action: Action::FriendOuting,
                choice: 0,
            },
        )?;
    }
    assert_eq!(game.base.friend.out_used, vec![true; 5], "5 段全部用掉");
    // S5：每段「最低演目 Lv+1」+ 出行入槽 +1，逐段按三则落点 / 满槽跳过推进
    let gauge_max = repo.data.rules.act_gauge_max;
    let expected_level = expected_outing_levels(level_before, gauge_before, gauge_max);
    assert_eq!(
        game.view().act_level,
        expected_level,
        "5 段出行的演目 Lv 落点（Lv 最低 → 槽最低 → 属性序）"
    );
    assert_eq!(
        game.persons()[friend_index].friendship(),
        (friendship_before + 25).min(100),
        "5 段各 +5 羁绊（封顶 100）"
    );
    // 5 段段事件（809052011–015）各结算 1 次
    for key in 1..=5 {
        let id = Data::get()?
            .events
            .friend_events
            .get(&key.to_string())
            .context("友人出行段事件应存在")?
            .id;
        assert_eq!(event_count(&game, id), 1, "出行第 {key} 段事件应结算 1 次");
    }
    let after = Stats::of(&game);
    // 第 3 段取上档（无属性），其余各段属性合计（memo 基础值）：速 10 / 耐 5 / 力 5 / 毅 20 / 智 20 / SP 45。
    // turn 26–30 期间随机事件只会额外增加，断言取下界。
    let delta = [
        after.five[0] - before.five[0],
        after.five[1] - before.five[1],
        after.five[2] - before.five[2],
        after.five[3] - before.five[3],
        after.five[4] - before.five[4],
    ];
    let floor = [10, 5, 5, 20, 20];
    for (axis, (got, need)) in delta.iter().zip(floor).enumerate() {
        assert!(
            *got >= need,
            "5 段出行属性下界（轴 {axis}）：{got} < {need}，delta={delta:?}"
        );
    }
    assert!(
        after.pt - before.pt >= 45,
        "5 段出行 SP 合计应 ≥ 45：{}",
        after.pt - before.pt
    );
    assert!(
        !game.list_actions()?.contains(&Action::FriendOuting),
        "5 段用完后不可再出行"
    );
    println!(
        "出行：普通出行 1 个通用事件；友人出行 5 段 → 演目 Lv {level:?} / 羁绊 +25 / 五维下界 +[10,5,5,20,20] / SP+45",
        level = game.view().act_level
    );
    Ok(())
}

/// 演目稽古（S5 两层）：成功 → 演目 Lv+1、槽清零；数值 = 下层 + 上层（含 +10 全基础 flat）
#[test]
fn keiko_success_levels_up_and_clears_gauge() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    game.uma_mut().vital = 100;
    let gauge_max = repo.data.rules.act_gauge_max;
    // 手动点满槽（本回合进入排练），先从 Distribute 阶段跑出真实分布
    game.acts.gauge[0] = gauge_max;
    let level_before = game.view().act_level;
    let mut decision = StdRng::seed_from_u64(7);
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    game.run_stage(&trainer, &mut decision)?; // Distribute
    assert!(game.next(), "Distribute → Train");
    // 下层期望：稽古 Junior 速行 16 × 引擎公式各乘区（人数 / 干劲 / 成长率 / 卡面）
    let buffs = game.calc_training_buff(0)?;
    let person_count = game.count_training_persons(0);
    let uma = game.uma();
    let motivation_adj = ((uma.motivation - 3) * 10) as f32;
    let lower = ((16 + buffs.bonus[0]) as f32
        * (1.0 + 0.01 * buffs.youqing)
        * (1.0 + 0.01 * motivation_adj * (1.0 + 0.01 * buffs.ganjing as f32))
        * (1.0 + 0.01 * buffs.xunlian as f32)
        * (1.0 + 0.05 * person_count as f32)
        * (1.0 + 0.01 * uma.five_status_bonus[0] as f32))
        .floor() as i32;
    let before = Stats::of(&game);
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::ActTrain(0),
            choice: 0,
        },
    )?;
    let after = Stats::of(&game);
    let level_after = game.view().act_level;
    assert_eq!(level_after[0], level_before[0] + 1, "稽古成功演目 Lv+1");
    assert_eq!(game.view().act_gauge[0], 0, "稽古成功该演目槽清零");
    assert!(
        after.five[0] - before.five[0] >= lower + 10,
        "稽古数值至少含下层 {lower} + 全基础 +10 flat（实际 +{}）",
        after.five[0] - before.five[0]
    );
    // 非主属性也吃 +10 全基础（速度稽古的耐力基础为 0，除 flat 外无下层收益）
    assert!(
        after.five[1] - before.five[1] >= 10,
        "稽古全基础 +10 应作用于非主属性（耐力 +{}）",
        after.five[1] - before.five[1]
    );
    println!(
        "演目稽古 S5：Lv {} -> {} / 槽清零 / 速 +{}（下层 {lower} + 上层）",
        level_before[0],
        level_after[0],
        after.five[0] - before.five[0]
    );
    Ok(())
}

/// 每回合 `AfterTrain` 把动作 / 训练后挂出的事件全部结算，不跨回合泄漏
#[test]
fn unresolved_events_drained_each_turn() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    for turn in 0..=MAX_TURN {
        for stage in 0..5 {
            game.run_stage(&trainer, &mut decision)?;
            if stage >= 3 {
                assert!(
                    game.base.unresolved_events.is_empty(),
                    "turn {turn} stage {stage} 后仍有 {} 个待结算事件",
                    game.base.unresolved_events.len()
                );
            }
            game.next();
        }
    }
    assert!(
        !game.base.events.is_empty(),
        "整局应结算过事件（比赛奖励等）"
    );
    println!("事件排空：78 回合每回合 AfterTrain / End 后无待结算事件");
    Ok(())
}

/// 演目槽来源：目标赛 +4（按 4 次 +1）、自选赛 / 休息 / 出行 +1
#[test]
fn gauge_gains_from_race_and_rest() -> Result<()> {
    let repo = Repo::load()?;
    let gauge_max = repo.data.rules.act_gauge_max;
    // 目标赛（turn 11 生涯目标赛）：+4，按三则落点 4 次 +1
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 11)?;
    let level = game.view().act_level;
    let gauge = game.view().act_gauge;
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Race,
            choice: 0,
        },
    )?;
    let expect = expected_gauge_up(level, gauge, 4, gauge_max);
    assert_eq!(
        game.view().act_gauge,
        [0, 1, 2, 3, 4].map(|act| gauge[act] + expect[act]),
        "目标赛演目槽 +4（含满槽跳过 / 并列分摊）"
    );

    // 自选赛（非目标）：+1
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    assert!(
        game.list_actions()?.contains(&Action::Race),
        "turn 20 应有可自选比赛"
    );
    let level = game.view().act_level;
    let gauge = game.view().act_gauge;
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Race,
            choice: 0,
        },
    )?;
    let expect = expected_gauge_up(level, gauge, 1, gauge_max);
    assert_eq!(
        game.view().act_gauge,
        [0, 1, 2, 3, 4].map(|act| gauge[act] + expect[act]),
        "自选比赛演目槽 +1"
    );

    // 休息：+1
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    let level = game.view().act_level;
    let gauge = game.view().act_gauge;
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Rest,
            choice: 0,
        },
    )?;
    let expect = expected_gauge_up(level, gauge, 1, gauge_max);
    assert_eq!(
        game.view().act_gauge,
        [0, 1, 2, 3, 4].map(|act| gauge[act] + expect[act]),
        "休息演目槽 +1"
    );
    println!("演目槽：目标赛 +4 / 自选赛 +1 / 休息 +1（三则落点 + 满槽跳过）");
    Ok(())
}

/// 训练成功：参与担当按担当归集入槽（闪彩 +2）；排练回合不涨
#[test]
fn gauge_gains_from_training_participants() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 20)?;
    game.acts.gauge = [0; 5];
    // 先跑完 Distribute 得到真实分布（此时无排练锁位）
    let mut decision = StdRng::seed_from_u64(7);
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    game.run_stage(&trainer, &mut decision)?;
    assert!(game.next(), "Distribute → Train");
    let distribution = game.distribution()[0].clone();
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Train(0),
            choice: 0,
        },
    )?;
    // 槽入账发生在训练后处理（羁绊结算）**之后** ⇒ 闪彩判定按结算时的羁绊算，
    // 不能用回合开始前的羁绊快照（有人正好在这一回合跨过 80 门槛）
    let mut expect = [0; 5];
    for index in &distribution {
        if *index < 0 {
            continue;
        }
        let person = &game.persons()[*index as usize];
        let Some(act) = person.act else {
            continue;
        };
        let shining = game.is_shining_at(*index as usize, 0);
        expect[act] += if shining { 2 } else { 1 };
    }
    assert_eq!(
        game.view().act_gauge,
        expect,
        "训练参与担当按担当归集入槽（闪彩 +2）"
    );
    println!("训练入槽：{expect:?}（闪彩按 +2）");
    Ok(())
}

/// 整局机制链：教诲按窗口获得 / 融合、発表会结果结算、本番演出激活
#[test]
fn s5_mechanism_chain_over_full_run() -> Result<()> {
    let repo = Repo::load()?;
    let obs = sweep(repo.game(repo.cfg.sim.uma)?, |turn, game| {
        if turn == 1 {
            assert!(
                game.view().advice_slots.iter().all(|slot| *slot == 0),
                "T2 之前不应获得教诲"
            );
        }
        if turn == 2 {
            assert!(
                game.view().advice_slots.iter().all(|slot| *slot != 0),
                "T2 Begin（NPC / 友人加入后）立即授予 5 个基础教诲，效果对 T2 训练生效"
            );
        }
        if turn == 24 {
            assert_ne!(
                game.view().check_points[0],
                CheckPointResult::NotReached,
                "turn 23 结束应结算第 1 回発表会"
            );
            assert!(
                game.view().advice_fused.iter().any(|fused| *fused != 0),
                "turn 23 结束应完成第 2 年融合"
            );
        }
        if turn == 48 {
            assert_ne!(game.view().check_points[1], CheckPointResult::NotReached);
            assert!(
                game.view().advice_fused.iter().all(|fused| *fused != 0),
                "turn 47 结束应完成第 3 年融合"
            );
        }
        if turn == 68 {
            assert_ne!(game.view().check_points[2], CheckPointResult::NotReached);
            assert!(game.view().finals_done, "本番演出已结算");
        }
        if turn == 70 {
            assert!(game.plan.ura_period, "turn 70 在 URA 收尾期");
        }
        Ok(())
    })?;
    assert_eq!(obs.len(), (MAX_TURN + 1) as usize);
    println!("S5 机制链：教诲 / 発表会 / 融合 / 本番演出按回合窗口结算");
    Ok(())
}

/// S6 事件链：剧本固定事件「教え見直し」在 turn 11 行动前结算（体力 +15 / 干劲 +1）
#[test]
fn scenario_fixed_event_at_turn11() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    drive_to_turn(&mut game, 10)?;
    finish_turn(
        &mut game,
        &ScriptedTrainer {
            action: Action::Train(0),
            choice: 0,
        },
    )?;
    // 回合 10 已走完，当前在 turn 11 的 Begin（尚未执行）
    assert_eq!(game.turn(), 11);
    game.uma_mut().vital = 50;
    game.uma_mut().motivation = 2;
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(3);
    game.run_stage(&trainer, &mut decision)?;
    assert_eq!(
        event_count(&game, 400015105),
        1,
        "教え見直し事件应结算 1 次"
    );
    assert_eq!(game.uma().vital, 65, "行动前体力 +15");
    assert_eq!(game.uma().motivation, 3, "行动前干劲 +1");
    println!("S6：turn 11 教え見直し事件（体力+15 / 干劲+1）");
    Ok(())
}

/// S6 事件链整局：通用固定事件 / 随机分布 / 剧本固定事件 / 友人链 / 抽签 / 结局
#[test]
fn s6_event_chain_over_full_run() -> Result<()> {
    let repo = Repo::load()?;
    let mut game = repo.game(repo.cfg.sim.uma)?;
    let trainer = MatsuriTrainer::new(MatsuriConfig::default());
    let mut decision = StdRng::seed_from_u64(7);
    game.run_full_game(&trainer, &mut decision)?;

    let count = |id: u32| event_count(&game, id);
    // 引擎通用固定事件
    assert_eq!(count(400000400), 1, "turn 0 马娘登场");
    assert_eq!(count(4009), 1, "turn 24 经典年-新年");
    assert_eq!(count(4010), 1, "turn 48 古马年-新年");
    assert_eq!(count(4012), 1, "turn 30 第二年继承");
    assert_eq!(count(4013), 1, "turn 54 第三年继承");
    // 引擎通用「回合后」事件（B2/Q9 按引擎接）
    assert_eq!(count(4011), 1, "turn 48 新年抽签");
    assert_eq!(count(5011), 1, "turn 77 育成结束事件");
    // 剧本固定事件：教え見直し ×3（turn 11 / 35 / 59）
    assert_eq!(count(400015105), 3, "教え見直し事件按 3 个窗口结算");
    // 友人链：首次事件 / 训练后事件（40%）/ 出行解锁 / 正月 / 结局
    assert!(count(809052001) >= 1, "首次点击事件应发生");
    assert!(count(809052032) >= 1, "训练后点击事件应发生");
    assert_eq!(count(809052004), 1, "出行解锁事件应发生 1 次");
    assert_eq!(count(809052021), 1, "已解锁时 turn 24 应发生正月事件");
    assert_eq!(count(809052023), 1, "已解锁时 turn 77 应发生友人结局");
    // 随机分布：至少发生一些支援卡 / 马娘事件
    let random_events = count(8001) + count(8002) + count(8003) + count(5006);
    assert!(random_events >= 1, "随机事件分布应至少命中一次");
    // 旧出行占位事件已删除、不再结算
    for id in 8201..=8205 {
        assert_eq!(count(id), 0, "旧出行占位事件 {id} 不应再结算");
    }
    let big_eater = count(5010);
    assert!(big_eater <= 3, "大胃王最多 3 次");
    println!(
        "S6 整局事件链：通用固定 ✓ / 教え見直し ×{} / 友人链 first+click+out+newyear+end ✓ / 随机≥{} / 大胃王 {} 次",
        count(400015105),
        random_events,
        big_eater
    );
    Ok(())
}
