//! 生成内置 seed 题库 `data/seed/en/cet4/core.qpack.json`（内容三级保障①，docs/16 §3）。
//! 运行：`cargo run -p ambition-gen --example seed_gen`（幂等：重跑覆盖同文件）。
//! 题目全部英文（Jev state 无中文红线）；qid 由内容哈希自动计算。

use std::path::PathBuf;

use ambition_core::{
    BlankItem, ChoiceBody, FillBody, OptionItem, Provenance, Question, QuestionKind,
};
use ambition_gen::{Manifest, PackItem, QPACK_SCHEMA_VERSION, QPack};

fn choice(
    stem: String,
    options: &[&str],
    answer: usize,
    difficulty: u8,
    tags: &[&str],
) -> Question {
    let items: Vec<OptionItem> = options
        .iter()
        .enumerate()
        .map(|(i, text)| OptionItem {
            id: i as u32,
            text: (*text).to_string(),
        })
        .collect();
    Question::new(
        stem,
        QuestionKind::SingleChoice(ChoiceBody {
            options: items,
            answer: vec![answer as u32],
        }),
        difficulty,
        tags.iter().map(|t| (*t).to_string()).collect(),
        Provenance::Seed,
    )
}

fn multi(
    stem: String,
    options: &[&str],
    answers: &[usize],
    difficulty: u8,
    tags: &[&str],
) -> Question {
    Question::new(
        stem,
        QuestionKind::MultiChoice(ChoiceBody {
            options: options
                .iter()
                .enumerate()
                .map(|(i, text)| OptionItem {
                    id: i as u32,
                    text: (*text).to_string(),
                })
                .collect(),
            answer: answers.iter().map(|i| *i as u32).collect(),
        }),
        difficulty,
        tags.iter().map(|t| (*t).to_string()).collect(),
        Provenance::Seed,
    )
}

/// 一般现在时第三人称单数（8 主语 × 8 动词 = 64）
fn present_sva() -> Vec<(Question, String)> {
    let subjects = [
        "He",
        "She",
        "My brother",
        "The manager",
        "Every candidate",
        "Anna",
        "My sister",
        "The professor",
    ];
    let verbs: [(&str, &str, &str); 8] = [
        ("go", "goes", "to school by bus every day"),
        ("study", "studies", "at the library every evening"),
        ("watch", "watches", "the news after dinner"),
        ("finish", "finishes", "work at six o'clock"),
        ("take", "takes", "the subway to the office"),
        ("write", "writes", "two emails every morning"),
        ("speak", "speaks", "English fluently"),
        ("read", "reads", "a chapter before bed"),
    ];
    let explain = "Third-person singular subjects take the -s form in the present simple.";
    let mut out = Vec::new();
    for subject in subjects {
        for (base, s_form, complement) in verbs {
            let q = choice(
                format!("{subject} ___ {complement}."),
                &[base, s_form, &format!("{base}ing"), &format!("{base}ed")],
                1,
                2,
                &["grammar", "sva"],
            );
            out.push((q, explain.to_string()));
        }
    }
    out
}

/// 一般过去时规则变位（6 主语 × 10 动词 = 60）
fn past_regular() -> Vec<(Question, String)> {
    let subjects = [
        "Yesterday he",
        "Last week she",
        "On Monday we",
        "After dinner they",
        "At noon I",
        "Then the assistant",
    ];
    let verbs: [(&str, &str, &str); 10] = [
        ("finish", "finished", "the report"),
        ("call", "called", "his client"),
        ("open", "opened", "the door"),
        ("clean", "cleaned", "the kitchen"),
        ("paint", "painted", "the wall"),
        ("copy", "copied", "the file"),
        ("enjoy", "enjoyed", "the concert"),
        ("arrive", "arrived", "at the station"),
        ("help", "helped", "the new worker"),
        ("carry", "carried", "the box"),
    ];
    let explain =
        "Regular verbs in the past simple take the -ed ending (y → ied after consonants).";
    let mut out = Vec::new();
    for subject in subjects {
        for (base, past, complement) in verbs {
            let q = choice(
                format!("{subject} ___ {complement}."),
                &[base, &format!("{base}s"), past, &format!("{base}ing")],
                2,
                2,
                &["grammar", "tense"],
            );
            out.push((q, explain.to_string()));
        }
    }
    out
}

/// 将来时 will（20）
fn future_will() -> Vec<(Question, String)> {
    let subjects = [
        "Tomorrow the team",
        "Next year I",
        "On Friday we",
        "Soon the system",
        "After the exam she",
    ];
    let verbs: [(&str, &str, &str); 4] = [
        ("announce", "announce", "the final results"),
        ("publish", "publish", "a new report"),
        ("release", "release", "the updated version"),
        ("hold", "hold", "a special meeting"),
    ];
    let explain = "The future with will uses the base form after will.";
    let mut out = Vec::new();
    for subject in subjects {
        for (base, will_form, complement) in verbs {
            let q = choice(
                format!("{subject} ___ {complement} as planned."),
                &["will", will_form, &format!("{base}s"), "was"],
                1,
                2,
                &["grammar", "tense"],
            );
            out.push((q, explain.to_string()));
        }
    }
    out
}

/// 冠词 a/an/the（30）
fn articles() -> Vec<(Question, String)> {
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
fn prepositions() -> Vec<(Question, String)> {
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

/// 时态辨析（7）
fn tense_mix() -> Vec<(Question, String)> {
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
fn fill_blanks() -> Vec<(Question, String)> {
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
fn multi_choice() -> Vec<(Question, String)> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scored: Vec<(Question, String)> = Vec::new();
    scored.extend(present_sva());
    scored.extend(past_regular());
    scored.extend(future_will());
    scored.extend(articles());
    scored.extend(prepositions());
    scored.extend(tense_mix());
    scored.extend(fill_blanks());
    scored.extend(multi_choice());
    assert!(
        scored.len() >= 200,
        "docs/16 requires >=200 questions per track, got {}",
        scored.len()
    );

    // 唯一性兜底（qid 内容哈希，重复内容必须暴露）
    let mut qids = std::collections::HashSet::new();
    for (q, _) in &scored {
        if !qids.insert(q.qid.0.clone()) {
            return Err(format!("duplicate question content: {}", q.stem).into());
        }
    }

    let questions: Vec<Question> = scored.iter().map(|(q, _)| q.clone()).collect();
    let pack = QPack {
        schema_version: QPACK_SCHEMA_VERSION,
        manifest: Manifest::local("core", "cet4"),
        questions: questions
            .iter()
            .zip(scored.iter())
            .map(|(q, (_, explain))| PackItem::with_explanations(q.clone(), vec![explain.clone()]))
            .collect(),
    };

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_path = root.join("data/seed/en/cet4/core.qpack.json");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&pack)?;
    std::fs::write(&out_path, json)?;
    println!(
        "wrote {} questions -> {}",
        pack.questions.len(),
        out_path.display()
    );
    Ok(())
}
