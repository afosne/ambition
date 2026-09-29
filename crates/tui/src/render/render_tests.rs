//! render 主模块配对测试（从 mod.rs 拆出以控制行数 ≤500）
use super::*;
use crate::common::*;
use ambition_core::Verdict;

#[test]
fn test_render_mock_shows_banner_countdown_and_score() {
    let mut s = new_state(vec![crate::sample_question()]);
    s.screen = Screen::Mock;
    s.mock = Some(crate::MockRun {
        run_id: "r1".into(),
        title: "CET4 全真模考（MVP 卷）".into(),
        deadline: ambition_core::now_unix() + 1800,
    });
    s.finished = true;
    s.mock_result = Some((34.0, 40.0));
    let out = crate::common::render_sized(&mut s, 76, 24);
    assert!(out.contains('⏱'), "倒计时可见: {out}");
    assert!(out.contains("34.0 / 40"), "得分行: {out}");
}

#[test]
fn test_render_essay_shows_prompt_and_red_lines() {
    let q = Question::new(
        "Write an essay in English according to the prompt.".into(),
        QuestionKind::Essay(ambition_core::EssayPrompt {
            prompt_zh: "以\u{201c}绿色校园\u{201d}为题写一篇短文。".into(),
            genre: "argument".into(),
            track: "cet4".into(),
            rubric_id: "gaokao25".into(),
            word_range: (120, 180),
            minutes: 30,
        }),
        3,
        vec!["essay".to_string(), "cet4".to_string()],
        ambition_core::Provenance::Seed,
    );
    let mut s = new_state(vec![q]);
    let out = crate::common::render_sized(&mut s, 72, 18);
    assert!(
        out.contains("题面"),
        "中文题面展示（仅题面，不进 state）: {out}"
    );
    assert!(out.contains("字数 120-180 词"), "字数红线可见: {out}");
    assert!(out.contains("限时 30 分钟"), "限时可见: {out}");
}

#[test]
fn test_render_draws_stem_options_and_grade() {
    let render_to = |s: &mut AppState| crate::common::render_sized(s, 72, 18);

    let mut s = new_state(vec![sample_question()]);
    let plain = render_to(&mut s);
    assert!(plain.contains("university"), "题干可见");
    assert!(plain.contains("❯"), "选中光标可见");
    apply_grade(&mut s, grade(Verdict::Correct));
    let graded = render_to(&mut s);
    assert!(graded.contains("✔"), "判分浮层可见");
    s.finished = true;
    let final_buf = render_to(&mut s);
    assert!(final_buf.contains("完成"), "结束态可见: {final_buf:?}");
}

#[test]
fn test_render_shows_error_line_and_fill_input_separately() {
    let render_to = |s: &mut AppState| crate::common::render_sized(s, 72, 20);

    let mut s = new_state(vec![sample_question()]);
    apply_error(&mut s, "rule layer failed".into());
    let buf = render_to(&mut s);
    assert!(buf.contains("提交失败"), "错误行可见: {buf:?}");
    assert!(buf.contains("rule layer failed"), "错误内容可见");
    assert!(!buf.contains("得分"), "错误态无判分浮层");

    let mut f = new_state(vec![fill_question(1)]);
    update(&mut f, Msg::Char('a'));
    update(&mut f, Msg::Char('m'));
    let fbuf = render_to(&mut f);
    assert!(fbuf.contains("答案:"), "输入行标签可见");
    assert!(fbuf.contains("am"), "输入内容可见");
    assert!(fbuf.contains("退出编辑"), "提示行取 hint(ctx)：编辑态");
    update(&mut f, Msg::StopEdit);
    let bbuf = render_to(&mut f);
    assert!(bbuf.contains("打字恢复编辑"), "提示随 ctx 变化: {bbuf:?}");
    let mut m = new_state(vec![fill_question(2)]);
    update(&mut m, Msg::Char('a'));
    update(&mut m, Msg::MoveDown);
    update(&mut m, Msg::Char('b'));
    let mbuf = render_to(&mut m);
    assert!(mbuf.contains("答案 1"), "多空编号 1: {mbuf:?}");
    assert!(mbuf.contains("答案 2"), "多空编号 2: {mbuf:?}");
}

#[test]
fn test_render_marks_checked_option_for_multi_select() {
    let render_to = |s: &mut AppState| crate::common::render_sized(s, 72, 18);

    let mut s = new_state(vec![multi_question()]);
    update(&mut s, Msg::Select);
    update(&mut s, Msg::MoveDown);
    update(&mut s, Msg::Select);
    let buf = render_to(&mut s);
    assert!(buf.contains("✓"), "选中标记可见: {buf:?}");
    assert_eq!(s.checked, vec![0, 1], "多选两枚");
}

#[test]
fn test_render_shows_pending_arbitration_and_error_tags() {
    let render_to = |s: &mut AppState| crate::common::render_sized(s, 76, 24);

    let mut s = new_state(vec![sample_question()]);
    let hint = render_to(&mut s);
    assert!(hint.contains("1-4 自评把握"), "自评入口可见: {hint:?}");
    update(&mut s, Msg::Confidence(3));
    let hinted = render_to(&mut s);
    assert!(hinted.contains("3/4"), "档位可见: {hinted:?}");
    let mut pending = grade(Verdict::Pending);
    pending.error_tags = vec![ambition_core::ErrorTag::Collocation];
    pending.confidence = 0.42;
    apply_grade(&mut s, pending);
    let buf = render_to(&mut s);
    assert!(buf.contains("待裁定"), "待裁定行可见: {buf:?}");
    assert!(buf.contains("❯判对"), "默认高亮判对: {buf:?}");
    assert!(buf.contains("Enter 确认"), "裁决键提示: {buf:?}");
    update(&mut s, Msg::MoveDown);
    let flipped = render_to(&mut s);
    assert!(flipped.contains("❯判错"), "箭头切换高亮: {flipped:?}");
    assert!(flipped.contains("错因: 搭配"), "错因中文标签: {flipped:?}");
    assert!(flipped.contains("0.42"), "结构化讲解含概率: {flipped:?}");
    apply_srs_hint(&mut s, Some(ambition_engine::CardAction::Create));
    let srs_buf = render_to(&mut s);
    assert!(
        srs_buf.contains("错词复习队列"),
        "SRS 提示行可见: {srs_buf:?}"
    );
    let mut human = grade(Verdict::Correct);
    human.graded_by = ambition_core::GradedBy::Human;
    apply_grade(&mut s, human);
    let human_buf = render_to(&mut s);
    assert!(human_buf.contains("人工裁决"), "裁决后可见: {human_buf:?}");
    assert!(!human_buf.contains("待裁定"), "已裁决不显示待裁定行");
}

#[test]
fn test_render_shows_annotations_inline() {
    use ambition_writing::{Annotation, CommentKind, Comments, SrsTarget, WordKind};
    let q = Question::new(
        "Write an essay.".into(),
        QuestionKind::Essay(ambition_core::EssayPrompt {
            prompt_zh: "以绿色校园为题。".into(),
            genre: "argument".into(),
            track: "cet4".into(),
            rubric_id: "gaokao25".into(),
            word_range: (120, 180),
            minutes: 30,
        }),
        3,
        vec!["essay".to_string()],
        ambition_core::Provenance::Seed,
    );
    let mut s = new_state(vec![q]);
    s.grade = Some(grade(ambition_core::Verdict::Correct));
    s.comments = Some(Comments {
        annotations: vec![Annotation {
            sentence: "Nowadays I think this is good.".to_string(),
            kind: CommentKind::Upgrade,
            suggested: Some("This measure is beneficial.".to_string()),
            brief_reason: "更书面的替换".to_string(),
            target: Some(SrsTarget {
                kind: WordKind::Phrase,
                text: "be beneficial to".to_string(),
            }),
        }],
    });
    let out = crate::common::render_sized(&mut s, 76, 24);
    assert!(out.contains("✎ 升级"), "升级图标: {out}");
    assert!(
        out.contains("Nowadays I think this is good."),
        "原句: {out}"
    );
    assert!(out.contains("This measure is beneficial."), "建议句: {out}");
    assert!(out.contains("短评"), "短评标签: {out}");
    assert!(out.contains("入卡目标"), "入卡目标: {out}");
}

#[test]
fn test_render_home_shows_menu_and_cursor() {
    let mut s = {
        let mut s = new_state(vec![]);
        s.screen = Screen::Home;
        s
    };
    let out = crate::common::render_sized(&mut s, 72, 40);
    assert!(out.contains("俺必胜"), "标题可见: {out}");
    assert!(out.contains("开始练习"), "菜单项可见: {out}");
    assert!(out.contains("❯"), "光标可见: {out}");
    assert!(out.contains("词汇本"), "末项可见: {out}");
    assert!(out.contains("Ctrl+Q"), "退出提示可见: {out}");
}

#[test]
fn test_render_home_cursor_moves() {
    let mut s = new_state(vec![]);
    s.screen = Screen::Home;
    s.home_cursor = 3;
    let out = crate::common::render_sized(&mut s, 72, 40);
    assert!(out.contains("❯ 模拟考试"), "光标在第 4 项: {out}");
    assert!(!out.contains("❯ 开始练习"), "第 1 项无光标: {out}");
}
