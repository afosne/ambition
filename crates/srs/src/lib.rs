//! Ambition srs：间隔重复调度决策（M1 决策层；FSRS 全量见 docs/08-adaptive）

use ambition_core::Verdict;

/// 复习评分（FSRS 四档）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

/// 作答后对卡片的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Create,
    Bump,
    Schedule(Rating),
    Skip,
}

/// 作答 → 卡片处置（错/假对入卡，Pending 不动 —— docs/08-adaptive §on_attempt）
pub fn decide_action(verdict: Verdict, is_guess: bool, has_card: bool) -> CardAction {
    match verdict {
        Verdict::Wrong | Verdict::Partial => {
            if has_card {
                CardAction::Bump
            } else {
                CardAction::Create
            }
        }
        Verdict::Correct | Verdict::Acceptable => {
            if is_guess {
                if has_card {
                    CardAction::Bump
                } else {
                    CardAction::Create
                }
            } else if has_card {
                CardAction::Schedule(Rating::Good)
            } else {
                CardAction::Skip
            }
        }
        Verdict::Pending => CardAction::Skip,
    }
}

/// 检索难度档（连续答对升档，答错回基础档 —— docs/08-adaptive §next_mode）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    Choice,
    Fill,
    Spell,
}

/// 复习题型递进：失败回 Choice；≥2 连对进 Fill；≥4 连对进 Spell
pub fn next_mode(ok_streak: u32, failed_last: bool) -> ReviewMode {
    if failed_last {
        ReviewMode::Choice
    } else if ok_streak >= 4 {
        ReviewMode::Spell
    } else if ok_streak >= 2 {
        ReviewMode::Fill
    } else {
        ReviewMode::Choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decide_action_on_wrong_and_partial() {
        assert_eq!(
            decide_action(Verdict::Wrong, false, false),
            CardAction::Create
        );
        assert_eq!(decide_action(Verdict::Wrong, false, true), CardAction::Bump);
        assert_eq!(
            decide_action(Verdict::Partial, false, true),
            CardAction::Bump
        );
        assert_eq!(
            decide_action(Verdict::Partial, false, false),
            CardAction::Create
        );
    }

    #[test]
    fn test_decide_action_on_correct_with_and_without_guess() {
        // 正常答对：已有卡 → Good 调度；无卡 → 不建卡
        assert_eq!(
            decide_action(Verdict::Correct, false, true),
            CardAction::Schedule(Rating::Good)
        );
        assert_eq!(
            decide_action(Verdict::Correct, false, false),
            CardAction::Skip
        );
        // 假对（蒙对）→ 必须入卡
        assert_eq!(
            decide_action(Verdict::Correct, true, false),
            CardAction::Create
        );
        assert_eq!(
            decide_action(Verdict::Correct, true, true),
            CardAction::Bump
        );
        assert_eq!(
            decide_action(Verdict::Acceptable, true, false),
            CardAction::Create
        );
    }

    #[test]
    fn test_decide_action_pending_skips() {
        assert_eq!(
            decide_action(Verdict::Pending, false, true),
            CardAction::Skip
        );
        assert_eq!(
            decide_action(Verdict::Pending, true, false),
            CardAction::Skip
        );
    }

    #[test]
    fn test_next_mode_streak_escalation_and_reset() {
        assert_eq!(next_mode(0, false), ReviewMode::Choice);
        assert_eq!(next_mode(1, false), ReviewMode::Choice);
        assert_eq!(next_mode(2, false), ReviewMode::Fill);
        assert_eq!(next_mode(4, false), ReviewMode::Spell);
        assert_eq!(next_mode(9, true), ReviewMode::Choice); // 答错回退
    }
}
