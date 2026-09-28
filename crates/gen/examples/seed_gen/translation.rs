//! 翻译题组（M5-W2，CET 段落翻译句子级；docs/10 §3、DESIGN §8.5）
//! 源文 = l1（仅题面展示、**不进 Jev state**，docs/05 §3.5）；参考译文 = l2（进 state 作判分参照）。

use ambition_core::{
    LangTag, Provenance, Question, QuestionKind, TranslationBody, TranslationDirection,
};

/// 中译英句子（`translation4` rubric 判分；难度 2-3、标签 translation/cet4）
pub(crate) fn sentences() -> Vec<(Question, String)> {
    let items: [(&str, &str, u8); 8] = [
        (
            "这个城市以其悠久的历史和美丽的风景闻名于世。",
            "This city is famous the world over for its long history and beautiful scenery.",
            2,
        ),
        (
            "随着经济的发展，越来越多的家庭拥有了自己的汽车。",
            "With the development of the economy, more and more families own their own cars.",
            2,
        ),
        (
            "中国政府高度重视环境保护，并采取了一系列措施。",
            "The Chinese government attaches great importance to environmental protection and has taken a series of measures.",
            3,
        ),
        (
            "剪纸是中国最古老的民间艺术之一，至今仍深受人们喜爱。",
            "Paper-cutting is one of China's oldest folk arts and remains deeply loved today.",
            3,
        ),
        (
            "大学生应该多参加社会实践，为将来的工作做好准备。",
            "College students should take part in more social practice to prepare for their future careers.",
            2,
        ),
        (
            "这款新应用让用户能够随时随地学习英语。",
            "This new app enables users to learn English anytime, anywhere.",
            2,
        ),
        (
            "中医强调预防胜于治疗，注重身心的平衡。",
            "Traditional Chinese medicine emphasizes prevention over cure and values the balance of body and mind.",
            3,
        ),
        (
            "春节是中国最重要的传统节日，一家人团聚在一起吃年夜饭。",
            "Spring Festival is China's most important traditional festival, when the whole family gathers for the reunion dinner.",
            3,
        ),
    ];
    let explain =
        "Keep the original meaning intact; prefer natural collocations over literal wording.";
    items
        .into_iter()
        .map(|(source, reference, difficulty)| {
            let q = Question::new(
                "Translate the following sentence into English.".to_string(),
                QuestionKind::Translation(TranslationBody {
                    direction: TranslationDirection {
                        from: LangTag(ambition_core::DEFAULT_L1.into()),
                        to: LangTag("en".into()),
                    },
                    source_text: source.to_string(),
                    reference: reference.to_string(),
                }),
                difficulty,
                vec!["translation".to_string(), "cet4".to_string()],
                Provenance::Seed,
            );
            (q, explain.to_string())
        })
        .collect()
}
