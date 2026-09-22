//! Ambition gamify：游戏层底座（事件订阅旁路，可整体关闭）
//! 设计见 docs/13-gamify-growth —— 只做反馈与乐趣，不影响排程/评分。
//! M2：GameProjector 实现 core::EventBus（engine 只 publish 不直调，改进 #3），效果由 engine 落 game_state（改进 #1）。

use std::sync::Mutex;

use ambition_core::{DomainEvent, Verdict};

/// 游戏效果（engine 应用后落 game_state）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEffect {
    Xp(i32),
    ComboUp,
    ComboBreak,
}

/// 游戏状态（投影器持有；xp 连击落 game_state，docs/13）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameState {
    pub xp: i64,
    /// 当前连击（ComboUp 累加、ComboBreak 清零）
    pub streak: i64,
}

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
fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, where_: &str) -> std::sync::MutexGuard<'a, T> {
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
        let effects = on_event(&ev);
        if !effects.is_empty() {
            lock_or_recover(&self.pending, "GameProjector.pending").extend(effects.clone());
        }
        self.apply(&effects);
    }
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

    fn graded_event(verdict: Verdict) -> DomainEvent {
        DomainEvent::AttemptGraded {
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
        use ambition_core::EventBus;
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
        use ambition_core::EventBus;
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
        use ambition_core::EventBus;
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
