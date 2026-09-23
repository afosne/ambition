//! 用法题组：冠词 / 介词（docs/16 §3）

use ambition_core::Question;

use super::choice;

/// 冠词 a/an/the（30）
pub(crate) fn articles() -> Vec<(Question, String)> {
    let items: [(&str, usize, &str); 30] = [
        (
            "She studies at ___ university.",
            0,
            "University starts with a /j/ sound, so it takes 'a'.",
        ),
        (
            "There is ___ book on the desk.",
            0,
            "'Book' is a singular countable noun needing an article.",
        ),
        (
            "My uncle drives ___ taxi for a living.",
            0,
            "Singular countable noun needs an article.",
        ),
        (
            "We adopted ___ puppy last week.",
            0,
            "Singular countable noun needs an article.",
        ),
        (
            "He is ___ capable teacher.",
            0,
            "'Capable' starts with a consonant sound.",
        ),
        (
            "This is ___ one-way street.",
            0,
            "'One-way' starts with a /w/ sound, so it takes 'a'.",
        ),
        (
            "She bought ___ new blouse.",
            0,
            "'New' starts with a consonant sound.",
        ),
        (
            "They live in ___ small village.",
            0,
            "Described singular noun needs an article.",
        ),
        (
            "I need ___ pen to write a note.",
            0,
            "First mention of a singular countable noun.",
        ),
        (
            "He made ___ clever decision.",
            0,
            "'Clever' starts with a consonant sound.",
        ),
        (
            "We waited for ___ hour.",
            1,
            "'Hour' is pronounced with a vowel sound, so it takes 'an'.",
        ),
        (
            "My father is ___ honest man.",
            1,
            "'Honest' starts with a vowel sound (/ˈɒnɪst/).",
        ),
        (
            "He bought ___ orange at the market.",
            1,
            "'Orange' starts with a vowel sound.",
        ),
        (
            "There is ___ elephant in the zoo.",
            1,
            "'Elephant' starts with a vowel sound.",
        ),
        (
            "She ate ___ apple before class.",
            1,
            "'Apple' starts with a vowel sound.",
        ),
        (
            "Jack is ___ American student.",
            1,
            "'American' starts with a vowel sound.",
        ),
        (
            "He shared ___ unusual idea.",
            1,
            "'Unusual' starts with a vowel sound.",
        ),
        (
            "There is ___ umbrella by the door.",
            1,
            "'Umbrella' starts with a vowel sound.",
        ),
        (
            "It was ___ amazing performance.",
            1,
            "'Amazing' starts with a vowel sound.",
        ),
        (
            "She has ___ hour free at noon.",
            1,
            "'Hour' is pronounced with a vowel sound.",
        ),
        (
            "___ sun rises in the east.",
            2,
            "Natural phenomena with unique nouns take 'the'.",
        ),
        (
            "Please close ___ window before you leave.",
            2,
            "Specific window known to both speakers.",
        ),
        (
            "She plays ___ piano remarkably well.",
            2,
            "Musical instruments take 'the'.",
        ),
        (
            "He is ___ tallest boy in his class.",
            2,
            "Superlatives take 'the'.",
        ),
        (
            "___ moon looks bright tonight.",
            2,
            "Unique noun in nature takes 'the'.",
        ),
        (
            "Could you pass ___ salt, please?",
            2,
            "Objects on the table between speakers take 'the'.",
        ),
        (
            "I enjoyed ___ movie we saw yesterday.",
            2,
            "The relative clause makes the noun specific.",
        ),
        (
            "___ capital of France is Paris.",
            2,
            "Unique noun with 'of' phrase takes 'the'.",
        ),
        (
            "He is ___ tallest student in the group.",
            2,
            "Superlatives take 'the'.",
        ),
        (
            "Turn off ___ lights before leaving.",
            2,
            "Specific lights known to both speakers.",
        ),
    ];
    let mut out = Vec::new();
    for (stem, answer, explain) in items {
        let q = choice(
            stem.to_string(),
            &["a", "an", "the", "some"],
            answer,
            2,
            &["grammar", "article"],
        );
        out.push((q, explain.to_string()));
    }
    out
}

/// 固定介词搭配（10）
pub(crate) fn prepositions() -> Vec<(Question, String)> {
    let items: [(&str, [&str; 4], usize, &str); 10] = [
        (
            "She is interested ___ biology.",
            ["in", "on", "at", "for"],
            0,
            "'Interested in' is a fixed collocation.",
        ),
        (
            "The meeting is scheduled ___ Monday.",
            ["in", "on", "at", "to"],
            1,
            "Days of the week take 'on'.",
        ),
        (
            "We arrived ___ the airport two hours early.",
            ["in", "on", "at", "of"],
            2,
            "Specific points in space take 'at'.",
        ),
        (
            "She depends ___ her parents for advice.",
            ["in", "on", "at", "with"],
            1,
            "'Depend on' is a fixed collocation.",
        ),
        (
            "He apologized ___ his colleague.",
            ["for", "on", "at", "in"],
            0,
            "'Apologize to/for' — 'apologize for a colleague' fits the meaning here.",
        ),
        (
            "The book on the shelf belongs ___ me.",
            ["for", "on", "at", "to"],
            3,
            "'Belong to' is a fixed collocation.",
        ),
        (
            "She is good ___ mathematics.",
            ["in", "on", "at", "for"],
            2,
            "'Good at' for skills and subjects.",
        ),
        (
            "He is afraid ___ dogs.",
            ["in", "on", "at", "of"],
            3,
            "'Afraid of' is a fixed collocation.",
        ),
        (
            "We are proud ___ your progress.",
            ["in", "on", "at", "of"],
            3,
            "'Proud of' is a fixed collocation.",
        ),
        (
            "The train leaves ___ 8:30 in the morning.",
            ["in", "on", "at", "to"],
            2,
            "Clock times take 'at'.",
        ),
    ];
    let mut out = Vec::new();
    for (stem, options, answer, explain) in items {
        let q = choice(
            stem.to_string(),
            &options,
            answer,
            3,
            &["grammar", "preposition"],
        );
        out.push((q, explain.to_string()));
    }
    out
}
