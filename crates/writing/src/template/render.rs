//! 套槽成文（`{{slot_id}}` → 学生填值；缺槽保留占位 + tracing::warn 不静默，docs/10 §1 render）

use std::collections::BTreeMap;

use crate::template::*;

/// 单行套槽：替换全部 `{{id}}`；缺槽 → 保留占位并回报（防静默丢内容 + 防死循环止扫）
fn fill_line(line: &str, fills: &BTreeMap<String, String>) -> (String, Vec<String>) {
    let mut out = line.to_string();
    let mut missing = Vec::new();
    while let Some(start) = out.find("{{") {
        let Some(rel_end) = out[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + rel_end;
        let id = &out[start + 2..end];
        match fills.get(id) {
            Some(value) => {
                out.replace_range(start..end + 2, value);
            }
            None => {
                missing.push(id.to_string());
                break; // 占位保留：跳过该槽继续扫后文（不重复报）
            }
        }
    }
    (out, missing)
}

/// 套槽成文（骨架逐段逐行替换；缺槽保留 `{{id}}` 并汇总 warn——写作者一眼可见漏填）
pub fn render(t: &Template, fills: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut missing_all: Vec<String> = Vec::new();
    for para in &t.skeleton {
        for line in &para.lines {
            let (filled, missing) = fill_line(line, fills);
            out.push_str(&filled);
            out.push('\n');
            for id in missing {
                if !missing_all.contains(&id) {
                    missing_all.push(id);
                }
            }
        }
        out.push('\n');
    }
    if !missing_all.is_empty() {
        tracing::warn!(slots = ?missing_all, "template render missing slots, placeholders kept");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Template {
        Template {
            template_id: "t1".into(),
            track: "cet4".into(),
            genre: "argument".into(),
            skeleton: vec![ParaFunc {
                key: "hook".into(),
                label_zh: "引出话题".into(),
                lines: vec![
                    "In recent years, {{topic}} has become a hot issue.".into(),
                    "As far as I am concerned, {{thesis}}".into(),
                ],
            }],
            slots: vec![
                Slot {
                    id: "topic".into(),
                    label_zh: "话题".into(),
                    options_en: vec![],
                },
                Slot {
                    id: "thesis".into(),
                    label_zh: "论点".into(),
                    options_en: vec![],
                },
            ],
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Builtin,
        }
    }

    #[test]
    fn test_fill_line_substitutes_all_slots_and_reports_missing() {
        let mut fills = BTreeMap::new();
        fills.insert("a".to_string(), "AAA".to_string());
        fills.insert("b".to_string(), "BBB".to_string());
        let (line, missing) = fill_line("x {{a}} y {{b}} z", &fills);
        assert_eq!(line, "x AAA y BBB z", "多槽逐个替换");
        assert!(missing.is_empty());

        let (line, missing) = fill_line("keep {{gone}} here", &fills);
        assert_eq!(line, "keep {{gone}} here", "缺槽保留占位");
        assert_eq!(missing, vec!["gone".to_string()], "缺槽回报");
    }

    #[test]
    fn test_render_fills_skeleton_and_keeps_missing_placeholder() {
        let mut fills = BTreeMap::new();
        fills.insert("topic".to_string(), "campus safety".to_string());
        fills.insert("thesis".to_string(), "we should act now".to_string());
        let out = render(&t(), &fills);
        assert!(out.contains("campus safety"), "首槽替换: {out}");
        assert!(out.contains("we should act now"), "次槽替换: {out}");
        assert!(!out.contains("{{topic}}"), "占位已消");
        assert!(!out.contains("{{thesis}}"), "占位已消");

        let empty = render(&t(), &BTreeMap::new());
        assert!(
            empty.contains("{{topic}}"),
            "缺槽保留占位（不静默）: {empty}"
        );
    }

    #[test]
    fn test_render_ignores_unused_fills() {
        let mut fills = BTreeMap::new();
        fills.insert("topic".to_string(), "T".to_string());
        fills.insert("thesis".to_string(), "S".to_string());
        fills.insert("extra".to_string(), "X".to_string());
        let out = render(&t(), &fills);
        assert!(!out.contains('X'), "未在骨架出现的槽不注入");
    }
}
