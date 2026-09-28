//! 翻译工作台状态机（M5-W7，docs/11 §2/§5、docs/03 §5.3 Ctrl+W）：输入/选屏/收藏/转练习 + 回填口
//!
//! 由 update() 在 `screen ∈ Tx*` 时前置拦截（与 scaffold 同位：先于 finished 守卫）；
//! 异步副作用统一回 Cmd，main 按 docs/03 translate_* 门面执行后回填。

use crate::*;

/// 工作台翻译参数（缺省中→英 + Neutral 语域；多语与语域切换随 16-config stages，docs/11 §2）
pub fn tx_opts() -> ambition_translate::TranslateOpts {
    ambition_translate::TranslateOpts::new(
        ambition_core::LangTag(ambition_core::DEFAULT_L1.to_string()),
        ambition_core::LangTag::english(),
    )
}

/// 工作台族状态机（Msg → Cmd；答题族消息与屏幕切换已在 update 顶部处理，这里不串台）
pub fn tx_update(state: &mut AppState, msg: Msg) -> Cmd {
    match msg {
        Msg::Quit => Cmd::Quit,
        Msg::Char(c) => {
            match state.screen {
                Screen::Tx => state.tx_source.push(c),
                Screen::TxMyTurn => state.tx_mine.push(c),
                _ => {}
            }
            Cmd::None
        }
        Msg::Backspace => {
            match state.screen {
                Screen::Tx => {
                    state.tx_source.pop();
                }
                Screen::TxMyTurn => {
                    state.tx_mine.pop();
                }
                _ => {}
            }
            Cmd::None
        }
        Msg::TxTranslate => {
            if state.tx_source.trim().is_empty() {
                state.error = Some("请先输入要翻译的文本".to_string());
                return Cmd::None;
            }
            Cmd::TxTranslate
        }
        Msg::TxAlign => {
            if state.tx_source.trim().is_empty() {
                state.error = Some("请先输入源文，再请求对齐".to_string());
                return Cmd::None;
            }
            Cmd::TxAlign
        }
        Msg::TxMyTurnOpen => {
            if state.tx_source.trim().is_empty() {
                state.error = Some("请先出译，再进入我来翻".to_string());
                return Cmd::None;
            }
            state.tx_mine.clear();
            state.screen = Screen::TxMyTurn;
            Cmd::None
        }
        Msg::TxHistoryOpen => {
            state.screen = Screen::TxHistory;
            Cmd::TxHistory
        }
        Msg::TxMyTurnSubmit => {
            if state.tx_mine.trim().is_empty() {
                state.error = Some("请先输入你的译文再对照".to_string());
                return Cmd::None;
            }
            Cmd::TxMyTurn
        }
        Msg::TxHistoryUp => {
            state.tx_sel = state.tx_sel.saturating_sub(1);
            Cmd::None
        }
        Msg::TxHistoryDown => {
            if state.tx_sel + 1 < state.tx_history.len() {
                state.tx_sel += 1;
            }
            Cmd::None
        }
        Msg::TxToggleFavorite => {
            if state.tx_history.is_empty() {
                return Cmd::None;
            }
            Cmd::TxFavorite
        }
        Msg::TxToPractice => {
            if state.tx_history.is_empty() {
                return Cmd::None;
            }
            Cmd::TxToPractice
        }
        // 答题族/模板族消息在工作台屏忽略（update 顶部已拦 Screen 切换；防误触）
        _ => Cmd::None,
    }
}

/// 入场复位（Ctrl+W 进入清工作台上下文；不碰答题/游戏态）
pub(crate) fn reset_tx(state: &mut AppState) {
    state.tx_source.clear();
    state.tx_mine.clear();
    state.tx_result = None;
    state.tx_align = None;
    state.tx_compare = None;
    state.tx_history.clear();
    state.tx_favorites.clear();
    state.tx_sel = 0;
    state.error = None;
}

/// 出译回填（主屏结果区：译文 + 术语命中）
pub fn apply_tx_result(state: &mut AppState, tx: ambition_core::Translation) {
    state.tx_result = Some(tx);
    state.error = None;
}

/// 对齐回填（TxAlign 分段视图）
pub fn apply_tx_align(state: &mut AppState, alignment: ambition_translate::Alignment) {
    state.tx_align = Some(alignment);
    state.error = None;
}

/// 我来翻回填（差异 + 可选评分）
pub fn apply_tx_compare(state: &mut AppState, compare: ambition_translate::CompareResult) {
    state.tx_compare = Some(compare);
    state.error = None;
}

/// 历史+收藏回填（游标回首位）
pub fn apply_tx_history(
    state: &mut AppState,
    history: Vec<ambition_core::Translation>,
    favorites: Vec<String>,
) {
    state.tx_history = history;
    state.tx_favorites = favorites;
    state.tx_sel = 0;
}

/// 转练习回填（单题替换练习流——与模考开卷同复位口径，docs/11 §2）
pub fn apply_tx_practice(state: &mut AppState, question: Question) {
    state.questions = vec![question];
    state.idx = 0;
    state.finished = false;
    state.screen = Screen::Practice;
    super::nav::reset_question_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx_state() -> AppState {
        let mut state = new_state(vec![]);
        state.screen = Screen::Tx;
        state
    }

    #[test]
    fn test_tx_opts_defaults_to_english_neutral() {
        let opts = tx_opts();
        assert_eq!(
            opts.l2,
            ambition_core::LangTag::english(),
            "缺省目标语 l2=en"
        );
        assert_eq!(
            opts.register,
            ambition_core::Register::Neutral,
            "缺省 Neutral 语域"
        );
        assert!(opts.glossary.is_empty(), "缺省无术语表");
    }

    #[test]
    fn test_reset_tx_clears_workspace_context() {
        let mut state = tx_state();
        state.tx_source = "我喜欢春天。".to_string();
        state.tx_mine = "I like spring.".to_string();
        state.tx_sel = 3;
        state.error = Some("x".to_string());
        reset_tx(&mut state);
        assert!(state.tx_source.is_empty() && state.tx_mine.is_empty());
        assert!(
            state.tx_result.is_none() && state.tx_align.is_none() && state.tx_compare.is_none()
        );
        assert!(state.tx_history.is_empty() && state.tx_favorites.is_empty());
        assert_eq!(state.tx_sel, 0);
        assert!(state.error.is_none(), "入场清错误行");
    }

    #[test]
    fn test_tx_update_routes_input_and_guards_empty_submit() {
        let mut state = tx_state();
        // 空源文 → 守卫（不发 Cmd）
        assert_eq!(tx_update(&mut state, Msg::TxTranslate), Cmd::None);
        assert!(state.error.as_deref().unwrap_or("").contains("源文") || state.error.is_some());
        state.error = None;
        assert_eq!(
            tx_update(&mut state, Msg::TxAlign),
            Cmd::None,
            "空源文不对齐"
        );
        assert!(state.error.is_some());
        state.error = None;
        assert_eq!(
            tx_update(&mut state, Msg::TxMyTurnOpen),
            Cmd::None,
            "空源文不进我来翻"
        );
        assert_eq!(state.screen, Screen::Tx, "屏幕不切换");
        state.error = None;
        // 源文输入 → 出译/对齐/我来翻
        tx_update(&mut state, Msg::Char('你'));
        tx_update(&mut state, Msg::Char('好'));
        assert_eq!(state.tx_source, "你好", "字符逐个入源文");
        assert_eq!(tx_update(&mut state, Msg::TxTranslate), Cmd::TxTranslate);
        assert_eq!(tx_update(&mut state, Msg::TxAlign), Cmd::TxAlign);
        // 退格（仍在主屏：落在源文；切屏后同键落译文输入位）
        tx_update(&mut state, Msg::Backspace);
        assert_eq!(state.tx_source, "你", "退格删尾");
        tx_update(&mut state, Msg::Char('好'));
        assert_eq!(state.tx_source, "你好", "再输入复原");
        assert_eq!(tx_update(&mut state, Msg::TxMyTurnOpen), Cmd::None);
        assert_eq!(state.screen, Screen::TxMyTurn, "进入我来翻");
        assert_eq!(
            tx_update(&mut state, Msg::TxMyTurnSubmit),
            Cmd::None,
            "空译文不对照"
        );
        assert!(state.error.is_some());
        state.error = None;
        tx_update(&mut state, Msg::Char('I'));
        assert_eq!(tx_update(&mut state, Msg::TxMyTurnSubmit), Cmd::TxMyTurn);
        // 历史屏
        assert_eq!(tx_update(&mut state, Msg::TxHistoryOpen), Cmd::TxHistory);
        assert_eq!(state.screen, Screen::TxHistory);
        assert_eq!(
            tx_update(&mut state, Msg::TxHistoryDown),
            Cmd::None,
            "空历史不动游标"
        );
        state.tx_history.push(ambition_core::Translation {
            tx_id: "tx-1".to_string(),
            source: "s".to_string(),
            target: "t".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "stub".to_string(),
            glossary_hits: 0,
        });
        state.tx_history.push(ambition_core::Translation {
            tx_id: "tx-2".to_string(),
            source: "s2".to_string(),
            target: "t2".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "stub".to_string(),
            glossary_hits: 0,
        });
        assert_eq!(tx_update(&mut state, Msg::TxHistoryDown), Cmd::None);
        assert_eq!(state.tx_sel, 1, "↓ 游标后移（末行不动——单行时守卫）");
        assert_eq!(tx_update(&mut state, Msg::TxHistoryUp), Cmd::None);
        assert_eq!(state.tx_sel, 0, "↑ 游标回位");
        assert_eq!(
            tx_update(&mut state, Msg::TxToggleFavorite),
            Cmd::TxFavorite
        );
        assert_eq!(tx_update(&mut state, Msg::TxToPractice), Cmd::TxToPractice);
        // 答题族消息不串台
        assert_eq!(
            tx_update(&mut state, Msg::Submit),
            Cmd::None,
            "工作台屏忽略提交"
        );
    }

    #[test]
    fn test_apply_tx_result_sets_result_and_clears_error() {
        let mut state = tx_state();
        state.error = Some("旧错".to_string());
        let tx = ambition_core::Translation {
            tx_id: "tx-a".to_string(),
            source: "s".to_string(),
            target: "t".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "m".to_string(),
            glossary_hits: 1,
        };
        apply_tx_result(&mut state, tx.clone());
        assert_eq!(
            state.tx_result.as_ref().map(|t| t.tx_id.as_str()),
            Some("tx-a")
        );
        assert!(state.error.is_none());
    }

    #[test]
    fn test_apply_tx_align_sets_alignment() {
        let mut state = tx_state();
        let alignment = ambition_translate::Alignment {
            segments: vec![ambition_translate::AlignSeg {
                src: "我喜欢".to_string(),
                tgt: "I like".to_string(),
                kind: ambition_translate::AlignKind::Literal,
            }],
        };
        apply_tx_align(&mut state, alignment);
        assert_eq!(state.tx_align.as_ref().map(|a| a.segments.len()), Some(1));
    }

    #[test]
    fn test_apply_tx_compare_sets_compare() {
        let mut state = tx_state();
        let reference = ambition_core::Translation {
            tx_id: "tx-r".to_string(),
            source: "s".to_string(),
            target: "t".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "m".to_string(),
            glossary_hits: 0,
        };
        let compare = ambition_translate::CompareResult {
            reference,
            diff: vec![],
            grade: None,
        };
        apply_tx_compare(&mut state, compare);
        assert!(state.tx_compare.is_some());
        assert!(state.tx_compare.as_ref().unwrap().grade.is_none());
    }

    #[test]
    fn test_apply_tx_history_lists_and_resets_cursor() {
        let mut state = tx_state();
        state.tx_sel = 7;
        state.tx_history.push(ambition_core::Translation {
            tx_id: "tx-old".to_string(),
            source: "s".to_string(),
            target: "t".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "m".to_string(),
            glossary_hits: 0,
        });
        apply_tx_history(&mut state, vec![], vec!["tx-9".to_string()]);
        assert!(state.tx_history.is_empty(), "列表替换");
        assert_eq!(state.tx_favorites, vec!["tx-9".to_string()]);
        assert_eq!(state.tx_sel, 0, "游标回首位");
    }

    #[test]
    fn test_apply_tx_practice_replaces_question_stream() {
        let mut state = tx_state();
        state.finished = true;
        let question = crate::sample_question();
        apply_tx_practice(&mut state, question);
        assert_eq!(state.questions.len(), 1, "单题替换练习流");
        assert_eq!(state.idx, 0);
        assert!(!state.finished, "重开完成态");
        assert_eq!(state.screen, Screen::Practice, "回到练习屏");
    }
}
