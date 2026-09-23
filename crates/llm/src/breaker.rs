//! 全局熔断：Breaker/window_take/count_failure（连续失败 5 开闸、跳过 10 试探，docs/05 §6）

/// 熔断开闸阈值：连续失败达其次数 → 进入快速失败窗口（docs/05 §1，M3）
pub const BREAKER_FAIL_LIMIT: u32 = 5;

/// 熔断快速失败窗口：拒发请求次数，耗尽后放行 1 次试探（成功即关闸）
pub const BREAKER_SKIP_LIMIT: u32 = 10;

/// 全局熔断状态（连续失败计数 + 快速失败窗口；锁不跨 await）
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Breaker {
    pub(crate) fails: u32,
    pub(crate) skip_left: u32,
}

/// 窗口取票：有票则取一张并拒发；无票放行（试探机会）
pub(crate) fn window_take(breaker: &mut Breaker) -> bool {
    if breaker.skip_left > 0 {
        breaker.skip_left -= 1;
        return true;
    }
    false
}

/// 失败计数（达阈值开闸：发满快速失败窗口票）
pub(crate) fn count_failure(breaker: &mut Breaker) {
    breaker.fails += 1;
    if breaker.fails >= BREAKER_FAIL_LIMIT {
        breaker.skip_left = BREAKER_SKIP_LIMIT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_take_consumes_one_ticket_per_call() {
        let mut breaker = Breaker {
            fails: BREAKER_FAIL_LIMIT,
            skip_left: 2,
        };
        assert!(window_take(&mut breaker), "第一张票");
        assert!(window_take(&mut breaker), "第二张票");
        assert!(!window_take(&mut breaker), "票尽放行（试探）");
        assert_eq!(breaker.skip_left, 0, "票数递减到零");
    }

    #[test]
    fn test_count_failure_opens_window_only_at_limit() {
        let mut breaker = Breaker::default();
        for _ in 0..(BREAKER_FAIL_LIMIT - 1) {
            count_failure(&mut breaker);
        }
        assert_eq!(breaker.skip_left, 0, "未达阈值不开闸");
        count_failure(&mut breaker);
        assert_eq!(breaker.fails, BREAKER_FAIL_LIMIT, "达阈值");
        assert_eq!(breaker.skip_left, BREAKER_SKIP_LIMIT, "开闸即发满窗口票");
        // 试探再败：窗口续票
        breaker.skip_left = 0;
        count_failure(&mut breaker);
        assert_eq!(breaker.skip_left, BREAKER_SKIP_LIMIT, "再败续票");
    }
}
