//! 生成内置 seed 题库 `data/seed/en/cet4/core.qpack.json`（内容三级保障①，docs/16 §3）。
//! 运行：`cargo run -p ambition-gen --example seed_gen`（幂等：重跑覆盖同文件）。
//! 题目全部英文（Jev state 无中文红线；唯一例外 = 翻译题源文 passage 字段——l1 不进 state，docs/05 §3.5）；qid 由内容哈希自动计算。

use std::path::PathBuf;

use ambition_core::{BlankItem, ChoiceBody, OptionItem, Provenance, Question, QuestionKind};
use ambition_gen::{Manifest, PackItem, QPACK_SCHEMA_VERSION, QPack};

mod essay;
mod grammar;
mod mixed;
mod passage;
mod translation;
mod usage;

use essay::essays;
use grammar::{future_will, past_regular, present_sva};
use mixed::{fill_blanks, multi_choice, tense_mix};
use passage::{cloze, grammar_fill, reading, seven_of_five};
use translation::sentences;
use usage::{articles, prepositions};

pub(crate) fn choice(
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

pub(crate) fn multi(
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

/// 语篇填空题参数（完形/语法填空共用；docs/01 §1：参数 >5 组 struct）
pub(crate) struct PassageFill<'a> {
    pub title: &'a str,
    pub passage: &'a str,
    pub acceptable: &'a [&'a str],
    pub difficulty: u8,
    pub tags: &'a [&'a str],
}

/// 语篇填空题（完形/语法填空共用构造：passage + 逐空单答案；Kind 包装与标签由调用方给——
/// 原两处逐字重复的 `Question::new + blanks map` 收口，2026-09-27 复用合并）
pub(crate) fn passage_fill(
    spec: PassageFill<'_>,
    wrap: impl FnOnce(String, Vec<BlankItem>) -> QuestionKind,
) -> Question {
    let blanks = spec
        .acceptable
        .iter()
        .map(|ok| BlankItem {
            acceptable: vec![(*ok).to_string()],
        })
        .collect();
    Question::new(
        spec.title.to_string(),
        wrap(spec.passage.to_string(), blanks),
        spec.difficulty,
        spec.tags.iter().map(|t| (*t).to_string()).collect(),
        Provenance::Seed,
    )
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
    // M4 题型补全：语篇四题型（完形/语法填空/阅读/七选五）
    scored.extend(cloze());
    scored.extend(grammar_fill());
    scored.extend(reading());
    scored.extend(seven_of_five());
    // M5-W2：翻译题（源文 l1 不进 Jev state，docs/05 §3.5）
    scored.extend(sentences());
    // M5-W3：作文题（中文题面不进 state；词数/限时走卷面层，docs/07 §1）
    scored.extend(essays());
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
