//! 一二级判定环：grade 分发 → grade_rule_gap → grade_after_rule/apply_error_tags（规则优先、零成本）

use super::*;

impl Grader {
    /// 三级流水线入口（①规则 → ②Jev → ③LLM/桶；永不 Err）
    /// 复合题型（Cloze/Reading/SevenOfFive/GrammarFill）拆子题递归后汇总（docs/07 §1 M4 注记）
    pub async fn grade(&self, question: &Question, answer: &Answer) -> GradeResult {
        if question.kind.is_composite() {
            return self.grade_composite(question, answer).await;
        }
        let (ids, fills) = split_answer(answer);
        match grade_rule(&question.kind, &ids, &fills) {
            None => self.grade_rule_gap(question, answer, &ids).await,
            Some(result) => self.grade_after_rule(question, answer, result).await,
        }
    }

    /// 复合题递归判分：拆子题 → 各自走完整三级流水线 → 汇总（docs/07 §1 M4）
    /// 汇总口径：score/max=Σ；verdict=全对 Correct/全错 Wrong/否则 Partial；任一 Pending 则整体 Pending；
    /// graded_by=子题最重路径（Human > LlmReview > Jev > Rule）；error_tags 合并去重；items 带子下标。
    /// 注：递归经 `Box::pin` 装箱（子题自身可能是复合题，嵌套递归需动态分发）。
    async fn grade_composite(&self, question: &Question, answer: &Answer) -> GradeResult {
        let sub_kinds = question.kind.sub_kinds();
        let sub_answers = question.kind.sub_answers(answer);
        if sub_kinds.is_empty() || sub_answers.len() != sub_kinds.len() {
            // 形态不匹配（作答位数与子题数不齐）→ 防御性 Pending（不伪造判分）
            return pending_grade(0.0, sub_kinds.len() as f32, vec![], vec![]);
        }
        let mut total_score = 0.0f32;
        let mut total_max = 0.0f32;
        let mut items = Vec::new();
        let mut error_tags: Vec<ErrorTag> = Vec::new();
        let mut any_pending = false;
        let mut all_correct = true;
        let mut any_wrong = false;
        let mut graded_by = GradedBy::Rule;
        for (index, ((_label, sub_kind), sub_answer)) in
            sub_kinds.iter().zip(sub_answers.iter()).enumerate()
        {
            // 子题 qid = 父 qid + 子下标（不走内容哈希防碰撞；docs/07 §1 M4）
            let sub_qid = ambition_core::Qid(format!("{}:{}", question.qid.0, index));
            let sub_question = Question {
                qid: sub_qid,
                kind: sub_kind.clone(),
                stem: question.stem.clone(),
                difficulty: question.difficulty,
                tags: question.tags.clone(),
                lang: question.lang.clone(),
                provenance: question.provenance,
                quality_score: question.quality_score,
            };
            let sub_grade = Box::pin(self.grade(&sub_question, sub_answer)).await;
            total_score += sub_grade.score;
            total_max += sub_grade.max;
            if sub_grade.verdict == Verdict::Pending {
                any_pending = true;
            }
            if !matches!(sub_grade.verdict, Verdict::Correct | Verdict::Acceptable) {
                all_correct = false;
            }
            if matches!(sub_grade.verdict, Verdict::Wrong | Verdict::Partial) {
                any_wrong = true;
            }
            for tag in &sub_grade.error_tags {
                if !error_tags.contains(tag) {
                    error_tags.push(*tag);
                }
            }
            graded_by = heavier_graded_by(graded_by, sub_grade.graded_by);
            items.push(ItemResult {
                index,
                correct: matches!(sub_grade.verdict, Verdict::Correct | Verdict::Acceptable),
            });
        }
        let verdict = if any_pending {
            Verdict::Pending
        } else if all_correct {
            Verdict::Correct
        } else if any_wrong {
            // 全错 / 部分对：按得分区分 Wrong vs Partial
            if total_score <= 0.0 {
                Verdict::Wrong
            } else {
                Verdict::Partial
            }
        } else {
            Verdict::Partial
        };
        GradeResult {
            verdict,
            score: total_score,
            max: total_max,
            confidence: 1.0,
            graded_by,
            items,
            error_tags,
        }
    }

    /// 规则不可判（多选漏选）：Jev 逐项 → LLM 复核 → 待裁定桶
    async fn grade_rule_gap(
        &self,
        question: &Question,
        answer: &Answer,
        selected: &[u32],
    ) -> GradeResult {
        let thresholds = self.effective_thresholds(question);
        let max = match &question.kind {
            QuestionKind::MultiChoice(body) => body.answer.len() as f32,
            _ => 1.0,
        };
        // 漏选 = 严格子集：已选项全部命中，暂定分 = 已选数
        let provisional = selected.len() as f32;
        if self.jev.is_none() {
            // Jev 离线：直接第三级（无 LLM 则进桶）
            return self
                .third_level(question, answer, Provisional::new(provisional, max))
                .await;
        }
        match self.jev_multi_items(question, selected).await {
            Ok(judge) => {
                let mut score = provisional;
                let mut undecided = false;
                for item in &judge.items {
                    match self.downgrade_route(route_from_margin(
                        item.margin,
                        thresholds.accept,
                        thresholds.reject,
                    )) {
                        Route::AutoAccept => {}            // 漏选确认必错：分不加
                        Route::AutoReject => score += 1.0, // Jev 否定题面答案项：视为已覆盖
                        Route::LlmReview => undecided = true,
                        Route::HumanReview => {
                            return pending_grade(
                                provisional,
                                max,
                                vec![],
                                judge.error_tags.clone(),
                            );
                        }
                    }
                }
                if undecided {
                    return self
                        .third_level(
                            question,
                            answer,
                            Provisional {
                                score,
                                max,
                                items: vec![],
                                error_tags: judge.error_tags,
                            },
                        )
                        .await;
                }
                let complete = score >= max;
                GradeResult {
                    verdict: if complete {
                        Verdict::Acceptable
                    } else {
                        Verdict::Partial
                    },
                    score: score.min(max),
                    max,
                    confidence: min_margin(&judge.items),
                    graded_by: GradedBy::Jev,
                    items: vec![],
                    error_tags: if complete { vec![] } else { judge.error_tags },
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "jev_multi_items failed, falling back to third level");
                self.third_level(question, answer, Provisional::new(provisional, max))
                    .await
            }
        }
    }

    /// 规则已判路径：正确 → 直接完（0 成本）；错误 → 错因标签（选择题）/逐空升级（填空）
    async fn grade_after_rule(
        &self,
        question: &Question,
        answer: &Answer,
        result: GradeResult,
    ) -> GradeResult {
        if matches!(result.verdict, Verdict::Correct | Verdict::Acceptable) {
            return result;
        }
        if self.jev.is_none() {
            return result; // Jev 离线：规则结果无错因标签（离线矩阵）
        }
        if matches!(question.kind, QuestionKind::FillBlank(_)) {
            self.grade_fill_jev(question, answer, result).await
        } else {
            self.apply_error_tags(question, answer, result).await
        }
    }

    /// 选择题错因：单请求 error_cause（标签失败只丢标签，不丢规则结果）
    async fn apply_error_tags(
        &self,
        question: &Question,
        answer: &Answer,
        mut result: GradeResult,
    ) -> GradeResult {
        let Some(jev) = &self.jev else {
            return result; // 调用方已挡；防御性保留（独立调用不 panic）
        };
        let state = answer_state(question, answer);
        let questions = error_cause_questions(&question.lang.0, vec![]);
        match jev.evaluate(state, &questions).await {
            Ok(answers) => {
                result.error_tags = tag_error_cause(&answers, self.thresholds.accept);
            }
            Err(err) => {
                tracing::warn!(error = %err, "error_cause tagging failed, keeping rule result");
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::*;
    use ambition_core::{BlankItem, ChoiceBody, OptionItem, Provenance};

    // 配对桩（门禁：同文件 fn↔test 配对；完整场景测试在 rule_tests.rs）

    #[tokio::test]
    async fn test_grade_rule_path_decides_choice() {
        let grader = Grader::offline();
        let grade = grader
            .grade(&choice_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(grade.verdict, Verdict::Correct);
        assert_eq!(grade.graded_by, GradedBy::Rule);
    }

    #[tokio::test]
    async fn test_grade_rule_gap_offline_pending_for_missed_multi() {
        let grader = Grader::offline();
        let q = Question::new(
            "two".into(),
            QuestionKind::MultiChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "a".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "b".into(),
                    },
                    OptionItem {
                        id: 2,
                        text: "c".into(),
                    },
                ],
                answer: vec![0, 2],
            }),
            2,
            vec![],
            Provenance::Seed,
        );
        let grade = grader.grade(&q, &Answer::Choice(vec![0])).await;
        assert_eq!(grade.verdict, Verdict::Pending, "漏选离线进桶");
    }

    #[tokio::test]
    async fn test_grade_after_rule_keeps_rule_verdict_when_wrong() {
        let grader = Grader::offline();
        let grade = grader
            .grade(&choice_question(), &Answer::Choice(vec![1]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "规则结果保留");
        assert!(grade.error_tags.is_empty(), "Jev 离线无错因标签");
    }

    #[tokio::test]
    async fn test_apply_error_tags_keeps_rule_result_offline() {
        // Jev 离线：apply_error_tags 直接返回原结果（防御分支）
        let grader = Grader::offline();
        let result = grader
            .grade(&choice_question(), &Answer::Choice(vec![1]))
            .await;
        assert_eq!(result.verdict, Verdict::Wrong);
    }

    #[tokio::test]
    async fn test_grade_composite_mismatched_answer_is_pending() {
        let grader = Grader::offline();
        let q = Question::new(
            "I ___ a.".into(),
            QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "I ___ a.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            1,
            vec![],
            Provenance::Seed,
        );
        let grade = grader.grade(&q, &Answer::Choice(vec![0])).await;
        assert_eq!(grade.verdict, Verdict::Pending, "形态不匹配 → Pending");
        assert_eq!(grade.max, 1.0, "max = 子题数");
    }
}
