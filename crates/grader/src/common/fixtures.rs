//! grader 跨文件测试夹具（收口前 common/pipeline/rule 六份逐字复制，2026-09-27 复用合并）

use ambition_core::{
    BlankItem, ChoiceBody, FillBody, OptionItem, Provenance, Question, QuestionKind,
};

use ambition_jev::JevClient;
use wiremock::MockServer;

use crate::Grader;

/// 单选样例题（一二级判定 / Jev 升级 / 仲裁测试共用）
pub(crate) fn choice_question() -> Question {
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

/// 填空样例题（填空 Jev 升级 / 仲裁测试共用）
pub(crate) fn fill_question() -> Question {
    Question::new(
        "She ___ a student.".into(),
        QuestionKind::FillBlank(FillBody {
            sentence: "She ___ a student.".into(),
            blanks: vec![BlankItem {
                acceptable: vec!["is".into(), "'s".into()],
            }],
        }),
        2,
        vec!["grammar".into()],
        Provenance::Seed,
    )
}

/// 多选样例题（多选逐项 noul / 仲裁测试共用）
pub(crate) fn multi_question() -> Question {
    Question::new(
        "Pick two.".into(),
        QuestionKind::MultiChoice(ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "alpha".into(),
                },
                OptionItem {
                    id: 1,
                    text: "beta".into(),
                },
                OptionItem {
                    id: 2,
                    text: "gamma".into(),
                },
            ],
            answer: vec![0, 2],
        }),
        2,
        vec![],
        Provenance::Seed,
    )
}

/// 离线判题器（无 Jev 无 LLM：规则 + 待裁定桶）
pub(crate) fn offline_router() -> Grader {
    Grader::offline()
}

/// 指向 wiremock 服务器的 Jev 客户端（`{uri}/v1` + key=k；Jev 升级用例共用）
pub(crate) async fn client_with(server: &MockServer) -> JevClient {
    JevClient::with_options(
        &format!("{}/v1", server.uri()),
        ambition_jev::DEFAULT_JEV_MODEL,
        Some("k".into()),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choice_question_fixture_is_single_choice_quick() {
        let q = choice_question();
        assert!(
            matches!(q.kind, QuestionKind::SingleChoice(_)),
            "单选夹具（规则层测试共用）"
        );
        assert_eq!(q.provenance, Provenance::Seed);
    }

    #[test]
    fn test_fill_question_fixture_has_one_blank_two_acceptables() {
        let q = fill_question();
        let QuestionKind::FillBlank(fill) = &q.kind else {
            panic!("填空夹具必须是 FillBlank");
        };
        assert_eq!(fill.blanks.len(), 1, "单空夹具");
        assert_eq!(
            fill.blanks[0].acceptable,
            vec!["is".to_string(), "'s".to_string()]
        );
    }

    #[test]
    fn test_multi_question_fixture_answers_two_of_three() {
        let q = multi_question();
        let QuestionKind::MultiChoice(choice) = &q.kind else {
            panic!("多选夹具必须是 MultiChoice");
        };
        assert_eq!(choice.answer, vec![0, 2], "三选二夹具");
        assert_eq!(choice.options.len(), 3);
    }

    #[test]
    fn test_offline_router_fixture_is_offline_english_thresholds() {
        let grader = offline_router();
        assert_eq!(
            grader.thresholds,
            crate::Thresholds::english(),
            "离线形态默认英语阈值档"
        );
        assert!(grader.jev.is_none() && grader.llm.is_none(), "无 AI 供给方");
    }

    #[tokio::test]
    async fn test_client_with_fixture_builds_against_mock_server() {
        let server = MockServer::start().await;
        let client = client_with(&server).await;
        // 端点/key 为内部字段；wire 形状由 fill/ring 的 wiremock 用例覆盖，此处钉构造路径
        drop(client);
    }
}
