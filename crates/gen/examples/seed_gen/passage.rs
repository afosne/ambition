//! 语篇题组：完形 / 语法填空 / 阅读 / 七选五（M4 题型补全，DESIGN §13.4；docs/16 §3 seed）

use ambition_core::{
    ClozeBody, GrammarFillBody, OptionItem, Provenance, Question, QuestionKind, ReadingBody,
    ReadingItem, SevenOfFiveBody,
};

use crate::{PassageFill, passage_fill};

/// 完形填空（2 篇，每篇 4 空）
pub(crate) fn cloze() -> Vec<(Question, String)> {
    let items: [(&str, &str, [&str; 4], &str); 2] = [
        (
            "Cloze: daily routine",
            "Every morning, Kate ___ early, ___ breakfast with her family, and then ___ the bus to school. She never ___ the bus.",
            ["gets", "has", "takes", "misses"],
            "Present simple: third-person singular takes -s (gets/has/takes/misses).",
        ),
        (
            "Cloze: a visit to the museum",
            "Last Sunday we ___ the science museum. The guide ___ us about the stars. We ___ a lot and ___ home before dark.",
            ["visited", "told", "learned", "returned"],
            "Narrative past: the story is finished, so all verbs take the past form.",
        ),
    ];
    let mut out = Vec::new();
    for (title, passage, acceptable, explain) in items {
        assert!(
            passage.matches("___").count() == acceptable.len(),
            "cloze marker/blank mismatch: {title}"
        );
        let q = passage_fill(
            PassageFill {
                title,
                passage,
                acceptable: &acceptable,
                difficulty: 3,
                tags: &["cloze", "reading"],
            },
            |passage, blanks| QuestionKind::Cloze(ClozeBody { passage, blanks }),
        );
        out.push((q, explain.to_string()));
    }
    out
}

/// 语法填空（2 篇，每篇 3 空）
pub(crate) fn grammar_fill() -> Vec<(Question, String)> {
    let items: [(&str, &str, [&str; 3], &str); 2] = [
        (
            "Grammar fill: tense",
            "By next year we ___ this project. She ___ already ___ the report when I arrived.",
            ["will finish", "had", "finished"],
            "Future with will; past perfect 'had + past participle' for the earlier action.",
        ),
        (
            "Grammar fill: voice and articles",
            "The bridge ___ last year. He is ___ honest man and ___ honest man never lies.",
            ["was built", "an", "an"],
            "Passive voice 'was built'; 'honest' starts with a vowel sound → 'an'.",
        ),
    ];
    let mut out = Vec::new();
    for (title, passage, acceptable, explain) in items {
        assert!(
            passage.matches("___").count() == acceptable.len(),
            "grammar_fill marker/blank mismatch: {title}"
        );
        let q = passage_fill(
            PassageFill {
                title,
                passage,
                acceptable: &acceptable,
                difficulty: 3,
                tags: &["grammar", "fill"],
            },
            |passage, blanks| QuestionKind::GrammarFill(GrammarFillBody { passage, blanks }),
        );
        out.push((q, explain.to_string()));
    }
    out
}

/// 阅读子题数据（问题、四选项、答案下标）
type ReadingSeed<'a> = (&'a str, [&'a str; 4], usize);
/// 阅读篇数据（标题、语篇、子题、讲解）
type ReadingPassage<'a> = (&'a str, &'a str, [ReadingSeed<'a>; 3], &'a str);

/// 阅读理解（2 篇，每篇 3 子题）
pub(crate) fn reading() -> Vec<(Question, String)> {
    let items: [ReadingPassage<'_>; 2] = [
        (
            "Reading: the early bird café",
            "Mara opened a small café near the station three years ago. She wakes at five every day to bake bread herself. At first only regular commuters came, but the smell soon drew office workers from the next street. Last year she hired two assistants, yet she still checks the ovens before dawn.",
            [
                (
                    "When does Mara bake the bread?",
                    ["At noon", "Before dawn", "After lunch", "At midnight"],
                    1,
                ),
                (
                    "Why did office workers come?",
                    [
                        "The smell of bread",
                        "Cheap prices",
                        "A newspaper report",
                        "A radio show",
                    ],
                    0,
                ),
                (
                    "How many assistants work there now?",
                    ["One", "Two", "Three", "None"],
                    1,
                ),
            ],
            "Details are stated: 'wakes at five… bake bread', 'the smell soon drew office workers', 'hired two assistants'.",
        ),
        (
            "Reading: recycling in Riverside",
            "Riverside town introduced blue recycling bins in 2019. At first many residents ignored them, but the council ran a school programme that taught children to sort waste. Within two years the town recycled sixty percent of its household waste, and neighbouring villages copied the model. The programme now costs less than the old landfill fees.",
            [
                (
                    "What changed residents' habits?",
                    [
                        "Higher taxes",
                        "A school programme",
                        "New shops",
                        "A road closing",
                    ],
                    1,
                ),
                (
                    "How much waste is recycled today?",
                    [
                        "Ten percent",
                        "Thirty percent",
                        "Sixty percent",
                        "All of it",
                    ],
                    2,
                ),
                (
                    "What happened to the cost?",
                    ["It rose", "It fell", "It stayed equal", "It became unknown"],
                    1,
                ),
            ],
            "The passage states the school programme, sixty percent, and 'costs less than the old landfill fees'.",
        ),
    ];
    let mut out = Vec::new();
    for (title, passage, questions, explain) in items {
        let reading_items: Vec<ReadingItem> = questions
            .iter()
            .map(|(q, options, answer)| ReadingItem {
                question: (*q).to_string(),
                options: options
                    .iter()
                    .enumerate()
                    .map(|(i, text)| OptionItem {
                        id: i as u32,
                        text: (*text).to_string(),
                    })
                    .collect(),
                answer: vec![*answer as u32],
            })
            .collect();
        let q = Question::new(
            title.to_string(),
            QuestionKind::Reading(ReadingBody {
                passage: passage.to_string(),
                items: reading_items,
            }),
            3,
            vec!["reading".to_string(), "comprehension".to_string()],
            Provenance::Seed,
        );
        out.push((q, explain.to_string()));
    }
    out
}

/// 七选五篇数据（标题、语篇、7 候选、5 缺口答案、讲解）
type SevenOfFivePassage<'a> = (&'a str, &'a str, [&'a str; 7], [usize; 5], &'a str);

/// 七选五（2 篇，每篇 5 缺口 + 7 候选）
pub(crate) fn seven_of_five() -> Vec<(Question, String)> {
    let items: [SevenOfFivePassage<'_>; 2] = [
        (
            "Seven of five: how to study a word",
            "When you meet a new word, first ___ it in a sentence you understand. Next, ___ the word the same evening; sleep helps the brain keep new traces. Then, ___ using the word in your own writing within three days. After that, ___ its family: verb, noun, adjective. Finally, ___ the word again after a week.",
            [
                "guess its meaning from context",
                "copy the word three times",
                "try",
                "collect",
                "review",
                "throw away the note",
                "close the dictionary at once",
            ],
            [0, 1, 2, 3, 4],
            "The five blanks follow the study sequence: guess → copy → use → build the word family → review.",
        ),
        (
            "Seven of five: a healthy breakfast",
            "A good breakfast ___ the body after a night without food. Children who eat breakfast ___ better at school. Choose food that ___ slowly, such as oats, ___ sugar, and ___ water instead of juice.",
            [
                "fuels",
                "concentrate",
                "releases energy",
                "limit",
                "drink",
                "skip the meal",
                "sweeten the coffee",
            ],
            [0, 1, 2, 3, 4],
            "Each blank follows logically: fuels → concentrate → releases energy → limit sugar → drink water.",
        ),
    ];
    let mut out = Vec::new();
    for (title, passage, options, answer, explain) in items {
        assert!(
            passage.matches("___").count() == answer.len(),
            "seven_of_five marker/gap mismatch: {title}"
        );
        assert!(options.len() >= 7, "seven_of_five needs 7 options");
        let q = Question::new(
            title.to_string(),
            QuestionKind::SevenOfFive(SevenOfFiveBody {
                passage: passage.to_string(),
                options: options.iter().map(|o| (*o).to_string()).collect(),
                answer: answer.iter().map(|i| *i as u32).collect(),
            }),
            4,
            vec!["reading".to_string(), "seven_of_five".to_string()],
            Provenance::Seed,
        );
        out.push((q, explain.to_string()));
    }
    out
}
