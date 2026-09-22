//! Ambition gamify：游戏层底座（M1；事件订阅旁路，可整体关闭）
//! 设计见 docs/13-gamify-growth —— 只做反馈与乐趣，不影响排程/评分。

use ambition_core::{DomainEvent, Verdict};

/// 游戏效果（engine 应用后落 game_state）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEffect {
    Xp(i32),
    ComboUp,
    ComboBreak,
}

/// 事件 → 效果映射（唯一结算入口 round_settle 的事件版）
pub fn on_event(event: &DomainEvent) -> Vec<GameEffect> {
    match event {
        DomainEvent::AttemptGraded { verdict, .. } => match verdict {
            Verdict::Correct | Verdict::Acceptable => vec![GameEffect::Xp(2), GameEffect::ComboUp],
            Verdict::Partial => vec![GameEffect::Xp(1), GameEffect::ComboUp],
            Verdict::Wrong => vec![GameEffect::Xp(1), GameEffect::ComboBreak],
            Verdict::Pending => vec![],
        },
    }
}

/// 等级曲线（每级需求 ×1.15 复利，起始 100 XP）
pub fn level_of(xp: i64) -> u32 {
    let mut level = 1u32;
    let mut need = 100.0f64;
    let mut remain = xp.max(0) as f64;
    while remain >= need {
        remain -= need;
        need *= 1.15;
        level += 1;
    }
    level
}

/// 一局结算（Boss/小测复用）：满分 +5 奖励，零对鼓励 +1 且断连击
pub fn round_settle(correct: usize, total: usize) -> Vec<GameEffect> {
    let base = correct as i32 * 2;
    if total == 0 {
        return vec![];
    }
    if correct == total {
        vec![GameEffect::Xp(base + 5), GameEffect::ComboUp]
    } else if correct == 0 {
        vec![GameEffect::Xp(base.max(1)), GameEffect::ComboBreak]
    } else {
        vec![GameEffect::Xp(base)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::Qid;

    fn graded(verdict: Verdict) -> DomainEvent {
        DomainEvent::AttemptGraded {
            attempt_id: "a1".into(),
            qid: Qid("q1".into()),
            verdict,
            correct: matches!(verdict, Verdict::Correct | Verdict::Acceptable),
        }
    }

    #[test]
    fn test_on_event_maps_all_verdicts() {
        assert_eq!(
            on_event(&graded(Verdict::Correct)),
            vec![GameEffect::Xp(2), GameEffect::ComboUp]
        );
        assert_eq!(
            on_event(&graded(Verdict::Acceptable)),
            vec![GameEffect::Xp(2), GameEffect::ComboUp]
        );
        assert_eq!(
            on_event(&graded(Verdict::Partial)),
            vec![GameEffect::Xp(1), GameEffect::ComboUp]
        );
        assert_eq!(
            on_event(&graded(Verdict::Wrong)),
            vec![GameEffect::Xp(1), GameEffect::ComboBreak]
        );
        assert!(on_event(&graded(Verdict::Pending)).is_empty());
    }

    #[test]
    fn test_level_of_follows_compound_curve() {
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(99), 1);
        assert_eq!(level_of(100), 2);
        // 100 + 115 = 215 → 3 级
        assert_eq!(level_of(215), 3);
        assert_eq!(level_of(-5), 1); // 负值防御
    }

    #[test]
    fn test_round_settle_bonus_and_encourage() {
        assert_eq!(
            round_settle(5, 5),
            vec![GameEffect::Xp(15), GameEffect::ComboUp]
        );
        assert_eq!(
            round_settle(0, 5),
            vec![GameEffect::Xp(1), GameEffect::ComboBreak]
        );
        assert_eq!(round_settle(3, 5), vec![GameEffect::Xp(6)]);
        assert!(round_settle(0, 0).is_empty());
    }
}
