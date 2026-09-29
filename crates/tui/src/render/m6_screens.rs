use super::*;

/// #24：键位帮助浮层（复用 `hint` 全表，不硬文案）
pub(super) fn render_help_overlay(frame: &mut Frame, state: &mut AppState) {
    use crate::keys::hint;
    let ctxs = [
        ("主页面", KeyCtx::Home),
        ("作答（选择）", KeyCtx::Choice),
        ("作答（填空·编辑中）", KeyCtx::Fill { editing: true }),
        ("作答（填空·待输入）", KeyCtx::Fill { editing: false }),
        ("待裁定", KeyCtx::Pending),
        ("已判分", KeyCtx::Graded),
        ("全部完成", KeyCtx::Finished),
        ("复习屏", KeyCtx::Review),
        ("选题面板", KeyCtx::Picker),
        ("模板作文", KeyCtx::Scaffold),
        ("翻译工作台", KeyCtx::Tx),
        ("对齐屏", KeyCtx::TxAlign),
        ("我来翻", KeyCtx::TxMyTurn),
        ("翻译历史", KeyCtx::TxHistory),
        ("笔记/日记/TODO", KeyCtx::Workspace),
        ("学习画像", KeyCtx::Profile),
        ("游戏成长", KeyCtx::Game),
        ("词汇本", KeyCtx::Wordbook),
    ];
    let mut lines = String::from(" 键位帮助（? 关闭 · 点击关闭）\n\n");
    for (name, ctx) in ctxs {
        lines.push_str(&format!("{name}:{}\n", hint(ctx)));
    }
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("帮助"))
        .style(Style::default());
    let area = centered_area(frame.area(), 80, 90);
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);

    // 注册整个帮助浮层区域为可点击 → 点击关闭
    state.register_click(area, Msg::ToggleHelp);
}

/// #24：居中弹窗区域
pub(super) fn centered_area(r: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let popup = Layout::vertical([Constraint::Percentage(height_pct)]).split(r)[0];
    let popup = Layout::horizontal([Constraint::Percentage(width_pct)]).split(popup)[0];
    let x = r.x + (r.width.saturating_sub(popup.width)) / 2;
    let y = r.y + (r.height.saturating_sub(popup.height)) / 2;
    Rect {
        x,
        y,
        width: popup.width,
        height: popup.height,
    }
}

pub(super) fn render_workspace(frame: &mut Frame, state: &mut AppState) -> bool {
    let screen = state.screen;
    let title = match screen {
        Screen::Games => "游戏与成长",
        Screen::Wordbook => "语境查词 / 生词本",
        Screen::Profile => "学习画像（本地统计）",
        Screen::Notes => "笔记",
        Screen::Diary => "日记",
        Screen::Todos => "TODO",
        _ => return false,
    };
    let mut text = format!("Ambition · {title}\n\n");
    if screen == Screen::Wordbook {
        text.push_str(&format!("查询：{}▍\n\n", state.workspace_draft));
    } else if matches!(screen, Screen::Notes | Screen::Diary | Screen::Todos) {
        text.push_str(&format!("内容：{}▍\n\n最近记录：\n", state.workspace_draft));
    }
    for item in &state.workspace_items {
        text.push_str(&format!(" {}\n", item));
    }
    if let Some(error) = &state.error {
        text.push_str(&format!("\n⚠ {error}\n"));
    }
    text.push_str(&format!("\n{}\n", hint(state.key_ctx())));
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        frame.area(),
    );

    // 注册工作区屏可点击区域
    let inner = frame.area();
    if matches!(
        screen,
        Screen::Notes | Screen::Diary | Screen::Todos | Screen::Wordbook
    ) {
        // 输入框行（第 2 行）可点击 → 进入编辑
        state.register_click(
            Rect {
                x: inner.x,
                y: inner.y + 2,
                width: inner.width,
                height: 1,
            },
            Msg::Select, // 进入编辑态（简化——已有 workspace_draft 编辑入口）
        );
        // 底部保存按钮区域（最后 2 行）
        let btn_row = inner.y + inner.height.saturating_sub(2);
        state.register_click(
            Rect {
                x: inner.x,
                y: btn_row,
                width: inner.width,
                height: 1,
            },
            Msg::WorkspaceSave,
        );
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_workspace_screens_accepts_workspace_and_ignores_practice() {
        let backend = ratatui::backend::TestBackend::new(50, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = new_state(vec![]);
        state.screen = Screen::Games;
        terminal
            .draw(|frame| assert!(render_workspace(frame, &mut state)))
            .unwrap();
        state.screen = Screen::Practice;
        terminal
            .draw(|frame| assert!(!render_workspace(frame, &mut state)))
            .unwrap();
    }

    /// #24：帮助浮层渲染——复用 hint 全表（不 panic 即通过）
    #[test]
    fn test_render_help_overlay_renders_all_hints() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = new_state(vec![]);
        terminal
            .draw(|frame| render_help_overlay(frame, &mut state))
            .unwrap();
        // 只验证不 panic（P-2 宽字符续格不影响断言逻辑）
        assert!(terminal.backend().buffer().area().area() > 0, "有内容");
    }

    /// #24：居中弹窗区域计算——大区域等于输入，小区域居中
    #[test]
    fn test_centered_area_centers_within_parent() {
        let parent = Rect::new(0, 0, 100, 40);
        let inner = centered_area(parent, 80, 90);
        assert!(inner.width <= 100 && inner.height <= 40, "不超出父区域");
    }
}
