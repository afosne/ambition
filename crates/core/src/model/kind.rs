//! 题型判定与子题拆解：QuestionKind 辅助（is_choice/is_fill/is_composite/sub_kinds/sub_answers/blank_count/option_count）
//! M4 题型补全（DESIGN §13.4）：复合题拆子题递归判分的契约实现（docs/07 §1 M4 注记）

use crate::question::{ChoiceBody, FillBody, OptionItem, QuestionKind};
use crate::*;

impl QuestionKind {
    /// 是否选择类（单/多选；子题拆解后 Reading/SevenOfFive 拆出的选择子题也归此类）
    pub fn is_choice(&self) -> bool {
        matches!(self, Self::SingleChoice(_) | Self::MultiChoice(_))
    }

    /// 是否填空类（FillBlank/Cloze/GrammarFill；子题拆解后归此类）
    pub fn is_fill(&self) -> bool {
        matches!(
            self,
            Self::FillBlank(_) | Self::Cloze(_) | Self::GrammarFill(_)
        )
    }

    /// 是否复合题型（需拆子题递归判分；docs/07 §1 M4 注记）
    pub fn is_composite(&self) -> bool {
        matches!(
            self,
            Self::Cloze(_) | Self::Reading(_) | Self::SevenOfFive(_) | Self::GrammarFill(_)
        )
    }

    /// 复合题拆子题（非复合题返回空；子题 qid = 父 qid + 子下标，不走内容哈希防碰撞）
    /// 返回 Vec<(子题序号标签, 子题 Question)>；子题 Question 的 qid 由调用方（grader）按父 qid+下标拼接。
    pub fn sub_kinds(&self) -> Vec<(&'static str, QuestionKind)> {
        match self {
            Self::Cloze(body) => body
                .blanks
                .iter()
                .enumerate()
                .map(|(i, blank)| {
                    (
                        "blank",
                        QuestionKind::FillBlank(FillBody {
                            sentence: format!("第 {} 空", i + 1),
                            blanks: vec![blank.clone()],
                        }),
                    )
                })
                .collect(),
            Self::GrammarFill(body) => body
                .blanks
                .iter()
                .enumerate()
                .map(|(i, blank)| {
                    (
                        "blank",
                        QuestionKind::FillBlank(FillBody {
                            sentence: format!("第 {} 空", i + 1),
                            blanks: vec![blank.clone()],
                        }),
                    )
                })
                .collect(),
            Self::Reading(body) => body
                .items
                .iter()
                .map(|item| {
                    (
                        "item",
                        QuestionKind::SingleChoice(ChoiceBody {
                            options: item.options.clone(),
                            answer: item.answer.clone(),
                        }),
                    )
                })
                .collect(),
            Self::SevenOfFive(body) => {
                let option_items: Vec<OptionItem> = body
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, text)| OptionItem {
                        id: i as u32,
                        text: text.clone(),
                    })
                    .collect();
                body.answer
                    .iter()
                    .map(|&ans| {
                        (
                            "gap",
                            QuestionKind::SingleChoice(ChoiceBody {
                                options: option_items.clone(),
                                answer: vec![ans],
                            }),
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// 复合题拆作答（index 对齐 sub_kinds；非复合题返回空）
    pub fn sub_answers(&self, answer: &Answer) -> Vec<Answer> {
        match (self, answer) {
            (Self::Cloze(_) | Self::GrammarFill(_), Answer::Fill(values)) => values
                .iter()
                .map(|v| Answer::Fill(vec![v.clone()]))
                .collect(),
            (Self::Reading(_), Answer::Group(subs)) => subs.clone(),
            (Self::SevenOfFive(_), Answer::Choice(ids)) => {
                ids.iter().map(|id| Answer::Choice(vec![*id])).collect()
            }
            _ => Vec::new(),
        }
    }

    /// 空位数（填空类 + 完形/语法填空；选择类 0——TUI inputs 长度据此重建）
    pub fn blank_count(&self) -> usize {
        match self {
            Self::FillBlank(body) => body.blanks.len(),
            Self::Cloze(body) => body.blanks.len(),
            Self::GrammarFill(body) => body.blanks.len(),
            _ => 0,
        }
    }

    /// 选项数（选择类；填空类 0——TUI 光标移动据此）
    pub fn option_count(&self) -> usize {
        match self {
            Self::SingleChoice(body) | Self::MultiChoice(body) => body.options.len(),
            Self::SevenOfFive(body) => body.options.len(),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::question::{BlankItem, ChoiceBody, FillBody, OptionItem, ReadingBody, ReadingItem};

    /// 共享夹具（P-16：跨模块 helper 非 test_ 前缀——但钩子按 test_ 前缀识别测试，fixture 用 test_ 前缀在本文件亦可）
    fn test_choice_body() -> ChoiceBody {
        ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "quick".into(),
                },
                OptionItem {
                    id: 1,
                    text: "slowly".into(),
                },
            ],
            answer: vec![0],
        }
    }

    #[test]
    fn test_is_choice_matches_single_and_multi_only() {
        assert!(QuestionKind::SingleChoice(test_choice_body()).is_choice());
        assert!(QuestionKind::MultiChoice(test_choice_body()).is_choice());
        assert!(
            !QuestionKind::FillBlank(FillBody {
                sentence: "x".into(),
                blanks: vec![],
            })
            .is_choice()
        );
        assert!(
            !QuestionKind::Cloze(ClozeBody {
                passage: "p".into(),
                blanks: vec![],
            })
            .is_choice()
        );
    }

    #[test]
    fn test_is_fill_covers_fill_family() {
        assert!(
            QuestionKind::FillBlank(FillBody {
                sentence: "x".into(),
                blanks: vec![],
            })
            .is_fill()
        );
        assert!(
            QuestionKind::Cloze(ClozeBody {
                passage: "p".into(),
                blanks: vec![],
            })
            .is_fill()
        );
        assert!(
            QuestionKind::GrammarFill(GrammarFillBody {
                passage: "p".into(),
                blanks: vec![],
            })
            .is_fill()
        );
        assert!(!QuestionKind::SingleChoice(test_choice_body()).is_fill());
    }

    #[test]
    fn test_is_composite_marks_composite_kinds() {
        assert!(
            QuestionKind::Cloze(ClozeBody {
                passage: "p".into(),
                blanks: vec![],
            })
            .is_composite()
        );
        assert!(
            QuestionKind::Reading(ReadingBody {
                passage: "p".into(),
                items: vec![],
            })
            .is_composite()
        );
        assert!(
            QuestionKind::SevenOfFive(SevenOfFiveBody {
                passage: "p".into(),
                options: vec![],
                answer: vec![],
            })
            .is_composite()
        );
        assert!(
            QuestionKind::GrammarFill(GrammarFillBody {
                passage: "p".into(),
                blanks: vec![],
            })
            .is_composite()
        );
        assert!(!QuestionKind::SingleChoice(test_choice_body()).is_composite());
        assert!(
            !QuestionKind::FillBlank(FillBody {
                sentence: "x".into(),
                blanks: vec![],
            })
            .is_composite()
        );
    }

    #[test]
    fn test_option_count_reads_choice_and_seven_of_five() {
        assert_eq!(
            QuestionKind::SingleChoice(test_choice_body()).option_count(),
            2
        );
        let s75 = QuestionKind::SevenOfFive(SevenOfFiveBody {
            passage: "p".into(),
            options: vec!["a".into(), "b".into()],
            answer: vec![0],
        });
        assert_eq!(s75.option_count(), 2);
        assert_eq!(
            QuestionKind::FillBlank(FillBody {
                sentence: "x".into(),
                blanks: vec![],
            })
            .option_count(),
            0
        );
    }
    #[test]
    fn test_blank_count_and_option_count_by_kind() {
        let fill = QuestionKind::FillBlank(FillBody {
            sentence: "s".into(),
            blanks: vec![
                BlankItem {
                    acceptable: vec!["a".into()],
                },
                BlankItem {
                    acceptable: vec!["b".into()],
                },
            ],
        });
        assert_eq!(fill.blank_count(), 2);
        assert_eq!(fill.option_count(), 0);
        let cloze = QuestionKind::Cloze(ClozeBody {
            passage: "p".into(),
            blanks: vec![BlankItem {
                acceptable: vec!["c".into()],
            }],
        });
        assert_eq!(cloze.blank_count(), 1);
        let gf = QuestionKind::GrammarFill(GrammarFillBody {
            passage: "p".into(),
            blanks: vec![
                BlankItem {
                    acceptable: vec!["x".into()],
                },
                BlankItem {
                    acceptable: vec!["y".into()],
                },
                BlankItem {
                    acceptable: vec!["z".into()],
                },
            ],
        });
        assert_eq!(gf.blank_count(), 3);
        let s75 = QuestionKind::SevenOfFive(SevenOfFiveBody {
            passage: "p".into(),
            options: vec!["o1".into(), "o2".into()],
            answer: vec![0],
        });
        assert_eq!(s75.option_count(), 2);
        assert_eq!(s75.blank_count(), 0);
        assert_eq!(
            QuestionKind::SingleChoice(test_choice_body()).option_count(),
            2
        );
    }
    #[test]
    fn test_sub_kinds_cloze_and_grammar_fill_split_blanks() {
        let blank = BlankItem {
            acceptable: vec!["am".into()],
        };
        let cloze = QuestionKind::Cloze(ClozeBody {
            passage: "I ___ a student.".into(),
            blanks: vec![blank.clone(), blank.clone()],
        });
        let subs = cloze.sub_kinds();
        assert_eq!(subs.len(), 2, "两空拆两子题");
        for (label, kind) in &subs {
            assert_eq!(*label, "blank");
            assert!(kind.is_fill(), "子题为填空");
            assert_eq!(kind.blank_count(), 1, "每子题单空");
        }
        let gf = QuestionKind::GrammarFill(GrammarFillBody {
            passage: "p".into(),
            blanks: vec![blank],
        });
        assert_eq!(gf.sub_kinds().len(), 1);
    }
    #[test]
    fn test_sub_kinds_reading_and_seven_of_five_split_items() {
        let item = ReadingItem {
            question: "What?".into(),
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
        };
        let reading = QuestionKind::Reading(ReadingBody {
            passage: "p".into(),
            items: vec![item.clone(), item],
        });
        let subs = reading.sub_kinds();
        assert_eq!(subs.len(), 2, "两子题拆两");
        for (label, kind) in &subs {
            assert_eq!(*label, "item");
            assert!(kind.is_choice());
            assert_eq!(kind.option_count(), 2);
        }
        let s75 = QuestionKind::SevenOfFive(SevenOfFiveBody {
            passage: "p".into(),
            options: vec!["o1".into(), "o2".into(), "o3".into()],
            answer: vec![0, 2],
        });
        let subs = s75.sub_kinds();
        assert_eq!(subs.len(), 2, "两缺口拆两子题");
        assert!(subs.iter().all(|(_, k)| k.is_choice()));
        assert_eq!(subs[0].1.option_count(), 3, "每子题 3 候选");
    }
    #[test]
    fn test_sub_answers_aligns_with_sub_kinds() {
        let blank = BlankItem {
            acceptable: vec!["is".into()],
        };
        let cloze = QuestionKind::Cloze(ClozeBody {
            passage: "p".into(),
            blanks: vec![blank.clone(), blank],
        });
        let ans = Answer::Fill(vec!["is".into(), "was".into()]);
        let subs = cloze.sub_answers(&ans);
        assert_eq!(subs.len(), 2, "两空拆两子答");
        assert_eq!(subs[0], Answer::Fill(vec!["is".into()]));
        assert_eq!(subs[1], Answer::Fill(vec!["was".into()]));
        let s75 = QuestionKind::SevenOfFive(SevenOfFiveBody {
            passage: "p".into(),
            options: vec!["a".into(), "b".into()],
            answer: vec![0, 1],
        });
        let subs = s75.sub_answers(&Answer::Choice(vec![1, 0]));
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], Answer::Choice(vec![1]));
        assert_eq!(subs[1], Answer::Choice(vec![0]));
        let reading = QuestionKind::Reading(ReadingBody {
            passage: "p".into(),
            items: vec![],
        });
        let group = Answer::Group(vec![Answer::Choice(vec![0]), Answer::Choice(vec![1])]);
        let subs = reading.sub_answers(&group);
        assert_eq!(subs.len(), 2, "Group 直接拆");
        // 非复合题返回空
        assert!(
            QuestionKind::SingleChoice(test_choice_body())
                .sub_answers(&Answer::Choice(vec![0]))
                .is_empty()
        );
        // 形态不匹配返回空（防御）
        assert!(cloze.sub_answers(&Answer::Choice(vec![0])).is_empty());
    }
}
