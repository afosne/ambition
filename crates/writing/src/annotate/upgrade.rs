//! 升级句候选（M5-W5，docs/10 §2：低级句→高级句 good→beneficial…）——本地两路：整句反模板命中优先、词级同义表兜底

use crate::template::Template;

/// 升级候选（低级→高级；why_zh = 中文理由——TUI 直显）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Upgrade {
    pub from: String,
    pub to: String,
    pub why_zh: String,
}

/// 词级同义表（本地、无 AI——短语级书面化替换；非判分语料故入码，docs/05 语料红线不适用）
const SYNONYMS: &[(&str, &str)] = &[
    ("good", "beneficial"),
    ("bad", "harmful"),
    ("important", "crucial"),
    ("think", "maintain"),
    ("many", "a multitude of"),
    ("very", "exceptionally"),
    ("get", "obtain"),
    ("help", "facilitate"),
    ("show", "illustrate"),
    ("but", "nevertheless"),
    ("also", "moreover"),
    ("so", "therefore"),
    ("nowadays", "in recent years"),
];

/// 升级候选（整句反模板命中优先并返回；否则逐词同义候选；无命中 → 空列表，不臆造）
pub fn upgrade_sentence(sentence: &str, templates: &[Template]) -> Vec<Upgrade> {
    // 1) 整句级：反模板黑名单命中 → 子串替换（保留句中上下文；大小写不敏感仅限 ASCII 句）
    let lowered = sentence.to_lowercase();
    for t in templates {
        for anti in &t.anti_patterns {
            let pattern = anti.pattern_en.trim();
            if pattern.is_empty() || anti.replacement_en.trim().is_empty() {
                continue;
            }
            let hit = sentence
                .find(pattern)
                .map(|pos| (pos, pattern.len()))
                .or_else(|| {
                    sentence
                        .is_ascii()
                        .then(|| {
                            lowered
                                .find(&pattern.to_lowercase())
                                .map(|pos| (pos, pattern.len()))
                        })
                        .flatten()
                });
            let Some((pos, len)) = hit else { continue };
            let to = format!(
                "{}{}{}",
                &sentence[..pos],
                anti.replacement_en,
                &sentence[pos + len..]
            );
            if to != sentence {
                return vec![Upgrade {
                    from: sentence.to_string(),
                    to,
                    why_zh: anti.note_zh.clone(),
                }];
            }
        }
    }
    // 2) 词级：同义表逐词命中（保留原词标点形态，大小写按原词首字母）
    let mut out = Vec::new();
    for (low, high) in SYNONYMS {
        for word in sentence.split_whitespace() {
            let bare = word.trim_matches(|c: char| !c.is_alphabetic());
            if !bare.eq_ignore_ascii_case(low) {
                continue;
            }
            let replaced = if word.starts_with(|c: char| c.is_uppercase()) {
                let mut h = high.to_string();
                let mut cs = h.chars();
                if let Some(first) = cs.next() {
                    h = first.to_uppercase().collect::<String>() + cs.as_str();
                }
                h
            } else {
                (*high).to_string()
            };
            let to = word.replacen(bare, &replaced, 1);
            if to == *word {
                continue;
            }
            out.push(Upgrade {
                from: word.to_string(),
                to,
                why_zh: format!("「{low}」→「{high}」更书面（语域升档）"),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{AntiPattern, TemplateSource};

    fn template_with(pattern_en: &str, replacement_en: &str) -> Template {
        Template {
            template_id: "t".into(),
            track: "cet4".into(),
            genre: "argument".into(),
            skeleton: vec![],
            slots: vec![],
            phrases: vec![],
            anti_patterns: vec![AntiPattern {
                pattern_en: pattern_en.into(),
                note_zh: "模板痕迹过重".into(),
                replacement_en: replacement_en.into(),
            }],
            source: TemplateSource::Builtin,
        }
    }

    #[test]
    fn test_upgrade_sentence_whole_sentence_antipattern_has_priority() {
        let templates = vec![template_with(
            "Nowadays with the development of society",
            "In recent years, this issue has aroused wide public concern.",
        )];
        let sentence = "Nowadays with the development of society, we face many problems.";
        let ups = upgrade_sentence(sentence, &templates);
        assert_eq!(ups.len(), 1, "整句命中只回一条");
        assert!(
            ups[0].to.starts_with("In recent years"),
            "整句替换: {}",
            ups[0].to
        );
        assert!(!ups[0].why_zh.is_empty(), "中文理由在场");
    }

    #[test]
    fn test_upgrade_sentence_word_synonym_fallback() {
        let ups = upgrade_sentence("This method is good for everyone.", &[]);
        assert!(!ups.is_empty(), "词级同义命中");
        assert_eq!(ups[0].from, "good");
        assert_eq!(ups[0].to, "beneficial", "good→beneficial（docs/10 示例）");
    }

    #[test]
    fn test_upgrade_sentence_keeps_capitalization_and_reports_empty_when_clean() {
        let ups = upgrade_sentence("Good habits help teenagers.", &[]);
        assert_eq!(ups[0].to, "Beneficial", "句首大写保持");
        let none = upgrade_sentence("We must protect the river basin.", &[]);
        assert!(none.is_empty(), "无命中不臆造");
    }
}
