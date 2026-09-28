//! 键位上下文路由（从 state 拆出，docs/01 §1 文件 ≤500 行）：keys.rs 唯一路由入参、键位表只此一份
use crate::*;

impl AppState {
    /// 键位上下文（keys.rs 唯一路由入参；键位表只此一份）
    pub fn key_ctx(&self) -> KeyCtx {
        if matches!(
            self.screen,
            Screen::Tx | Screen::TxAlign | Screen::TxMyTurn | Screen::TxHistory
        ) {
            return match self.screen {
                Screen::TxAlign => KeyCtx::TxAlign,
                Screen::TxMyTurn => KeyCtx::TxMyTurn,
                Screen::TxHistory => KeyCtx::TxHistory,
                _ => KeyCtx::Tx,
            };
        }
        if self.screen == Screen::Scaffold {
            return KeyCtx::Scaffold;
        }
        if self.screen == Screen::Review {
            return KeyCtx::Review;
        }
        if self.picker_open {
            return KeyCtx::Picker;
        }
        if self.finished {
            return KeyCtx::Finished;
        }
        match &self.grade {
            Some(g) if g.verdict == ambition_core::Verdict::Pending => KeyCtx::Pending,
            Some(_) => KeyCtx::Graded,
            None if matches!(self.questions[self.idx].kind, QuestionKind::FillBlank(_)) => {
                KeyCtx::Fill {
                    editing: self.editing,
                }
            }
            None => KeyCtx::Choice,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::Verdict;

    #[test]
    fn test_key_ctx_routes_states() {
        let choice = new_state(vec![sample_question()]);
        assert_eq!(choice.key_ctx(), KeyCtx::Choice);
        let mut fill = new_state(vec![fill_question(1)]);
        assert_eq!(
            fill.key_ctx(),
            KeyCtx::Fill { editing: false },
            "打字前未编辑（首键直接进编辑）"
        );
        fill.editing = true;
        assert_eq!(fill.key_ctx(), KeyCtx::Fill { editing: true });
        apply_grade(&mut fill, grade(Verdict::Pending));
        assert_eq!(fill.key_ctx(), KeyCtx::Pending, "待裁定单独上下文");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert_eq!(fill.key_ctx(), KeyCtx::Graded);
        fill.finished = true;
        assert_eq!(fill.key_ctx(), KeyCtx::Finished);
    }
}
