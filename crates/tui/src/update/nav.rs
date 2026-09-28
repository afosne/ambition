//! 题导航辅助（从 update 拆出，docs/01 §组织：文件 ≤500 行）：状态复位 + 前进共用（Next/Skip/Prev/Jump）
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

use crate::*;

/// 复位当前题作答状态（Next/Skip/Prev/Jump 共用；含错误行清理）
pub(crate) fn reset_question_state(state: &mut AppState) {
    state.grade = None;
    state.sel = 0;
    state.revisions = 0;
    state.self_confidence = None;
    state.editing = false;
    state.checked.clear();
    state.sub_answers.clear();
    state.srs_hint = None;
    state.arbitration_sel = 0;
    state.error = None;
    state.comments = None;
    state.submitted_text = None;
    state.started_at = Instant::now();
    // 按当前题重建空位输入（选择题 = 0 位）
    state.inputs = vec![String::new(); blank_count(&state.questions[state.idx].kind)];
    state.blank_sel = 0;
}

/// 前进一题并复位（Next/Skip 共用）；越界 = 完成屏
pub(crate) fn advance_question(state: &mut AppState) {
    if state.idx + 1 >= state.questions.len() {
        state.finished = true;
        reset_question_state(state);
        return;
    }
    state.idx += 1;
    reset_question_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    // 配对测试（同文件强制配对；从 update/mod.rs 随函数迁入，P-16）

    #[test]
    fn test_reset_question_state_clears_every_answer_field() {
        let mut s = new_state(vec![crate::sample_question()]);
        s.sel = 1;
        s.revisions = 5;
        s.self_confidence = Some(2);
        s.editing = true;
        s.checked = vec![1];
        s.sub_answers = vec![0];
        s.srs_hint = Some("x".into());
        s.arbitration_sel = 1;
        s.error = Some("boom".into());
        reset_question_state(&mut s);
        assert_eq!(
            (
                s.grade.is_none(),
                s.sel,
                s.revisions,
                s.self_confidence,
                s.editing,
                s.checked.is_empty(),
                s.sub_answers.is_empty(),
                s.srs_hint.is_none(),
                s.arbitration_sel,
                s.error.is_none(),
            ),
            (true, 0, 0, None, false, true, true, true, 0, true),
            "全部复位"
        );
    }

    #[test]
    fn test_advance_question_reaches_finished_at_tail() {
        let mut s = new_state(vec![crate::sample_question()]);
        advance_question(&mut s);
        assert!(s.finished, "末题前进 = 完成屏");
        assert_eq!(s.idx, 0, "完成屏不越界");
    }
}
