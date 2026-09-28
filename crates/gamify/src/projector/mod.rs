//! 游戏投影器（细分域，docs/13）：订阅 DomainEvent → 应用效果到状态（engine 统一落库）
//! Mutex 中毒不静默吞（P-8）：取内层状态 + eprintln 可见信号。

use std::sync::Mutex;

use crate::{GameEffect, GameState};

/// 游戏投影器：订阅 DomainEvent → 应用效果到状态（engine 统一落库）
/// Mutex 中毒不静默吞（P-8）：取内层状态 + eprintln 可见信号。
pub struct GameProjector {
    state: Mutex<GameState>,
    pending: Mutex<Vec<GameEffect>>,
}

impl GameProjector {
    /// 从持久化状态水合（无存档 = 零值）
    pub fn hydrate(xp: i64, streak: i64) -> Self {
        Self {
            state: Mutex::new(GameState { xp, streak }),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// 当前状态快照（落库前读取）
    pub fn snapshot(&self) -> GameState {
        *lock_or_recover(&self.state, "GameProjector.state")
    }

    /// 取走并清空待结算效果（engine 返回给调用方）
    pub fn drain_effects(&self) -> Vec<GameEffect> {
        std::mem::take(&mut *lock_or_recover(
            &self.pending,
            "GameProjector.pending",
        ))
    }

    /// 效果 → 状态（唯一结算点）
    fn apply(&self, effects: &[GameEffect]) {
        let mut state = lock_or_recover(&self.state, "GameProjector.state");
        for effect in effects {
            match effect {
                GameEffect::Xp(n) => state.xp += i64::from(*n),
                GameEffect::ComboUp => state.streak += 1,
                GameEffect::ComboBreak => state.streak = 0,
            }
        }
    }
}

/// Mutex 锁定（中毒取内层 + 可见信号，不静默、不 panic）
pub(crate) fn lock_or_recover<'a, T>(
    mutex: &'a Mutex<T>,
    where_: &str,
) -> std::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[warn] mutex poisoned at {where_}, recovering inner state");
            poisoned.into_inner()
        }
    }
}

impl ambition_core::EventBus for GameProjector {
    fn publish(&self, ev: ambition_core::DomainEvent) {
        let effects = crate::on_event(&ev);
        if !effects.is_empty() {
            lock_or_recover(&self.pending, "GameProjector.pending").extend(effects.clone());
        }
        self.apply(&effects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameState;
    use ambition_core::{EventBus, Qid, Verdict};

    fn graded_event(verdict: Verdict) -> ambition_core::DomainEvent {
        ambition_core::DomainEvent::AttemptGraded {
            attempt_id: "a2".into(),
            qid: Qid("q2".into()),
            verdict,
            correct: matches!(verdict, Verdict::Correct | Verdict::Acceptable),
        }
    }

    #[test]
    fn test_hydrate_starts_from_persisted_state() {
        let fresh = GameProjector::hydrate(0, 0);
        assert_eq!(fresh.snapshot(), GameState { xp: 0, streak: 0 });
        let resumed = GameProjector::hydrate(120, 7);
        assert_eq!(
            resumed.snapshot(),
            GameState { xp: 120, streak: 7 },
            "从 game_state 水合"
        );
    }

    #[test]
    fn test_publish_applies_effects_and_buffers_them() {
        let projector = GameProjector::hydrate(10, 1);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(
            projector.snapshot(),
            GameState { xp: 12, streak: 2 },
            "+2xp +1连击"
        );
        assert_eq!(
            projector.drain_effects(),
            vec![GameEffect::Xp(2), GameEffect::ComboUp],
            "效果可取走"
        );
        assert!(projector.drain_effects().is_empty(), "取走即清空");

        projector.publish(graded_event(Verdict::Wrong));
        assert_eq!(
            projector.snapshot(),
            GameState { xp: 13, streak: 0 },
            "答错 +1xp 断连击"
        );
    }

    #[test]
    fn test_apply_is_idempotent_per_buffered_effects() {
        let projector = GameProjector::hydrate(0, 0);
        projector.publish(graded_event(Verdict::Pending));
        assert_eq!(
            projector.snapshot(),
            GameState { xp: 0, streak: 0 },
            "Pending 无效果"
        );
        assert!(projector.drain_effects().is_empty());
    }

    #[test]
    fn test_drain_effects_clears_after_read() {
        let projector = GameProjector::hydrate(0, 0);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(projector.drain_effects().len(), 2);
        assert!(projector.drain_effects().is_empty(), "二次取走为空");
        assert_eq!(
            projector.drain_effects(),
            Vec::<GameEffect>::new(),
            "空态返回空向量"
        );
    }

    #[test]
    fn test_snapshot_is_a_value_copy() {
        let projector = GameProjector::hydrate(50, 3);
        let mut first = projector.snapshot();
        assert_eq!(first.xp, 50);
        assert_eq!(first.streak, 3);
        first.xp = 999;
        assert_eq!(first.xp, 999, "局部可变副本");
        assert_eq!(projector.snapshot().xp, 50, "快照不影响内部状态");
    }

    #[test]
    fn test_lock_or_recover_returns_guard() {
        let mutex = Mutex::new(5);
        {
            let guard = lock_or_recover(&mutex, "test.point");
            assert_eq!(*guard, 5);
        }
        let mut guard = lock_or_recover(&mutex, "test.point");
        *guard = 6;
        drop(guard);
        assert_eq!(*lock_or_recover(&mutex, "test.point"), 6);
    }
}
