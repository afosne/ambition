//! 模考屏回填（M5-W6，docs/03 S3 mock_start/mock_finish）：开卷换题复位 + 收卷记分
use super::nav::reset_question_state;
use crate::*;

/// 开卷回填（Cmd::MockStart 的 main 回填口：换题 + 全量复位 + 记 run 元数据）
pub fn apply_mock_start(state: &mut AppState, session: ambition_engine::MockSession) {
    state.questions = session.questions;
    state.idx = 0;
    state.finished = false;
    state.screen = Screen::Mock;
    state.mock = Some(MockRun {
        run_id: session.run_id,
        title: session.title,
        deadline: session.deadline,
    });
    state.mock_result = None;
    reset_question_state(state);
}

/// 收卷回填（分数由 engine 换算：最新一答口径 × 卷面总分，M5-W6）
pub fn apply_mock_finish(state: &mut AppState, score: f32, max: f32) {
    state.mock_result = Some((score, max));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> ambition_engine::MockSession {
        ambition_engine::MockSession {
            run_id: "run-1".to_string(),
            paper_id: "p-1".to_string(),
            title: "CET4 全真模考".to_string(),
            deadline: 1_000_000,
            questions: vec![crate::sample_question()],
        }
    }

    #[test]
    fn test_apply_mock_start_swaps_questions_and_records_run() {
        let mut state = new_state(vec![]);
        state.finished = true;
        apply_mock_start(&mut state, session());
        assert_eq!(state.screen, Screen::Mock, "切模考屏");
        assert!(!state.finished, "新卷未完成");
        assert_eq!(state.questions.len(), 1, "换入卷题");
        assert_eq!(state.idx, 0);
        let mock = state.mock.clone().expect("run 元数据");
        assert_eq!(mock.run_id, "run-1");
        assert_eq!(mock.deadline, 1_000_000);
        assert!(state.mock_result.is_none(), "开卷无得分");
        assert!(
            state.grade.is_none() && state.inputs.iter().all(String::is_empty),
            "作答态已复位"
        );
    }

    #[test]
    fn test_apply_mock_finish_records_score() {
        let mut state = new_state(vec![]);
        apply_mock_start(&mut state, session());
        apply_mock_finish(&mut state, 34.0, 40.0);
        assert_eq!(state.mock_result, Some((34.0, 40.0)), "得分回填");
        assert!(state.mock.is_some(), "run 元数据保留（完成页标题）");
    }
}
