//! 文本规范化与假对检测纯函数：normalize_text/is_guess/baseline_from_timings（docs/08 §3）

use crate::*;

/// 规范化比对：小写、全角转半角、折叠空白、去尾部标点
pub fn normalize_text(input: &str) -> String {
    let mapped: String = input
        .chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect();
    let lowered = mapped.to_lowercase();
    lowered
        .trim()
        .trim_end_matches(|c: char| ".,!?;:".contains(c))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 假对判定（猜对 ≠ 会做，docs/08-adaptive §假对三形态）
pub fn is_guess(timing: &Timing, self_confidence: Option<u8>, baseline: &UserBaseline) -> bool {
    if self_confidence == Some(1) {
        return true;
    }
    if timing.revision_count == 0
        && u128::from(timing.total_ms) < u128::from(baseline.latency_p50_ms) * 3 / 10
    {
        return true;
    }
    self_confidence.is_some_and(|c| c <= 2) && timing.total_ms > baseline.latency_p90_ms
}

/// 假对基线：近 N 次 total_ms 滚动 p50/p90（nearest-rank）；空历史用默认 15s/60s（冷启动不误判，docs/08 §2）
pub fn baseline_from_timings(timings: &[Timing]) -> UserBaseline {
    if timings.is_empty() {
        return UserBaseline {
            latency_p50_ms: 15_000,
            latency_p90_ms: 60_000,
        };
    }
    let mut samples: Vec<u64> = timings.iter().map(|t| t.total_ms).collect();
    samples.sort_unstable();
    let n = samples.len();
    let pick = |pct: usize| samples[(n * pct).div_ceil(100) - 1];
    UserBaseline {
        latency_p50_ms: pick(50),
        latency_p90_ms: pick(90),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text_lowercases_and_strips() {
        assert_eq!(normalize_text("  The Student,  "), "the student");
        assert_eq!(normalize_text("Ａ Student!"), "a student"); // 全角转半角
        assert_eq!(normalize_text("She\tis\nok."), "she is ok");
        assert_eq!(normalize_text(""), "");
    }

    #[test]
    fn test_is_guess_covers_three_forms() {
        let base = UserBaseline {
            latency_p50_ms: 10_000,
            latency_p90_ms: 30_000,
        };
        // 形态①：秒选（< p50×0.3 且零修订）
        let quick = Timing {
            total_ms: 1_000,
            revision_count: 0,
        };
        assert!(is_guess(&quick, Some(4), &base));
        // 形态②：久犹豫低自评（> p90 且 conf ≤ 2）
        let slow = Timing {
            total_ms: 35_000,
            revision_count: 5,
        };
        assert!(is_guess(&slow, Some(2), &base));
        // 形态③：自评完全没把握
        let mid = Timing {
            total_ms: 15_000,
            revision_count: 1,
        };
        assert!(is_guess(&mid, Some(1), &base));
        // 正常作答不是假对
        let normal = Timing {
            total_ms: 12_000,
            revision_count: 2,
        };
        assert!(!is_guess(&normal, Some(4), &base));
    }

    #[test]
    fn test_baseline_from_timings_percentiles_and_empty_default() {
        // 空历史 → 默认 15s/60s（冷启动不误判）
        assert_eq!(
            baseline_from_timings(&[]),
            UserBaseline {
                latency_p50_ms: 15_000,
                latency_p90_ms: 60_000
            }
        );
        // 1..=100ms 样本：nearest-rank p50 = 50, p90 = 90
        let samples: Vec<Timing> = (1..=100)
            .map(|i| Timing {
                total_ms: i,
                revision_count: 0,
            })
            .collect();
        assert_eq!(
            baseline_from_timings(&samples),
            UserBaseline {
                latency_p50_ms: 50,
                latency_p90_ms: 90
            }
        );
        // 单样本退化不越界
        let one = [Timing {
            total_ms: 42,
            revision_count: 1,
        }];
        assert_eq!(baseline_from_timings(&one).latency_p50_ms, 42);
    }
}
