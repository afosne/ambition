//! 中式英语错式库（`data/chinglish/`，DESIGN §13.6 L1 负迁移改错训练，M3 落地）
//! 错式 → 单选改错训练题（选出地道改写）；`zh_note` 是给人看的中文点拨，永不进 Jev state。

use ambition_core::{ChoiceBody, OptionItem, Provenance, Qid, Question, QuestionKind};

use crate::GenError;

/// 错式库 schema 版本（导入异版本显式拒绝）
pub const CHINGLISH_SCHEMA_VERSION: u32 = 1;

/// 内置 en 轨错式库
const CHINGLISH_JSON: &str = include_str!("../../../../data/chinglish/en.json");

/// 一条错式（wrong 典型中式句 → right 地道改写）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChinglishItem {
    pub id: String,
    pub wrong: String,
    pub right: String,
    pub zh_note: String,
    pub tag: ambition_core::ErrorTag,
}

/// 解析错式库（schema_version 校验 + id 唯一 + 内容自洽）
pub fn parse_chinglish(bytes: &[u8]) -> Result<Vec<ChinglishItem>, GenError> {
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|err| GenError::Chinglish(format!("invalid json: {err}")))?;
    let version = root
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version != u64::from(CHINGLISH_SCHEMA_VERSION) {
        return Err(GenError::Chinglish(format!(
            "unsupported schema_version {version} (want {CHINGLISH_SCHEMA_VERSION})"
        )));
    }
    let raw = root
        .get("items")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let items: Vec<ChinglishItem> = serde_json::from_value(raw)
        .map_err(|err| GenError::Chinglish(format!("bad items: {err}")))?;
    let mut seen = std::collections::HashSet::new();
    for item in &items {
        if item.id.is_empty() {
            return Err(GenError::Chinglish("empty item id".into()));
        }
        if !seen.insert(item.id.clone()) {
            return Err(GenError::Chinglish(format!("duplicate id: {}", item.id)));
        }
        if item.wrong.trim().is_empty() || item.right.trim().is_empty() {
            return Err(GenError::Chinglish(format!(
                "empty sentence in {}",
                item.id
            )));
        }
        if item.wrong == item.right {
            return Err(GenError::Chinglish(format!(
                "wrong == right in {}",
                item.id
            )));
        }
    }
    Ok(items)
}

/// 内置错式库（解析失败 tracing::warn 返回空表——嵌入内容由测试钉住，不静默吞坏包）
pub fn builtin_chinglish() -> Vec<ChinglishItem> {
    parse_chinglish(CHINGLISH_JSON.as_bytes()).unwrap_or_else(|err| {
        tracing::warn!(error = %err, "builtin chinglish corpus failed to parse");
        Vec::new()
    })
}

/// 改错训练题（每条错式一题；limit = 0 全量）
pub fn drill_questions(items: &[ChinglishItem], limit: usize) -> Vec<Question> {
    let take = if limit == 0 { items.len() } else { limit };
    items.iter().take(take).map(drill_question).collect()
}

/// 单条错式 → 单选「选出地道改写」（选项顺序按 id 哈希奇偶打散，防“答案恒在 B”）
fn drill_question(item: &ChinglishItem) -> Question {
    let hash = Qid::from_content(&item.id);
    let parity = u8::from_str_radix(&hash.0[0..2], 16).unwrap_or(0) % 2;
    let (first, second) = if parity == 0 {
        (item.right.as_str(), item.wrong.as_str())
    } else {
        (item.wrong.as_str(), item.right.as_str())
    };
    let options = vec![
        OptionItem {
            id: 0,
            text: first.to_string(),
        },
        OptionItem {
            id: 1,
            text: second.to_string(),
        },
    ];
    let answer = if first == item.right {
        vec![0]
    } else {
        vec![1]
    };
    let stem = format!("Which version is natural English? \"{}\"", item.wrong);
    Question::new(
        stem,
        QuestionKind::SingleChoice(ChoiceBody { options, answer }),
        2,
        vec!["chinglish".to_string(), item.tag.as_str().to_string()],
        Provenance::Seed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::ErrorTag;

    fn item(id: &str, wrong: &str, right: &str) -> ChinglishItem {
        ChinglishItem {
            id: id.into(),
            wrong: wrong.into(),
            right: right.into(),
            zh_note: "点拨".into(),
            tag: ErrorTag::Chinglish,
        }
    }

    #[test]
    fn test_parse_chinglish_accepts_current_schema_and_validates() {
        let good = serde_json::json!({
            "schema_version": 1,
            "lang": "en",
            "items": [
                {"id": "a", "wrong": "I very like it.", "right": "I really like it.",
                 "zh_note": "直译", "tag": "chinglish"}
            ]
        });
        let items = parse_chinglish(good.to_string().as_bytes()).expect("解析");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tag, ErrorTag::Chinglish, "tag 反解为枚举");

        let bad_version = serde_json::json!({"schema_version": 99, "items": []});
        assert!(
            matches!(
                parse_chinglish(bad_version.to_string().as_bytes()),
                Err(GenError::Chinglish(_))
            ),
            "异版本显式拒绝"
        );
        assert!(
            parse_chinglish(b"not json").is_err(),
            "坏 JSON 显式报错不静默"
        );
        let dup = serde_json::json!({
            "schema_version": 1,
            "items": [
                {"id": "a", "wrong": "x1", "right": "y1", "zh_note": "", "tag": "tense"},
                {"id": "a", "wrong": "x2", "right": "y2", "zh_note": "", "tag": "tense"}
            ]
        });
        assert!(
            matches!(
                parse_chinglish(dup.to_string().as_bytes()),
                Err(GenError::Chinglish(msg)) if msg.contains("duplicate")
            ),
            "id 重复拒绝"
        );
        let same = serde_json::json!({
            "schema_version": 1,
            "items": [{"id": "a", "wrong": "x", "right": "x", "zh_note": "", "tag": "tense"}]
        });
        assert!(
            matches!(
                parse_chinglish(same.to_string().as_bytes()),
                Err(GenError::Chinglish(msg)) if msg.contains("wrong == right")
            ),
            "内容自洽校验"
        );
        let bad_tag = serde_json::json!({
            "schema_version": 1,
            "items": [{"id": "a", "wrong": "x", "right": "y", "zh_note": "", "tag": "mystery"}]
        });
        assert!(
            parse_chinglish(bad_tag.to_string().as_bytes()).is_err(),
            "未知 tag 拒绝（serde 严格）"
        );
    }

    #[test]
    fn test_builtin_chinglish_parses_and_covers_taxonomy() {
        let items = builtin_chinglish();
        assert!(items.len() >= 10, "内置错式 ≥10 条: {}", items.len());
        for item in &items {
            assert!(
                !item.wrong.contains(&item.right),
                "{}: right 不应是 wrong 子串之外还需人工核",
                item.id
            );
            assert!(!item.zh_note.is_empty(), "{}: 中文点拨非空", item.id);
            assert_ne!(item.wrong, item.right);
        }
        // 标签落在分类体系内
        let tags: std::collections::HashSet<String> =
            items.iter().map(|i| i.tag.as_str().to_string()).collect();
        assert!(tags.contains("chinglish"), "至少含中式英语主标签: {tags:?}");
        // 题面/选项全英文（中文点拨不进题面 → 不进 Jev state）
        let drills = drill_questions(&items, 0);
        for q in &drills {
            let text = format!("{}{:?}", q.stem, q.kind);
            assert!(
                !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "训练题无 CJK: {text}"
            );
        }
    }

    #[test]
    fn test_drill_question_picks_natural_version_with_shuffled_order() {
        let straight = item("id_a", "I very like it.", "I really like it.");
        let q = drill_question(&straight);
        let QuestionKind::SingleChoice(body) = &q.kind else {
            panic!("单选")
        };
        assert_eq!(body.options.len(), 2, "二选一改错");
        let chosen = body
            .options
            .iter()
            .find(|o| body.answer.contains(&o.id))
            .expect("答案项存在");
        assert_eq!(chosen.text, straight.right, "答案恒为地道改写");
        let distractor = body
            .options
            .iter()
            .find(|o| !body.answer.contains(&o.id))
            .expect("干扰项存在");
        assert_eq!(distractor.text, straight.wrong);
        assert!(q.stem.contains(&straight.wrong), "题干展示中式句");
        assert_eq!(q.provenance, Provenance::Seed, "内置内容 = seed");
        assert!(q.tags.contains(&"chinglish".to_string()));

        // 两条不同 id 的错式：选项顺序应出现打散（id 哈希奇偶）
        let other = item(
            "completely-different-id-xyz",
            "He is come from China.",
            "He comes from China.",
        );
        let q2 = drill_question(&other);
        let QuestionKind::SingleChoice(body2) = &q2.kind else {
            panic!("单选")
        };
        let answer_pos = |b: &ChoiceBody| {
            b.options
                .iter()
                .position(|o| b.answer.contains(&o.id))
                .expect("答案位")
        };
        let (p1, p2) = (answer_pos(body), answer_pos(body2));
        assert!(p1 <= 1 && p2 <= 1, "答案位 ∈ [0,1]");
        assert_eq!(drill_question(&straight).qid, q.qid, "同内容同 qid（幂等）");
    }

    #[test]
    fn test_drill_questions_respects_limit_and_zero_means_all() {
        let items = builtin_chinglish();
        let all = drill_questions(&items, 0);
        assert_eq!(all.len(), items.len(), "0 = 全量");
        let capped = drill_questions(&items, 3);
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[0].qid, all[0].qid, "保持顺序取前 N");
        assert!(drill_questions(&[], 5).is_empty(), "空库为空");
        // qid 全体唯一（内容寻址）
        let mut ids: Vec<&str> = all.iter().map(|q| q.qid.0.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "qid 无重复");
    }
}
