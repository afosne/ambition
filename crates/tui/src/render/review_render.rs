//! 复习屏渲染：render_review（FSRS 翻卡：正面/背面/评分反馈，docs/03 §3 M4）
//! 从 render.rs 拆出（docs/01 §组织：文件 ≤500 行）

use super::*;

/// 渲染复习屏（空队列/本轮完成/翻面前/翻面后四态 + 错误行）
pub fn render_review(frame: &mut Frame, state: &mut AppState) {
    let mut text = String::from(
        "Ambition · 复习（FSRS 翻卡）

",
    );
    let review = &state.review;
    if !review.loaded {
        text.push_str(
            " 正在加载到期队列…
",
        );
    } else if review.cards.is_empty() {
        text.push_str(
            " 暂无到期卡片。

",
        );
        text.push_str(&format!(
            "{}
",
            hint(KeyCtx::Review)
        ));
    } else if review.idx >= review.cards.len() {
        text.push_str(&format!(
            " 本轮 {} 张复习完成！
",
            review.cards.len()
        ));
        if let Some(message) = &review.message {
            text.push_str(&format!(
                " {message}
"
            ));
        }
        text.push_str(&format!(
            "
{}
",
            hint(KeyCtx::Review)
        ));
    } else {
        let card = &review.cards[review.idx];
        text.push_str(&format!(
            " [{}/{}] 组：{}

",
            review.idx + 1,
            review.cards.len(),
            card.group
        ));
        text.push_str(&format!(
            " 正面：{}

",
            card.front
        ));
        if review.revealed {
            text.push_str(&format!(
                " 背面：{}

",
                card.back
            ));
        } else {
            text.push_str(
                " （背面隐藏 · 翻面后可评分）

",
            );
        }
        text.push_str(&format!(
            "{}
",
            hint(KeyCtx::Review)
        ));
        if let Some(message) = &review.message {
            text.push_str(&format!(
                " ✓ {message}
"
            ));
        }
    }
    // 独立错误行（评分失败/加载失败，P-8）
    if let Some(err) = &state.error {
        text.push_str(&format!(
            "
 ⚠ {err}
"
        ));
    }
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

    // 注册复习屏可点击区域
    let inner = Block::default().borders(Borders::ALL).inner(frame.area());
    let review = &state.review;
    if review.loaded && review.idx < review.cards.len() {
        if !review.revealed {
            // 翻面区域：正面文本行可点击翻面
            let flip_row = inner.y + 6; // 标题(2) + 空行(1) + 进度(1) + 空行(1) + 正面(1)
            state.register_click(
                Rect {
                    x: inner.x,
                    y: flip_row,
                    width: inner.width,
                    height: 2,
                },
                Msg::Flip,
            );
        } else {
            // 评分按钮 1-4：底部 4 列各占 1/4
            let btn_row = inner.y + inner.height.saturating_sub(2);
            let btn_width = inner.width / 4;
            for i in 0..4u16 {
                state.register_click(
                    Rect {
                        x: inner.x + i * btn_width,
                        y: btn_row,
                        width: btn_width,
                        height: 1,
                    },
                    Msg::Rate((i + 1) as u8),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to(s: &mut AppState) -> String {
        // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
        crate::common::render_sized(s, 76, 22)
    }

    fn card() -> ReviewCard {
        ReviewCard {
            card_id: "q:1".into(),
            front: "She is quick.".into(),
            back: "quick".into(),
            group: "vocab".into(),
        }
    }

    #[test]
    fn test_render_review_covers_empty_loading_done_and_flip_states() {
        // 未加载
        let mut s = new_state(vec![crate::sample_question()]);
        s.screen = Screen::Review;
        let loading = render_to(&mut s);
        assert!(loading.contains("加载"), "未加载态: {loading:?}");
        // 空队列
        apply_review_cards(&mut s, vec![]);
        let empty = render_to(&mut s);
        assert!(empty.contains("暂无到期卡片"), "空队列: {empty:?}");
        // 有卡未翻面
        apply_review_cards(&mut s, vec![card()]);
        let front = render_to(&mut s);
        assert!(front.contains("She is quick."), "正面: {front:?}");
        assert!(front.contains("背面隐藏"), "未翻面: {front:?}");
        assert!(front.contains("翻面"), "键位提示: {front:?}");
        // 翻面后
        s.review.revealed = true;
        let back = render_to(&mut s);
        assert!(back.contains("quick"), "背面: {back:?}");
        assert!(!back.contains("背面隐藏"), "翻面后无隐藏行");
        // 评分完成
        s.review.idx = 1;
        s.review.message = Some("已安排 3 天后再复习".into());
        let done = render_to(&mut s);
        assert!(done.contains("本轮"), "完成态: {done:?}");
        assert!(done.contains("3 天后"), "反馈可见: {done:?}");
    }
}
