//! 属性软上限（剧本 15「1200 软上限」）
//!
//! 真机口径（2026-11-01 步骤 3 报文实证）：五维增量中**超出 1200 的部分减半**（向下取整）。
//!
//! 报文算例（同一训练、同一属性格，名义增量相同、赛前值不同 ⇒ 实得不同）：
//! - pre = 1195、名义 30 → 实得 17（= 5 + ⌊25/2⌋）
//! - pre = 1196、名义 14 → 实得 9（= 4 + ⌊10/2⌋）
//! - pre = 1179、名义 38 → 实得 29（= 21 + ⌊17/2⌋）
//! - pre ≥ 1200、名义 7 → 实得 3（= ⌊7/2⌋）
//! - pre = 451、名义 7 → 实得 7（没过线，全额）
//!
//! 适用范围：普通训练 / 演目稽古 / 赛后奖励（含自选比赛）/ 発表会全能力奖励。
//! **技能点不受软上限影响**（只有五维过线才打折）。

use umaai_core::Array5;
use umaai_core::gamedata::ActionValue;

/// 属性软上限：增量中**超出 `cap` 的部分**减半（向下取整）
///
/// - `pre = 1195` / 满增量 30 → 5 + ⌊25/2⌋ = 17
/// - `pre ≥ 1200` / 满增量 7 → ⌊7/2⌋ = 3
///
/// `cap <= 0` 视为关闭（增量原样返回）。
pub(crate) fn soft_cap_gain(cap: i32, pre: i32, gain: i32) -> i32 {
    if cap <= 0 || gain <= 0 {
        return gain;
    }
    let below = gain.min((cap - pre).max(0));
    below + (gain - below) / 2
}

/// 对一次入账的五维增量套软上限（`status_pt[5]` 的技能点不参与）
///
/// `pre` = 入账前的五维（逐维取，过线只看该维自己的赛前值）。
pub(crate) fn apply_soft_cap(cap: i32, pre: &Array5, value: &mut ActionValue) {
    if cap <= 0 {
        return;
    }
    let gains = value.status_pt;
    for (slot, (&p, &g)) in pre.iter().zip(gains.iter()).enumerate() {
        value.status_pt[slot] = soft_cap_gain(cap, p, g);
    }
}

/// 把「已入账的增量」折算成软上限后的增量（五维逐维；技能点不动）
///
/// 用于赛后奖励这类**结算发生在外部**的通道：先按原值入账，再按赛前值折算回填。
pub(crate) fn capped_gain(cap: i32, pre: &Array5, gained: &Array5) -> Array5 {
    let mut out: Array5 = [0; 5];
    for (slot, (&p, &g)) in pre.iter().zip(gained.iter()).enumerate() {
        out[slot] = soft_cap_gain(cap, p, g);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_cap_partial_halving() {
        // 报文算例：pre=1195 名义 30 → 17；pre=1196 名义 14 → 9；pre=1179 名义 38 → 29
        assert_eq!(soft_cap_gain(1200, 1195, 30), 17);
        assert_eq!(soft_cap_gain(1200, 1196, 14), 9);
        assert_eq!(soft_cap_gain(1200, 1179, 38), 29);
    }

    #[test]
    fn soft_cap_above_and_below_cap() {
        // 赛后奖励算例：pre=1300 名义 7 → 3（cid168 t54 大阪杯 速度）
        assert_eq!(soft_cap_gain(1200, 1300, 7), 3);
        // 没过线：全额
        assert_eq!(soft_cap_gain(1200, 451, 7), 7);
        assert_eq!(soft_cap_gain(1200, 1193, 7), 7);
    }

    #[test]
    fn soft_cap_disabled_returns_gain() {
        assert_eq!(soft_cap_gain(0, 1300, 7), 7);
        assert_eq!(soft_cap_gain(-1, 1300, 7), 7);
        // 无增量 / 负增量不动
        assert_eq!(soft_cap_gain(1200, 100, 0), 0);
    }

    #[test]
    fn apply_soft_cap_keeps_skill_point() {
        let mut value = ActionValue {
            status_pt: [10, 0, 0, 0, 0, 30],
            ..Default::default()
        };
        let pre: Array5 = [1200, 0, 0, 0, 0];
        apply_soft_cap(1200, &pre, &mut value);
        assert_eq!(value.status_pt[0], 5, "过线的那一维减半");
        assert_eq!(value.status_pt[5], 30, "技能点不受软上限影响");
    }
}
