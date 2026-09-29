//! 领域事件与旁路订阅：DomainEvent/EventBus（docs/02 §4，engine 只 publish 不直调）

use crate::*;

/// 领域事件（gamify/sync/profile 订阅，docs/02-core-store §DomainEvent）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainEvent {
    AttemptGraded {
        attempt_id: String,
        qid: Qid,
        verdict: Verdict,
        correct: bool,
    },
    /// 复习评分完成（M4：FSRS 调度后发布，gamify → Xp(1)）
    ReviewDone {
        card_id: String,
        /// Rating 持久化字符串（again/hard/good/easy）
        rating: String,
    },
    /// 模考交卷（M5-W6：finish_run 后发布，gamify → Xp(10)、不进连击——docs/13）
    PaperFinished {
        run_id: String,
        paper_id: String,
        score: f32,
        max: f32,
    },
    /// 日记批改完成（M6-W1：diary_review 成功后发布，gamify → Xp(2)、不进连击——docs/13）
    DiaryReviewed { entry_id: String, date: String },
    /// 待办完成（M6-W1：todo_complete 发布，gamify → Xp(1)、不进连击——docs/13）
    TodoCompleted { todo_id: String, title: String },
    /// 作文批改完成（M5-W5：grade_essay 落两表后发布，gamify → Xp(5)、不进连击——docs/13）
    EssayGraded {
        essay_id: String,
        rubric_id: String,
        score: f32,
        max: f32,
    },
}

/// 事件总线：订阅者（gamify/sync 投影/profile）旁路消费，各自幂等；engine 只 publish 不直调（改进 #3）
pub trait EventBus: Send + Sync {
    /// 消费一条不可变事实
    fn publish(&self, ev: DomainEvent);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeBus;
    impl EventBus for FakeBus {
        fn publish(&self, ev: DomainEvent) {
            drop(ev);
        }
    }

    #[test]
    fn test_publish_feeds_subscriber() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counting(AtomicUsize);
        impl EventBus for Counting {
            fn publish(&self, ev: DomainEvent) {
                drop(ev);
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let bus = Counting(AtomicUsize::new(0));
        bus.publish(DomainEvent::AttemptGraded {
            attempt_id: "a".into(),
            qid: Qid("q".into()),
            verdict: Verdict::Correct,
            correct: true,
        });
        assert_eq!(bus.0.load(Ordering::SeqCst), 1, "publish 送达订阅者");
        // EventBus 对象安全（Box<dyn EventBus> 可用）
        let dyn_bus: Box<dyn EventBus> = Box::new(FakeBus);
        dyn_bus.publish(DomainEvent::AttemptGraded {
            attempt_id: "b".into(),
            qid: Qid("q2".into()),
            verdict: Verdict::Wrong,
            correct: false,
        });
    }
}
