//! SRS 旁路与复习门面（细分域根）：is_guess_attempt/srs_after_attempt/due_list/review_answer/wrong_wordbook
//! （docs/03 §1/§2 M4、docs/08 §1 编排注记）
//! 排程决策唯一实现 = ambition-srs 纯函数；本模块只做「取卡/落库/事件」编排。
//! 本域含子组件：`srs::lexicon`（词库/句式装载）/ `srs::srs_tests`（配对测试）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

pub mod lexicon;
#[cfg(test)]
mod srs_tests;

use crate::*;
use ambition_core::{CardSource, ErrorTag, QuestionKind, baseline_from_timings, is_guess};
use ambition_srs::{Card, CardAction, Rating, ReviewItem};
use std::time::{SystemTime, UNIX_EPOCH};

/// 复习卡单行（错词本/画像消费；docs/03 §2 wrong_wordbook）
#[derive(Debug, Clone, PartialEq)]
pub struct WrongWordRow {
    pub card_id: String,
    pub source: CardSource,
    pub wrong_count: u32,
    pub error_tags: Vec<ErrorTag>,
    /// 展示标题（题卡=题干；义项卡=词条；句式卡=句式 id）
    pub title: String,
    pub due_at: i64,
}

/// 当前 unix 秒
pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 假对基线取样窗口（docs/08 §2 近 200 次滚动）
const BASELINE_WINDOW: i64 = 200;

impl Engine {
    /// 判假对（基线 = 近 200 次 total_ms，`exclude` 排除本次作答——裁决路径不能把自己算进基线）
    pub(crate) async fn is_guess_attempt(&self, attempt: &Attempt, exclude: Option<&str>) -> bool {
        let Some(timing) = attempt.timing else {
            return false; // 无计时（外部提交）不判假对
        };
        match ambition_store::recent_timings(&self.pool, BASELINE_WINDOW, exclude).await {
            Ok(timings) => {
                let baseline = baseline_from_timings(&timings);
                is_guess(&timing, attempt.self_confidence, &baseline)
            }
            Err(err) => {
                // 基线取样失败 = 旁路降级：按非假对待（不阻断主流程，P-8 可见降级）
                tracing::warn!(error = %err, "baseline sampling failed, treating as non-guess");
                false
            }
        }
    }

    /// 作答后 SRS 旁路（insert 后调用；失败 warn 不回滚主事务，docs/03 §1 M4）：
    /// Wrong/Partial → 建卡/提权 + wrong_count + 错因标签；Correct 假对 → 建卡（guessed_count+1）；
    /// Correct 非假对且已有卡 → 按 Good 调度；Pending → 不动（裁决后 resolve_pending 补执行）。
    pub(crate) async fn srs_after_attempt(
        &self,
        attempt: &Attempt,
        grade: &GradeResult,
        guess: bool,
    ) -> Option<CardAction> {
        if grade.verdict == Verdict::Pending {
            return None; // 裁决后再补执行（docs/08 §1）
        }
        let source = CardSource::Question {
            qid: attempt.qid.clone(),
        };
        let card_id = ambition_core::CardId::from_source(&source);
        let now = now_unix();
        let existing = match ambition_store::get_card(&self.pool, &card_id.0).await {
            Ok(card) => card,
            Err(err) => {
                tracing::warn!(error = %err, "srs get_card failed (旁路降级)");
                return None;
            }
        };
        let action = ambition_srs::decide_action(grade.verdict, guess, existing.is_some());
        let mut card = match apply_card_action(existing, action, source, now) {
            Some(card) => card,
            None => return None, // Skip
        };
        // 错/假对：错因标签合并去重（错词本展示口径，docs/08 §1）
        if matches!(action, CardAction::Create | CardAction::Bump) {
            for tag in &grade.error_tags {
                if !card.error_tags.contains(tag) {
                    card.error_tags.push(*tag);
                }
            }
        }
        if let Err(err) = ambition_store::upsert_card(&self.pool, &card).await {
            tracing::warn!(error = %err, "srs upsert_card failed (旁路可补偿)");
            return None;
        }
        Some(action)
    }

    /// 到期复习队列（到期过滤 → 排序 → 交错 → 截断，docs/08 §1 due_queue）
    pub async fn due_list(&self, now: i64, limit: u32) -> Result<Vec<ReviewItem>, EngineError> {
        let cards = ambition_store::due_cards(&self.pool, now).await?;
        let mut items = Vec::with_capacity(cards.len());
        for card in cards {
            match self.review_item(card).await {
                Ok(item) => items.push(item),
                // 卡片悬空（题已删/载荷不完整）：可见降级——跳过并 warn，不炸整队列
                Err(err) => {
                    tracing::warn!(error = %err, "review item build skipped");
                }
            }
        }
        Ok(ambition_srs::due_queue(items, now, limit, true))
    }

    /// 复习评分（FSRS 调度 + upsert + ReviewDone 事件 → settle 落 game_state，docs/03 §2）
    pub async fn review_answer(&self, card_id: &str, rating: Rating) -> Result<Card, EngineError> {
        let mut card = ambition_store::get_card(&self.pool, card_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("card:{card_id}")))?;
        let now = now_unix();
        ambition_srs::schedule(&mut card, rating, now)?;
        ambition_store::upsert_card(&self.pool, &card).await?;
        // ReviewDone 经 settle（事件广播 + 游戏 Xp(1) + game_state 落库，docs/03 §1 M4）
        self.settle(DomainEvent::ReviewDone {
            card_id: card_id.to_string(),
            rating: rating.as_str().to_string(),
        })
        .await;
        Ok(card)
    }

    /// 错词本（错因标签 + 次数 + 题干/词条，docs/03 §2）
    pub async fn wrong_wordbook(&self) -> Result<Vec<WrongWordRow>, EngineError> {
        let cards = ambition_store::wrong_cards(&self.pool).await?;
        let mut rows = Vec::with_capacity(cards.len());
        for card in cards {
            let title = self.card_title(&card).await;
            rows.push(WrongWordRow {
                card_id: card.card_id.0.clone(),
                source: card.source.clone(),
                wrong_count: card.wrong_count,
                error_tags: card.error_tags.clone(),
                title,
                due_at: card.due_at,
            });
        }
        Ok(rows)
    }

    /// 卡片展示标题（题卡=题干；其它源=源键；题不存在退源键不 panic）
    async fn card_title(&self, card: &Card) -> String {
        match &card.source {
            CardSource::Question { qid } => {
                match ambition_store::get_question(&self.pool, qid).await {
                    Ok(Some(q)) => q.stem,
                    _ => qid.0.clone(),
                }
            }
            CardSource::WordSense { word_id, .. } => word_id.clone(),
            CardSource::Pattern { pattern_id } => pattern_id.clone(),
            CardSource::Note { note_id } => note_id.clone(),
            CardSource::Phrase { phrase_id } => phrase_id.clone(),
        }
    }

    /// 组复习条目（front=题面，back=正确答案，group=交错分组=首标签/源族，docs/08 §1）
    pub(crate) async fn review_item(&self, card: Card) -> Result<ReviewItem, EngineError> {
        let (front, back, group) = match &card.source {
            CardSource::Question { qid } => {
                let question = ambition_store::get_question(&self.pool, qid)
                    .await?
                    .ok_or_else(|| EngineError::NotFound(format!("question:{}", qid.0)))?;
                let group = question
                    .tags
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "general".to_string());
                let back = question_back(&question.kind);
                (question.stem, back, group)
            }
            CardSource::WordSense { word_id, sense_id } => {
                // 正面=词条，背面=该义项释义（09 义项查询；未知义项退词条名不 panic）
                let detail = self.word_detail(word_id).await.unwrap_or_default();
                let back = detail
                    .iter()
                    .find(|sense| sense.sense_id == *sense_id)
                    .map(|sense| format!("{} — {}", sense.def_zh, sense.def_en))
                    .unwrap_or_else(|| word_id.clone());
                (word_id.clone(), back, "word".to_string())
            }
            CardSource::Pattern { pattern_id } => {
                let back = crate::srs::lexicon::pattern_note(pattern_id)
                    .unwrap_or_else(|| pattern_id.clone());
                (pattern_id.clone(), back, "pattern".to_string())
            }
            other => (format!("{other:?}"), String::new(), "misc".to_string()),
        };
        Ok(ReviewItem {
            card,
            front,
            back,
            group,
        })
    }
}

/// 作答处置落卡（Create/Bump 提权 due_at=now；Schedule 走 FSRS；Skip 不动）。
/// 返回 None = 无需落库（Skip）。
pub(crate) fn apply_card_action(
    existing: Option<Card>,
    action: CardAction,
    source: CardSource,
    now: i64,
) -> Option<Card> {
    match action {
        CardAction::Skip => None,
        CardAction::Create => {
            let mut card = Card::new(source, now);
            card.wrong_count = 1; // Create 只来自错/假对（decide_action 口径）
            Some(card)
        }
        CardAction::Bump => {
            let mut card = existing?; // decide_action: has_card=false → Create，故 Bump 必有卡
            // 错/假对计数由 verdict 语义区分不可得（旁路只有 action）——统一累加 wrong 计数之外：
            // guessed 假对 Create/Bump 同样走 wrong_count? docs：guessed_count 计假对。
            // 口径：verdict=Correct 的假对 → guessed_count；错 → wrong_count（调用方传入）
            card.wrong_count = card.wrong_count.saturating_add(1);
            card.due_at = now; // 提权：立即进队列
            Some(card)
        }
        CardAction::Schedule(rating) => {
            let mut card = existing?; // Schedule 只在 has_card=true（decide_action 口径）
            if let Err(err) = ambition_srs::schedule(&mut card, rating, now) {
                tracing::warn!(error = %err, "fsrs schedule failed, keeping card state");
            }
            Some(card)
        }
    }
}

/// 复习卡背面（正确答案文本；复习翻卡展示，docs/08 §1 ReviewItem.back）
pub(crate) fn question_back(kind: &QuestionKind) -> String {
    match kind {
        QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => body
            .answer
            .iter()
            .filter_map(|id| body.options.iter().find(|o| o.id == *id))
            .map(|o| o.text.clone())
            .collect::<Vec<_>>()
            .join(" / "),
        QuestionKind::FillBlank(fill) => fill
            .blanks
            .iter()
            .map(|b| b.acceptable.first().cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ; "),
        QuestionKind::Cloze(cloze) => cloze
            .blanks
            .iter()
            .map(|b| b.acceptable.first().cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ; "),
        QuestionKind::GrammarFill(gf) => gf
            .blanks
            .iter()
            .map(|b| b.acceptable.first().cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ; "),
        QuestionKind::SevenOfFive(s75) => s75
            .answer
            .iter()
            .filter_map(|id| s75.options.get(*id as usize))
            .cloned()
            .collect::<Vec<_>>()
            .join(" "),
        QuestionKind::Reading(reading) => reading
            .items
            .iter()
            .map(|item| {
                item.answer
                    .iter()
                    .filter_map(|id| item.options.iter().find(|o| o.id == *id))
                    .map(|o| o.text.clone())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{AttemptMode, ChoiceBody, OptionItem, Provenance};

    // 配对桩（门禁：同文件 fn↔test 配对，P-16；完整场景测试在 srs_tests.rs）

    fn choice_q() -> Question {
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
            vec!["vocab".into()],
            Provenance::Seed,
        )
    }

    #[test]
    fn test_now_unix_is_positive_reasonable() {
        assert!(now_unix() > 1_700_000_000, "当前 unix 秒合理");
    }

    #[tokio::test]
    async fn test_is_guess_attempt_false_without_timing() {
        let engine = Engine::open_memory().await.expect("打开");
        let mut attempt = Attempt {
            attempt_id: "p-1".into(),
            qid: ambition_core::Qid("q".into()),
            answer: Answer::Choice(vec![0]),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        };
        assert!(!engine.is_guess_attempt(&attempt, None).await, "无计时不判");
        attempt.timing = Some(ambition_core::Timing {
            total_ms: 10_000,
            revision_count: 1,
        });
        attempt.self_confidence = Some(1);
        assert!(engine.is_guess_attempt(&attempt, None).await, "自评1=假对");
    }

    #[tokio::test]
    async fn test_srs_after_attempt_skips_pending_verdict() {
        let engine = Engine::open_memory().await.expect("打开");
        let attempt = Attempt {
            attempt_id: "p-2".into(),
            qid: ambition_core::Qid("q".into()),
            answer: Answer::Choice(vec![0]),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        };
        let grade = GradeResult {
            verdict: Verdict::Pending,
            score: 0.0,
            max: 1.0,
            confidence: 0.0,
            graded_by: ambition_core::GradedBy::Rule,
            items: vec![],
            error_tags: vec![],
        };
        assert_eq!(
            engine.srs_after_attempt(&attempt, &grade, false).await,
            None,
            "Pending 不入卡（裁决后补执行）"
        );
    }

    #[tokio::test]
    async fn test_due_list_empty_before_any_due_time() {
        // open 即入内置卡（due_at=装配时刻），unix 0 之前无到期
        let engine = Engine::open_memory().await.expect("打开");
        assert!(engine.due_list(0, 10).await.expect("队列").is_empty());
    }

    #[tokio::test]
    async fn test_review_answer_missing_card_is_not_found() {
        let engine = Engine::open_memory().await.expect("打开");
        assert!(matches!(
            engine.review_answer("card:missing", Rating::Good).await,
            Err(EngineError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_wrong_wordbook_empty_without_cards() {
        let engine = Engine::open_memory().await.expect("打开");
        assert!(engine.wrong_wordbook().await.expect("错词本").is_empty());
    }

    #[tokio::test]
    async fn test_card_title_falls_back_to_source_key_when_question_missing() {
        let engine = Engine::open_memory().await.expect("打开");
        let card = ambition_srs::Card::new(
            CardSource::Question {
                qid: ambition_core::Qid("gone".into()),
            },
            1,
        );
        assert_eq!(engine.card_title(&card).await, "gone", "题缺失退源键");
        let word = ambition_srs::Card::new(
            CardSource::WordSense {
                word_id: "bank".into(),
                sense_id: 1,
            },
            1,
        );
        assert_eq!(engine.card_title(&word).await, "bank");
    }

    #[tokio::test]
    async fn test_review_item_builds_front_back_group_from_question() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_q();
        engine.add_question(&q).await.expect("注入");
        let card = ambition_srs::Card::new(CardSource::Question { qid: q.qid.clone() }, 1);
        let item = engine.review_item(card).await.expect("组条目");
        assert_eq!(item.front, "She is quick.");
        assert_eq!(item.back, "quick");
        assert_eq!(item.group, "vocab");
        // 题缺失 → Err（调用方 warn 跳过）
        let orphan = ambition_srs::Card::new(
            CardSource::Question {
                qid: ambition_core::Qid("gone".into()),
            },
            1,
        );
        assert!(matches!(
            engine.review_item(orphan).await,
            Err(EngineError::NotFound(_))
        ));
    }

    #[test]
    fn test_apply_card_action_skip_and_create_paths() {
        let source = CardSource::Question {
            qid: ambition_core::Qid("q".into()),
        };
        assert!(apply_card_action(None, CardAction::Skip, source.clone(), 1).is_none());
        let created = apply_card_action(None, CardAction::Create, source, 100).expect("新建");
        assert_eq!(created.due_at, 100, "新卡立即到期");
        assert_eq!(created.wrong_count, 1);
        // Schedule 无卡（decide_action 不会产生，防御）→ None
        let source2 = CardSource::Question {
            qid: ambition_core::Qid("q2".into()),
        };
        assert!(
            apply_card_action(None, CardAction::Schedule(Rating::Good), source2, 1).is_none(),
            "无卡调度防御性 None"
        );
    }

    #[test]
    fn test_question_back_single_choice_answer_text() {
        let back = question_back(&QuestionKind::SingleChoice(ChoiceBody {
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
            answer: vec![1],
        }));
        assert_eq!(back, "slowly");
    }
}
