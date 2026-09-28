//! tui 跨文件测试夹具（收口前 state/render/update 三份逐字复制 + 7 处宽度感知渲染收集，
//! 2026-09-27 复用合并）

use ambition_core::{
    BlankItem, FillBody, GradeResult, GradedBy, Provenance, Question, QuestionKind, Verdict,
};

use crate::AppState;

/// 判分夹具（按 verdict 给分：Correct 满分、其余 0；状态机与渲染断言共用）
pub(crate) fn grade(verdict: Verdict) -> GradeResult {
    GradeResult {
        verdict,
        score: if verdict == Verdict::Correct {
            1.0
        } else {
            0.0
        },
        max: 1.0,
        confidence: 1.0,
        graded_by: GradedBy::Rule,
        items: vec![],
        error_tags: vec![],
    }
}

/// 填空题夹具（blanks 个空位；渲染与状态机共用）
pub(crate) fn fill_question(blanks: usize) -> Question {
    let sentence = if blanks == 1 {
        "I ___ a student.".to_string()
    } else {
        "She ___ to school ___ bus.".to_string()
    };
    Question::new(
        sentence.clone(),
        QuestionKind::FillBlank(FillBody {
            sentence,
            blanks: (0..blanks)
                .map(|_| BlankItem {
                    acceptable: vec!["am".into(), "by".into()],
                })
                .collect(),
        }),
        1,
        vec!["grammar".into()],
        Provenance::Seed,
    )
}

/// 多选题夹具（三选二；渲染与状态机共用）
pub(crate) fn multi_question() -> Question {
    Question::new(
        "Pick two.".into(),
        QuestionKind::MultiChoice(ambition_core::ChoiceBody {
            options: (0..3)
                .map(|i| ambition_core::OptionItem {
                    id: i,
                    text: format!("opt{i}"),
                })
                .collect(),
            answer: vec![0, 2],
        }),
        2,
        vec![],
        Provenance::Seed,
    )
}

/// 宽度感知渲染收集（P-2：每状态全新 TestBackend、按 unicode-width 跳过宽字符续格——
/// 原 7 处逐字复制收口到此；各测试按自己的面板尺寸调用）
pub(crate) fn render_sized(s: &AppState, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use unicode_width::UnicodeWidthStr;

    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|f| crate::render(f, s)).expect("draw");
    let cells = &terminal.backend().buffer().content;
    let mut out = String::new();
    let mut i = 0;
    while i < cells.len() {
        let sym = cells[i].symbol();
        out.push_str(sym);
        i += 1 + usize::from(sym.width() == 2);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_fixture_scores_correct_full_else_zero() {
        assert_eq!(grade(Verdict::Correct).score, 1.0, "答对满分");
        assert_eq!(grade(Verdict::Wrong).score, 0.0, "答错零分");
        assert_eq!(grade(Verdict::Correct).graded_by, GradedBy::Rule);
    }

    #[test]
    fn test_fill_question_fixture_builds_requested_blank_count() {
        let one = fill_question(1);
        let QuestionKind::FillBlank(fill) = &one.kind else {
            panic!("填空夹具必须是 FillBlank");
        };
        assert_eq!(fill.blanks.len(), 1, "单空");
        let two = fill_question(2);
        let QuestionKind::FillBlank(fill) = &two.kind else {
            panic!("填空夹具必须是 FillBlank");
        };
        assert_eq!(fill.blanks.len(), 2, "双空句式不同");
    }

    #[test]
    fn test_multi_question_fixture_answers_two_of_three_options() {
        let q = multi_question();
        let QuestionKind::MultiChoice(choice) = &q.kind else {
            panic!("多选夹具必须是 MultiChoice");
        };
        assert_eq!(choice.answer, vec![0, 2]);
        assert_eq!(choice.options.len(), 3);
    }

    #[test]
    fn test_render_sized_collects_width_aware_non_empty_buffer() {
        let s = crate::new_state(vec![crate::sample_question()]);
        let buf = render_sized(&s, 72, 18);
        assert!(!buf.is_empty(), "渲染缓冲可收集");
        assert!(buf.contains("university"), "题干可见");
    }
}
