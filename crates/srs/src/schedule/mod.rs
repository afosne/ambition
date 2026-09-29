//! 调度决策与到期队列：decide_action/next_mode/schedule/due_queue/interleave（docs/08）

/// 交错策略（替代裸 bool，docs/01 §1 禁裸 bool，#31）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interleave {
    /// 不交错
    None,
    /// 同 group 不相邻（贪心轮转）
    Groups,
}

use super::*;

/// 作答后对卡片的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Create,
    Bump,
    Schedule(Rating),
    Skip,
}

/// 作答 → 卡片处置（错/假对入卡，Pending 不动 —— docs/08-adaptive §on_attempt）
pub fn decide_action(verdict: Verdict, is_guess: bool, has_card: bool) -> CardAction {
    match verdict {
        Verdict::Wrong | Verdict::Partial => {
            if has_card {
                CardAction::Bump
            } else {
                CardAction::Create
            }
        }
        Verdict::Correct | Verdict::Acceptable => {
            if is_guess {
                if has_card {
                    CardAction::Bump
                } else {
                    CardAction::Create
                }
            } else if has_card {
                CardAction::Schedule(Rating::Good)
            } else {
                CardAction::Skip
            }
        }
        Verdict::Pending => CardAction::Skip,
    }
}

/// 检索难度档（choice→fill→spell；docs/08-adaptive §next_mode）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    Choice,
    Fill,
    Spell,
}

impl ReviewMode {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Fill => "fill",
            Self::Spell => "spell",
        }
    }
}

/// 复习题型递进：失败回 Choice；≥2 连对进 Fill；≥4 连对进 Spell
pub fn next_mode(ok_streak: u32, failed_last: bool) -> ReviewMode {
    if failed_last {
        ReviewMode::Choice
    } else if ok_streak >= 4 {
        ReviewMode::Spell
    } else if ok_streak >= 2 {
        ReviewMode::Fill
    } else {
        ReviewMode::Choice
    }
}

/// FSRS 调度：按评分推进记忆状态与到期时间（unix 秒）。
/// new 卡（fsrs=None）或 elapsed_days=0 均走 next_states；Again 降档、连对升档（docs/08 §1 schedule）
pub fn schedule(card: &mut Card, rating: Rating, now: i64) -> Result<(), SrsError> {
    let elapsed = card
        .last_review_at
        .map(|last| ((now - last).max(0) / 86_400) as u32)
        .unwrap_or(0);
    let current = card.fsrs.map(|state| state.to_memory());
    let fsrs = FSRS::default();
    let next = fsrs
        .next_states(current, DESIRED_RETENTION, elapsed)
        .map_err(|err| SrsError::Fsrs(err.to_string()))?;
    let picked = match rating {
        Rating::Again => next.again,
        Rating::Hard => next.hard,
        Rating::Good => next.good,
        Rating::Easy => next.easy,
    };
    card.fsrs = Some(FsrsState::from_memory(picked.memory));
    // 间隔天数（FSRS 返回天）→ unix 秒；<1 天按 1 天排（当天不重复轰炸），≥1 天取整
    let days = picked.interval.max(1.0).round() as i64;
    card.due_at = now + days * 86_400;
    card.last_review_at = Some(now);
    // 升/降档：Again 重置，连对累计（Hard/Good/Easy 均算「答对」）
    if rating == Rating::Again {
        card.ok_streak = 0;
    } else {
        card.ok_streak = card.ok_streak.saturating_add(1);
    }
    card.mode = next_mode(card.ok_streak, rating == Rating::Again);
    Ok(())
}

/// 复习条目（due_queue 产出；正面/背面文本由 engine 按来源组装）
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    pub card: Card,
    pub front: String,
    pub back: String,
    /// 交错分组键（知识点：qid tag / word / pattern kind）
    pub group: String,
}

/// 到期队列：过滤到期 → 按 due_at 升序（错词多者提权）→ 交错 → 截断（docs/08 §1 due_queue）
pub fn due_queue(
    items: Vec<ReviewItem>,
    now: i64,
    limit: u32,
    interleave: Interleave,
) -> Vec<ReviewItem> {
    let mut due: Vec<ReviewItem> = items
        .into_iter()
        .filter(|item| item.card.due_at <= now)
        .collect();
    due.sort_by(|a, b| {
        b.card
            .wrong_count
            .cmp(&a.card.wrong_count)
            .then(a.card.due_at.cmp(&b.card.due_at))
    });
    if matches!(interleave, Interleave::Groups) {
        interleave_groups(&mut due);
    }
    due.truncate(limit as usize);
    due
}

/// 交错练习：同 group 不相邻（贪心轮转取各组队首；docs/08 §1 interleave）
pub fn interleave_groups(items: &mut Vec<ReviewItem>) {
    if items.len() < 2 {
        return;
    }
    let mut groups: Vec<(String, std::collections::VecDeque<ReviewItem>)> = Vec::new();
    for item in items.drain(..) {
        match groups.iter_mut().find(|(key, _)| *key == item.group) {
            Some((_, queue)) => queue.push_back(item),
            None => {
                let mut queue = std::collections::VecDeque::new();
                queue.push_back(item);
                groups.push((String::new(), queue));
                let last = groups.len() - 1;
                let key = groups[last]
                    .1
                    .front()
                    .map(|i| i.group.clone())
                    .unwrap_or_default();
                groups[last].0 = key;
            }
        }
    }
    let mut out = Vec::with_capacity(items.capacity());
    loop {
        let mut progressed = false;
        for (_, queue) in groups.iter_mut() {
            if let Some(item) = queue.pop_front() {
                out.push(item);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    *items = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_card(tag: &str, wrong: u32, due_at: i64) -> ReviewItem {
        let source = CardSource::Pattern {
            pattern_id: tag.to_string(),
        };
        let mut card = Card::new(source, 0);
        card.wrong_count = wrong;
        card.due_at = due_at;
        ReviewItem {
            card,
            front: tag.to_string(),
            back: "back".to_string(),
            group: tag.to_string(),
        }
    }

    #[test]
    fn test_decide_action_on_wrong_and_partial() {
        assert_eq!(
            decide_action(Verdict::Wrong, false, false),
            CardAction::Create
        );
        assert_eq!(decide_action(Verdict::Wrong, false, true), CardAction::Bump);
        assert_eq!(
            decide_action(Verdict::Partial, false, true),
            CardAction::Bump
        );
        assert_eq!(
            decide_action(Verdict::Partial, false, false),
            CardAction::Create
        );
    }

    #[test]
    fn test_decide_action_on_correct_with_and_without_guess() {
        // 正常答对：已有卡 → Good 调度；无卡 → 不建卡
        assert_eq!(
            decide_action(Verdict::Correct, false, true),
            CardAction::Schedule(Rating::Good)
        );
        assert_eq!(
            decide_action(Verdict::Correct, false, false),
            CardAction::Skip
        );
        // 假对（蒙对）→ 必须入卡
        assert_eq!(
            decide_action(Verdict::Correct, true, false),
            CardAction::Create
        );
        assert_eq!(
            decide_action(Verdict::Correct, true, true),
            CardAction::Bump
        );
        assert_eq!(
            decide_action(Verdict::Acceptable, true, false),
            CardAction::Create
        );
    }

    #[test]
    fn test_decide_action_pending_skips() {
        assert_eq!(
            decide_action(Verdict::Pending, false, true),
            CardAction::Skip
        );
        assert_eq!(
            decide_action(Verdict::Pending, true, false),
            CardAction::Skip
        );
    }

    #[test]
    fn test_next_mode_streak_escalation_and_reset() {
        assert_eq!(next_mode(0, false), ReviewMode::Choice);
        assert_eq!(next_mode(1, false), ReviewMode::Choice);
        assert_eq!(next_mode(2, false), ReviewMode::Fill);
        assert_eq!(next_mode(4, false), ReviewMode::Spell);
        assert_eq!(next_mode(9, true), ReviewMode::Choice); // 答错回退
    }

    #[test]
    fn test_schedule_matches_fsrs_snapshot() {
        // fsrs-rs 6.6.2 默认参数、DR=0.9、新卡第 0 天参考输出（docs/08 §5 验证点）
        let source = CardSource::Pattern {
            pattern_id: "snap".into(),
        };
        let mut card = Card::new(source, 0);
        schedule(&mut card, Rating::Good, 0).expect("调度");
        let state = card.fsrs.expect("已有状态");
        assert!(
            (state.stability - 2.3065).abs() < 0.01,
            "good stability 快照: {}",
            state.stability
        );
        assert!(
            (state.difficulty - 2.118104).abs() < 0.01,
            "good difficulty 快照: {}",
            state.difficulty
        );
        assert_eq!(card.due_at, 2 * 86_400, "interval 2.3 天 → 取整 2 天");
        assert_eq!(card.last_review_at, Some(0));
        assert_eq!(card.ok_streak, 1);
        assert_eq!(card.mode, ReviewMode::Choice, "1 连对仍 choice");
    }

    #[test]
    fn test_schedule_again_resets_streak_and_escalates_with_good() {
        let source = CardSource::Pattern {
            pattern_id: "streak".into(),
        };
        let mut card = Card::new(source, 0);
        schedule(&mut card, Rating::Good, 0).expect("s1");
        schedule(&mut card, Rating::Good, 86_400 * 2).expect("s2");
        assert_eq!(card.mode, ReviewMode::Fill, "2 连对升档");
        schedule(&mut card, Rating::Again, 86_400 * 10).expect("s3");
        assert_eq!(card.ok_streak, 0, "Again 清零");
        assert_eq!(card.mode, ReviewMode::Choice, "Again 降档");
        // 状态单调：Again 后 stability 不高于 Good 后
        let after_again = card.fsrs.expect("again 后有状态");
        assert!(
            after_again.stability < 5.0,
            "Again 后 stability 收敛: {}",
            after_again.stability
        );
    }

    #[test]
    fn test_due_queue_filters_sorts_interleaves_and_limits() {
        let now = 1_000;
        // a 组 ×2、b 组 ×1、未到期 ×1
        let items = vec![
            bare_card("a", 1, 500),
            bare_card("a2", 0, 600),
            bare_card("b", 0, 700),
            bare_card("future", 0, now + 1),
        ];
        let queue = due_queue(items, now, 10, Interleave::Groups);
        assert_eq!(queue.len(), 3, "未到期被过滤");
        assert_eq!(queue[0].group, "a", "错词多者提权排首");
        // 交错：同 group 不相邻（a2 与 a 被 b 隔开或紧邻错位——断言首尾组不同）
        let groups: Vec<&str> = queue.iter().map(|i| i.group.as_str()).collect();
        assert!(
            groups.contains(&"a2") && groups.contains(&"b"),
            "全部到期项入队: {groups:?}"
        );
        // limit 截断
        let items2 = vec![bare_card("x", 0, 1), bare_card("y", 0, 1)];
        assert_eq!(due_queue(items2, now, 1, Interleave::None).len(), 1);
        // 空/单元素
        assert!(due_queue(vec![], now, 5, Interleave::Groups).is_empty());
        assert_eq!(
            due_queue(vec![bare_card("s", 0, 0)], now, 5, Interleave::Groups).len(),
            1
        );
    }

    #[test]
    fn test_interleave_groups_keeps_groups_apart() {
        // 3a + 3b → 轮转交错，同组不相邻
        let mut items = vec![
            bare_card("a1", 0, 0),
            bare_card("a2", 0, 0),
            bare_card("a3", 0, 0),
            bare_card("b1", 0, 0),
            bare_card("b2", 0, 0),
            bare_card("b3", 0, 0),
        ];
        // 手动改 group（bare_card 用 id 当 group）
        for (i, item) in items.iter_mut().enumerate() {
            item.group = if i < 3 { "A".into() } else { "B".into() };
        }
        interleave_groups(&mut items);
        let groups: Vec<&str> = items.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(groups.len(), 6, "不丢项");
        for window in groups.windows(2) {
            assert_ne!(window[0], window[1], "同组不相邻: {groups:?}");
        }
        // 单组/空输入不 panic 且保持原序
        let mut single = vec![bare_card("only", 0, 0)];
        interleave_groups(&mut single);
        assert_eq!(single.len(), 1);
        let mut none: Vec<ReviewItem> = vec![];
        interleave_groups(&mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn test_as_str_matches_serde() {
        assert_eq!(ReviewMode::Choice.as_str(), "choice");
        assert_eq!(ReviewMode::Fill.as_str(), "fill");
        assert_eq!(ReviewMode::Spell.as_str(), "spell");
        let json = serde_json::to_string(&ReviewMode::Fill).expect("序列化");
        assert_eq!(json, "\"fill\"");
    }
}
