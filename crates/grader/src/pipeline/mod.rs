//! 三级判题流水线（细分域根，docs/07-grading §1/§3/§4，M3 落地）：规则 → Jev → LLM 复核/待裁定桶。
//! `grade()` 永不 Err：AI 缺席/网络失败按离线矩阵降级（规则结果或 Pending 进桶，tracing 可见）。
//! 本域含子组件：`pipeline::fill`（Jev 填空/多选升级）/ `pipeline::arbitrate`（第三级 LLM 复核）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod arbitrate;
mod fill;

pub use arbitrate::*;

use crate::*;

/// 判题器（AI 全可空 = 离线形态；阈值按题目 lang 取）
pub struct Grader {
    pub(crate) jev: Option<std::sync::Arc<JevClient>>,
    pub(crate) llm: Option<std::sync::Arc<LlmRouter>>,
    pub(crate) thresholds: Thresholds,
}

impl Grader {
    /// 构造（jev/llm 可空；thresholds 按 lang 覆盖由调用方在 grade 时决定默认档）
    pub fn new(
        jev: Option<std::sync::Arc<JevClient>>,
        llm: Option<std::sync::Arc<LlmRouter>>,
        thresholds: Thresholds,
    ) -> Self {
        Self {
            jev,
            llm,
            thresholds,
        }
    }

    /// 离线形态（无 Jev 无 LLM：规则 + 待裁定桶）
    pub fn offline() -> Self {
        Self::new(None, None, Thresholds::english())
    }

    /// 生效阈值（按题目目标语；默认英语档）
    pub(crate) fn effective_thresholds(&self, question: &Question) -> Thresholds {
        if self.thresholds == Thresholds::english() {
            Thresholds::for_lang(&question.lang.0)
        } else {
            self.thresholds
        }
    }

    /// 灰区降级：无 LLM 时 LlmReview → HumanReview（进桶，docs/05 §2 语义）
    pub(crate) fn downgrade_route(&self, route: Route) -> Route {
        match route {
            Route::LlmReview if self.llm.is_none() => Route::HumanReview,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{ChoiceBody, OptionItem, Provenance};
    use std::sync::Arc;

    fn choice_question() -> Question {
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

    fn offline_router() -> Grader {
        Grader::offline()
    }

    /// 无供给方请求的路由器占位（downgrade 判「有 LLM」用；structured 永不被调到）
    fn dummy_router() -> LlmRouter {
        struct NoCall;
        #[async_trait::async_trait]
        impl ambition_llm::LlmProvider for NoCall {
            async fn complete(
                &self,
                req: &ambition_llm::GenRequest,
            ) -> Result<ambition_llm::LlmOut, LlmError> {
                let _ = req.temperature;
                Err(LlmError::Offline)
            }
            fn model(&self) -> &str {
                "no-call"
            }
        }
        LlmRouter::new(
            Box::new(NoCall),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        )
    }

    #[test]
    fn test_new_and_offline_construct_grader() {
        let grader = Grader::new(None, None, Thresholds::english());
        assert!(grader.jev.is_none() && grader.llm.is_none());
        let offline = offline_router();
        assert_eq!(offline.thresholds, Thresholds::english());
        let custom = Grader::new(None, None, Thresholds::non_english());
        assert_eq!(custom.thresholds, Thresholds::non_english());
    }

    #[test]
    fn test_offline_builds_without_ai_clients() {
        let offline = Grader::offline();
        assert!(offline.jev.is_none(), "离线无 Jev 客户端");
        assert!(offline.llm.is_none(), "离线无 LLM 路由");
        assert_eq!(
            offline.thresholds,
            Thresholds::english(),
            "离线默认英语档（grade 时按题目 lang 再切）"
        );
    }

    #[test]
    fn test_effective_thresholds_picks_lang_band_unless_overridden() {
        let grader = Grader::offline();
        let mut ja = choice_question();
        ja.lang = ambition_core::LangTag("ja".into());
        assert_eq!(
            grader.effective_thresholds(&ja),
            Thresholds::non_english(),
            "非英语题切保守档"
        );
        assert_eq!(
            grader.effective_thresholds(&choice_question()),
            Thresholds::english(),
            "英语题默认档"
        );
        let forced = Grader::new(None, None, Thresholds::non_english());
        assert_eq!(
            forced.effective_thresholds(&choice_question()),
            Thresholds::non_english(),
            "显式阈值优先于 lang 推断"
        );
    }

    #[test]
    fn test_downgrade_route_demotes_llm_review_without_llm() {
        let offline = Grader::offline();
        assert_eq!(
            offline.downgrade_route(Route::LlmReview),
            Route::HumanReview,
            "无 LLM → 人工裁决"
        );
        assert_eq!(
            offline.downgrade_route(Route::AutoAccept),
            Route::AutoAccept
        );
        assert_eq!(
            offline.downgrade_route(Route::AutoReject),
            Route::AutoReject
        );
        assert_eq!(
            offline.downgrade_route(Route::HumanReview),
            Route::HumanReview
        );
        let with_llm = Grader::new(None, Some(Arc::new(dummy_router())), Thresholds::english());
        assert_eq!(
            with_llm.downgrade_route(Route::LlmReview),
            Route::LlmReview,
            "有 LLM 保持复核"
        );
    }
}
