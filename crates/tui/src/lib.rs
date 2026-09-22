//! Ambition TUI：壳层状态机（纯 update + 渲染，设计见 docs/03-engine-ui §3）
//! M2：填空输入行 · 判分错误独立展示（≠ Pending，P-8/改进 #6）· 提交计时（假对检测输入）
//! M3：待裁定 y/n 裁决回路 · 判分浮层错因标签（结构化讲解①）· 1-4 自评把握（改进 #10）

use std::time::Instant;

use ambition_core::{Answer, GradeResult, Question, QuestionKind, Timing};
use crossterm::event::KeyCode;
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
    /// 填空输入行（多空用 `;` 分隔）
    pub input: String,
    /// 判分/提交失败的独立错误行（P-8：错误 ≠ Pending）
    pub error: Option<String>,
    /// 本题起始时刻（提交计时 → Timing，假对检测输入）
    pub started_at: Instant,
    /// 修订次数（光标移动/输入删改，Timing.revision_count）
    pub revisions: u32,
    /// 自评把握 1-4（提交前键入 → Attempt.self_confidence，假对检测输入，改进 #10）
    pub self_confidence: Option<u8>,
}

/// 输入消息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    MoveDown,
    MoveUp,
    Submit,
    Next,
    Quit,
    /// 填空输入行追加字符
    Char(char),
    /// 填空输入行退格
    Backspace,
    /// 人工裁决（待裁定态：y=判对 / n=判错）
    Arbitrate(bool),
    /// 自评把握 1-4
    Confidence(u8),
}

/// 状态机输出（Quit/Submit 由 main 执行副作用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    None,
    Quit,
    Submit(Answer),
    /// 人工裁决（main 调 engine.resolve_pending）
    Arbitrate(bool),
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
        input: String::new(),
        error: None,
        started_at: Instant::now(),
        revisions: 0,
        self_confidence: None,
    }
}

impl AppState {
    /// 当前题是否处于填空输入态（有题、未判分、题型为填空）
    pub fn is_fill_input(&self) -> bool {
        !self.finished
            && self.grade.is_none()
            && matches!(self.questions[self.idx].kind, QuestionKind::FillBlank(_))
    }

    /// 采集本题计时（提交时调用；毫秒）
    pub fn timing(&self) -> Timing {
        Timing {
            total_ms: self.started_at.elapsed().as_millis() as u64,
            revision_count: self.revisions,
        }
    }
}

/// 键位 → 消息（填空输入态吞可打印字符；Esc 恒可退出，docs/03 §5.3）
pub fn key_to_msg(fill_mode: bool, code: KeyCode) -> Option<Msg> {
    match code {
        KeyCode::Esc => Some(Msg::Quit),
        KeyCode::Enter => Some(Msg::Submit),
        KeyCode::Backspace if fill_mode => Some(Msg::Backspace),
        KeyCode::Char(c) if fill_mode => Some(Msg::Char(c)),
        KeyCode::Char('q') => Some(Msg::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Msg::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Msg::MoveUp),
        // 待裁定裁决与自评把握（仅非输入态；docs/03 §5.3）
        KeyCode::Char('y') => Some(Msg::Arbitrate(true)),
        KeyCode::Char('n') => Some(Msg::Arbitrate(false)),
        KeyCode::Char(c) if ('1'..='4').contains(&c) => {
            Some(Msg::Confidence(c.to_digit(10).unwrap_or(1) as u8))
        }
        _ => None,
    }
}

/// 纯状态机：j/k 移动、Enter 提交/下一题、q/Esc 退出、填空输入
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
        Msg::Char(c) => {
            if state.is_fill_input() {
                state.input.push(c);
                state.revisions += 1;
            }
            Cmd::None
        }
        Msg::Backspace => {
            if state.is_fill_input() && state.input.pop().is_some() {
                state.revisions += 1;
            }
            Cmd::None
        }
        Msg::Arbitrate(accept) => {
            // 仅待裁定态响应裁决键；其余状态忽略（防误触）
            if state
                .grade
                .as_ref()
                .is_some_and(|g| g.verdict == ambition_core::Verdict::Pending)
            {
                return Cmd::Arbitrate(accept);
            }
            Cmd::None
        }
        Msg::Confidence(level) => {
            if state.grade.is_none() && !state.is_fill_input() {
                state.self_confidence = Some(level.clamp(1, 4));
            }
            Cmd::None
        }
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
            state.revisions += 1;
            Cmd::None
        }
        Msg::Submit => {
            if state.grade.is_some() {
                return Cmd::None;
            }
            state.error = None; // 重试前清掉上一次的错误行
            match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(_) | QuestionKind::MultiChoice(_) => {
                    Cmd::Submit(Answer::Choice(vec![state.sel as u32]))
                }
                QuestionKind::FillBlank(body) => {
                    // 多空输入：`;` 分隔（单空直接整串）
                    let answers: Vec<String> = state
                        .input
                        .split(';')
                        .map(|part| part.trim().to_string())
                        .collect();
                    if answers.len() != body.blanks.len() {
                        state.error = Some(format!(
                            "本题 {} 个空，请用 ; 分隔 {} 个答案",
                            body.blanks.len(),
                            body.blanks.len()
                        ));
                        return Cmd::None;
                    }
                    Cmd::Submit(Answer::Fill(
                        state
                            .input
                            .split(';')
                            .map(|p| p.trim().to_string())
                            .collect(),
                    ))
                }
            }
        }
        Msg::Next => {
            if state.grade.is_none() {
                return Cmd::None;
            }
            state.grade = None;
            state.sel = 0;
            state.input.clear();
            state.revisions = 0;
            state.self_confidence = None;
            state.started_at = Instant::now();
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

/// 写入独立错误行（main 在 submit 失败后调用；P-8：绝不降级成 Pending 假判分）
pub fn apply_error(state: &mut AppState, message: String) {
    state.error = Some(message);
}

/// 内置示例题（测试夹具；运行时题库来自 seed，docs/16-config-i18n §seed）
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
        ambition_core::Provenance::Seed,
    )
}

/// 填空题各空的可接受答案表（渲染正确答案用；非填空 → 空）
pub fn fill_answers(kind: &QuestionKind) -> Vec<Vec<String>> {
    match kind {
        QuestionKind::FillBlank(fill) => fill.blanks.iter().map(|b| b.acceptable.clone()).collect(),
        _ => Vec::new(),
    }
}

/// 渲染当前状态（含判分浮层 + 错误行 + 填空输入行）
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
        if matches!(q.kind, QuestionKind::FillBlank(_)) && state.grade.is_none() {
            text.push_str(&format!("\n  答案: {}▍\n", state.input));
            text.push_str("  多空用 ; 分隔 · Enter 提交 · Esc 退出\n");
        } else if state.grade.is_none() {
            text.push_str("\n j/k 移动 · Enter 提交/下一题 · 1-4 自评把握 · q 退出\n");
            if let Some(level) = state.self_confidence {
                text.push_str(&format!(" 自评把握: {level}/4\n"));
            }
        } else {
            text.push_str("\n j/k 移动 · Enter 提交/下一题 · q 退出\n");
        }
        // 独立错误行（红字，≠ 判分浮层；P-8）
        if let Some(err) = &state.error {
            text.push_str(&format!("\n ⚠ 提交失败（未判分）: {err}\n"));
        }
        if let Some(grade) = &state.grade {
            let icon = match grade.verdict {
                ambition_core::Verdict::Correct => "✔",
                ambition_core::Verdict::Wrong => "✘",
                ambition_core::Verdict::Pending => "?",
                ambition_core::Verdict::Partial => "≈",
                ambition_core::Verdict::Acceptable => "≈",
            };
            text.push_str(&format!("\n {} 得分 {}/{}", icon, grade.score, grade.max));
            if grade.graded_by == ambition_core::GradedBy::Human {
                text.push_str(" · 人工裁决");
            }
            // 结构化讲解①：错因标签（中文展示名）
            if !grade.error_tags.is_empty() {
                let labels: Vec<&str> = grade.error_tags.iter().map(|tag| tag.label_zh()).collect();
                text.push_str(&format!(
                    "\n 错因: {}（置信 {:.2}）",
                    labels.join("、"),
                    grade.confidence
                ));
            }
            if matches!(q.kind, QuestionKind::FillBlank(_)) {
                let sets = fill_answers(&q.kind);
                let answers: Vec<&str> = sets
                    .iter()
                    .map(|set| set.first().map(String::as_str).unwrap_or(""))
                    .collect();
                text.push_str(&format!("\n 正确答案: {}", answers.join(" ; ")));
            }
            if grade.verdict == ambition_core::Verdict::Pending {
                text.push_str("\n ⏳ 待裁定：y 判对 / n 判错（裁决回流规则层）\n");
            }
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
    use ambition_core::{BlankItem, FillBody, GradedBy, Provenance, Verdict};

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

    fn fill_question(blanks: usize) -> Question {
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

    #[test]
    fn test_new_state_initializes_position() {
        let s = new_state(vec![sample_question()]);
        assert_eq!(s.idx, 0);
        assert_eq!(s.sel, 0);
        assert!(s.grade.is_none());
        assert!(!s.finished);
        assert!(s.input.is_empty());
        assert!(s.error.is_none());
        assert_eq!(s.revisions, 0);
        assert!(new_state(vec![]).finished, "空题库直接结束");
        assert_eq!(s.screen, Screen::Practice);
    }

    #[test]
    fn test_is_fill_input_gates_char_input() {
        let choice = new_state(vec![sample_question()]);
        assert!(!choice.is_fill_input(), "选择题不吞字符");
        let mut fill = new_state(vec![fill_question(1)]);
        assert!(fill.is_fill_input(), "填空未判分 = 输入态");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert!(!fill.is_fill_input(), "判分后退出输入态");
        fill.finished = true;
        let mut c2 = choice.clone();
        c2.finished = true;
        assert!(!c2.is_fill_input(), "结束后非输入态");
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
        assert!(s.revisions >= 3, "移动计入修订次数");
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
    fn test_update_fill_input_builds_answer_and_counts_revisions() {
        let mut s = new_state(vec![fill_question(1)]);
        assert_eq!(update(&mut s, Msg::Char('a')), Cmd::None);
        assert_eq!(update(&mut s, Msg::Char('m')), Cmd::None);
        assert_eq!(s.input, "am");
        assert_eq!(s.revisions, 2, "逐字符计入修订");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string()])),
            "单空整串提交"
        );
        assert_eq!(update(&mut s, Msg::Backspace), Cmd::None, "判分后不删改");

        // 多空分号分隔
        let mut multi = new_state(vec![fill_question(2)]);
        for c in "am;by".chars() {
            update(&mut multi, Msg::Char(c));
        }
        assert_eq!(
            update(&mut multi, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string(), "by".to_string()]))
        );
    }

    #[test]
    fn test_update_fill_rejects_wrong_blank_count_with_error() {
        let mut s = new_state(vec![fill_question(2)]);
        for c in "onlyone".chars() {
            update(&mut s, Msg::Char(c));
        }
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None, "空数不匹配不提交");
        let err = s.error.clone().expect("错误行被设置");
        assert!(err.contains(";"), "错误提示分隔符: {err}");
        // 再次提交前错误清除
        update(&mut s, Msg::Char(';'));
        update(&mut s, Msg::Char('x'));
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["onlyone".to_string(), "x".to_string()]))
        );
        assert!(s.error.is_none(), "进入提交即清错误");
    }

    #[test]
    fn test_update_next_advances_and_finishes() {
        let mut s = new_state(vec![sample_question(), sample_question()]);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1, "进入第二题");
        assert!(s.grade.is_none() && s.sel == 0, "状态复位");
        assert!(s.input.is_empty(), "输入行复位");
        assert_eq!(s.revisions, 0, "修订计数复位");
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
    fn test_key_to_msg_fill_mode_swallows_printables() {
        use Msg::*;
        assert_eq!(key_to_msg(false, KeyCode::Char('q')), Some(Quit));
        assert_eq!(key_to_msg(false, KeyCode::Char('j')), Some(MoveDown));
        assert_eq!(key_to_msg(false, KeyCode::Char('k')), Some(MoveUp));
        assert_eq!(key_to_msg(false, KeyCode::Enter), Some(Submit));
        assert_eq!(
            key_to_msg(false, KeyCode::Char('a')),
            None,
            "选择态字符无效"
        );
        assert_eq!(key_to_msg(false, KeyCode::Backspace), None);
        // 填空输入态：可打印字符进输入行，q 也输入（Esc 才退出）
        assert_eq!(key_to_msg(true, KeyCode::Char('a')), Some(Char('a')));
        assert_eq!(key_to_msg(true, KeyCode::Char('q')), Some(Char('q')));
        assert_eq!(key_to_msg(true, KeyCode::Backspace), Some(Backspace));
        assert_eq!(key_to_msg(true, KeyCode::Enter), Some(Submit));
        assert_eq!(key_to_msg(true, KeyCode::Esc), Some(Quit), "Esc 恒退出");
        assert_eq!(key_to_msg(true, KeyCode::Down), Some(MoveDown));
        assert_eq!(key_to_msg(true, KeyCode::Tab), None);
    }

    #[test]
    fn test_apply_error_keeps_grade_none_and_is_overwritten() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none() && s.error.is_none());
        apply_error(&mut s, "store: disk full".into());
        assert!(s.grade.is_none(), "错误绝不落成 Pending 判分（P-8）");
        assert_eq!(s.error.as_deref(), Some("store: disk full"));
        // 成功路径不写错误
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Correct));
        assert!(s.error.is_some(), "错误行独立存活直到下一次提交清除");
        apply_error(&mut s, "second failure".into());
        assert_eq!(s.error.as_deref(), Some("second failure"), "后写覆盖");
    }

    #[test]
    fn test_timing_collects_elapsed_and_revisions() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::MoveUp);
        let t = s.timing();
        assert!(t.total_ms < 60_000, "耗时合理: {}", t.total_ms);
        assert_eq!(t.revision_count, 2, "修订次数来自移动/输入");
        // 复位后归零
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Next);
        let t2 = s.timing();
        assert_eq!(t2.revision_count, 0, "下一题重新计数");
        assert!(t2.total_ms <= t.total_ms, "计时重置");
    }

    #[test]
    fn test_sample_question_is_single_choice_with_stable_qid() {
        let q = sample_question();
        assert!(matches!(q.kind, QuestionKind::SingleChoice(_)));
        assert_eq!(q.qid, sample_question().qid, "同内容同 qid");
        assert!(q.tags.contains(&"article".to_string()));
        assert_eq!(q.provenance, Provenance::Seed);
    }

    #[test]
    fn test_apply_grade_sets_and_overrides_grade() {
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
        assert!(s.error.is_none(), "成功路径不写错误行");
    }

    #[test]
    fn test_fill_answers_exposes_acceptable_sets() {
        let single = fill_answers(&fill_question(1).kind);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0], vec!["am".to_string(), "by".to_string()]);
        let multi = fill_answers(&fill_question(2).kind);
        assert_eq!(multi.len(), 2, "两空两组答案");
        assert!(
            fill_answers(&sample_question().kind).is_empty(),
            "非填空为空"
        );
    }

    #[test]
    fn test_render_draws_stem_options_and_grade() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        // 每个状态用全新终端渲染；按显示宽度跳过宽字符续格（ratatui 续格不落笔）
        fn render_to(s: &AppState) -> String {
            use unicode_width::UnicodeWidthStr;

            let mut terminal = Terminal::new(TestBackend::new(72, 18)).expect("terminal");
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

    #[test]
    fn test_render_shows_error_line_and_fill_input_separately() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        fn render_to(s: &AppState) -> String {
            use unicode_width::UnicodeWidthStr;
            let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
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

        // 错误行独立可见，且没有判分浮层
        let mut s = new_state(vec![sample_question()]);
        apply_error(&mut s, "rule layer failed".into());
        let buf = render_to(&s);
        assert!(buf.contains("提交失败"), "错误行可见: {buf:?}");
        assert!(buf.contains("rule layer failed"), "错误内容可见");
        assert!(!buf.contains("得分"), "错误态无判分浮层");

        // 填空输入行
        let mut f = new_state(vec![fill_question(1)]);
        update(&mut f, Msg::Char('a'));
        update(&mut f, Msg::Char('m'));
        let fbuf = render_to(&f);
        assert!(fbuf.contains("答案:"), "输入行标签可见");
        assert!(fbuf.contains("am"), "输入内容可见");
    }

    #[test]
    fn test_key_to_msg_arbitration_and_confidence_keys() {
        use Msg::*;
        assert_eq!(
            key_to_msg(false, KeyCode::Char('y')),
            Some(Arbitrate(true)),
            "y = 判对"
        );
        assert_eq!(
            key_to_msg(false, KeyCode::Char('n')),
            Some(Arbitrate(false)),
            "n = 判错"
        );
        for level in 1..=4 {
            assert_eq!(
                key_to_msg(false, KeyCode::Char(char::from(b'0' + level))),
                Some(Confidence(level)),
                "{level} 档自评"
            );
        }
        assert_eq!(key_to_msg(false, KeyCode::Char('5')), None, "5 档不存在");
        // 填空输入态：y/n/数字仍是输入字符（裁决/自评只在选择态）
        assert_eq!(key_to_msg(true, KeyCode::Char('y')), Some(Char('y')));
        assert_eq!(key_to_msg(true, KeyCode::Char('1')), Some(Char('1')));
    }

    #[test]
    fn test_update_arbitrate_only_fires_on_pending_grade() {
        let mut s = new_state(vec![sample_question()]);
        // 未判分：裁决键无效
        assert_eq!(update(&mut s, Msg::Arbitrate(true)), Cmd::None);
        // 非 Pending 判分：忽略
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            update(&mut s, Msg::Arbitrate(false)),
            Cmd::None,
            "已定判不裁决"
        );
        // Pending：转出裁决命令
        apply_grade(&mut s, grade(Verdict::Pending));
        assert_eq!(update(&mut s, Msg::Arbitrate(true)), Cmd::Arbitrate(true));
        assert_eq!(update(&mut s, Msg::Arbitrate(false)), Cmd::Arbitrate(false));
    }

    #[test]
    fn test_update_confidence_records_pre_submit_and_resets_on_next() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::Confidence(3)), Cmd::None);
        assert_eq!(
            s.self_confidence,
            Some(3),
            "自评落状态（→ Attempt.self_confidence）"
        );
        update(&mut s, Msg::Confidence(9));
        assert_eq!(s.self_confidence, Some(4), "越界钳到 4");
        update(&mut s, Msg::Confidence(0));
        assert_eq!(s.self_confidence, Some(1), "越界钳到 1");
        // 判分后不再改（防误触）
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Confidence(2));
        assert_eq!(s.self_confidence, Some(1), "判分后锁定");
        // 下一题重置
        update(&mut s, Msg::Next);
        assert_eq!(s.self_confidence, None, "新题重新自评");
        // 填空输入态不吞自评（键位层已路由为字符，状态机双保险）
        let mut fill = new_state(vec![fill_question(1)]);
        update(&mut fill, Msg::Confidence(2));
        assert_eq!(fill.self_confidence, None, "填空输入态不自评");
    }

    #[test]
    fn test_render_shows_pending_arbitration_and_error_tags() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        fn render_to(s: &AppState) -> String {
            use unicode_width::UnicodeWidthStr;
            let mut terminal = Terminal::new(TestBackend::new(76, 24)).expect("terminal");
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
        // 自评提示与档位
        let hint = render_to(&s);
        assert!(hint.contains("自评把握"), "自评入口可见: {hint:?}");
        update(&mut s, Msg::Confidence(3));
        let hinted = render_to(&s);
        assert!(hinted.contains("3/4"), "档位可见: {hinted:?}");
        // 待裁定：y/n 裁决提示 + 错因标签 + 人工裁决标记
        let mut pending = grade(Verdict::Pending);
        pending.error_tags = vec![ambition_core::ErrorTag::Collocation];
        pending.confidence = 0.42;
        apply_grade(&mut s, pending);
        let buf = render_to(&s);
        assert!(buf.contains("待裁定"), "待裁定行可见: {buf:?}");
        assert!(buf.contains("y 判对"), "裁决键提示: {buf:?}");
        assert!(buf.contains("错因: 搭配"), "错因中文标签: {buf:?}");
        assert!(buf.contains("0.42"), "结构化讲解含概率: {buf:?}");
        // 人工裁决标记
        let mut human = grade(Verdict::Correct);
        human.graded_by = ambition_core::GradedBy::Human;
        apply_grade(&mut s, human);
        let human_buf = render_to(&s);
        assert!(human_buf.contains("人工裁决"), "裁决后可见: {human_buf:?}");
        assert!(!human_buf.contains("待裁定"), "已裁决不显示待裁定行");
    }
}
