//! 互斥锁工具（通用）：中毒取内层 + 可见信号，不静默不 panic（P-8 可见降级）
//!
//! 收口前同一恢复样板在 gamify/projector、llm/router、llm/ledger、engine/practice 逐处重写（2026-09-27 复用合并）。

use std::sync::{Mutex, MutexGuard};

/// 锁定互斥量：中毒时取内层状态并打 eprintln 可见信号（不静默吞、不 panic）
pub fn lock_recover<'a, T>(mutex: &'a Mutex<T>, where_: &str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[warn] mutex poisoned at {where_}, recovering inner state");
            poisoned.into_inner()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_recover_returns_guard_and_writes_back() {
        let mutex = Mutex::new(5);
        {
            let guard = lock_recover(&mutex, "test.point");
            assert_eq!(*guard, 5);
        }
        let mut guard = lock_recover(&mutex, "test.point");
        *guard = 6;
        drop(guard);
        assert_eq!(*lock_recover(&mutex, "test.point"), 6, "守卫写回可见");
    }

    #[test]
    fn test_lock_recover_takes_inner_from_poisoned() {
        use std::panic::AssertUnwindSafe;
        let mutex = Mutex::new(7);
        let poisoned = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut guard = mutex.lock().expect("锁");
            *guard = 8;
            panic!("人为持锁 panic 制造中毒");
        }));
        assert!(poisoned.is_err(), "锁已中毒");
        let guard = lock_recover(&mutex, "test.poison");
        assert_eq!(*guard, 8, "中毒取内层数据，不 panic 不静默");
    }
}
