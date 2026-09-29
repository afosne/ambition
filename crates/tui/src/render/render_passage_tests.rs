//! render 语篇题型显示测试（从 render.rs 拆出，docs/01 §组织：文件 ≤500 行）

#[cfg(test)]
mod tests {
    use crate::*;
    use ambition_core::{BlankItem, Provenance, Question, QuestionKind};

    #[test]
    fn test_render_draws_passage_kinds() {
        fn render_to(s: &mut AppState) -> String {
            // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
            crate::common::render_sized(s, 76, 24)
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
        let buf = render_to(&mut s);
        assert!(buf.contains("I ＿＿＿ a student."), "语篇可见: {buf:?}");
        assert!(buf.contains("答案"), "填空输入行: {buf:?}");
        update(&mut s, Msg::Char('a'));
        let typed = render_to(&mut s);
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
        let mut s = new_state(vec![s75]);
        let buf = render_to(&mut s);
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
        let mut s = new_state(vec![reading]);
        let buf = render_to(&mut s);
        assert!(buf.contains("cats"), "语篇可见: {buf:?}");
        assert!(buf.contains("What do cats eat?"), "子题可见: {buf:?}");
        assert!(buf.contains("fish"), "子题选项可见: {buf:?}");
    }
}
