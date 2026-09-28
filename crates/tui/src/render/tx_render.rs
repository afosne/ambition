//! 翻译工作台渲染（M5-W7，docs/03 §5.3 / docs/11 §5）：主屏输入+出译 · 对齐分段 · 我来翻差异 · 历史收藏
//!
//! 四屏共用一个入口（按 state.screen 分派）；宽度感知收集走 common::render_sized（P-2 续格口径）。

use super::*;

/// 工作台族入口（Tx/TxAlign/TxMyTurn/TxHistory → 各自正文 + 统一错误行/提示行）
pub fn render_tx_frame(frame: &mut Frame, state: &AppState) {
    let mut text = String::from(
        "Ambition · 翻译工作台（Enter 出译 · F5 对齐 · F6 我来翻 · F7 历史 · Esc 返回）\n\n",
    );
    match state.screen {
        Screen::Tx => render_tx_main(&mut text, state),
        Screen::TxAlign => render_tx_align(&mut text, state),
        Screen::TxMyTurn => render_tx_my_turn(&mut text, state),
        Screen::TxHistory => render_tx_history(&mut text, state),
        _ => {}
    }
    if let Some(err) = &state.error {
        text.push_str(&format!("\n ⚠ {err}\n"));
    }
    text.push_str(&format!("\n{}\n", hint(state.key_ctx())));
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    frame.render_widget(Clear, frame.area());
    frame.render_widget(paragraph, frame.area());
}

/// 主屏：源文输入 + 译文结果（术语命中/模型）
fn render_tx_main(text: &mut String, state: &AppState) {
    text.push_str(&format!(" 源文: {}▍\n", state.tx_source));
    if let Some(tx) = &state.tx_result {
        text.push_str(&format!("\n 译文: {}\n", tx.target));
        text.push_str(&format!(
            " 术语命中 {} 条 · 模型 {}\n",
            tx.glossary_hits, tx.model
        ));
    } else {
        text.push_str("\n （Enter 出译；F5 对齐 / F6 我来翻 / F7 历史）\n");
    }
}

/// 对齐屏：分段逐行（源 → 译 [类型]）
fn render_tx_align(text: &mut String, state: &AppState) {
    text.push_str(" 对齐分段（逐段对照，docs/11 §2）:\n\n");
    match &state.tx_align {
        Some(alignment) if !alignment.segments.is_empty() => {
            for seg in &alignment.segments {
                let kind = match seg.kind {
                    ambition_translate::AlignKind::Literal => "逐字",
                    ambition_translate::AlignKind::Idiomatic => "地道",
                    ambition_translate::AlignKind::Reordered => "语序",
                    ambition_translate::AlignKind::Omitted => "省略",
                };
                text.push_str(&format!("  {}  →  {}  [{}]\n", seg.src, seg.tgt, kind));
            }
        }
        _ => text.push_str(" （无对齐结果——先 Enter 出译再 F5；Enter 在本屏可重对齐）\n"),
    }
}

/// 我来翻：源文 + 我的输入 + 差异标记 + 可选评分
fn render_tx_my_turn(text: &mut String, state: &AppState) {
    text.push_str(&format!(" 源文: {}\n", state.tx_source));
    text.push_str(&format!(" 你的译文: {}▍\n", state.tx_mine));
    if let Some(compare) = &state.tx_compare {
        text.push_str("\n 差异（~ 改写 / - 漏译 / + 多译）:\n");
        for point in &compare.diff {
            match point.kind {
                ambition_translate::DiffKind::Same => {}
                ambition_translate::DiffKind::Changed => {
                    text.push_str(&format!("  ~ {} → {}\n", point.src, point.tgt));
                }
                ambition_translate::DiffKind::Missing => {
                    text.push_str(&format!("  - {}\n", point.src));
                }
                ambition_translate::DiffKind::Extra => {
                    text.push_str(&format!("  + {}\n", point.tgt));
                }
            }
        }
        if let Some(grade) = &compare.grade {
            // 判分图标与主屏浮层同源（docs/03 同源文案原则）
            let icon = match grade.verdict {
                ambition_core::Verdict::Correct => "✔",
                ambition_core::Verdict::Wrong => "✘",
                ambition_core::Verdict::Pending => "?",
                ambition_core::Verdict::Partial => "≈",
                ambition_core::Verdict::Acceptable => "≈",
            };
            text.push_str(&format!(
                "\n {} 得分 {:.1} / {}（置信 {:.2}）\n",
                icon, grade.score, grade.max, grade.confidence
            ));
        } else {
            text.push_str("\n （非英语目标语：评分降级待多语粗判，docs/11 §2）\n");
        }
    }
}

/// 历史屏：游标 + 收藏标记 + 源→译
fn render_tx_history(text: &mut String, state: &AppState) {
    text.push_str(" 译文历史（↑/↓ 选择 · Enter 转练习 · Space 收藏）:\n\n");
    if state.tx_history.is_empty() {
        text.push_str(" （空——F7 拉取；先在主屏出译才会落历史）\n");
        return;
    }
    for (i, tx) in state.tx_history.iter().enumerate() {
        let cursor = if i == state.tx_sel { "❯" } else { " " };
        let star = if state.tx_favorites.contains(&tx.tx_id) {
            "♥"
        } else {
            " "
        };
        text.push_str(&format!(
            " {}{} {:>2}. {} → {}\n",
            cursor,
            star,
            i + 1,
            tx.source,
            tx.target
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_render_tx_frame_dispatches_all_screens() {
        for screen in [
            Screen::Tx,
            Screen::TxAlign,
            Screen::TxMyTurn,
            Screen::TxHistory,
        ] {
            let state = tx_state(screen);
            let out = crate::common::render_sized(&state, 76, 24);
            assert!(
                out.contains("翻译工作台"),
                "工作台横幅随屏出现 {screen:?}: {out}"
            );
        }
    }

    use super::*;
    use crate::new_state;

    fn tx_state(screen: Screen) -> AppState {
        let mut state = new_state(vec![]);
        state.screen = screen;
        state
    }

    #[test]
    fn test_render_tx_main_shows_source_caret_and_result() {
        let mut state = tx_state(Screen::Tx);
        state.tx_source = "我喜欢春天。".to_string();
        let out = crate::common::render_sized(&state, 76, 24);
        assert!(out.contains("源文: 我喜欢春天。"), "源文可见: {out}");
        assert!(out.contains('▍'), "光标可见");
        // 出译回填后结果区
        let tx = ambition_core::Translation {
            tx_id: "tx-1".to_string(),
            source: "我喜欢春天。".to_string(),
            target: "I like spring.".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "stub".to_string(),
            glossary_hits: 2,
        };
        state.tx_result = Some(tx);
        let out = crate::common::render_sized(&state, 76, 24);
        assert!(out.contains("译文: I like spring."), "译文可见: {out}");
        assert!(out.contains("术语命中 2 条"), "命中数可见");
    }

    #[test]
    fn test_render_tx_align_shows_segments_with_kind() {
        let mut state = tx_state(Screen::TxAlign);
        state.tx_align = Some(ambition_translate::Alignment {
            segments: vec![
                ambition_translate::AlignSeg {
                    src: "我喜欢".to_string(),
                    tgt: "I like".to_string(),
                    kind: ambition_translate::AlignKind::Literal,
                },
                ambition_translate::AlignSeg {
                    src: "春天".to_string(),
                    tgt: "spring".to_string(),
                    kind: ambition_translate::AlignKind::Idiomatic,
                },
            ],
        });
        let out = crate::common::render_sized(&state, 76, 24);
        assert!(out.contains("我喜欢  →  I like  [逐字]"), "逐字段: {out}");
        assert!(out.contains("[地道]"), "地道段可见");
    }

    #[test]
    fn test_render_tx_my_turn_marks_diff_and_grade() {
        let mut state = tx_state(Screen::TxMyTurn);
        state.tx_source = "The cat.".to_string();
        state.tx_mine = "A cat.".to_string();
        let reference = ambition_core::Translation {
            tx_id: "tx-r".to_string(),
            source: "The cat.".to_string(),
            target: "The cat.".to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "stub".to_string(),
            glossary_hits: 0,
        };
        let compare = ambition_translate::CompareResult {
            reference,
            diff: vec![ambition_translate::DiffPoint {
                src: "The".to_string(),
                tgt: "A".to_string(),
                kind: ambition_translate::DiffKind::Changed,
            }],
            grade: None,
        };
        state.tx_compare = Some(compare);
        let out = crate::common::render_sized(&state, 76, 24);
        assert!(out.contains("~ The → A"), "改写标记: {out}");
        assert!(out.contains("非英语目标语"), "降级提示可见");
    }

    #[test]
    fn test_render_tx_history_marks_cursor_and_favorite() {
        let mut state = tx_state(Screen::TxHistory);
        let mk = |id: &str, src: &str, tgt: &str| ambition_core::Translation {
            tx_id: id.to_string(),
            source: src.to_string(),
            target: tgt.to_string(),
            dir: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".to_string()),
                to: ambition_core::LangTag::english(),
            },
            model: "stub".to_string(),
            glossary_hits: 0,
        };
        state.tx_history = vec![mk("tx-1", "甲", "A"), mk("tx-2", "乙", "B")];
        state.tx_favorites = vec!["tx-2".to_string()];
        state.tx_sel = 1;
        let out = crate::common::render_sized(&state, 76, 24);
        assert!(out.contains("❯♥"), "游标+收藏同排: {out}");
        assert!(out.contains("2. 乙 → B"), "游标行内容: {out}");
        assert!(out.contains("1. 甲 → A"), "非游标行内容: {out}");
    }
}
