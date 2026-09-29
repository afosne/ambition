//! 语篇题型作答测试（从 update_tests.rs 拆出，docs/01 §组织：文件 ≤500 行）

#[cfg(test)]
mod tests {
    use crate::update::*;
    use crate::*;
    use ambition_core::{Answer, BlankItem, Provenance, Question, QuestionKind};

    #[test]
    fn test_update_submits_passage_kind_answers() {
        use ambition_core::{
            ClozeBody, GrammarFillBody, OptionItem, ReadingBody, ReadingItem, SevenOfFiveBody,
        };
        // 完形：逐空输入 → Answer::Fill（M4 题型补全）
        let cloze = Question::new(
            "Cloze.".into(),
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
        let mut s = new_state(vec![cloze]);
        assert_eq!(s.inputs.len(), 1, "完形一位");
        assert!(s.is_fill_pending(), "完形是填空待答");
        update(&mut s, Msg::Char('a'));
        update(&mut s, Msg::Char('m'));
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string()])),
            "完形提交逐空"
        );
        // 语法填空同路
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
        assert_eq!(s.inputs.len(), 1);
        // 七选五：Space 选中 → 逐缺口 Choice
        let s75 = Question::new(
            "S.".into(),
            QuestionKind::SevenOfFive(SevenOfFiveBody {
                passage: "A ___ B ___ C".into(),
                options: vec!["o1".into(), "o2".into(), "o3".into()],
                answer: vec![0, 2],
            }),
            3,
            vec![],
            Provenance::Seed,
        );
        let mut s = new_state(vec![s75]);
        assert_eq!(s.inputs.len(), 0, "七选五无输入位");
        update(&mut s, Msg::Select); // 缺口1 = 光标位 0
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select); // 缺口2 = 选项2
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![0, 2])),
            "逐缺口提交"
        );
        // 阅读：逐子题选中 → Answer::Group
        let reading = Question::new(
            "R.".into(),
            QuestionKind::Reading(ReadingBody {
                passage: "p".into(),
                items: vec![
                    ReadingItem {
                        question: "q1".into(),
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
                    },
                    ReadingItem {
                        question: "q2".into(),
                        options: vec![
                            OptionItem {
                                id: 0,
                                text: "x".into(),
                            },
                            OptionItem {
                                id: 1,
                                text: "y".into(),
                            },
                        ],
                        answer: vec![0],
                    },
                ],
            }),
            3,
            vec![],
            Provenance::Seed,
        );
        let mut s = new_state(vec![reading]);
        assert_eq!(s.inputs.len(), 0, "阅读无输入位");
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select); // 子题1 选 b（sel=1）
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Group(vec![
                Answer::Choice(vec![1]),
                Answer::Choice(vec![0])
            ])),
            "未选子题回退光标位/已选子题按选中集"
        );
    }

    #[test]
    fn test_update_screen_switch_and_review_flow() {
        // Ctrl+A/Ctrl+R / Tab 切屏：进 Review 触发 ReviewLoad
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(s.screen, Screen::Practice);
        assert_eq!(
            update(&mut s, Msg::Screen(Screen::Review)),
            Cmd::ReviewLoad,
            "进复习屏 → 加载队列"
        );
        assert_eq!(s.screen, Screen::Review);
        assert_eq!(
            update(&mut s, Msg::Screen(Screen::Review)),
            Cmd::ReviewLoad,
            "同屏切换触发重载（#25）"
        );
        // 复习屏：未翻面评分忽略（防误触）
        assert_eq!(update(&mut s, Msg::Rate(3)), Cmd::None, "未翻面不评分");
        // 翻面 → 评分需有卡
        update(&mut s, Msg::Flip);
        assert!(s.review.revealed, "翻面生效");
        assert_eq!(update(&mut s, Msg::Rate(3)), Cmd::None, "无卡评分忽略");
        // 载卡后评分 → ReviewRate
        apply_review_cards(
            &mut s,
            vec![crate::ReviewCard {
                card_id: "q:1".into(),
                front: "f".into(),
                back: "b".into(),
                group: "g".into(),
            }],
        );
        assert_eq!(update(&mut s, Msg::Rate(3)), Cmd::None, "翻面被复位后忽略");
        update(&mut s, Msg::Flip);
        assert_eq!(
            update(&mut s, Msg::Rate(3)),
            Cmd::ReviewRate(3),
            "翻面后评分"
        );
        // 返回答题屏
        assert_eq!(
            update(&mut s, Msg::Screen(Screen::Practice)),
            Cmd::None,
            "回 Practice 无需加载"
        );
        assert_eq!(s.screen, Screen::Practice);
        // finished 后仍可切屏
        let mut done = new_state(vec![sample_question()]);
        done.finished = true;
        assert_eq!(
            update(&mut done, Msg::Screen(Screen::Review)),
            Cmd::ReviewLoad
        );
        // Practice 屏收 Review 专属消息 = 忽略
        let mut p = new_state(vec![sample_question()]);
        assert_eq!(update(&mut p, Msg::Flip), Cmd::None);
        assert_eq!(update(&mut p, Msg::Rate(2)), Cmd::None);
    }
}
