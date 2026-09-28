//! SRS 旁路/复习门面测试（从 srs.rs 拆出，docs/01 §组织：文件 ≤500 行；P-16 测试随被测物走）

#[cfg(test)]
mod tests {
    use crate::srs::*;
    use crate::*;
    use ambition_core::{
        Answer, Attempt, AttemptMode, BlankItem, CardSource, ChoiceBody, ErrorTag, FillBody,
        GradeResult, OptionItem, Provenance, Question, QuestionKind, Timing, Verdict, now_unix,
    };
    use ambition_srs::{Card, CardAction, Rating};

    fn choice_question(tag: &str) -> Question {
        Question::new(
            "She is quick.".into(),
            QuestionKind::SingleChoice(ChoiceBody {
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
            }),
            2,
            vec![tag.to_string()],
            Provenance::Seed,
        )
    }

    fn attempt(qid: ambition_core::Qid, pick: u32, id: &str) -> Attempt {
        Attempt {
            attempt_id: id.into(),
            qid,
            answer: Answer::Choice(vec![pick]),
            timing: Some(Timing {
                total_ms: 10_000,
                revision_count: 1,
            }),
            self_confidence: Some(3),
            mode: AttemptMode::Practice,
        }
    }

    #[test]
    fn test_apply_card_action_create_bump_schedule_and_skip() {
        let source = CardSource::Question {
            qid: ambition_core::Qid("q".into()),
        };
        assert!(apply_card_action(None, CardAction::Skip, source.clone(), 1).is_none());
        let created = apply_card_action(None, CardAction::Create, source.clone(), 100)
            .expect("新建")
            .clone();
        assert_eq!(created.due_at, 100, "新卡立即到期");
        assert_eq!(created.wrong_count, 1);
        let mut existing = created.clone();
        existing.wrong_count = 3;
        let bumped =
            apply_card_action(Some(existing), CardAction::Bump, source.clone(), 200).expect("提权");
        assert_eq!(bumped.wrong_count, 4, "Bump 累加");
        assert_eq!(bumped.due_at, 200, "提权到当前时刻");
        let scheduled = apply_card_action(
            Some(bumped),
            CardAction::Schedule(Rating::Good),
            source,
            100_000,
        )
        .expect("调度");
        assert!(
            scheduled.due_at > 200,
            "Good 调度推后到期（实际 {}）",
            scheduled.due_at
        );
        assert!(scheduled.fsrs.is_some(), "FSRS 状态写入");
    }

    #[tokio::test]
    async fn test_srs_after_attempt_creates_card_on_wrong_and_schedules_on_correct() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question("vocab");
        engine.add_question(&q).await.expect("注入");
        let wrong = attempt(q.qid.clone(), 1, "srs-1");
        let grade = GradeResult {
            verdict: Verdict::Wrong,
            score: 0.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: ambition_core::GradedBy::Rule,
            items: vec![],
            error_tags: vec![ErrorTag::WordSense],
        };
        let action = engine.srs_after_attempt(&wrong, &grade, false).await;
        assert_eq!(action, Some(CardAction::Create), "错题建卡");
        let card_id =
            ambition_core::CardId::from_source(&CardSource::Question { qid: q.qid.clone() });
        let card = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(card.wrong_count, 1);
        assert_eq!(card.error_tags, vec![ErrorTag::WordSense], "错因入卡");
        // 同题再错 → Bump（提权）
        let wrong2 = attempt(q.qid.clone(), 1, "srs-2");
        let action = engine.srs_after_attempt(&wrong2, &grade, false).await;
        assert_eq!(action, Some(CardAction::Bump));
        let card = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(card.wrong_count, 2);
        // 答对且非假对 + 已有卡 → Schedule(Good)
        let ok = attempt(q.qid.clone(), 0, "srs-3");
        let ok_grade = GradeResult {
            verdict: Verdict::Correct,
            ..grade.clone()
        };
        let action = engine.srs_after_attempt(&ok, &ok_grade, false).await;
        assert_eq!(action, Some(CardAction::Schedule(Rating::Good)));
        // 答对且无卡、非假对 → Skip（不入卡）
        let q2 = choice_question("other-tag");
        engine.add_question(&q2).await.expect("注入");
        let fresh = attempt(q2.qid.clone(), 0, "srs-4");
        let action = engine.srs_after_attempt(&fresh, &ok_grade, false).await;
        assert_eq!(action, None, "无卡答对不入卡");
        // Pending → 不动
        let pending_grade = GradeResult {
            verdict: Verdict::Pending,
            ..ok_grade.clone()
        };
        let action = engine.srs_after_attempt(&fresh, &pending_grade, true).await;
        assert_eq!(action, None, "Pending 不入卡（裁决后补执行）");
    }

    #[tokio::test]
    async fn test_due_list_review_answer_and_wrong_wordbook() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question("grammar");
        engine.add_question(&q).await.expect("注入");
        // 错题入卡 → 到期
        let wrong = attempt(q.qid.clone(), 1, "dl-1");
        let grade = GradeResult {
            verdict: Verdict::Wrong,
            score: 0.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: ambition_core::GradedBy::Rule,
            items: vec![],
            error_tags: vec![ErrorTag::Tense],
        };
        engine.srs_after_attempt(&wrong, &grade, false).await;
        let due = engine.due_list(now_unix() + 1, 10).await.expect("队列");
        // 错词卡（wrong_count 提权）排在装配的内置卡之前（docs/08 §1 排序）
        assert!(!due.is_empty(), "至少错词卡到期");
        assert_eq!(due[0].front, "She is quick.", "front=题干（错词提权序）");
        assert_eq!(due[0].back, "quick", "back=正确答案");
        assert_eq!(due[0].group, "grammar", "group=首标签");
        // 未到期为空（unix 0 早于一切卡）
        let empty = engine.due_list(0, 10).await.expect("更早");
        assert!(empty.is_empty(), "未到期为空");
        // 复习评分 → FSRS 调度 + ReviewDone 落 game_state（Xp(1)）
        let card_id = due[0].card.card_id.0.clone();
        let before = engine.game_state().xp;
        let card = engine
            .review_answer(&card_id, Rating::Good)
            .await
            .expect("评分");
        assert!(card.fsrs.is_some(), "FSRS 状态写入");
        assert!(card.due_at > now_unix(), "Good 推后到期（{}）", card.due_at);
        assert_eq!(engine.game_state().xp, before + 1, "ReviewDone → Xp(1)");
        // 错词本（内置卡 wrong_count=0 不进）
        let rows = engine.wrong_wordbook().await.expect("错词本");
        assert_eq!(rows.len(), 1, "仅错词卡入册");
        assert_eq!(rows[0].wrong_count, 1);
        assert_eq!(rows[0].error_tags, vec![ErrorTag::Tense]);
        assert_eq!(rows[0].title, "She is quick.", "题干展示");
        // 不存在的卡 → NotFound
        assert!(matches!(
            engine.review_answer("card:missing", Rating::Good).await,
            Err(EngineError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_is_guess_attempt_uses_baseline_window() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question("vocab");
        engine.add_question(&q).await.expect("注入");
        // 空历史：默认基线 15s/60s → 10s 中速 + conf3 不是假对
        let normal = attempt(q.qid.clone(), 0, "g-1");
        assert!(
            !engine.is_guess_attempt(&normal, None).await,
            "空历史中速非假对"
        );
        // conf=1 → 秒判假对（三形态之一）
        let mut quick = normal.clone();
        quick.self_confidence = Some(1);
        assert!(engine.is_guess_attempt(&quick, None).await, "自评1=假对");
        // 无计时不判
        let mut no_timing = normal.clone();
        no_timing.timing = None;
        assert!(!engine.is_guess_attempt(&no_timing, None).await);
    }

    #[test]
    fn test_question_back_covers_all_kinds() {
        use ambition_core::{GrammarFillBody, ReadingBody, ReadingItem, SevenOfFiveBody};
        assert_eq!(
            question_back(&QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "a".into()
                    },
                    OptionItem {
                        id: 1,
                        text: "b".into()
                    },
                ],
                answer: vec![1],
            })),
            "b"
        );
        assert_eq!(
            question_back(&QuestionKind::FillBlank(FillBody {
                sentence: "s".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into(), "'m".into()]
                }],
            })),
            "am",
            "取首选可接受"
        );
        assert_eq!(
            question_back(&QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "p".into(),
                blanks: vec![
                    BlankItem {
                        acceptable: vec!["goes".into()]
                    },
                    BlankItem {
                        acceptable: vec!["takes".into()]
                    },
                ],
            })),
            "goes ; takes"
        );
        assert_eq!(
            question_back(&QuestionKind::GrammarFill(GrammarFillBody {
                passage: "p".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["was".into()]
                }],
            })),
            "was"
        );
        assert_eq!(
            question_back(&QuestionKind::SevenOfFive(SevenOfFiveBody {
                passage: "p".into(),
                options: vec!["o1".into(), "o2".into()],
                answer: vec![1, 0],
            })),
            "o2 o1"
        );
        assert_eq!(
            question_back(&QuestionKind::Reading(ReadingBody {
                passage: "p".into(),
                items: vec![ReadingItem {
                    question: "q".into(),
                    options: vec![
                        OptionItem {
                            id: 0,
                            text: "x".into()
                        },
                        OptionItem {
                            id: 1,
                            text: "y".into()
                        },
                    ],
                    answer: vec![1],
                }],
            })),
            "y"
        );
    }

    #[tokio::test]
    async fn test_review_item_covers_word_sense_and_pattern_sources() {
        let engine = Engine::open_memory().await.expect("打开");
        // 义项卡：正面=词条，背面=该义项释义
        let sense_card = Card::new(
            CardSource::WordSense {
                word_id: "bank".into(),
                sense_id: 1,
            },
            1,
        );
        let item = engine.review_item(sense_card).await.expect("义项条目");
        assert_eq!(item.front, "bank");
        assert!(item.back.contains("银行"), "中文释义: {}", item.back);
        assert_eq!(item.group, "word");
        // 未知义项 id：退词条名不 panic
        let ghost = Card::new(
            CardSource::WordSense {
                word_id: "bank".into(),
                sense_id: 999,
            },
            1,
        );
        let item = engine.review_item(ghost).await.expect("义项条目");
        assert_eq!(item.back, "bank", "未知义项退词条名");
        // 句式卡：正面=句式 id，背面=中文讲解 + form
        let pattern_card = Card::new(
            CardSource::Pattern {
                pattern_id: "not_only_but_also".into(),
            },
            1,
        );
        let item = engine.review_item(pattern_card).await.expect("句式条目");
        assert_eq!(item.front, "not_only_but_also");
        assert!(item.back.contains("不但"), "句式讲解: {}", item.back);
        assert!(item.back.contains("not only"), "句式形式: {}", item.back);
        assert_eq!(item.group, "pattern");
        // 未知句式 id：退源键
        let unknown = Card::new(
            CardSource::Pattern {
                pattern_id: "ghost".into(),
            },
            1,
        );
        let item = engine.review_item(unknown).await.expect("句式条目");
        assert_eq!(item.back, "ghost", "未知句式退源键");
    }
}
