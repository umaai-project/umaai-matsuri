//! 在线会话装配 + 一帧一决策的主循环
//!
//! **不走 `umaai::Session` 编排**：`Session::step_turn` 的 `build` 只拿得到 `BaseGame`，
//! 路由是 `source == "event" ? 事件 : 训练` 的二分，覆盖不了骏大祭的 `special` 中间态、
//! 同回合重复帧与「等级本帧无值」。这里只复用引擎的 pub 零件：
//! `parse_base_status → to_base_game → set_rule_master → 决策`。
//!
//! 跨帧状态只有三样（都必须逐帧传回）：
//!
//! 1. `prev_levels`：上一帧的有效训练等级（`G12` 基准）；
//! 2. `seen`：已处理帧的内容摘要（`G7` 幂等）；
//! 3. `last_train_turn`：本回合是否已给过训练决策（同回合多帧不重复报）。
//!
//! 三者都按**局号**（`single_mode_chara_id`）隔离：换局自动清空。

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use matsuri_core::game::restore::RestoreInput;
use matsuri_core::{
    MatsuriConfig, MatsuriData, MatsuriGame, MatsuriSearchTrainer, MatsuriTrainer, Stage,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::{BaseParams, FrameOutcome};
use umaai_core::rng::{StreamTag, derive_seed};
use umaai_core::{Array5, Array6, Data};
use umasim::game::{Game, SupportCard, Trainer};

use crate::final_score::parse_final_score;
use crate::frame::ParsedFrame;
use crate::online::{self, Route};
use crate::output::{FinalTurnOutput, OnlineTurnOutput, TurnKind};

/// 剧本友人（吾妻）首次点击事件 id（`BaseParams.friend_first_event`；0 = 未知则跳过判定）
pub const FRIEND_FIRST_EVENT: i32 = 809052001;

/// 训练员档位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerKind {
    /// 手写档（快、可解释；JSON 里没有搜索分）
    Hand,
    /// 搜索档（MCTS 预算由剧本配置 `[search].search_n` 决定）
    Search,
}

impl TrainerKind {
    /// 解析命令行取值
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "matsuri" | "hand" => Ok(Self::Hand),
            "search" => Ok(Self::Search),
            other => Err(anyhow!(
                "未知训练员: {other}（在线档可选 matsuri（手写） / search（搜索））"
            )),
        }
    }

    /// 是否带搜索分
    #[must_use]
    pub fn is_search(self) -> bool {
        matches!(self, Self::Search)
    }
}

/// 训练员适配（两个实现都满足 `Trainer<MatsuriGame>`）
enum TrainerChoice {
    /// 手写档（盒子：`MatsuriTrainer` 带一整份配置，直接内联会让枚举差两个数量级）
    Hand(Box<MatsuriTrainer>),
    Search(Box<MatsuriSearchTrainer>),
}

impl TrainerChoice {
    fn as_dyn(&self) -> &dyn Trainer<MatsuriGame> {
        match self {
            Self::Hand(trainer) => trainer.as_ref(),
            Self::Search(trainer) => trainer.as_ref(),
        }
    }

}

/// 在线会话：装配一次、逐帧喂入
pub struct OnlineSession {
    /// 剧本资产
    data: Arc<MatsuriData>,
    /// 剧本配置（融合预设等）
    config: MatsuriConfig,
    /// 宿主注入的继承额外属性
    extra_count: Array6,
    /// 比赛等级表（`BaseParams` 借用）
    race_grades: Vec<i32>,
    /// 剧本友人角色 id（0 = 编成里没有友人卡）
    friend_chara_id: u32,
    /// 规则主种子（每帧重建局面都要重注入）
    master: u64,
    /// 训练员
    trainer: TrainerChoice,
    /// 当前局号
    epoch: Option<u64>,
    /// 已处理帧摘要
    seen: HashSet<u64>,
    /// 上一帧有效训练等级（`G12` 基准）
    prev_levels: Option<Array5>,
    /// 本回合是否已给过训练决策
    last_train_turn: Option<i32>,
    /// 是否输出训练验算表
    verify: bool,
}

impl OnlineSession {
    /// 装配会话
    ///
    /// `cfg` 提供继承额外属性 / 比赛等级表 / 搜索内核参数；`matsuri_cfg` 提供剧本侧
    /// 配置（含 `[search].search_n`）。两者都由宿主提前载入，本层不读文件。
    pub fn new(
        cfg: &umaai::Config,
        data: Arc<MatsuriData>,
        matsuri_cfg: MatsuriConfig,
        trainer: TrainerKind,
        verify: bool,
    ) -> Result<Self> {
        // 规则主种子：与离线 `Runner` 同源（`cfg.sim.seed`），保证同种子同决策
        let master = cfg.sim.seed;
        let friend_chara_id = cfg
            .sim
            .cards
            .iter()
            .filter_map(|id| SupportCard::new(*id).ok())
            .find(|card| card.card_type >= 5)
            .map_or(0, |card| card.data.chara_id);
        let trainer = match trainer {
            TrainerKind::Hand => {
                TrainerChoice::Hand(Box::new(MatsuriTrainer::new(matsuri_cfg.clone())))
            }
            TrainerKind::Search => TrainerChoice::Search(Box::new(MatsuriSearchTrainer::new(
                matsuri_cfg.clone(),
                cfg.search.clone(),
            )?)),
        };
        Ok(Self {
            data,
            config: matsuri_cfg,
            extra_count: cfg.inherit().extra_count,
            race_grades: cfg.race_grades()?.to_vec(),
            friend_chara_id,
            master,
            trainer,
            epoch: None,
            seen: HashSet::new(),
            prev_levels: None,
            last_train_turn: None,
            verify,
        })
    }

    /// 当前局号
    #[must_use]
    pub fn epoch(&self) -> Option<u64> {
        self.epoch
    }

    /// 处理一帧（**一帧一决策**；不推进局面，不改真机）
    pub fn handle_frame(&mut self, contents: &str) -> Result<OnlineTurnOutput> {
        let frame = ParsedFrame::parse(contents)?;
        self.sync_epoch(frame.epoch_key());

        // 帧 → 局面（还原失败即拒绝计算：编成 / 人头 / 分布不合规时抛错，绝不带病出决策）
        let (game, notes) = self.restore(&frame)?;
        let view = game.view();
        let turn = game.turn();
        let scenario = "matsuri".to_string();

        // `G7` 幂等：同内容帧只处理一次（真机同回合会重复写同内容）
        if !self.seen.insert(frame.digest) {
            return Ok(OnlineTurnOutput {
                turn,
                epoch: frame.epoch_key(),
                scenario,
                frame_digest: frame.digest,
                kind: TurnKind::Skipped,
                record: None,
                skip_reason: Some("重复帧（内容与已处理帧一致）".to_string()),
                notes: notes.clone(),
                view,
                search_backed: false,
                verify: None,
                game,
                actions: Vec::new(),
                candidate_scores: Vec::new(),
            });
        }

        // 过期快照（预览非零但分布全空）不验算：还原态人数恒 0，只会产出假偏差；
        // 预览全零的帧照常验算（`verify_training` 会跳过全零行，产出空表）
        let verify = if self.verify && !online::stale_snapshot(&game, &frame) {
            Some(online::verify_training(&game, &frame.training)?)
        } else {
            None
        };

        let output = match online::route(&game, &frame) {
            Route::Skip(reason) => OnlineTurnOutput {
                turn,
                epoch: frame.epoch_key(),
                scenario,
                frame_digest: frame.digest,
                kind: TurnKind::Skipped,
                record: None,
                skip_reason: Some(reason.to_string()),
                notes: notes.clone(),
                view,
                search_backed: false,
                verify,
                game,
                actions: Vec::new(),
                candidate_scores: Vec::new(),
            },
            Route::Event => {
                let event = online::pending_event(&game)
                    .ok_or_else(|| anyhow!("路由判定为事件帧，但取不到待选事件"))?;
                let mut rng = self.decision_rng(turn);
                let choice = self.trainer.as_dyn().select_event_choice(
                    &game,
                    event,
                    &event.choices,
                    &mut rng,
                )?;
                // 搜索档：把本次搜索的选项均分带进记录；手写档保持 `None`（无搜索分）
                let scores: Option<Vec<(usize, f64, u32)>> = match &self.trainer {
                    TrainerChoice::Search(trainer) => trainer.last_search().map(|summary| {
                        summary
                            .candidates
                            .iter()
                            .map(|candidate| (candidate.index, candidate.score, candidate.n))
                            .collect()
                    }),
                    TrainerChoice::Hand(_) => None,
                };
                let search_backed = scores.is_some();
                let record = online::event_record(event, choice, scores.as_deref());
                OnlineTurnOutput {
                    turn,
                    epoch: frame.epoch_key(),
                    scenario,
                    frame_digest: frame.digest,
                    kind: TurnKind::Decision,
                    record: Some(record),
                    skip_reason: None,
                    notes: notes.clone(),
                    view,
                    search_backed,
                    verify,
                    game,
                    actions: Vec::new(),
                    candidate_scores: Vec::new(),
                }
            }
            Route::Train => {
                // 同回合已经给过训练决策 → 不再重复报（真机同回合可能先出 event 帧再出 command 帧）
                if self.last_train_turn == Some(turn) {
                    OnlineTurnOutput {
                        turn,
                        epoch: frame.epoch_key(),
                        scenario,
                        frame_digest: frame.digest,
                        kind: TurnKind::Skipped,
                        record: None,
                        skip_reason: Some("本回合已给出训练决策".to_string()),
                        notes: notes.clone(),
                        view,
                        search_backed: false,
                        verify,
                        game,
                        actions: Vec::new(),
                        candidate_scores: Vec::new(),
                    }
                } else {
                    let actions = game.list_actions()?;
                    let mut rng = self.decision_rng(turn);
                    let index = self
                        .trainer
                        .as_dyn()
                        .select_action(&game, &actions, &mut rng)?;
                    // 手写档：解释与实际决策同源（`explain_action` 内部走同一个 `choose_action`）；
                    // 搜索档：把最近一次搜索的候选均分 / 样本数带进记录（fallback 时保持无分）
                    let (record, candidate_scores, search_backed) = match &self.trainer {
                        TrainerChoice::Hand(trainer) => {
                            let explain = trainer.explain_action(&game, &actions)?;
                            let scores = explain.candidates.clone();
                            let record = online::train_record(
                                &game,
                                &actions,
                                index,
                                Some(online::TrainExplanation {
                                    candidates: explain.candidates.as_slice(),
                                    counts: None,
                                    chosen: explain.chosen,
                                    reason: explain.reason.clone(),
                                }),
                            )?;
                            // 手写档的候选分只给面板看；JSON 仍是 `score = 0`（无搜索分）
                            (record, scores, false)
                        }
                        TrainerChoice::Search(trainer) => match trainer.last_search() {
                            Some(summary) => {
                                let candidates: Vec<(usize, f64)> = summary
                                    .candidates
                                    .iter()
                                    .map(|candidate| (candidate.index, candidate.score))
                                    .collect();
                                let counts: Vec<u32> = summary
                                    .candidates
                                    .iter()
                                    .map(|candidate| candidate.n)
                                    .collect();
                                let record = online::train_record(
                                    &game,
                                    &actions,
                                    index,
                                    Some(online::TrainExplanation {
                                        candidates: candidates.as_slice(),
                                        counts: Some(counts.as_slice()),
                                        chosen: index,
                                        reason: "搜索：候选排名加权均分最高".to_string(),
                                    }),
                                )?;
                                (record, candidates, true)
                            }
                            None => (
                                online::train_record(&game, &actions, index, None)?,
                                Vec::new(),
                                false,
                            ),
                        },
                    };
                    self.last_train_turn = Some(turn);
                    OnlineTurnOutput {
                        turn,
                        epoch: frame.epoch_key(),
                        scenario,
                        frame_digest: frame.digest,
                        kind: TurnKind::Decision,
                        record: Some(record),
                        skip_reason: None,
                        notes: notes.clone(),
                        view,
                        search_backed,
                        verify,
                        game,
                        actions,
                        candidate_scores,
                    }
                }
            }
        };
        Ok(output)
    }

    /// 处理终局帧（`game{id}_finalScore.json`）
    ///
    /// 三道闸顺序与引擎一致：**必填解析 → `is_usable` → 局号匹配 → 算分**。
    /// 局号不符 / 数值不可用只拒绝本帧（`accepted = false`），不推进 / 不清会话；
    /// 换局仍由下一张 `load` 帧的 [`Self::sync_epoch`] 处理。
    pub fn handle_final(&mut self, contents: &str) -> Result<FinalTurnOutput> {
        let frame = parse_final_score(contents)?;
        let scenario = "matsuri".to_string();
        let current = self.epoch;
        let epoch = current.unwrap_or_else(|| frame.epoch_key().unwrap_or(0));

        if !frame.is_usable() {
            return Ok(FinalTurnOutput {
                turn: frame.turn,
                epoch,
                scenario,
                accepted: false,
                score: None,
                rank: None,
                reason: Some("终局帧数值不可用（五维或上限全零）".to_string()),
            });
        }
        let Some(current) = current else {
            return Ok(FinalTurnOutput {
                turn: frame.turn,
                epoch,
                scenario,
                accepted: false,
                score: None,
                rank: None,
                reason: Some("本会话尚未建立局号，无法匹配终局帧".to_string()),
            });
        };
        if !frame.matches_epoch(current) {
            return Ok(FinalTurnOutput {
                turn: frame.turn,
                epoch,
                scenario,
                accepted: false,
                score: None,
                rank: None,
                reason: Some(format!("局号不匹配（当前 {current}）")),
            });
        }
        Ok(FinalTurnOutput {
            turn: frame.turn,
            epoch,
            scenario,
            accepted: true,
            score: Some(frame.calc_score()?),
            rank: Some(frame.rank_name()?),
            reason: None,
        })
    }

    /// 帧 → 局面
    fn restore(&mut self, frame: &ParsedFrame) -> Result<(MatsuriGame, Vec<String>)> {
        let params = self.params();
        let FrameOutcome { game: base, notes } = frame.status.to_base_game(&params)?;
        // 入口阶段：动作已执行 → `AfterTrain`；否则 `Train`（绝不进 `Begin` / `Distribute`）
        let stage = if frame.is_after_training() {
            Stage::AfterTrain
        } else {
            Stage::Train
        };
        let mut game = MatsuriGame::restore(
            RestoreInput {
                base,
                festa: &frame.festa,
                stage,
                frame_persons: &frame.status.persons,
                prev_train_levels: self.prev_levels,
                noncard_friendship: Some((
                    frame.status.friendship_noncard_yayoi,
                    frame.status.friendship_noncard_reporter,
                )),
            },
            Arc::clone(&self.data),
            self.config.clone(),
        )?;
        // 每帧重建局面 → 每帧都要重注入规则主种子（引擎 `Session::step_turn` 同口径）
        game.set_rule_master(self.master);
        // `G12` 基准：记下**还原后**的有效等级（不是帧原值），保证跨帧链条自洽
        self.prev_levels = Some(std::array::from_fn(|train| game.train_level(train) as i32));
        // 引擎 `FrameOutcome.notes` 原样带给宿主（例如「事件数值不完整，该事件未入队」）
        let notes = notes
            .iter()
            .map(|note| format!("{note:?}"))
            .collect::<Vec<_>>();
        Ok((game, notes))
    }

    /// 局号变化 → 清空跨帧基准（换局 / 重开）
    fn sync_epoch(&mut self, epoch: u64) {
        if self.epoch == Some(epoch) {
            return;
        }
        self.epoch = Some(epoch);
        self.seen.clear();
        self.prev_levels = None;
        self.last_train_turn = None;
    }

    /// 本帧的决策随机流（按 `(master, turn)` 派生：同帧可复现，不跨帧位移）
    ///
    /// 用 `Probe` 标签另开一条流：既不与引擎的回合固定流 / 事件流共用，也不动策略流的序列。
    fn decision_rng(&self, turn: i32) -> StdRng {
        StdRng::seed_from_u64(derive_seed(
            self.master,
            &[turn as u64, StreamTag::Probe.tag()],
        ))
    }

    /// 宿主注入参数（协议不发的量）
    fn params(&self) -> BaseParams<'_> {
        BaseParams {
            extra_count: self.extra_count,
            race_grades: &self.race_grades,
            friend_chara_id: self.friend_chara_id,
            friend_first_event: FRIEND_FIRST_EVENT,
        }
    }
}

/// 确保静态数据已初始化（宿主忘记 `Data::init` 时给出可读报错）
pub fn ensure_data(data_root: impl AsRef<std::path::Path>) -> Result<()> {
    Data::init(data_root)
        .map(|_| ())
        .context("引擎静态数据初始化失败")
}
