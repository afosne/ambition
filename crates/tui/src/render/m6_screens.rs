use super::*;

pub(super) fn render_workspace(frame: &mut Frame, state: &AppState) -> bool {
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
            .draw(|frame| assert!(render_workspace(frame, &state)))
            .unwrap();
        state.screen = Screen::Practice;
        terminal
            .draw(|frame| assert!(!render_workspace(frame, &state)))
            .unwrap();
    }
}
