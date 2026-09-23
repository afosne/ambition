//! 渲染：render（题干/选项/判分浮层/待裁定行/错误行，ratatui Frame）

use super::*;

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
