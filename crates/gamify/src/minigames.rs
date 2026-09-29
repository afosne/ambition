//! 离线复习小游戏与健康限时规则（docs/13 §1）。

use crate::{GameEffect, GameError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryMatch {
    pub pairs: Vec<(String, String)>,
    pub cards: Vec<String>,
    pub matched: Vec<bool>,
    pub attempts: u32,
}

impl MemoryMatch {
    pub fn new(pairs: Vec<(String, String)>) -> Result<Self, GameError> {
        if pairs.is_empty() {
            return Err(GameError::InvalidRound);
        }
        // 按来源/释义分组，记忆配对按相同 pair 索引判定。
        let cards = pairs
            .iter()
            .map(|(a, _)| a.clone())
            .chain(pairs.iter().map(|(_, b)| b.clone()))
            .collect();
        Ok(Self {
            matched: vec![false; pairs.len()],
            pairs,
            cards,
            attempts: 0,
        })
    }
    pub fn turn(&mut self, left: usize, right: usize) -> bool {
        self.attempts += 1;
        if left >= self.cards.len()
            || right >= self.cards.len()
            || left == right
            || self.matched[left % self.pairs.len()]
            || self.matched[right % self.pairs.len()]
        {
            return false;
        }
        let left_pair = left % self.pairs.len();
        let right_pair = right % self.pairs.len();
        let left_translation = left >= self.pairs.len();
        let right_translation = right >= self.pairs.len();
        let hit = left_pair == right_pair
            && left_translation != right_translation
            && self.cards[left] != self.cards[right];
        if hit {
            self.matched[left_pair] = true;
        }
        hit
    }
    pub fn complete(&self) -> bool {
        self.matched.iter().all(|m| *m)
    }
}

pub fn combo_sprint(correct: usize, total: usize) -> Vec<GameEffect> {
    crate::round_settle(correct, total)
}

/// 词根/错因排序：给定依赖顺序，完全按先修项优先判定。
pub fn root_composer(chosen: &[String], prerequisites: &[String]) -> bool {
    !prerequisites.is_empty() && prerequisites.iter().all(|p| chosen.contains(p))
}

pub fn boss_battle(correct: usize, total: usize) -> Result<Vec<GameEffect>, GameError> {
    if total == 0 || correct > total {
        return Err(GameError::InvalidRound);
    }
    Ok(crate::round_settle(correct, total))
}

/// 努力分：学习量占 60%，相对个人基线的进步占 40%，避免按绝对成绩排行。
pub fn effort_score(minutes: u32, progress: f32) -> f32 {
    let volume = (minutes as f32 / 60.0).clamp(0.0, 1.0);
    (volume * 0.6 + progress.clamp(0.0, 1.0) * 0.4) * 100.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayGuard {
    Allowed,
    DailyLimit,
    BreakRequired,
}

pub fn playtime_guard(
    minutes_today: u32,
    session_minutes: u32,
    daily_limit: u32,
    underage: bool,
) -> PlayGuard {
    if underage && session_minutes >= 45 {
        return PlayGuard::BreakRequired;
    }
    if minutes_today.saturating_add(session_minutes) > daily_limit {
        PlayGuard::DailyLimit
    } else {
        PlayGuard::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_empty_deck() {
        assert_eq!(MemoryMatch::new(vec![]), Err(GameError::InvalidRound));
    }

    #[test]
    fn test_turn_and_complete_track_matching_pairs() {
        let mut game = MemoryMatch::new(vec![("term".into(), "释义".into())]).unwrap();
        assert!(game.turn(0, 1));
        assert!(game.complete());
    }

    #[test]
    fn test_combo_sprint_and_boss_battle_share_round_settlement() {
        assert_eq!(combo_sprint(2, 2), boss_battle(2, 2).unwrap());
        assert_eq!(boss_battle(2, 1), Err(GameError::InvalidRound));
    }

    #[test]
    fn test_root_composer_requires_all_prerequisites() {
        let required = vec!["a".to_string(), "b".to_string()];
        assert!(root_composer(&required, &required));
        assert!(!root_composer(&required[..1], &required));
    }

    #[test]
    fn test_effort_score_weights_volume_and_progress() {
        assert_eq!(effort_score(30, 0.5), 50.0);
    }

    #[test]
    fn test_playtime_guard_enforces_underage_break() {
        assert_eq!(playtime_guard(0, 45, 120, true), PlayGuard::BreakRequired);
    }

    #[test]
    fn test_boss_battle_rejects_invalid_and_settles() {
        assert_eq!(boss_battle(0, 0), Err(GameError::InvalidRound));
        assert_eq!(boss_battle(3, 2), Err(GameError::InvalidRound));
        assert_eq!(boss_battle(2, 2).unwrap(), crate::round_settle(2, 2));
    }

    #[test]
    fn test_complete_only_when_all_pairs_matched() {
        let mut game = MemoryMatch::new(vec![
            ("bank".into(), "银行".into()),
            ("light".into(), "光".into()),
        ])
        .expect("game");
        assert!(!game.complete(), "未配对完");
        game.turn(0, 2);
        assert!(!game.complete(), "只配了一对");
        game.turn(1, 3);
        assert!(game.complete(), "全配对完");
    }

    #[test]
    fn memory_match_pairs_terms_bidirectionally() {
        let mut game = MemoryMatch::new(vec![
            ("bank".into(), "银行".into()),
            ("light".into(), "光".into()),
        ])
        .expect("game");
        assert!(!game.turn(0, 3), "bank 不配 光");
        assert!(game.turn(2, 0), "倒序翻牌仍可配对");
        assert!(!game.turn(0, 1), "已配对卡不能重复");
        assert!(game.turn(3, 1));
        assert!(game.complete());
    }

    #[test]
    fn effort_and_playtime_guard_are_bounded() {
        assert_eq!(effort_score(30, 0.5), 50.0);
        assert_eq!(effort_score(240, 2.0), 100.0);
        assert_eq!(playtime_guard(40, 45, 120, true), PlayGuard::BreakRequired);
        assert_eq!(playtime_guard(110, 20, 120, false), PlayGuard::DailyLimit);
    }
}
