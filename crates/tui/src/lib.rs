//! Ambition TUI：壳层状态机（纯 update + 渲染，设计见 docs/03-engine-ui §3）

use ambition_core::{Answer, GradeResult, Question, QuestionKind};
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// 屏幕（M1 只有答题流；完整 Screen 集见 docs/03-engine-ui）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Practice,
}

/// 应用状态
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub screen: Screen,
    pub questions: Vec<Question>,
    pub idx: usize,
    pub sel: usize,
    pub grade: Option<GradeResult>,
    pub finished: bool,
}

/// 输入消息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    MoveDown,
    MoveUp,
    Submit,
    Next,
    Quit,
}

/// 状态机输出（Quit/Submit 由 main 执行副作用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    None,
    Quit,
    Submit(Answer),
}

/// 构造初始状态（空题库直接 finished）
pub fn new_state(questions: Vec<Question>) -> AppState {
    let finished = questions.is_empty();
    AppState {
        screen: Screen::Practice,
        questions,
        idx: 0,
        sel: 0,
        grade: None,
        finished,
    }
}

/// 纯状态机：j/k 移动、Enter 提交/下一题、q 退出
pub fn update(state: &mut AppState, msg: Msg) -> Cmd {
    if state.finished {
        return if matches!(msg, Msg::Quit) {
            Cmd::Quit
        } else {
            Cmd::None
        };
    }
    match msg {
        Msg::Quit => Cmd::Quit,
        Msg::MoveDown | Msg::MoveUp => {
            if state.grade.is_some() {
                return Cmd::None;
            }
            let kind = &state.questions[state.idx].kind;
            let option_count = match kind {
                QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
                    body.options.len()
                }
                QuestionKind::FillBlank(_) => 0,
            };
            if option_count == 0 {
                return Cmd::None;
            }
            let delta = if matches!(msg, Msg::MoveDown) {
                1
            } else {
                option_count - 1
            };
            state.sel = (state.sel + delta) % option_count;
            Cmd::None
        }
        Msg::Submit => {
            if state.grade.is_some() {
                return Cmd::None;
            }
            match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(_) | QuestionKind::MultiChoice(_) => {
                    Cmd::Submit(Answer::Choice(vec![state.sel as u32]))
                }
                // TODO(M2): 填空输入行（占位形参届时一并加入，见 docs/01-engineering §占位参数规则）
                QuestionKind::FillBlank(_) => Cmd::None,
            }
        }
        Msg::Next => {
            if state.grade.is_none() {
                return Cmd::None;
            }
            state.grade = None;
            state.sel = 0;
            if state.idx + 1 >= state.questions.len() {
                state.finished = true;
            } else {
                state.idx += 1;
            }
            Cmd::None
        }
    }
}

/// 写入判分结果（main 在 submit 成功后调用）
pub fn apply_grade(state: &mut AppState, grade: GradeResult) {
    state.grade = Some(grade);
}

/// 内置示例题（seed 雏形，docs/16-config-i18n §seed）
pub fn sample_question() -> Question {
    Question::new(
        "She is ___ university student.".into(),
        QuestionKind::SingleChoice(ambition_core::ChoiceBody {
            options: vec![
                ambition_core::OptionItem {
                    id: 0,
                    text: "a".into(),
                },
                ambition_core::OptionItem {
                    id: 1,
                    text: "an".into(),
                },
                ambition_core::OptionItem {
                    id: 2,
                    text: "the".into(),
                },
                ambition_core::OptionItem {
                    id: 3,
                    text: "two".into(),
                },
            ],
            answer: vec![1],
        }),
        1,
        vec!["article".into()],
    )
}

/// 渲染当前状态（含判分浮层）
pub fn render(frame: &mut Frame, state: &AppState) {
    let mut text = String::from("Ambition · 拾英 —— 单机模式（本地题库）\n\n");
    if state.finished {
        text.push_str("本组完成！按 q 退出。\n");
    } else {
        let q = &state.questions[state.idx];
        text.push_str(&format!(
            "[{}/{}] {}\n\n",
            state.idx + 1,
            state.questions.len(),
            q.stem
        ));
        if let QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) = &q.kind {
            for (i, opt) in body.options.iter().enumerate() {
                let mark = if state.sel == i { "❯" } else { " " };
                text.push_str(&format!("  {} {}. {}\n", mark, i + 1, opt.text));
            }
        }
        text.push_str("\n j/k 移动 · Enter 提交/下一题 · q 退出\n");
        if let Some(grade) = &state.grade {
            let icon = match grade.verdict {
                ambition_core::Verdict::Correct => "✔",
                ambition_core::Verdict::Wrong => "✘",
                _ => "?",
            };
            text.push_str(&format!("\n {} 得分 {}/{}", icon, grade.score, grade.max));
        }
    }
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .title("Ambition, 俺必胜！".bold())
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center),
        )
        .style(Style::default());
    frame.render_widget(Clear, frame.area());
    frame.render_widget(paragraph, frame.area());
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{GradedBy, Verdict};

    fn grade(verdict: Verdict) -> GradeResult {
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

    #[test]
    fn test_new_state_initializes_position() {
        let s = new_state(vec![sample_question()]);
        assert_eq!(s.idx, 0);
        assert_eq!(s.sel, 0);
        assert!(s.grade.is_none());
        assert!(!s.finished);
        assert!(new_state(vec![]).finished, "空题库直接结束");
        assert_eq!(s.screen, Screen::Practice);
    }

    #[test]
    fn test_update_moves_selection_with_wrap() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None);
        assert_eq!(s.sel, 0);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None, "向上环绕到末项");
        assert_eq!(s.sel, 3);
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.sel, 0, "向下环绕回首项");
    }

    #[test]
    fn test_update_submits_answer_and_ignores_after_grade() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1]))
        );
        apply_grade(&mut s, grade(Verdict::Correct));
        // 已判分：提交/移动无效
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1, "判分后选择不移动");
    }

    #[test]
    fn test_update_next_advances_and_finishes() {
        let mut s = new_state(vec![sample_question(), sample_question()]);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1, "进入第二题");
        assert!(s.grade.is_none() && s.sel == 0, "状态复位");
        // 未判分时 Next 无效
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Next);
        assert!(s.finished, "最后一题后结束");
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None, "结束后移动无效");
        assert_eq!(update(&mut s, Msg::Quit), Cmd::Quit);
        assert_eq!(update(&mut s, Msg::Next), Cmd::None, "结束后 Next 无效");
    }

    #[test]
    fn test_apply_grade_sets_grade() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none());
        apply_grade(&mut s, grade(Verdict::Wrong));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Wrong));
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            s.grade.as_ref().map(|g| g.verdict),
            Some(Verdict::Correct),
            "可覆盖"
        );
    }

    #[test]
    fn test_sample_question_is_single_choice_with_stable_qid() {
        let q = sample_question();
        assert!(matches!(q.kind, QuestionKind::SingleChoice(_)));
        assert_eq!(q.qid, sample_question().qid, "同内容同 qid");
        assert!(q.tags.contains(&"article".to_string()));
    }

    #[test]
    fn test_render_draws_stem_options_and_grade() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        // 每个状态用全新终端渲染；按显示宽度跳过宽字符续格（ratatui 续格不落笔）
        fn render_to(s: &AppState) -> String {
            use unicode_width::UnicodeWidthStr;

            let mut terminal = Terminal::new(TestBackend::new(64, 16)).expect("terminal");
            terminal.draw(|f| render(f, s)).expect("draw");
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

        let mut s = new_state(vec![sample_question()]);
        let plain = render_to(&s);
        assert!(plain.contains("university"), "题干可见");
        assert!(plain.contains("❯"), "选中光标可见");
        apply_grade(&mut s, grade(Verdict::Correct));
        let graded = render_to(&s);
        assert!(graded.contains("✔"), "判分浮层可见");
        s.finished = true;
        let final_buf = render_to(&s);
        assert!(final_buf.contains("完成"), "结束态可见: {final_buf:?}");
    }
}
