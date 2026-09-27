//! 复习屏状态：Screen/ReviewCard/ReviewState + apply_review_cards/apply_review_rated
//! （M4 复习屏，docs/03 §3；从 state.rs 拆出——配对测试同文件，P-16）

use super::*;

/// 复习卡展示数据（front/back/group 字符串化，不依赖 srs crate）
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewCard {
    pub card_id: String,
    pub front: String,
    pub back: String,
    pub group: String,
}

/// 复习屏状态（docs/03 §3 M4：Tab 切入/退出、翻面、1-4 评分）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReviewState {
    pub cards: Vec<ReviewCard>,
    /// 当前卡下标（= cards.len() 时本轮完成）
    pub idx: usize,
    /// 已翻面（翻面才能评分，防误触）
    pub revealed: bool,
    /// 队列已加载（区分「未加载」与「空队列」）
    pub loaded: bool,
    /// 上次评分反馈（"已安排 X 天后"）
    pub message: Option<String>,
}

/// 载入复习队列（main 在 Cmd::ReviewLoad 后调用；ReviewItem 已字符串化传入）
pub fn apply_review_cards(state: &mut AppState, cards: Vec<ReviewCard>) {
    state.review.cards = cards;
    state.review.idx = 0;
    state.review.revealed = false;
    state.review.loaded = true;
    state.review.message = None;
}

/// 评分完成一张（idx 前进、翻面复位、反馈下次到期）
pub fn apply_review_rated(state: &mut AppState, due_at: i64, now: i64) {
    state.review.idx += 1;
    state.review.revealed = false;
    let days = ((due_at - now).max(0)) / 86_400;
    state.review.message = Some(if days == 0 {
        "今天还会再见到这张卡".to_string()
    } else {
        format!("已安排 {days} 天后再复习")
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_review_cards_resets_queue_state() {
        let mut s = new_state(vec![sample_question()]);
        s.review.idx = 3;
        s.review.revealed = true;
        s.review.message = Some("old".into());
        let cards = vec![ReviewCard {
            card_id: "q:1".into(),
            front: "She is quick.".into(),
            back: "quick".into(),
            group: "vocab".into(),
        }];
        apply_review_cards(&mut s, cards.clone());
        assert_eq!(s.review.cards, cards, "队列载入");
        assert_eq!(s.review.idx, 0, "下标复位");
        assert!(!s.review.revealed, "翻面复位");
        assert!(s.review.loaded, "已加载标记");
        assert!(s.review.message.is_none(), "反馈复位");
    }

    #[test]
    fn test_apply_review_rated_advances_and_reports_due() {
        let mut s = new_state(vec![sample_question()]);
        s.review.cards = vec![ReviewCard {
            card_id: "q:1".into(),
            front: "f".into(),
            back: "b".into(),
            group: "g".into(),
        }];
        s.review.revealed = true;
        let now = 1_000_000_000;
        apply_review_rated(&mut s, now + 3 * 86_400, now);
        assert_eq!(s.review.idx, 1, "前进一张");
        assert!(!s.review.revealed, "翻面复位");
        assert_eq!(
            s.review.message.as_deref(),
            Some("已安排 3 天后再复习"),
            "到期反馈"
        );
        // 当天再见到（<1 天）
        let mut s2 = new_state(vec![sample_question()]);
        s2.review.cards = s.review.cards.clone();
        apply_review_rated(&mut s2, now + 60, now);
        assert!(
            s2.review.message.as_deref() == Some("今天还会再见到这张卡"),
            "间隔不足一天: {:?}",
            s2.review.message
        );
    }
}
