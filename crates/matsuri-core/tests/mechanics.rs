//! S3 机制集成：仓内真实资产建一局，逐阶段跑满 78 回合，守回合结构（`TurnPlan`）/ 赛程 /
//! 合宿窗口与延长 / 発表会 / 本番 / URA 收尾 / 马娘特殊事件（强制休息）/ `MatsuriView` 账本。
//!
//! 训练员用样例 `MatsuriTrainer`：本阶段只关心回合边界，不关心具体选了什么动作。
//! 数据根统一经 `matsuri_core::testing::data_dir()` 取仓根 `data/`（引擎静态表 + 剧本资产）。

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use matsuri_core::data::CheckPointResult;
use matsuri_core::game::deck::inspect_deck;
use matsuri_core::testing::data_dir;
use matsuri_core::{
    DeckError, DeckReport, DeckWarning, MAX_TURN, MatsuriConfig, MatsuriData, MatsuriGame,
    MatsuriPerson, MatsuriTrainer, MatsuriView, PersonOrigin, TurnPlan,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::Config;
use umaai_core::Data;
use umasim::game::{Game, Person, SupportCard};

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
