//! 内置模板库（`data/templates/*.json` include_str 内嵌——内容资源按 data/ 分层，docs/16 §3；
//! 运行时无文件读取。个人模板在 SQLite `templates` 表，W4b 经 engine 合并列表）

use std::sync::OnceLock;

use crate::template::*;

/// 内置模板源文件（增模板在此挂一行；template_id = 文件名）
const BUILTIN: &[(&str, &str)] = &[
    (
        "cet4_argument_procon",
        include_str!("../../../../data/templates/cet4_argument_procon.json"),
    ),
    (
        "gaokao_letter_invite",
        include_str!("../../../../data/templates/gaokao_letter_invite.json"),
    ),
];

/// 解析后的内置库（单例；坏文件 tracing::warn 跳过不阻断——同 rubric/语料口径 P-8）
pub fn builtin_templates() -> &'static Vec<Template> {
    static CACHE: OnceLock<Vec<Template>> = OnceLock::new();
    CACHE.get_or_init(|| {
        BUILTIN
            .iter()
            .filter_map(|(id, json)| match serde_json::from_str::<Template>(json) {
                Ok(mut template) => {
                    template.source = TemplateSource::Builtin;
                    Some(template)
                }
                Err(err) => {
                    tracing::warn!(error = %err, id, "template asset parse failed, skipped");
                    None
                }
            })
            .collect()
    })
}

/// 按 id 查内置模板（未知/坏文件 → None，调用方回退列表首项或报缺）
pub fn builtin(id: &str) -> Option<&'static Template> {
    builtin_templates().iter().find(|t| t.template_id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_builtin_templates_load_with_consistent_shape() {
        let templates = builtin_templates();
        assert_eq!(templates.len(), BUILTIN.len(), "全部内置文件可解析");
        for t in templates.iter() {
            assert!(!t.skeleton.is_empty(), "{} 骨架非空", t.template_id);
            assert!(t.source == TemplateSource::Builtin, "内置来源标记");
            // 槽位一致性：lines 里的每个 {{id}} 必须有 Slot 定义（防渲染漏填无提示）
            let defined: Vec<&str> = t.slots.iter().map(|s| s.id.as_str()).collect();
            let mut seen: Vec<String> = Vec::new();
            for para in &t.skeleton {
                for line in &para.lines {
                    let mut rest = line.as_str();
                    while let Some(start) = rest.find("{{") {
                        let Some(rel_end) = rest[start + 2..].find("}}") else {
                            break;
                        };
                        let id = &rest[start + 2..start + 2 + rel_end];
                        assert!(
                            defined.contains(&id),
                            "{} 槽 {{{{{id}}}}} 未定义",
                            t.template_id
                        );
                        assert!(
                            !seen.contains(&id.to_string()),
                            "{} 槽 id 不重复",
                            t.template_id
                        );
                        seen.push(id.to_string());
                        rest = &rest[start + 2 + rel_end + 2..];
                    }
                }
            }
            assert!(!seen.is_empty(), "{} 至少一个槽", t.template_id);
            // 权重无关：短语/反模板可为空但结构在
            assert!(
                t.anti_patterns
                    .iter()
                    .all(|a| !a.pattern_en.is_empty() && !a.replacement_en.is_empty()),
                "{} 反模板含替换句",
                t.template_id
            );
        }
    }

    #[test]
    fn test_builtin_lookup_finds_and_misses() {
        let t = builtin("cet4_argument_procon").expect("内置四六级议论模板");
        assert_eq!(t.track, "cet4");
        assert_eq!(t.genre, "argument");
        assert!(builtin("no-such-template").is_none(), "未知 id → None");
    }

    #[test]
    fn test_builtin_template_renders_with_demo_fills() {
        let t = builtin("gaokao_letter_invite").expect("内置高考书信模板");
        let mut fills = BTreeMap::new();
        for slot in &t.slots {
            fills.insert(slot.id.clone(), slot.options_en[0].clone());
        }
        let out = crate::template::render(t, &fills);
        assert!(!out.contains("{{"), "全槽填充无残留占位: {out}");
        assert!(out.contains("Dear "), "书信称呼成文: {out}");
    }
}
