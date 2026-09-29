//! 离线词库查询与语境消歧（docs/09、docs/11 M6）。

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sense {
    pub word_id: String,
    pub sense_id: u64,
    pub pos: String,
    pub def_zh: String,
    pub def_en: String,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExamplePair {
    pub plain: String,
    pub patterned: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pattern {
    pub pattern_id: String,
    pub form: String,
    pub kind: String,
    #[serde(default)]
    pub level_tags: Vec<String>,
    pub note_zh: String,
    #[serde(default)]
    pub examples: Vec<ExamplePair>,
    #[serde(default)]
    pub traps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextSense {
    pub lemma: String,
    pub senses: Vec<Sense>,
    pub selected: Option<Sense>,
    pub confidence: f32,
}

#[derive(serde::Deserialize)]
struct SenseFile {
    schema_version: u32,
    senses: Vec<Sense>,
}
#[derive(serde::Deserialize)]
struct PatternFile {
    schema_version: u32,
    patterns: Vec<Pattern>,
}

pub const SENSES_SCHEMA_VERSION: u32 = 1;
pub const PATTERNS_SCHEMA_VERSION: u32 = 1;
const SENSES: &str = include_str!("../../../data/lexicon/en/senses.json");
const PATTERNS: &str = include_str!("../../../data/patterns/en/patterns.json");

pub fn parse_senses(text: &str) -> Result<Vec<Sense>, String> {
    let file: SenseFile = serde_json::from_str(text).map_err(|e| format!("senses JSON: {e}"))?;
    if file.schema_version != SENSES_SCHEMA_VERSION || file.senses.is_empty() {
        return Err("义项 schema 版本错误或词表为空".into());
    }
    Ok(file.senses)
}

pub fn parse_patterns(text: &str) -> Result<Vec<Pattern>, String> {
    let file: PatternFile =
        serde_json::from_str(text).map_err(|e| format!("patterns JSON: {e}"))?;
    if file.schema_version != PATTERNS_SCHEMA_VERSION || file.patterns.is_empty() {
        return Err("句式 schema 版本错误或列表为空".into());
    }
    Ok(file.patterns)
}

pub fn builtin_senses() -> Result<Vec<Sense>, String> {
    parse_senses(SENSES)
}
pub fn builtin_patterns() -> Result<Vec<Pattern>, String> {
    parse_patterns(PATTERNS)
}

/// 返回词条全部义项；给出语境时按内置例句 token 重合度选最可能义项。
pub fn lookup(lemma: &str, context: Option<&str>) -> Result<ContextSense, String> {
    let senses: Vec<_> = builtin_senses()?
        .into_iter()
        .filter(|s| s.word_id.eq_ignore_ascii_case(lemma.trim()))
        .collect();
    let (selected, confidence) = context
        .map(|ctx| {
            let lower = ctx.to_ascii_lowercase();
            let tokens: Vec<_> = lower
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|t| t.len() > 2)
                .collect();
            let mut scores: Vec<_> = senses
                .iter()
                .map(|s| {
                    let sample = s.examples.join(" ").to_ascii_lowercase();
                    let hits = tokens.iter().filter(|t| sample.contains(**t)).count();
                    (hits, s.clone())
                })
                .collect();
            scores.sort_by_key(|x| std::cmp::Reverse(x.0));
            if let Some((hits, sense)) = scores.first() {
                (
                    Some(sense.clone()),
                    if *hits == 0 {
                        0.0
                    } else {
                        (*hits as f32 / tokens.len().max(1) as f32).min(1.0)
                    },
                )
            } else {
                (None, 0.0)
            }
        })
        .unwrap_or((None, 0.0));
    Ok(ContextSense {
        lemma: lemma.trim().to_string(),
        senses,
        selected,
        confidence,
    })
}

/// 提取本地词库能识别、且尚未掌握的词条（小写去重、保留文本首次顺序）。
pub fn new_words(text: &str, known: &[String]) -> Result<Vec<String>, String> {
    let known: std::collections::HashSet<_> =
        known.iter().map(|w| w.to_ascii_lowercase()).collect();
    let lexicon: std::collections::HashSet<_> = builtin_senses()?
        .into_iter()
        .map(|s| s.word_id.to_ascii_lowercase())
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for token in text.split(|c: char| !c.is_ascii_alphabetic()) {
        let word = token.to_ascii_lowercase();
        if word.len() > 2
            && lexicon.contains(&word)
            && !known.contains(&word)
            && seen.insert(word.clone())
        {
            out.push(word);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_contextual_selects_matching_builtin_sense() {
        let result = lookup("BANK", Some("We walked beside the river bank.")).expect("查词");
        assert_eq!(result.senses.len(), 3);
        assert_eq!(result.selected.expect("语境义").sense_id, 2);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_new_words_deduped_and_exclude_mastered() {
        let words = new_words("Bank, light and bank.", &["light".into()]).expect("提词");
        assert_eq!(words, vec!["bank"]);
        assert!(new_words("xyzzy", &[]).expect("提词").is_empty());
    }

    #[test]
    fn test_parse_senses_validates_builtin_asset() {
        let senses = builtin_senses().expect("内置义项");
        assert!(!senses.is_empty());
        assert!(parse_senses("{}").is_err());
    }

    #[test]
    fn test_parse_patterns_validates_builtin_asset() {
        let patterns = builtin_patterns().expect("内置句式");
        assert!(!patterns.is_empty());
        assert!(parse_patterns("{}").is_err());
    }

    #[test]
    fn test_builtin_patterns_returns_nonempty_list_with_form() {
        let patterns = builtin_patterns().expect("内置句式");
        assert!(!patterns.is_empty());
        assert!(patterns.iter().all(|p| !p.form.is_empty()));
    }

    #[test]
    fn test_builtin_senses_and_patterns_load_compiled_assets() {
        assert!(!builtin_senses().unwrap().is_empty());
        assert!(!builtin_patterns().unwrap().is_empty());
    }
}
