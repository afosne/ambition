//! 本地学习画像与周报载荷（docs/12 M6-W2/W4）。

use std::collections::BTreeMap;

use super::ErrorTag;

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct HeatMap {
    pub cells: BTreeMap<(String, String), f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Rising,
    Stable,
    Falling,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WeakPoint {
    pub tag: ErrorTag,
    pub knowledge: String,
    pub density: f32,
    pub trend: Trend,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProfileSnapshot {
    pub from: i64,
    pub to: i64,
    pub attempts: u32,
    pub correct: u32,
    pub heat: HeatMap,
    pub weak_points: Vec<WeakPoint>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WeeklyReport {
    pub week_start: String,
    pub week_end: String,
    pub attempts: u32,
    pub correct: u32,
    pub accuracy: f32,
    pub active_days: u8,
    pub top_weak_points: Vec<WeakPoint>,
    pub mock_runs: u32,
    pub mock_average: Option<f32>,
    pub score_estimate: ScoreEstimate,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScoreEstimate {
    pub estimate: f32,
    pub low: f32,
    pub high: f32,
    pub samples: u32,
}

impl HeatMap {
    pub fn add(&mut self, tag: ErrorTag, knowledge: impl Into<String>, value: f32) {
        *self
            .cells
            .entry((tag.as_str().to_string(), knowledge.into()))
            .or_default() += value;
    }
}

impl ProfileSnapshot {
    pub fn empty(from: i64, to: i64) -> Self {
        Self {
            from,
            to,
            attempts: 0,
            correct: 0,
            heat: HeatMap::default(),
            weak_points: Vec::new(),
        }
    }
}

pub fn estimate_score(runs: &[(f32, f32)], _target: &str) -> ScoreEstimate {
    let valid: Vec<f32> = runs
        .iter()
        .filter(|(score, max)| *max > 0.0 && score.is_finite() && max.is_finite())
        .map(|(score, max)| (score / max * 100.0).clamp(0.0, 100.0))
        .collect();
    if valid.is_empty() {
        return ScoreEstimate {
            estimate: 50.0,
            low: 30.0,
            high: 70.0,
            samples: 0,
        };
    }
    let average = valid.iter().sum::<f32>() / valid.len() as f32;
    let trend = if valid.len() > 1 {
        (valid.last().unwrap() - valid[0]) / (valid.len() - 1) as f32
    } else {
        0.0
    };
    let estimate = (0.75 * average + 0.25 * (average + trend).clamp(0.0, 100.0)).clamp(0.0, 100.0);
    let uncertainty = if valid.len() < 5 {
        20.0
    } else {
        10.0 / (valid.len() as f32).sqrt()
    };
    ScoreEstimate {
        estimate,
        low: (estimate - uncertainty).max(0.0),
        high: (estimate + uncertainty).min(100.0),
        samples: valid.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_accumulates_heat_by_tag_and_knowledge() {
        let mut heat = HeatMap::default();
        heat.add(ErrorTag::Article, "n1", 0.5);
        heat.add(ErrorTag::Article, "n1", 1.0);
        assert_eq!(heat.cells[&("article".into(), "n1".into())], 1.5);
    }

    #[test]
    fn test_empty_profile_snapshot_starts_without_activity() {
        let profile = ProfileSnapshot::empty(10, 20);
        assert_eq!((profile.from, profile.to, profile.attempts), (10, 20, 0));
        assert!(profile.heat.cells.is_empty());
    }

    #[test]
    fn test_estimate_score_wide_interval_until_five_runs() {
        let empty = estimate_score(&[], "cet4");
        assert_eq!(
            (empty.estimate, empty.low, empty.high, empty.samples),
            (50.0, 30.0, 70.0, 0)
        );
        let few = estimate_score(&[(60.0, 100.0), (80.0, 100.0)], "cet4");
        assert_eq!(few.samples, 2);
        assert_eq!((few.low, few.high), (55.0, 95.0));
        let enough = estimate_score(&[(80.0, 100.0); 5], "cet4");
        assert!(enough.high - enough.low < few.high - few.low);
    }
}
