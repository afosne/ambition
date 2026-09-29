//! 模板作文屏渲染（M5-W4b，docs/03 §5.3 scaffold 族）：屏顶模板列表 · 中部三阶段视图 · 槽位输入 · 屏底成文/自查
//! 从 render.rs 拆出（docs/01 §组织：文件 ≤500 行）

use super::*;

/// 模板作文屏（列表光标 + 阶段视图 + 槽位编辑行 + 成文/自查浮层）
pub fn render_scaffold(frame: &mut Frame, state: &mut AppState) {
    let mut text =
        String::from(" 模板作文（Esc 返回 · Space 切阶段 · Enter 成文 · Ctrl+S 自查）\n\n");
    let idx = state
        .scaffold_idx
        .min(state.scaffold_templates.len().saturating_sub(1));
    if state.scaffold_templates.is_empty() {
        text.push_str(" 模板装载中/为空（内置模板随 seed 题库可用）。\n");
    } else {
        for (i, t) in state.scaffold_templates.iter().enumerate() {
            let mark = if i == idx { "❯" } else { " " };
            text.push_str(&format!(
                " {} {:>2}. [{} · {}] {}\n",
                mark,
                i + 1,
                t.track,
                t.genre,
                t.template_id
            ));
        }
        if let Some(t) = state.scaffold_templates.get(idx) {
            // 三阶段视图（scaffold() 纯函数；随 Space 即时切换）
            let view = ambition_writing::scaffold(t, state.scaffold_stage);
            let stage_label = match state.scaffold_stage {
                ambition_writing::ScaffoldStage::Full => "全模板（句模板照抄替换）",
                ambition_writing::ScaffoldStage::Half => "半模板（段落功能提示）",
                ambition_writing::ScaffoldStage::Free => "脱模板（自由写）",
            };
            text.push_str(&format!("\n 阶段: {}\n", stage_label));
            for para in &view.paragraphs {
                text.push_str(&format!("  · {}\n", para.label_zh));
                for line in &para.lines {
                    text.push_str(&format!("      {}\n", line));
                }
            }
            if view.paragraphs.is_empty() {
                text.push_str("  （空骨架：按题目要求自由写，注意字数与时限）\n");
            }
            // 槽位输入行（与填空同形：❯ 当前槽 + ▍ 光标）
            for (i, slot) in t.slots.iter().enumerate() {
                let cursor = if state.scaffold_slot == i { "❯" } else { " " };
                let field = state
                    .scaffold_fills
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("");
                let caret = if state.scaffold_slot == i { "▍" } else { " " };
                text.push_str(&format!(
                    "\n  {} 槽位「{}」: {}{}",
                    cursor, slot.label_zh, field, caret
                ));
            }
            if !t.slots.is_empty() {
                text.push('\n');
            }
        }
    }
    // 成文预览（Enter 回填）
    if let Some(result) = &state.scaffold_result {
        text.push_str(&format!("\n 成文:\n{}\n", result));
    }
    // 自查结果（Ctrl+S 回填；分项原样展示提示哪边不过）
    if let Some(check) = &state.scaffold_check {
        let mark = if check.passed {
            "✓ 通过"
        } else {
            "✗ 未过"
        };
        text.push_str(&format!(
            "\n 自查 {}（原意 {:.2} · 贴合 {:.2}，≥0.5 过线）\n",
            mark, check.meaning_kept, check.fits_prompt
        ));
    }
    if let Some(err) = &state.error {
        text.push_str(&format!("\n ⚠ {err}\n"));
    }
    text.push_str(&format!("\n{}\n", hint(state.key_ctx())));

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title("Ambition, 俺必胜！".bold())
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center),
        )
        .style(Style::default());
    frame.render_widget(Clear, frame.area());
    frame.render_widget(paragraph, frame.area());
}

#[cfg(test)]
mod tests {
    use ambition_writing::{Slot, Template, TemplateSource};

    use super::*;

    fn template(template_id: &str) -> Template {
        Template {
            template_id: template_id.to_string(),
            track: "cet4".to_string(),
            genre: "argument".to_string(),
            skeleton: vec![ambition_writing::ParaFunc {
                key: "hook".into(),
                label_zh: "引入与中心论点".into(),
                lines: vec!["In recent years, {{topic}}.".into()],
            }],
            slots: vec![Slot {
                id: "topic".into(),
                label_zh: "话题".into(),
                options_en: vec!["campus safety".into()],
            }],
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Builtin,
        }
    }

    fn loaded_state() -> AppState {
        let mut state = new_state(vec![]);
        state.screen = Screen::Scaffold;
        state.scaffold_templates = vec![template("t1"), template("t2")];
        state.scaffold_fills = vec!["campus safety".to_string()];
        state
    }

    #[test]
    fn test_render_scaffold_shows_list_stage_slots_and_hint() {
        let mut state = loaded_state();
        let out = crate::common::render_sized(&mut state, 76, 28);
        assert!(out.contains("模板作文"), "标题: {out}");
        assert!(out.contains("t1") && out.contains("t2"), "模板列表: {out}");
        assert!(out.contains("❯"), "列表光标: {out}");
        assert!(out.contains("全模板"), "阶段标签: {out}");
        assert!(out.contains("槽位「话题」"), "槽位编辑行: {out}");
        assert!(out.contains("campus safety"), "已填槽内容: {out}");
        assert!(out.contains("Space 切阶段"), "键位提示与路由同源: {out}");
    }

    #[test]
    fn test_render_scaffold_shows_result_check_and_empty_list() {
        let mut state = loaded_state();
        crate::update::apply_scaffold_result(&mut state, "My draft essay body.".to_string());
        crate::update::apply_scaffold_check(
            &mut state,
            ambition_writing::ScaffoldCheck {
                meaning_kept: 0.92,
                fits_prompt: 0.31,
                passed: false,
            },
        );
        state.error = Some("请先 Enter 生成成文".to_string());
        let out = crate::common::render_sized(&mut state, 76, 28);
        assert!(out.contains("成文:"), "成文预览: {out}");
        assert!(out.contains("My draft essay body."), "成文内容: {out}");
        assert!(out.contains("✗ 未过"), "自查未过标记: {out}");
        assert!(out.contains("原意 0.92"), "分项原意: {out}");
        assert!(out.contains("贴合 0.31"), "分项贴合: {out}");
        assert!(out.contains("⚠"), "错误行可见: {out}");

        let mut empty = new_state(vec![]);
        empty.screen = Screen::Scaffold;
        let out = crate::common::render_sized(&mut empty, 76, 28);
        assert!(out.contains("模板装载中/为空"), "空态: {out}");
    }
}
