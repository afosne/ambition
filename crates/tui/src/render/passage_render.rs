//! 语篇题型渲染：render_passage（七选五逐槽反馈 / 阅读逐子题反馈 / 完形·语法填空语篇）
//! 从 render.rs 拆出（docs/01 §组织：文件 ≤500 行；2026-09-27 选择状态可见性修订）

use super::*;

/// 语篇类题型块（非语篇 → 不输出）
pub(crate) fn render_passage(text: &mut String, state: &AppState, q: &Question) {
    match &q.kind {
        QuestionKind::SevenOfFive(body) => {
            // 语篇先行（缺口用 ［＿］ 标注）
            text.push_str(&format!("  {}\n\n", body.passage.replace("___", "［＿］")));
            // 逐槽进度：Space 记录当前缺口（sub_answers），光标跳回起点选下一缺口
            let filled = state.sub_answers.len();
            text.push_str(&format!(
                " 当前缺口 {}/{} · 待选\n",
                (filled + 1).min(body.answer.len()),
                body.answer.len()
            ));
            for (i, opt) in body.options.iter().enumerate() {
                let cursor = if state.sel == i && filled < body.answer.len() {
                    "❯"
                } else {
                    " "
                };
                let tick = if state.sub_answers.contains(&(i as u32)) {
                    "✓"
                } else {
                    " "
                };
                text.push_str(&format!("  {}{} {}. {}\n", cursor, tick, i + 1, opt));
            }
            if filled > 0 {
                let picked: Vec<String> = state
                    .sub_answers
                    .iter()
                    .enumerate()
                    .map(|(slot, id)| {
                        format!(
                            "{}→{}",
                            slot + 1,
                            body.options
                                .get(*id as usize)
                                .map(String::as_str)
                                .unwrap_or("?")
                        )
                    })
                    .collect();
                text.push_str(&format!(" 已选：{}\n", picked.join("  ")));
            }
        }
        QuestionKind::Reading(body) => {
            text.push_str(&format!("  {}\n\n", body.passage.replace("\n", "\n  ")));
            // 当前作答子题 = sub_answers.len()（顺序流）；已答 ✓ + 展示所选（2026-09-27 修订）
            let current = state.sub_answers.len();
            for (i, item) in body.items.iter().enumerate() {
                let answered = state.sub_answers.get(i).copied();
                let tick = if answered.is_some() { "✓" } else { " " };
                let mark = if i == current && current < body.items.len() {
                    "❯"
                } else {
                    " "
                };
                text.push_str(&format!("  {}{}{}. {}\n", tick, mark, i + 1, item.question));
                if let Some(choice_id) = answered
                    && let Some(opt) = item.options.iter().find(|o| o.id == choice_id)
                {
                    text.push_str(&format!("     ✓ {}. {}\n", opt.id + 1, opt.text));
                }
                for opt in &item.options {
                    text.push_str(&format!("     {}. {}\n", opt.id + 1, opt.text));
                }
            }
            if current < body.items.len() {
                text.push_str(&format!(
                    " 当前第 {}/{} 题 · ↑/↓ 选项 · Space 确认进下一题\n",
                    current + 1,
                    body.items.len()
                ));
            }
        }
        QuestionKind::Cloze(body) => {
            text.push_str(&format!("  {}\n", body.passage.replace("___", "＿＿＿")));
        }
        QuestionKind::GrammarFill(body) => {
            text.push_str(&format!("  {}\n", body.passage.replace("___", "＿＿＿")));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{
        BlankItem, ClozeBody, GrammarFillBody, OptionItem, Provenance, ReadingBody, ReadingItem,
        SevenOfFiveBody,
    };

    fn s75_question() -> Question {
        Question::new(
            "Seven.".into(),
            QuestionKind::SevenOfFive(SevenOfFiveBody {
                passage: "A ___ B ___ C".into(),
                options: vec!["o1".into(), "o2".into(), "o3".into()],
                answer: vec![0, 2],
            }),
            3,
            vec![],
            Provenance::Seed,
        )
    }

    fn reading_question() -> Question {
        Question::new(
            "Read.".into(),
            QuestionKind::Reading(ReadingBody {
                passage: "A passage.".into(),
                items: vec![ReadingItem {
                    question: "Q1?".into(),
                    options: vec![
                        OptionItem {
                            id: 0,
                            text: "a".into(),
                        },
                        OptionItem {
                            id: 1,
                            text: "b".into(),
                        },
                    ],
                    answer: vec![1],
                }],
            }),
            3,
            vec![],
            Provenance::Seed,
        )
    }

    #[test]
    fn test_render_passage_seven_of_five_shows_slot_progress_and_ticks() {
        let mut s = new_state(vec![s75_question()]);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("［＿］"), "缺口标注: {text}");
        assert!(text.contains("当前缺口 1/2"), "进度: {text}");
        assert!(!text.contains("已选："), "未选无已选行");
        // Space 记录第一槽 → 选项1 ✓ + 已选行
        update(&mut s, Msg::Select);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("✓ 1. o1"), "选中✓: {text}");
        assert!(text.contains("已选：1→o1"), "已选行: {text}");
        assert!(text.contains("当前缺口 2/2"), "进入第二槽: {text}");
    }

    #[test]
    fn test_render_passage_reading_marks_answered_item_with_choice() {
        let mut s = new_state(vec![reading_question()]);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("❯1. Q1?"), "当前子题标记: {text}");
        assert!(text.contains("当前第 1/1 题"), "进度提示: {text}");
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("✓ 1. Q1?"), "已答子题 ✓: {text}");
        assert!(
            text.contains("✓ 2. b"),
            "展示所选项（选项 id 1 显示为 2）: {text}"
        );
        assert!(!text.contains("当前第"), "全部答完无进度行: {text}");
    }

    #[test]
    fn test_render_passage_cloze_and_grammar_fill_show_passage() {
        let cloze = Question::new(
            "C.".into(),
            QuestionKind::Cloze(ClozeBody {
                passage: "I ___ a.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            2,
            vec![],
            Provenance::Seed,
        );
        let s = new_state(vec![cloze]);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("I ＿＿＿ a."), "完形语篇: {text}");
        let gf = Question::new(
            "G.".into(),
            QuestionKind::GrammarFill(GrammarFillBody {
                passage: "She ___ .".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["goes".into()],
                }],
            }),
            2,
            vec![],
            Provenance::Seed,
        );
        let s = new_state(vec![gf]);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.contains("She ＿＿＿ ."), "语法填空语篇: {text}");
        // 非语篇 → 无输出
        let s = new_state(vec![crate::sample_question()]);
        let mut text = String::new();
        render_passage(&mut text, &s, &s.questions[0]);
        assert!(text.is_empty(), "非语篇无输出");
    }
}
