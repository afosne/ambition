//! 作文题 seed（M5-W3，docs/07 §1 Essay 行）：cet4 议论文（题面中文仅展示、**不进 state**）
//! 字数/限时按 docs/16 stages cet4 writing 口径（120-180 词 / 30 分钟）；
//! MVP：非雅思轨 rubric 复用高考 25 分制（独立 cet4 rubric 随 W6 真题套卷校准）。

use ambition_core::{EssayPrompt, Provenance, Question, QuestionKind};

/// 单篇作文题（题面中文 + 英文指令 stem）
fn essay(prompt_zh: &str, genre: &str, extra_tags: &[&str], difficulty: u8) -> (Question, String) {
    let mut tags = vec!["essay".to_string(), "cet4".to_string()];
    tags.extend(extra_tags.iter().map(|t| (*t).to_string()));
    let q = Question::new(
        "Write an essay in English according to the prompt.".to_string(),
        QuestionKind::Essay(EssayPrompt {
            prompt_zh: prompt_zh.to_string(),
            genre: genre.to_string(),
            track: "cet4".to_string(),
            rubric_id: "gaokao25".to_string(),
            word_range: (120, 180),
            minutes: 30,
        }),
        difficulty,
        tags,
        Provenance::Seed,
    );
    (
        q,
        "Structure: introduction - body - conclusion; one idea per paragraph; keep within the word range.".to_string(),
    )
}

/// cet4 作文两篇（议论 + 书信，体裁标签随 tags 传递）
pub(crate) fn essays() -> Vec<(Question, String)> {
    vec![
        essay(
            "随着手机和社交媒体的普及，越来越多的中学生花费大量时间上网。请以“Spending Time Wisely”为题，写一篇120词左右的短文，谈谈你的看法。",
            "argument",
            &["argument"],
            3,
        ),
        essay(
            "假如你是李华，你校英语社团将举办“绿色校园”主题征文活动。请你给外教Chris写一封邮件，邀请他担任评委，内容包括活动目的、时间地点和评委职责。",
            "letter",
            &["letter", "email"],
            3,
        ),
    ]
}
