//! 打字指标计算（细分域，docs/04-typing §指标）：wpm/准确率/热力/瓶颈键/基线

use super::*;

/// 由击键流计算统计：wpm = 正确字符/5 / 分钟；accuracy = 正确占比%
pub fn stats(keystrokes: &[KeyStroke]) -> TypingStats {
    let total = keystrokes.len();
    let correct = keystrokes.iter().filter(|k| k.correct).count();
    let accuracy = if total == 0 {
        0.0
    } else {
        correct as f32 / total as f32 * 100.0
    };
    let wpm = if total < 2 {
        0.0
    } else {
        let span_ms = keystrokes[total - 1]
            .t_ms
            .saturating_sub(keystrokes[0].t_ms);
        if span_ms == 0 {
            0.0
        } else {
            (correct as f32 / 5.0) / (span_ms as f32 / 60_000.0)
        }
    };
    TypingStats {
        wpm,
        accuracy,
        correct,
        total,
    }
}

/// 错误键位热力（按期望字符统计错误次数，计数降序、同数按键位升序）
pub fn heat(keystrokes: &[KeyStroke]) -> Vec<(char, u32)> {
    let mut counts: Vec<(char, u32)> = Vec::new();
    for k in keystrokes.iter().filter(|k| !k.correct) {
        match counts.iter_mut().find(|(c, _)| *c == k.expected) {
            Some((_, n)) => *n += 1,
            None => counts.push((k.expected, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts
}

/// 瓶颈键位（错误次数 ≥ 阈值），用于专项练习
pub fn bottleneck_keys(errors: &[(char, u32)], threshold: u32) -> Vec<char> {
    errors
        .iter()
        .filter(|(_, n)| *n >= threshold)
        .map(|(c, _)| *c)
        .collect()
}

/// 打字基线（历史 wpm 中位数；空历史返回 0）
pub fn baseline_wpm(history: &[f32]) -> f32 {
    if history.is_empty() {
        return 0.0;
    }
    let mut sorted = history.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: char, expected: char, t_ms: u64, correct: bool) -> KeyStroke {
        KeyStroke {
            key,
            t_ms,
            expected,
            correct,
        }
    }

    #[test]
    fn test_stats_computes_wpm_and_accuracy() {
        // 10 个正确字符、跨度正好 1 分钟（末点 60000ms）→ wpm = 10/5 = 2
        let ks: Vec<KeyStroke> = (0..10)
            .map(|i| stroke('a', 'a', i * 60_000 / 9, true))
            .collect();
        let s = stats(&ks);
        assert!((s.wpm - 2.0).abs() < 0.001);
        assert!((s.accuracy - 100.0).abs() < 0.001);
        assert_eq!(s.correct, 10);
        assert_eq!(s.total, 10);
    }

    #[test]
    fn test_stats_handles_empty_and_single() {
        assert_eq!(stats(&[]).wpm, 0.0);
        assert_eq!(stats(&[]).accuracy, 0.0);
        let one = vec![stroke('a', 'a', 0, true)];
        assert_eq!(stats(&one).wpm, 0.0);
    }

    #[test]
    fn test_heat_counts_errors_sorted() {
        let ks = vec![
            stroke('t', 'e', 0, false),
            stroke('e', 'e', 1, true),
            stroke('x', 'e', 2, false),
            stroke('y', 'r', 3, false),
        ];
        let h = heat(&ks);
        assert_eq!(h[0], ('e', 2)); // e 错两次排最前
        assert_eq!(h.len(), 2);
        assert_eq!(heat(&[]).len(), 0);
    }

    #[test]
    fn test_bottleneck_keys_filters_by_threshold() {
        let h = vec![('e', 5), ('r', 3), ('x', 1)];
        assert_eq!(bottleneck_keys(&h, 3), vec!['e', 'r']);
        assert!(bottleneck_keys(&h, 10).is_empty());
    }

    #[test]
    fn test_baseline_wpm_median_even_and_odd() {
        assert_eq!(baseline_wpm(&[]), 0.0);
        assert_eq!(baseline_wpm(&[10.0, 30.0, 50.0]), 30.0);
        assert_eq!(baseline_wpm(&[10.0, 30.0]), 20.0);
    }
}
