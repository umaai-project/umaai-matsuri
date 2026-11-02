//! 诊断埋点辅助（**只读**，不参与结算）
//!
//! 日志出口是引擎 [`diag!`](crate::diag)（target = `diagnostic`，编译期 feature + 运行时开关，
//! 搜索 rollout 由引擎 `DiagGuard` 自动静默）。本模块只提供「关键状态快照 + 变化摘要」与回合
//! 类型摘要，供各机制在结算点打「状态变化」行，口径与手动面板的「变化 / 类型」一致。
//!
//! 纪律：不耗随机流、不改局面；日志参数只在运行时开关打开时求值（`diag!` 宏内做判定）。

use umaai_core::Array5;
use umasim::game::Game;

use super::MatsuriGame;

/// 关键状态快照（诊断用）：体力 / 干劲 / 五维 / PT / 灵感合计 / 演目槽 / 演目 Lv
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyState {
    /// 体力
    pub(crate) vital: i32,
    /// 干劲
    pub(crate) motivation: i32,
    /// 五维
    pub(crate) five: Array5,
    /// 技能点
    pub(crate) pt: i32,
    /// 灵感合计（事件 / 训练 hint 收益；URA 期事件也计入）
    pub(crate) total_hints: i32,
    /// 演目 gauge
    pub(crate) gauge: Array5,
    /// 演目 Lv
    pub(crate) level: Array5,
}

impl KeyState {
    /// 与下一快照的差异摘要（零变化项省略；无变化返回「无变化」）
    pub(crate) fn diff(&self, next: &Self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (axis, name) in ["速", "耐", "力", "根", "智"].iter().enumerate() {
            let delta = next.five[axis] - self.five[axis];
            if delta != 0 {
                parts.push(format!("{name}{delta:+}"));
            }
        }
        if next.vital != self.vital {
            parts.push(format!("体力{:+}", next.vital - self.vital));
        }
        if next.motivation != self.motivation {
            parts.push(format!("干劲{:+}", next.motivation - self.motivation));
        }
        if next.pt != self.pt {
            parts.push(format!("PT{:+}", next.pt - self.pt));
        }
        if next.total_hints != self.total_hints {
            parts.push(format!("灵感{:+}", next.total_hints - self.total_hints));
        }
        for (act, (before, after)) in self.gauge.iter().zip(next.gauge.iter()).enumerate() {
            if before != after {
                parts.push(format!("演目{act}槽{before}→{after}"));
            }
        }
        for (act, (before, after)) in self.level.iter().zip(next.level.iter()).enumerate() {
            if before != after {
                parts.push(format!("演目{act}Lv{before}→{after}"));
            }
        }
        if parts.is_empty() {
            "无变化".to_string()
        } else {
            parts.join(" ")
        }
    }
}

impl MatsuriGame {
    /// 取诊断用关键状态快照（只读）
    pub(crate) fn key_state(&self) -> KeyState {
        KeyState {
            vital: self.uma().vital,
            motivation: self.uma().motivation,
            five: self.uma().five_status,
            pt: self.uma().skill_pt,
            total_hints: self.uma().total_hints,
            gauge: self.acts.gauge,
            level: self.acts.level,
        }
    }

    /// 回合类型摘要（诊断日志用；与手动面板「类型」同口径）
    pub(crate) fn diag_turn_kind(&self) -> String {
        let plan = &self.plan;
        let mut tags: Vec<String> = Vec::new();
        if plan.race {
            tags.push(
                if plan.ura_race {
                    "URA决赛"
                } else {
                    "目标赛"
                }
                .to_string(),
            );
        }
        if plan.finals() {
            tags.push("本番".to_string());
        } else if let Some(check_point) = plan.check_point {
            tags.push(format!("発表会{}", check_point + 1));
        }
        if plan.camp_zone {
            tags.push(format!(
                "合宿{}年{}",
                plan.camp_year.map_or(0, |year| year + 1),
                if plan.camp_extended {
                    "·延长"
                } else if plan.camp_regular {
                    "·常规"
                } else {
                    ""
                }
            ));
        }
        if plan.camp_rest {
            tags.push("合宿版休息".to_string());
        }
        if plan.camp_level_up {
            tags.push("合宿结算·全演目+1".to_string());
        }
        if plan.advice_window.is_some() {
            tags.push("教诲窗口".to_string());
        }
        if self.forced_rest {
            tags.push("强制休息".to_string());
        }
        if plan.interlude {
            tags.push("空窗期".to_string());
        }
        if plan.ura_period {
            tags.push("URA回合".to_string());
        }
        if tags.is_empty() {
            "普通回合".to_string()
        } else {
            tags.join("/")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_reports_only_changes() {
        let before = KeyState {
            vital: 100,
            motivation: 3,
            five: [10, 20, 30, 40, 50],
            pt: 100,
            total_hints: 7,
            gauge: [0; 5],
            level: [1; 5],
        };
        assert_eq!(before.diff(&before), "无变化");
        let after = KeyState {
            vital: 80,
            motivation: 4,
            five: [12, 20, 30, 40, 50],
            pt: 116,
            total_hints: 10,
            gauge: [1, 0, 0, 0, 0],
            level: [1, 2, 1, 1, 1],
        };
        assert_eq!(
            before.diff(&after),
            "速+2 体力-20 干劲+1 PT+16 灵感+3 演目0槽0→1 演目1Lv1→2"
        );
    }
}
