//! update 主模块配对测试（从 mod.rs 拆出以控制行数 ≤500）
use super::*;
use ambition_core::Verdict;

// 配对桩（同文件强制配对；完整场景测试在 update_tests.rs）

#[test]
fn test_update_moves_cursor_and_submits_choice() {
    let mut s = new_state(vec![crate::sample_question()]);
    assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
    assert_eq!(s.sel, 1, "光标下移");
    let cmd = update(&mut s, Msg::Submit);
    assert!(matches!(cmd, Cmd::Submit(Answer::Choice(_))), "提交选择");
    s.grade = Some(GradeResult {
        verdict: Verdict::Correct,
        score: 1.0,
        max: 1.0,
        confidence: 1.0,
        graded_by: ambition_core::GradedBy::Rule,
        detail: ambition_core::GradeDetail::Plain,
        error_tags: vec![],
    });
    assert_eq!(update(&mut s, Msg::Submit), Cmd::None, "已判分忽略提交");
}

#[test]
fn test_update_home_navigates_and_enters_practice() {
    let mut s = new_state(vec![]);
    s.screen = Screen::Home;
    assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
    assert_eq!(s.home_cursor, 1);
    assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None);
    assert_eq!(s.home_cursor, 0);
    // cursor=0 → PracticeSelect 屏
    let cmd = update(&mut s, Msg::HomeEnter);
    assert_eq!(cmd, Cmd::None);
    assert_eq!(s.screen, Screen::PracticeSelect);
    // cursor=6 → Quit
    s.screen = Screen::Home;
    s.home_cursor = 6;
    assert_eq!(update(&mut s, Msg::HomeEnter), Cmd::Quit);
}
