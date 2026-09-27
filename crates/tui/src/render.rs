//! 渲染：render（题干/选项/判分浮层/待裁定行/错误行，ratatui Frame）

use super::*;

/// 渲染当前状态（含判分浮层 + 错误行 + 填空输入行）
pub fn render(frame: &mut Frame, state: &AppState) {
    let mut text = String::from("Ambition · 拾英 —— 单机模式（本地题库）\n\n");
    if state.finished {
        text.push_str("本组完成！按 Ctrl+Q 退出。\n");
    } else {
        let q = &state.questions[state.idx];
        text.push_str(&format!(
            "[{}/{}] {}\n\n",
            state.idx + 1,
            state.questions.len(),
            q.stem
        ));
        // 选择类题型（单/多选 + 七选五缺口候选；M4 题型补全）
        match &q.kind {
            QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
                for (i, opt) in body.options.iter().enumerate() {
                    let cursor = if state.sel == i { "❯" } else { " " };
                    let tick = if state.checked.contains(&(i as u32)) {
                        "✓"
                    } else {
                        " "
                    };
                    text.push_str(&format!("  {}{} {}. {}\n", cursor, tick, i + 1, opt.text));
                }
            }
            QuestionKind::SevenOfFive(body) => {
                // 语篇先行（缺口用 ___ 标注）
                text.push_str(&format!("  {}\n\n", body.passage.replace("___", "［＿］")));
                for (i, opt) in body.options.iter().enumerate() {
                    let cursor = if state.sel == i { "❯" } else { " " };
                    let tick = if state.checked.contains(&(i as u32)) {
                        "✓"
                    } else {
                        " "
                    };
                    text.push_str(&format!("  {}{} {}. {}\n", cursor, tick, i + 1, opt));
                }
            }
            QuestionKind::Reading(body) => {
                text.push_str(&format!("  {}\n\n", body.passage.replace("\n", "\n  ")));
                for (i, item) in body.items.iter().enumerate() {
                    // 已作答子题标 ✓（checked 按子题下标；M4 题型补全）
                    let tick = if state.checked.contains(&(i as u32)) {
                        "✓"
                    } else {
                        " "
                    };
                    text.push_str(&format!("  {}{}. {}\n", tick, i + 1, item.question));
                    for opt in &item.options {
                        text.push_str(&format!("     {}. {}\n", opt.id + 1, opt.text));
                    }
                }
            }
            QuestionKind::Cloze(body) => {
                text.push_str(&format!("  {}\n", body.passage.replace("___", "＿＿＿")));
            }
            QuestionKind::GrammarFill(body) => {
                text.push_str(&format!("  {}\n", body.passage.replace("___", "＿＿＿")));
            }
            QuestionKind::FillBlank(_) => {}
        }
        if q.kind.is_fill() && state.grade.is_none() {
            // 每空一位独立输入行；❯+▍ 标当前编辑位（单空不编号）
            for (i, field) in state.inputs.iter().enumerate() {
                let cursor = if state.blank_sel == i { "❯" } else { " " };
                let label = if state.inputs.len() == 1 {
                    "答案".to_string()
                } else {
                    format!("答案 {}", i + 1)
                };
                let caret = if state.blank_sel == i { "▍" } else { " " };
                text.push_str(&format!("\n  {} {}: {}{}\n", cursor, label, field, caret));
            }
        }
        // 键位提示：与路由同源（keys.rs::hint，禁止硬编码）
        text.push_str(&format!("\n{}\n", hint(state.key_ctx())));
        if state.grade.is_none()
            && !q.kind.is_fill()
            && let Some(level) = state.self_confidence
        {
            text.push_str(&format!(" 自评把握: {level}/4\n"));
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
            if q.kind.is_fill() {
                let sets = fill_answers(&q.kind);
                let answers: Vec<&str> = sets
                    .iter()
                    .map(|set| set.first().map(String::as_str).unwrap_or(""))
                    .collect();
                text.push_str(&format!("\n 正确答案: {}", answers.join(" ; ")));
            }
            if grade.verdict == ambition_core::Verdict::Pending {
                // 裁决高亮（arbitration_sel 0=判对 1=判错）+ 键位取统一键位表口径
                let (a, b) = if state.arbitration_sel == 0 {
                    ("[❯判对]", "[ 判错]")
                } else {
                    ("[ 判对]", "[❯判错]")
                };
                text.push_str(&format!(
                    "\n ⏳ 待裁定：{a} {b} · ←/→ 选择 · Enter 确认 · Esc 跳过（裁决回流规则层）\n"
                ));
            }
        }
    }
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false })
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

    /// 多选题夹具（测试内 helper）
    fn multi_question() -> Question {
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
        assert!(fbuf.contains("退出编辑"), "提示行取 hint(ctx)：编辑态");
        update(&mut f, Msg::StopEdit);
        let bbuf = render_to(&f);
        assert!(bbuf.contains("打字恢复编辑"), "提示随 ctx 变化: {bbuf:?}");
        // 多空：每空一位独立行 + 编号
        let mut m = new_state(vec![fill_question(2)]);
        update(&mut m, Msg::Char('a'));
        update(&mut m, Msg::MoveDown);
        update(&mut m, Msg::Char('b'));
        let mbuf = render_to(&m);
        assert!(mbuf.contains("答案 1"), "多空编号 1: {mbuf:?}");
        assert!(mbuf.contains("答案 2"), "多空编号 2: {mbuf:?}");
    }

    #[test]
    fn test_render_marks_checked_option_for_multi_select() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

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

        let mut s = new_state(vec![multi_question()]);
        update(&mut s, Msg::Select);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select);
        let buf = render_to(&s);
        assert!(buf.contains("✓"), "选中标记可见: {buf:?}");
        assert_eq!(s.checked, vec![0, 1], "多选两枚");
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
        // 待裁定：箭头选择高亮 + 键位提示 + 错因标签 + 人工裁决标记
        let mut pending = grade(Verdict::Pending);
        pending.error_tags = vec![ambition_core::ErrorTag::Collocation];
        pending.confidence = 0.42;
        apply_grade(&mut s, pending);
        let buf = render_to(&s);
        assert!(buf.contains("待裁定"), "待裁定行可见: {buf:?}");
        assert!(buf.contains("❯判对"), "默认高亮判对: {buf:?}");
        assert!(buf.contains("Enter 确认"), "裁决键提示: {buf:?}");
        update(&mut s, Msg::MoveDown);
        let flipped = render_to(&s);
        assert!(flipped.contains("❯判错"), "箭头切换高亮: {flipped:?}");
        assert!(flipped.contains("错因: 搭配"), "错因中文标签: {flipped:?}");
        assert!(flipped.contains("0.42"), "结构化讲解含概率: {flipped:?}");
        // 人工裁决标记
        let mut human = grade(Verdict::Correct);
        human.graded_by = ambition_core::GradedBy::Human;
        apply_grade(&mut s, human);
        let human_buf = render_to(&s);
        assert!(human_buf.contains("人工裁决"), "裁决后可见: {human_buf:?}");
        assert!(!human_buf.contains("待裁定"), "已裁决不显示待裁定行");
    }

    #[test]
    fn test_render_draws_passage_kinds() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use unicode_width::UnicodeWidthStr;

        fn render_to(s: &AppState) -> String {
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

        // 完形：语篇 + 逐空输入行（M4 题型补全）
        let cloze = Question::new(
            "Cloze.".into(),
            QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "I ___ a student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            2,
            vec!["cloze".into()],
            Provenance::Seed,
        );
        let mut s = new_state(vec![cloze]);
        let buf = render_to(&s);
        assert!(buf.contains("I ＿＿＿ a student."), "语篇可见: {buf:?}");
        assert!(buf.contains("答案"), "填空输入行: {buf:?}");
        update(&mut s, Msg::Char('a'));
        let typed = render_to(&s);
        assert!(typed.contains("a"), "输入可见: {typed:?}");
        // 七选五：语篇缺口标注 + 候选项
        let s75 = Question::new(
            "Seven.".into(),
            QuestionKind::SevenOfFive(ambition_core::SevenOfFiveBody {
                passage: "A ___ B".into(),
                options: vec!["opt one".into(), "opt two".into()],
                answer: vec![0],
            }),
            3,
            vec!["reading".into()],
            Provenance::Seed,
        );
        let s = new_state(vec![s75]);
        let buf = render_to(&s);
        assert!(buf.contains("［＿］"), "缺口标注: {buf:?}");
        assert!(buf.contains("opt one"), "候选句可见: {buf:?}");
        // 阅读：语篇 + 子题
        let reading = Question::new(
            "Read.".into(),
            QuestionKind::Reading(ambition_core::ReadingBody {
                passage: "A passage about cats.".into(),
                items: vec![ambition_core::ReadingItem {
                    question: "What do cats eat?".into(),
                    options: vec![
                        ambition_core::OptionItem {
                            id: 0,
                            text: "fish".into(),
                        },
                        ambition_core::OptionItem {
                            id: 1,
                            text: "rock".into(),
                        },
                    ],
                    answer: vec![0],
                }],
            }),
            3,
            vec!["reading".into()],
            Provenance::Seed,
        );
        let s = new_state(vec![reading]);
        let buf = render_to(&s);
        assert!(buf.contains("cats"), "语篇可见: {buf:?}");
        assert!(buf.contains("What do cats eat?"), "子题可见: {buf:?}");
        assert!(buf.contains("fish"), "子题选项可见: {buf:?}");
    }
}
