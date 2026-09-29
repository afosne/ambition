use super::*;

pub(super) fn update_screen(state: &mut AppState, target: Screen) -> Cmd {
    if state.screen == target {
        return match target {
            Screen::Review => Cmd::ReviewLoad,
            Screen::Profile => Cmd::ProfileLoad,
            Screen::Games => Cmd::GameLoad,
            Screen::Wordbook => Cmd::VocabLoad,
            _ => Cmd::None,
        };
    }
    state.screen = target;
    if target == Screen::Review {
        Cmd::ReviewLoad
    } else if target == Screen::Scaffold {
        Cmd::ScaffoldLoad
    } else if matches!(target, Screen::Notes | Screen::Diary | Screen::Todos) {
        Cmd::WorkspaceLoad
    } else if target == Screen::Profile {
        Cmd::ProfileLoad
    } else if target == Screen::Games {
        Cmd::GameLoad
    } else if target == Screen::Wordbook {
        Cmd::VocabLoad
    } else {
        if target == Screen::Tx {
            tx::reset_tx(state);
        }
        Cmd::None
    }
}

pub(super) fn update_m6(state: &mut AppState, msg: Msg) -> Option<Cmd> {
    // ? 键位帮助浮层开/关（#24；任何上下文可用，非字母键不违反禁单字母总则）
    if matches!(msg, Msg::ToggleHelp) {
        state.help_open = !state.help_open;
        return Some(Cmd::None);
    }
    // F5 讲解浮层开/关（#17 讲解层①②；判分后可用，任何屏幕都可切换）
    if matches!(msg, Msg::ToggleExplain) {
        state.explain_open = !state.explain_open;
        return Some(Cmd::None);
    }
    if matches!(state.screen, Screen::Notes | Screen::Diary | Screen::Todos) {
        return Some(match msg {
            Msg::Quit => Cmd::Quit,
            Msg::Char(c) => {
                state.workspace_draft.push(c);
                Cmd::None
            }
            Msg::Backspace => {
                state.workspace_draft.pop();
                Cmd::None
            }
            Msg::WorkspaceSave if !state.workspace_draft.trim().is_empty() => {
                Cmd::WorkspaceSave(state.workspace_draft.clone())
            }
            Msg::WorkspaceSave => {
                state.error = Some("内容不能为空".into());
                Cmd::None
            }
            _ => Cmd::None,
        });
    }
    if state.screen == Screen::Profile {
        return Some(if matches!(msg, Msg::Quit) {
            Cmd::Quit
        } else {
            Cmd::None
        });
    }
    if state.screen == Screen::Games {
        return Some(match msg {
            Msg::Quit => Cmd::Quit,
            Msg::GameAction(action) => Cmd::GameAction(action),
            _ => Cmd::None,
        });
    }
    if state.screen == Screen::Wordbook {
        return Some(match msg {
            Msg::Quit => Cmd::Quit,
            Msg::Char(c) => {
                state.workspace_draft.push(c);
                Cmd::None
            }
            Msg::Backspace => {
                state.workspace_draft.pop();
                Cmd::None
            }
            Msg::VocabLookup if !state.workspace_draft.trim().is_empty() => {
                Cmd::VocabLookup(state.workspace_draft.clone())
            }
            Msg::VocabEnroll if !state.workspace_draft.trim().is_empty() => {
                Cmd::VocabEnroll(state.workspace_draft.clone())
            }
            _ => Cmd::None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_m6_edits_note_draft_and_dispatches_save() {
        let mut state = new_state(vec![]);
        state.screen = Screen::Notes;
        assert_eq!(update_m6(&mut state, Msg::Char('x')), Some(Cmd::None));
        assert_eq!(
            update_m6(&mut state, Msg::WorkspaceSave),
            Some(Cmd::WorkspaceSave("x".into()))
        );
    }

    #[test]
    fn test_update_screen_loads_profile_and_resets_tx() {
        let mut state = new_state(vec![]);
        assert_eq!(update_screen(&mut state, Screen::Profile), Cmd::ProfileLoad);
        assert_eq!(update_screen(&mut state, Screen::Profile), Cmd::ProfileLoad);
    }

    /// #25：Review 屏同屏重载——done 态按 Ctrl+R 同屏触发 ReviewLoad（不再 None）
    #[test]
    fn test_update_screen_review_reload_on_same_screen() {
        let mut state = new_state(vec![]);
        state.screen = Screen::Review;
        state.review.loaded = true;
        state.review.idx = state.review.cards.len(); // done 状态
        // 同屏再按 Ctrl+R → 重载队列（不再忽略）
        assert_eq!(
            update_screen(&mut state, Screen::Review),
            Cmd::ReviewLoad,
            "Review 同屏应触发 ReviewLoad（#25）"
        );
    }
}
