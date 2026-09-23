//! 时态题组：一般现在时三单 / 过去式 / will（docs/16 §3）

use ambition_core::Question;

use super::choice;

/// 一般现在时第三人称单数（8 主语 × 8 动词 = 64）
pub(crate) fn present_sva() -> Vec<(Question, String)> {
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
pub(crate) fn past_regular() -> Vec<(Question, String)> {
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
pub(crate) fn future_will() -> Vec<(Question, String)> {
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
