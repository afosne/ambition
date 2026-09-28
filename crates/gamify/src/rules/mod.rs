//! 游戏规则（细分域，docs/13 §原则）：事件→效果映射、等级曲线、一局结算

use ambition_core::{DomainEvent, Verdict};

use crate::GameEffect;

/// 事件 → 效果映射（唯一结算入口 round_settle 的事件版）
pub fn on_event(event: &DomainEvent) -> Vec<GameEffect> {
    match event {
        DomainEvent::AttemptGraded { verdict, .. } => match verdict {
            Verdict::Correct | Verdict::Acceptable => vec![GameEffect::Xp(2), GameEffect::ComboUp],
            Verdict::Partial => vec![GameEffect::Xp(1), GameEffect::ComboUp],
            Verdict::Wrong => vec![GameEffect::Xp(1), GameEffect::ComboBreak],
            Verdict::Pending => vec![],
        },
        // M4：复习只给小额经验、不进连击——排程不受游戏影响（docs/13 §原则）
        DomainEvent::ReviewDone { .. } => vec![GameEffect::Xp(1)],
        // M5：作文批改完成 → 大额经验、不进连击（docs/13：写作质量不受游戏影响）
        DomainEvent::EssayGraded { .. } => vec![GameEffect::Xp(5)],
        // M5：模考交卷 → 更大额经验、不进连击（docs/13）
        DomainEvent::PaperFinished { .. } => vec![GameEffect::Xp(10)],
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
        // ReviewDone：Xp(1) 且不断/不升连击
        assert_eq!(
            on_event(&DomainEvent::ReviewDone {
                card_id: "c1".into(),
                rating: "good".into(),
            }),
            vec![GameEffect::Xp(1)]
        );
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
    fn test_on_event_essay_graded_gives_xp_without_combo() {
        let effects = on_event(&DomainEvent::EssayGraded {
            essay_id: "e1".into(),
            rubric_id: "gaokao25".into(),
            score: 20.0,
            max: 25.0,
        });
        assert_eq!(
            effects,
            vec![GameEffect::Xp(5)],
            "只经验不连击（docs/13 M5）"
        );
    }

    #[test]
    fn test_on_event_paper_finished_gives_big_xp_without_combo() {
        let effects = on_event(&DomainEvent::PaperFinished {
            run_id: "run-1".into(),
            paper_id: "p-1".into(),
            score: 30.0,
            max: 40.0,
        });
        assert_eq!(
            effects,
            vec![GameEffect::Xp(10)],
            "模考大额经验、不进连击（docs/13 M5）"
        );
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
