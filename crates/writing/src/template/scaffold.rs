//! 三阶段脚手架（docs/10 §1.3：Full 句模板 → Half 段落功能提示 → Free 空骨架）

use crate::template::*;

/// 渐退式脚手架视图（纯函数；TUI scaffold 屏据此渲染，最终目标脱模板）
pub fn scaffold(t: &Template, stage: ScaffoldStage) -> ScaffoldView {
    let paragraphs = match stage {
        ScaffoldStage::Full => t
            .skeleton
            .iter()
            .map(|p| ScaffoldPara {
                label_zh: p.label_zh.clone(),
                lines: p.lines.clone(),
            })
            .collect(),
        ScaffoldStage::Half => t
            .skeleton
            .iter()
            .map(|p| ScaffoldPara {
                label_zh: p.label_zh.clone(),
                lines: Vec::new(),
            })
            .collect(),
        ScaffoldStage::Free => Vec::new(),
    };
    ScaffoldView {
        template_id: t.template_id.clone(),
        stage,
        paragraphs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_template() -> Template {
        Template {
            template_id: "t1".into(),
            track: "cet4".into(),
            genre: "argument".into(),
            skeleton: vec![
                ParaFunc {
                    key: "hook".into(),
                    label_zh: "引出话题".into(),
                    lines: vec!["In recent years, {{topic}}.".into(), "{{thesis}}".into()],
                },
                ParaFunc {
                    key: "conclusion".into(),
                    label_zh: "结论".into(),
                    lines: vec!["{{conclusion}}".into()],
                },
            ],
            slots: vec![Slot {
                id: "thesis".into(),
                label_zh: "中心论点".into(),
                options_en: vec!["I hold the view that ...".into()],
            }],
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Builtin,
        }
    }

    #[test]
    fn test_scaffold_full_keeps_sentence_templates_with_placeholders() {
        let view = scaffold(&demo_template(), ScaffoldStage::Full);
        assert_eq!(view.template_id, "t1");
        assert_eq!(view.paragraphs.len(), 2, "骨架段落数保留");
        assert!(
            view.paragraphs[0].lines[0].contains("{{topic}}"),
            "Full 含占位句模板"
        );
        assert_eq!(view.paragraphs[0].label_zh, "引出话题", "段落功能名保留");
    }

    #[test]
    fn test_scaffold_half_hides_lines_but_keeps_labels() {
        let view = scaffold(&demo_template(), ScaffoldStage::Half);
        assert_eq!(view.paragraphs.len(), 2, "半模板仍给段落功能序列");
        assert!(
            view.paragraphs.iter().all(|p| p.lines.is_empty()),
            "半模板隐藏句模板"
        );
        assert_eq!(view.paragraphs[1].label_zh, "结论");
    }

    #[test]
    fn test_scaffold_free_is_empty_skeleton() {
        let view = scaffold(&demo_template(), ScaffoldStage::Free);
        assert!(view.paragraphs.is_empty(), "脱模板 = 空骨架（自由写）");
        assert_eq!(view.stage, ScaffoldStage::Free, "阶段随视图传递");
    }
}
