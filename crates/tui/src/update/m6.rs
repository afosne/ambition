use super::*;

pub(super) fn update_screen(state: &mut AppState, target: Screen) -> Cmd {
    if state.screen == target {
        return match target {
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
}
