//! 置信度路由：Route/route_from_margin/route_by_confidence（accept 0.75/reject 0.35，docs/05 §3）

use super::*;

/// fan-out 作业：一次作答一单（state + 判断集，M3）
#[derive(Debug, Clone)]
pub struct JevJob {
    pub state: JevState,
    pub questions: JevQuestions,
}

/// 路由决策（docs/05 §2：阈值两端采信、灰区升级复核，M3）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// 高置信：采信判断（Noul 采信「是」）
    AutoAccept,
    /// 低置信：不采信该判断（Noul 采信「否」——确定性否定）
    AutoReject,
    /// 灰区：升级 LLM 粗判（只出判断不打分）
    LlmReview,
    /// LLM 复核不可用时的人工裁决（由 07 降级填入，不直接由阈值产生）
    HumanReview,
}

/// 阈值路由（纯 margin 版；`route_by_confidence` 的底座，grader 按裸 margin 复用）
pub fn route_from_margin(margin: f32, accept: f32, reject: f32) -> Route {
    if margin >= accept {
        Route::AutoAccept
    } else if margin <= reject {
        Route::AutoReject
    } else {
        Route::LlmReview
    }
}

/// 置信度路由（docs/05 §2：margin ≥ accept → AutoAccept；margin ≤ reject → AutoReject；灰区 → LlmReview）
/// Noul 的 margin = yes_prob（有向）；Choice/Score 的 margin = confidence。
pub fn route_by_confidence(ans: &JevAnswer, accept: f32, reject: f32) -> Route {
    let margin = match ans {
        JevAnswer::Noul { yes_prob } => *yes_prob as f32,
        JevAnswer::Choice { confidence, .. } => *confidence as f32,
        JevAnswer::Score { confidence, .. } => *confidence as f32,
    };
    route_from_margin(margin, accept, reject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_from_margin_orders_three_bands() {
        assert_eq!(route_from_margin(0.75, 0.75, 0.35), Route::AutoAccept);
        assert_eq!(route_from_margin(0.74, 0.75, 0.35), Route::LlmReview);
        assert_eq!(route_from_margin(0.35, 0.75, 0.35), Route::AutoReject);
        assert_eq!(route_from_margin(0.36, 0.75, 0.35), Route::LlmReview);
        assert_eq!(route_from_margin(0.0, 0.75, 0.35), Route::AutoReject);
        assert_eq!(route_from_margin(1.0, 0.75, 0.35), Route::AutoAccept);
        // 退化阈值（reject ≥ accept）仍不 panic：先判 accept
        assert_eq!(route_from_margin(0.5, 0.5, 0.5), Route::AutoAccept);
    }

    #[test]
    fn test_route_by_confidence_boundary_table() {
        let noul = |p: f64| JevAnswer::Noul { yes_prob: p };
        let choice = |c: f64| JevAnswer::Choice {
            choice: "x".into(),
            probabilities: BTreeMap::new(),
            confidence: c,
        };
        let score = |c: f64| JevAnswer::Score {
            score: 1.0,
            confidence: c,
        };
        let (accept, reject) = (0.75, 0.35); // 英语默认阈值
        // 边界（=阈值）表驱动：两端闭区间采信
        let table: Vec<(JevAnswer, Route)> = vec![
            (noul(0.75), Route::AutoAccept),
            (noul(0.749), Route::LlmReview),
            (noul(0.5), Route::LlmReview),
            (noul(0.351), Route::LlmReview),
            (noul(0.35), Route::AutoReject),
            (choice(0.9), Route::AutoAccept),
            (choice(0.75), Route::AutoAccept),
            (choice(0.5), Route::LlmReview),
            (choice(0.35), Route::AutoReject),
            (score(0.8), Route::AutoAccept),
            (score(0.1), Route::AutoReject),
            (score(0.4), Route::LlmReview),
        ];
        for (ans, expected) in &table {
            assert_eq!(
                route_by_confidence(ans, accept, reject),
                *expected,
                "路由不符: {ans:?}"
            );
        }
    }

    #[test]
    fn test_route_by_confidence_switches_to_non_english_thresholds() {
        // docs/05 §3：其它语种 0.85/0.20（更保守：更多灰区进复核）
        let (accept, reject) = (0.85, 0.20);
        let p80 = JevAnswer::Noul { yes_prob: 0.80 };
        assert_eq!(
            route_by_confidence(&p80, accept, reject),
            Route::LlmReview,
            "en 下 0.80 自动接受，其它语种进灰区"
        );
        assert_eq!(
            route_by_confidence(&p80, 0.75, 0.35),
            Route::AutoAccept,
            "英语阈值同输入不同结论"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.85 }, accept, reject),
            Route::AutoAccept,
            "=0.85 边界采信"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.20 }, accept, reject),
            Route::AutoReject,
            "=0.20 边界拒绝"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.21 }, accept, reject),
            Route::LlmReview,
            "0.21 进灰区"
        );
    }
}
