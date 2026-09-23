//! 综合题组：时态混填 / 语法填空 / 多选（docs/16 §3）

use ambition_core::{BlankItem, FillBody, Provenance, Question, QuestionKind};

use super::{choice, multi};

/// 时态辨析（7）
pub(crate) fn tense_mix() -> Vec<(Question, String)> {
    let items: [(&str, [&str; 4], usize, &str); 7] = [
        (
            "Yesterday, he ___ to the library.",
            ["go", "goes", "went", "going"],
            2,
            "'Yesterday' signals the past simple.",
        ),
        (
            "Look! The baby ___ .",
            ["sleep", "sleeps", "is sleeping", "slept"],
            2,
            "'Look!' signals an action happening now.",
        ),
        (
            "By the time we arrived, the film ___ .",
            ["has started", "had started", "starts", "is starting"],
            1,
            "An action completed before another past action uses the past perfect.",
        ),
        (
            "She ___ in this company since 2019.",
            ["works", "worked", "has worked", "is working"],
            2,
            "'Since' with a present result calls for the present perfect.",
        ),
        (
            "When I was young, I ___ swimming every summer.",
            ["go", "went", "will go", "am going"],
            1,
            "Repeated past habits use the past simple.",
        ),
        (
            "He usually ___ breakfast at seven.",
            ["has", "is having", "had", "have"],
            0,
            "'Usually' signals a habitual action; 'he' takes 'has'.",
        ),
        (
            "This time next week, we ___ in Paris.",
            ["are", "will be", "were", "have been"],
            1,
            "A future moment in progress uses 'will be'.",
        ),
    ];
    let mut out = Vec::new();
    for (stem, options, answer, explain) in items {
        let q = choice(stem.to_string(), &options, answer, 3, &["grammar", "tense"]);
        out.push((q, explain.to_string()));
    }
    out
}

/// 填空（10）
pub(crate) fn fill_blanks() -> Vec<(Question, String)> {
    let items: [(&str, &[&str], u8, &str); 10] = [
        (
            "She ___ a teacher at our school.",
            &["is"],
            1,
            "'She' takes 'is' in the present simple.",
        ),
        (
            "There ___ many books on the shelf.",
            &["are", "were"],
            2,
            "'Many books' is plural, so it takes 'are' or 'were'.",
        ),
        (
            "I look forward to ___ from you.",
            &["hearing"],
            3,
            "'Look forward to' is followed by a gerund.",
        ),
        (
            "It is no use ___ over spilt milk.",
            &["crying"],
            4,
            "'It is no use' is followed by a gerund.",
        ),
        (
            "The bridge must ___ repaired before winter.",
            &["be"],
            3,
            "A modal is followed by the bare infinitive; the passive needs 'be'.",
        ),
        (
            "He suggested ___ the meeting until Friday.",
            &["postponing", "delaying"],
            3,
            "'Suggest' is followed by a gerund.",
        ),
        (
            "Neither of the students ___ ready yet.",
            &["is"],
            4,
            "'Neither of' takes a singular verb in formal English.",
        ),
        (
            "The news ___ really surprising.",
            &["is"],
            2,
            "Uncountable nouns such as 'news' take a singular verb.",
        ),
        (
            "One of my friends ___ a doctor.",
            &["is"],
            2,
            "'One of + plural noun' takes a singular verb.",
        ),
        (
            "She prevented him ___ making the same mistake.",
            &["from"],
            3,
            "'Prevent someone from doing something' is the fixed pattern.",
        ),
    ];
    let mut out = Vec::new();
    for (stem, acceptable, difficulty, explain) in items {
        assert!(stem.contains("___"), "fill stem must contain blank: {stem}");
        let q = Question::new(
            stem.to_string(),
            QuestionKind::FillBlank(FillBody {
                sentence: stem.to_string(),
                blanks: vec![BlankItem {
                    acceptable: acceptable.iter().map(|s| (*s).to_string()).collect(),
                }],
            }),
            difficulty,
            vec!["grammar".to_string(), "fill".to_string()],
            Provenance::Seed,
        );
        out.push((q, explain.to_string()));
    }
    out
}

/// 多选（2）
pub(crate) fn multi_choice() -> Vec<(Question, String)> {
    vec![
        (
            multi(
                "Select the two correct sentences.".into(),
                &[
                    "She don't like coffee.",
                    "She doesn't like coffee.",
                    "They go to school every day.",
                    "They goes to school.",
                ],
                &[1, 2],
                3,
                &["grammar", "multi"],
            ),
            "'She' needs 'doesn't + base form'; 'they' takes the base form.".to_string(),
        ),
        (
            multi(
                "Select the two adverbs.".into(),
                &["quickly", "quick", "very", "beauty"],
                &[0, 2],
                2,
                &["grammar", "multi"],
            ),
            "'Quickly' modifies verbs; 'very' modifies adjectives and adverbs.".to_string(),
        ),
    ]
}
