//! 选题面板渲染：render_picker（F1 全屏列表：序号 + 题型 + 题干摘要，docs/03 §5.3 2026-09-27）
//! 从 render.rs 拆出（docs/01 §组织：文件 ≤500 行）

use super::*;

/// 题型短标签（面板列表用）
fn kind_label(kind: &QuestionKind) -> &'static str {
    match kind {
        QuestionKind::SingleChoice(_) => "单选",
        QuestionKind::MultiChoice(_) => "多选",
        QuestionKind::FillBlank(_) => "填空",
        QuestionKind::Cloze(_) => "完形",
        QuestionKind::Reading(_) => "阅读",
        QuestionKind::SevenOfFive(_) => "七选五",
        QuestionKind::GrammarFill(_) => "语法填空",
    }
}

/// 渲染选题面板（全屏列表；光标窗 ±8 行 + 首尾省略）
pub fn render_picker(frame: &mut Frame, state: &AppState) {
    let mut text = String::from(" 选题面板（↑/↓ 选择 · Enter 跳转 · Esc/F1 返回）\n\n");
    let total = state.questions.len();
    if total == 0 {
        text.push_str(" 题库为空。\n");
    } else {
        let cursor = state.picker_cursor.min(total - 1);
        // 视窗：光标居中 ±8，超出加省略行
        let start = cursor.saturating_sub(8);
        let end = (cursor + 9).min(total);
        if start > 0 {
            text.push_str(&format!("   …（前 {} 题省略）\n", start));
        }
        for i in start..end {
            let q = &state.questions[i];
            let mark = if i == cursor { "❯" } else { " " };
            let here = if i == state.idx { " ←当前" } else { "" };
            let stem: String = q.stem.chars().take(36).collect::<String>();
            let stem = stem.replace('\n', " ");
            text.push_str(&format!(
                " {} {:>3}. [{:^4}] {}{}\n",
                mark,
                i + 1,
                kind_label(&q.kind),
                stem,
                here
            ));
        }
        if end < total {
            text.push_str(&format!("   …（后 {} 题省略）\n", total - end));
        }
        text.push_str(&format!(
            "\n 共 {} 题 · 第 {}/{} 个\n",
            total,
            cursor + 1,
            total
        ));
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

    fn render_to(s: &AppState) -> String {
        // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
        crate::common::render_sized(s, 76, 24)
    }

    #[test]
    fn test_kind_label_covers_all_seven_kinds() {
        let choice = ambition_core::ChoiceBody {
            options: vec![],
            answer: vec![],
        };
        assert_eq!(
            kind_label(&QuestionKind::SingleChoice(choice.clone())),
            "单选"
        );
        assert_eq!(kind_label(&QuestionKind::MultiChoice(choice)), "多选");
        assert_eq!(
            kind_label(&QuestionKind::FillBlank(ambition_core::FillBody {
                sentence: "s".into(),
                blanks: vec![]
            })),
            "填空"
        );
        assert_eq!(
            kind_label(&QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "p".into(),
                blanks: vec![]
            })),
            "完形"
        );
        assert_eq!(
            kind_label(&QuestionKind::Reading(ambition_core::ReadingBody {
                passage: "p".into(),
                items: vec![]
            })),
            "阅读"
        );
        assert_eq!(
            kind_label(&QuestionKind::SevenOfFive(ambition_core::SevenOfFiveBody {
                passage: "p".into(),
                options: vec![],
                answer: vec![]
            })),
            "七选五"
        );
        assert_eq!(
            kind_label(&QuestionKind::GrammarFill(ambition_core::GrammarFillBody {
                passage: "p".into(),
                blanks: vec![]
            })),
            "语法填空"
        );
    }

    #[test]
    fn test_render_picker_lists_questions_with_cursor_and_current() {
        let q1 = crate::sample_question();
        let q2 = Question::new(
            "Another stem for the picker.".into(),
            q1.kind.clone(),
            q1.difficulty,
            q1.tags.clone(),
            q1.provenance,
        );
        let mut s = new_state(vec![q1, q2]);
        s.picker_open = true;
        s.picker_cursor = 1;
        let buf = render_to(&s);
        assert!(buf.contains("选题面板"), "标题: {buf:?}");
        assert!(buf.contains("单选"), "题型标签: {buf:?}");
        assert!(buf.contains("Another stem"), "列表题干: {buf:?}");
        assert!(buf.contains("←当前"), "当前题标记: {buf:?}");
        assert!(buf.contains("❯"), "光标: {buf:?}");
        assert!(buf.contains("共 2 题"), "计数: {buf:?}");
        // 空题库
        let mut empty = new_state(vec![]);
        empty.picker_open = true;
        let buf = render_to(&empty);
        assert!(buf.contains("题库为空"), "空态: {buf:?}");
    }
}
