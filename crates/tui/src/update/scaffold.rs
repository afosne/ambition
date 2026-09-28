//! 模板作文屏状态机（M5-W4b，docs/03 §5.3 scaffold 族）：
//! ↑↓ 选模板 · Tab/Shift+Tab、←/→ 换槽 · Space 三阶段循环 · Enter 成文 · Ctrl+S 自查（副作用回 Cmd）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

use ambition_writing::{ScaffoldCheck, ScaffoldStage, Template};

use crate::*;

/// 当前选中模板的槽位数（无模板 = 0）
fn slot_len(state: &AppState) -> usize {
    state
        .scaffold_templates
        .get(state.scaffold_idx)
        .map(|t| t.slots.len())
        .unwrap_or(0)
}

/// 换模板复位：槽位与新模板 slots 对齐，成文/自查清空（防串模板内容）
fn reset_scaffold(state: &mut AppState) {
    state.scaffold_fills = vec![String::new(); slot_len(state)];
    state.scaffold_slot = 0;
    state.scaffold_result = None;
    state.scaffold_check = None;
    state.error = None;
}

/// 模板屏独立状态机（screen==Scaffold 时由 update 前置拦截；键位路由在 keys.rs）
pub fn scaffold_update(state: &mut AppState, msg: Msg) -> Cmd {
    match msg {
        Msg::Quit => Cmd::Quit,
        Msg::ScaffoldUp => {
            if state.scaffold_idx > 0 {
                state.scaffold_idx -= 1;
                reset_scaffold(state);
            }
            Cmd::None
        }
        Msg::ScaffoldDown => {
            if state.scaffold_idx + 1 < state.scaffold_templates.len() {
                state.scaffold_idx += 1;
                reset_scaffold(state);
            }
            Cmd::None
        }
        Msg::ScaffoldStage => {
            // 三阶段渐退（docs/10 §1.3）：全模板 → 半模板 → 脱模板 → 全模板
            state.scaffold_stage = match state.scaffold_stage {
                ScaffoldStage::Full => ScaffoldStage::Half,
                ScaffoldStage::Half => ScaffoldStage::Free,
                ScaffoldStage::Free => ScaffoldStage::Full,
            };
            Cmd::None
        }
        Msg::ScaffoldSlotNext => {
            let len = slot_len(state);
            if len > 0 {
                state.scaffold_slot = (state.scaffold_slot + 1) % len;
            }
            Cmd::None
        }
        Msg::ScaffoldSlotPrev => {
            let len = slot_len(state);
            if len > 0 {
                state.scaffold_slot = (state.scaffold_slot + len - 1) % len;
            }
            Cmd::None
        }
        Msg::Char(c) => {
            // 字符直接落当前槽（键位总则：字母键只做输入文本，docs/03 §5.3）
            if let Some(field) = state.scaffold_fills.get_mut(state.scaffold_slot) {
                field.push(c);
            }
            Cmd::None
        }
        Msg::Backspace => {
            if let Some(field) = state.scaffold_fills.get_mut(state.scaffold_slot) {
                field.pop();
            }
            Cmd::None
        }
        Msg::ScaffoldRender => Cmd::ScaffoldRender,
        Msg::ScaffoldCheck => {
            if state.scaffold_result.is_none() {
                // 无成文不自查（不伪造通过；错误行可见 P-8）
                state.error = Some("请先 Enter 生成成文，再 Ctrl+S 自查".to_string());
                return Cmd::None;
            }
            Cmd::ScaffoldCheck
        }
        // 其余消息（屏幕切换在 update 顶部拦截；作答类键在模板屏无操作——防误触）
        _ => Cmd::None,
    }
}

/// 装载模板列表回填（main 执行 Cmd::ScaffoldLoad 后调用）
pub fn apply_scaffold_templates(state: &mut AppState, templates: Vec<Template>) {
    state.scaffold_templates = templates;
    state.scaffold_idx = 0;
    state.scaffold_stage = ScaffoldStage::Full;
    reset_scaffold(state);
}

/// 成文回填（main 调 engine.render_template 成功后）
pub fn apply_scaffold_result(state: &mut AppState, text: String) {
    state.scaffold_result = Some(text);
    state.error = None;
}

/// 自查回填（main 调 engine.check_scaffold 成功后）
pub fn apply_scaffold_check(state: &mut AppState, check: ScaffoldCheck) {
    state.scaffold_check = Some(check);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_writing::{Slot, TemplateSource};

    fn template(template_id: &str, slots: usize) -> Template {
        Template {
            template_id: template_id.to_string(),
            track: "cet4".to_string(),
            genre: "argument".to_string(),
            skeleton: vec![],
            slots: (0..slots)
                .map(|i| Slot {
                    id: format!("s{i}"),
                    label_zh: format!("槽{i}"),
                    options_en: vec![],
                })
                .collect(),
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Builtin,
        }
    }

    fn scaffold_state() -> AppState {
        let mut state = new_state(vec![]);
        state.screen = Screen::Scaffold;
        state
    }

    #[test]
    fn test_scaffold_update_cycles_stage_and_routes_cmds() {
        let mut state = scaffold_state();
        assert_eq!(scaffold_update(&mut state, Msg::ScaffoldStage), Cmd::None);
        assert_eq!(state.scaffold_stage, ScaffoldStage::Half, "Space → 半模板");
        scaffold_update(&mut state, Msg::ScaffoldStage);
        assert_eq!(state.scaffold_stage, ScaffoldStage::Free, "→ 脱模板");
        scaffold_update(&mut state, Msg::ScaffoldStage);
        assert_eq!(state.scaffold_stage, ScaffoldStage::Full, "循环回全模板");

        // 无成文不自查（防伪造）；有成文 → Cmd
        assert_eq!(scaffold_update(&mut state, Msg::ScaffoldCheck), Cmd::None);
        assert!(
            state.error.as_deref().unwrap_or_default().contains("Enter"),
            "提示先成文"
        );
        apply_scaffold_result(&mut state, "draft essay".to_string());
        assert_eq!(
            scaffold_update(&mut state, Msg::ScaffoldCheck),
            Cmd::ScaffoldCheck,
            "有成文 → 自查 Cmd"
        );
        assert_eq!(
            scaffold_update(&mut state, Msg::ScaffoldRender),
            Cmd::ScaffoldRender,
            "Enter → 成文 Cmd"
        );
    }

    #[test]
    fn test_scaffold_update_fills_slots_and_navigates() {
        let mut state = scaffold_state();
        apply_scaffold_templates(&mut state, vec![template("t1", 2)]);
        assert_eq!(state.scaffold_fills.len(), 2, "槽位与模板对齐");

        scaffold_update(&mut state, Msg::Char('a'));
        scaffold_update(&mut state, Msg::Char('b'));
        assert_eq!(state.scaffold_fills[0], "ab", "字符落当前槽");
        scaffold_update(&mut state, Msg::ScaffoldSlotNext);
        scaffold_update(&mut state, Msg::Char('x'));
        assert_eq!(state.scaffold_fills[1], "x", "换槽后落第二槽");
        scaffold_update(&mut state, Msg::ScaffoldSlotPrev);
        scaffold_update(&mut state, Msg::Backspace);
        assert_eq!(state.scaffold_fills[0], "a", "回槽退格");
        // 换槽越界环绕
        scaffold_update(&mut state, Msg::ScaffoldSlotNext);
        scaffold_update(&mut state, Msg::ScaffoldSlotNext);
        assert_eq!(state.scaffold_slot, 0, "槽位环绕");
        // ↑↓ 换模板复位
        apply_scaffold_templates(&mut state, vec![template("t1", 2), template("t2", 3)]);
        scaffold_update(&mut state, Msg::Char('z'));
        scaffold_update(&mut state, Msg::ScaffoldDown);
        assert_eq!(state.scaffold_idx, 1, "下移模板");
        assert_eq!(state.scaffold_fills.len(), 3, "新模板槽位");
        assert!(
            state.scaffold_fills.iter().all(String::is_empty),
            "换模板清空输入"
        );
        scaffold_update(&mut state, Msg::ScaffoldUp);
        assert_eq!(state.scaffold_idx, 0, "上移模板");
    }

    #[test]
    fn test_reset_scaffold_clears_inputs_result_and_error() {
        let mut state = scaffold_state();
        apply_scaffold_templates(&mut state, vec![template("t1", 2)]);
        scaffold_update(&mut state, Msg::Char('x'));
        apply_scaffold_result(&mut state, "draft".to_string());
        state.error = Some("旧错误".to_string());
        reset_scaffold(&mut state);
        assert!(state.scaffold_fills.iter().all(String::is_empty), "清输入");
        assert_eq!(state.scaffold_slot, 0, "槽位归零");
        assert!(state.scaffold_result.is_none(), "清成文");
        assert!(state.error.is_none(), "清错误行");
    }

    #[test]
    fn test_slot_len_reads_selected_template_slots() {
        let mut state = scaffold_state();
        assert_eq!(slot_len(&state), 0, "无模板 → 0");
        apply_scaffold_templates(&mut state, vec![template("t1", 2), template("t2", 5)]);
        assert_eq!(slot_len(&state), 2, "读选中模板");
        state.scaffold_idx = 1;
        assert_eq!(slot_len(&state), 5, "游标切换生效");
        state.scaffold_idx = 99;
        assert_eq!(slot_len(&state), 0, "越界防御 → 0");
    }

    #[test]
    fn test_apply_scaffold_templates_loads_first_as_full_stage() {
        let mut state = scaffold_state();
        state.scaffold_stage = ScaffoldStage::Free;
        apply_scaffold_templates(&mut state, vec![template("t1", 2)]);
        assert_eq!(state.scaffold_templates.len(), 1);
        assert_eq!(state.scaffold_idx, 0, "回到首模板");
        assert_eq!(state.scaffold_stage, ScaffoldStage::Full, "进屏复位全模板");
    }

    #[test]
    fn test_apply_scaffold_result_sets_text_and_clears_error() {
        let mut state = scaffold_state();
        state.error = Some("旧错误".to_string());
        apply_scaffold_result(&mut state, "hello essay".to_string());
        assert_eq!(state.scaffold_result.as_deref(), Some("hello essay"));
        assert!(state.error.is_none(), "成文成功清错误行");
    }

    #[test]
    fn test_apply_scaffold_check_sets_check_result() {
        let mut state = scaffold_state();
        apply_scaffold_check(
            &mut state,
            ScaffoldCheck {
                meaning_kept: 0.9,
                fits_prompt: 0.8,
                passed: true,
            },
        );
        let check = state.scaffold_check.expect("自查结果在位");
        assert!(check.passed);
        assert!((check.meaning_kept - 0.9).abs() < 1e-5);
    }
}
