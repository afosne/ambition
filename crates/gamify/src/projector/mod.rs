//! 游戏投影器（细分域，docs/13）：订阅 DomainEvent → 应用效果到状态（engine 统一落库）
//! Mutex 中毒不静默吞（P-8）：取内层状态 + eprintln 可见信号。

use std::sync::Mutex;

use ambition_core::lock_recover;

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
            state: Mutex::new(GameState {
                xp,
                streak,
                ..GameState::default()
            }),
            pending: Mutex::new(Vec::new()),
        }
    }

    pub fn hydrate_state(state: GameState) -> Self {
        Self {
            state: Mutex::new(state),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// 当前状态快照（落库前读取）
    pub fn snapshot(&self) -> GameState {
        lock_recover(&self.state, "GameProjector.state").clone()
    }

    /// 取走并清空待结算效果（engine 返回给调用方）
    pub fn drain_effects(&self) -> Vec<GameEffect> {
        std::mem::take(&mut *lock_recover(&self.pending, "GameProjector.pending"))
    }

    /// 效果 → 状态（唯一结算点）
    fn apply(&self, effects: &[GameEffect]) {
        let mut state = lock_recover(&self.state, "GameProjector.state");
        if !state.enabled {
            return;
        }
        for effect in effects {
            match effect {
                GameEffect::Xp(n) => {
                    let n = i64::from(*n) * if state.double_xp { 2 } else { 1 };
                    state.xp += n;
                    state.double_xp = false;
                }
                GameEffect::Coins(n) => state.coins = (state.coins + i64::from(*n)).max(0),
                GameEffect::ComboUp => {
                    state.streak += 1;
                    state.combo_best = state.combo_best.max(state.streak);
                    if state.streak >= 10 {
                        state.badges.insert("combo_10".into());
                    }
                }
                GameEffect::ComboBreak => {
                    if state.freeze_charges > 0 {
                        state.freeze_charges -= 1;
                    } else {
                        state.streak = 0;
                    }
                }
                GameEffect::UnlockBadge(id) => {
                    state.badges.insert(id.clone());
                }
                GameEffect::GiveItem(id, count) => {
                    *state.inventory.entry(id.clone()).or_default() += *count
                }
                GameEffect::MapProgress(node) => {
                    *state.map_progress.entry(node.clone()).or_default() += 1
                }
                GameEffect::Notify(_) => {}
            }
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        lock_recover(&self.state, "GameProjector.state").enabled = enabled;
    }

    pub fn grant_item(&self, item: &str, count: u32) {
        *lock_recover(&self.state, "GameProjector.state")
            .inventory
            .entry(item.to_string())
            .or_default() += count;
    }

    pub fn purchase_item(&self, item: crate::GameItem, count: u32) -> Result<(), crate::GameError> {
        let price = crate::item_price(item) * i64::from(count);
        let mut state = lock_recover(&self.state, "GameProjector.state");
        if !state.enabled {
            return Err(crate::GameError::ItemUnavailable("游戏已关闭".into()));
        }
        if count == 0 || state.coins < price {
            return Err(crate::GameError::ItemUnavailable("金币不足".into()));
        }
        state.coins -= price;
        *state
            .inventory
            .entry(item.as_str().to_string())
            .or_default() += count;
        Ok(())
    }

    pub fn use_item(&self, item: crate::GameItem) -> Result<Vec<GameEffect>, crate::GameError> {
        let mut state = lock_recover(&self.state, "GameProjector.state");
        if !state.enabled {
            return Ok(vec![GameEffect::Notify("游戏已关闭".into())]);
        }
        let key = item.as_str();
        let quantity = state
            .inventory
            .get_mut(key)
            .filter(|n| **n > 0)
            .ok_or_else(|| crate::GameError::ItemUnavailable(key.into()))?;
        *quantity -= 1;
        let effects = match item {
            crate::GameItem::Hint => vec![GameEffect::Notify("提示已启用".into())],
            crate::GameItem::Eliminate => vec![GameEffect::Notify("干扰项排除已启用".into())],
            crate::GameItem::DoubleXp => {
                state.double_xp = true;
                vec![GameEffect::Notify("下一次经验翻倍".into())]
            }
            crate::GameItem::StreakFreeze => {
                state.freeze_charges = state.freeze_charges.saturating_add(1);
                vec![GameEffect::Notify("连击保护已启用".into())]
            }
            crate::GameItem::Makeup => {
                state.streak = state.streak.max(1);
                vec![GameEffect::Notify("连击补签完成".into())]
            }
        };
        Ok(effects)
    }
}

impl ambition_core::EventBus for GameProjector {
    fn publish(&self, ev: ambition_core::DomainEvent) {
        if !lock_recover(&self.state, "GameProjector.state").enabled {
            return;
        }
        let effects = crate::on_event(&ev);
        if !effects.is_empty() {
            lock_recover(&self.pending, "GameProjector.pending").extend(effects.clone());
        }
        self.apply(&effects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameState;
    use ambition_core::{EventBus, Qid, Verdict};

    #[test]
    fn test_hydrate_state_preserves_full_profile() {
        let state = GameState {
            coins: 12,
            ..Default::default()
        };
        let projector = GameProjector::hydrate_state(state.clone());
        assert_eq!(projector.snapshot(), state);
    }

    #[test]
    fn test_grant_item_updates_inventory() {
        let projector = GameProjector::hydrate(0, 0);
        projector.grant_item("hint", 2);
        assert_eq!(projector.snapshot().inventory["hint"], 2);
    }

    #[test]
    fn test_purchase_item_charges_coins_and_adds_inventory() {
        let projector = GameProjector::hydrate_state(GameState {
            coins: 20,
            ..Default::default()
        });
        projector
            .purchase_item(crate::GameItem::Hint, 1)
            .expect("购买");
        assert_eq!(projector.snapshot().inventory["hint"], 1);
        assert!(projector.snapshot().coins < 20);
    }

    #[test]
    fn test_set_enabled_blocks_event_projection() {
        let projector = GameProjector::hydrate(0, 0);
        projector.set_enabled(false);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(projector.snapshot().xp, 0);
    }

    #[test]
    fn test_use_item_consumes_inventory_and_enables_effect() {
        let projector = GameProjector::hydrate(0, 0);
        projector.grant_item("double_xp", 1);
        projector.use_item(crate::GameItem::DoubleXp).expect("使用");
        assert!(projector.snapshot().double_xp);
        assert_eq!(projector.snapshot().inventory["double_xp"], 0);
    }

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
        assert_eq!(
            fresh.snapshot(),
            GameState {
                xp: 0,
                streak: 0,
                ..Default::default()
            }
        );
        let resumed = GameProjector::hydrate(120, 7);
        assert_eq!(
            resumed.snapshot(),
            GameState {
                xp: 120,
                streak: 7,
                combo_best: 0,
                ..Default::default()
            },
            "从 game_state 水合"
        );
    }

    #[test]
    fn test_publish_applies_effects_and_buffers_them() {
        let projector = GameProjector::hydrate(10, 1);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(
            projector.snapshot(),
            GameState {
                xp: 12,
                streak: 2,
                combo_best: 2,
                coins: 1,
                map_progress: [("q2".into(), 1)].into(),
                ..Default::default()
            },
            "+2xp +1连击"
        );
        assert_eq!(
            projector.drain_effects(),
            vec![
                GameEffect::Xp(2),
                GameEffect::Coins(1),
                GameEffect::ComboUp,
                GameEffect::MapProgress("q2".into())
            ],
            "效果可取走"
        );
        assert!(projector.drain_effects().is_empty(), "取走即清空");

        projector.publish(graded_event(Verdict::Wrong));
        assert_eq!(
            projector.snapshot(),
            GameState {
                xp: 13,
                streak: 0,
                combo_best: 2,
                coins: 1,
                map_progress: [("q2".into(), 1)].into(),
                ..Default::default()
            },
            "答错 +1xp 断连击"
        );
    }

    #[test]
    fn test_apply_is_idempotent_per_buffered_effects() {
        let projector = GameProjector::hydrate(0, 0);
        projector.publish(graded_event(Verdict::Pending));
        assert_eq!(
            projector.snapshot(),
            GameState {
                xp: 0,
                streak: 0,
                ..Default::default()
            },
            "Pending 无效果"
        );
        assert!(projector.drain_effects().is_empty());
    }

    #[test]
    fn test_drain_effects_clears_after_read() {
        let projector = GameProjector::hydrate(0, 0);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(projector.drain_effects().len(), 4);
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
    fn test_items_freeze_combo_double_xp_and_game_toggle() {
        let projector = GameProjector::hydrate(0, 2);
        projector.grant_item("streak_freeze", 1);
        projector.grant_item("double_xp", 1);
        projector
            .use_item(crate::GameItem::StreakFreeze)
            .expect("freeze");
        projector
            .use_item(crate::GameItem::DoubleXp)
            .expect("double");
        projector.publish(graded_event(Verdict::Wrong));
        let snapshot = projector.snapshot();
        assert_eq!(snapshot.xp, 2, "下一次 XP 翻倍");
        assert_eq!(snapshot.streak, 2, "冻结一次连击中断");
        assert_eq!(snapshot.freeze_charges, 0);
        projector.drain_effects();
        projector.set_enabled(false);
        projector.publish(graded_event(Verdict::Correct));
        assert_eq!(projector.snapshot().xp, 2, "关闭游戏不结算");
        assert!(projector.drain_effects().is_empty(), "关闭时零游戏效果");
    }
}
