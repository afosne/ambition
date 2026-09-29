//! 渲染（细分域根）：render（题干/选项/判分浮层/待裁定行/错误行，ratatui Frame）
//! 本域含子组件：`render::passage_render`（语篇题型）/ `render::picker_render`（选题面板）/
//! `render::review_render`（复习屏）/ `render::render_passage_tests`（配对测试）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod m6_screens;
pub mod passage_render;
pub mod picker_render;
#[cfg(test)]
mod render_passage_tests;
pub mod review_render;
pub mod scaffold_render;
pub mod tx_render;

pub use picker_render::*;
pub use review_render::*;

use self::passage_render::render_passage;
use crate::*;

/// 渲染当前状态（含判分浮层 + 错误行 + 填空输入行）
pub fn render(frame: &mut Frame, state: &AppState) {
    if m6_screens::render_workspace(frame, state) {
        return;
    }
    if state.picker_open {
        render_picker(frame, state);
        return;
    }
    if state.screen == Screen::Review {
        render_review(frame, state);
        return;
    }
    if state.screen == Screen::Scaffold {
        scaffold_render::render_scaffold(frame, state);
        return;
    }
    // 翻译工作台族（M5-W7：Tx/TxAlign/TxMyTurn/TxHistory 共用入口）
    if matches!(
        state.screen,
        Screen::Tx | Screen::TxAlign | Screen::TxMyTurn | Screen::TxHistory
    ) {
        tx_render::render_tx_frame(frame, state);
        return;
    }
    let mut text = String::from("Ambition · 拾英 —— 单机模式（本地题库）\n\n");
    // 模考横幅（M5-W6：屏顶标题 + 倒计时——deadline 来自 run 行，docs/03 §5.3）
    if state.screen == Screen::Mock
        && let Some(mock) = &state.mock
    {
        let left = (mock.deadline - ambition_core::now_unix()).max(0);
        text.push_str(&format!(
            " 模拟考试 · {} · ⏱ 剩 {:02}:{:02}\n\n",
            mock.title,
            left / 60,
            left % 60
        ));
    }
    if state.finished {
        if state.screen == Screen::Mock {
            text.push_str("模考完成！Ctrl+Q 退出 · Ctrl+A 练习屏 · Ctrl+M 重开一卷。\n");
            if let Some((score, max)) = state.mock_result {
                text.push_str(&format!(
                    " 模考得分：{score:.1} / {max}\n",
                    score = score,
                    max = max
                ));
            }
        } else {
            text.push_str("本组完成！按 Ctrl+Q 退出。\n");
        }
    } else {
        let q = &state.questions[state.idx];
        text.push_str(&format!(
            "[{}/{}] {}\n\n",
            state.idx + 1,
            state.questions.len(),
            q.stem
        ));
        // 选择类题型（单/多选 + 七选五缺口候选；M4 题型补全）
        match &q.kind {
            QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
                for (i, opt) in body.options.iter().enumerate() {
                    let cursor = if state.sel == i { "❯" } else { " " };
                    let tick = if state.checked.contains(&(i as u32)) {
                        "✓"
                    } else {
                        " "
                    };
                    text.push_str(&format!("  {}{} {}. {}\n", cursor, tick, i + 1, opt.text));
                }
            }
            // M5-W2：翻译——源文（l1）题面必显（l1 不进 state 只是判分口径，docs/05 §3.5）
            QuestionKind::Translation(body) => {
                text.push_str(&format!(
                    "
 源文: {}
",
                    body.source_text
                ));
            }
            // M5-W3：作文——中文题面 + 字数/限时红线（展示用，永不进 state）
            QuestionKind::Essay(body) => {
                text.push_str(&format!(
                    "
 题面: {}
",
                    body.prompt_zh
                ));
                text.push_str(&format!(
                    " 体裁 {} · 字数 {}-{} 词 · 限时 {} 分钟
",
                    body.genre, body.word_range.0, body.word_range.1, body.minutes
                ));
            }
            // 语篇类题型（七选五逐槽反馈/阅读逐子题反馈/完形·语法填空语篇，passage_render.rs）
            _ => render_passage(&mut text, state, q),
        }
        if q.kind.is_text_input() && state.grade.is_none() {
            // 每空一位独立输入行；❯+▍ 标当前编辑位（单空不编号）
            for (i, field) in state.inputs.iter().enumerate() {
                let cursor = if state.blank_sel == i { "❯" } else { " " };
                let label = if state.inputs.len() == 1 {
                    "答案".to_string()
                } else {
                    format!("答案 {}", i + 1)
                };
                let caret = if state.blank_sel == i { "▍" } else { " " };
                text.push_str(&format!("\n  {} {}: {}{}\n", cursor, label, field, caret));
            }
        }
        // 键位提示：与路由同源（keys.rs::hint，禁止硬编码）
        text.push_str(&format!("\n{}\n", hint(state.key_ctx())));
        if state.grade.is_none()
            && !q.kind.is_text_input()
            && let Some(level) = state.self_confidence
        {
            text.push_str(&format!(" 自评把握: {level}/4\n"));
        }
        // 独立错误行（红字，≠ 判分浮层；P-8）
        if let Some(err) = &state.error {
            text.push_str(&format!("\n ⚠ 提交失败（未判分）: {err}\n"));
        }
        if let Some(grade) = &state.grade {
            let icon = match grade.verdict {
                ambition_core::Verdict::Correct => "✔",
                ambition_core::Verdict::Wrong => "✘",
                ambition_core::Verdict::Pending => "?",
                ambition_core::Verdict::Partial => "≈",
                ambition_core::Verdict::Acceptable => "≈",
            };
            text.push_str(&format!("\n {} 得分 {}/{}", icon, grade.score, grade.max));
            if grade.graded_by == ambition_core::GradedBy::Human {
                text.push_str(" · 人工裁决");
            }
            // 结构化讲解①：错因标签（中文展示名）
            if !grade.error_tags.is_empty() {
                let labels: Vec<&str> = grade.error_tags.iter().map(|tag| tag.label_zh()).collect();
                text.push_str(&format!(
                    "\n 错因: {}（置信 {:.2}）",
                    labels.join("、"),
                    grade.confidence
                ));
            }
            // composite 维度分解（M5：翻译/作文 detail::Dims 浮层，docs/07 §3）
            if let ambition_core::GradeDetail::Dims(dims) = &grade.detail {
                for dim in dims {
                    text.push_str(&format!(
                        "
 维度 {}: {:.0}%（置信 {:.2}）",
                        dim.key,
                        dim.score * 100.0,
                        dim.confidence
                    ));
                }
            }
            // 批注区（M5-W5：Ctrl+E 回填；升级/亮点逐句对照展示）
            if let Some(comments) = &state.comments {
                text.push_str(&format!(
                    "
 批注（{} 条）:
",
                    comments.annotations.len()
                ));
                for ann in &comments.annotations {
                    let icon = match ann.kind {
                        ambition_writing::CommentKind::Upgrade => "✎ 升级",
                        ambition_writing::CommentKind::Highlight => "★ 亮点",
                    };
                    text.push_str(&format!(
                        "  {icon} {}
",
                        ann.sentence
                    ));
                    if let Some(suggested) = &ann.suggested {
                        text.push_str(&format!(
                            "      → {suggested}
"
                        ));
                    }
                    text.push_str(&format!(
                        "      短评: {}
",
                        ann.brief_reason
                    ));
                    if let Some(target) = &ann.target {
                        text.push_str(&format!(
                            "      入卡目标: {}
",
                            target.text
                        ));
                    }
                }
            }
            if q.kind.is_fill() {
                let sets = fill_answers(&q.kind);
                let answers: Vec<&str> = sets
                    .iter()
                    .map(|set| set.first().map(String::as_str).unwrap_or(""))
                    .collect();
                text.push_str(&format!("\n 正确答案: {}", answers.join(" ; ")));
            } else if let QuestionKind::Translation(body) = &q.kind {
                // M5-W2：判后可见参考译文（对照学习）
                text.push_str(&format!("\n 参考译文: {}", body.reference));
            } else if let QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) =
                &q.kind
            {
                // 选择题对照（2026-09-27：判分后可见「你的选择 / 正确答案」）
                let pick_ids: Vec<u32> = if state.checked.is_empty() {
                    vec![state.sel as u32]
                } else {
                    state.checked.clone()
                };
                let texts = |ids: &[u32]| -> String {
                    ids.iter()
                        .filter_map(|id| body.options.iter().find(|o| o.id == *id))
                        .map(|o| o.text.clone())
                        .collect::<Vec<_>>()
                        .join("、")
                };
                text.push_str(&format!(
                    "
 你的选择: {} · 正确答案: {}",
                    texts(&pick_ids),
                    texts(&body.answer)
                ));
            }
            // SRS 提示行（入卡处置回传，docs/03 §3 M4）
            if let Some(hint_line) = &state.srs_hint
                && !hint_line.is_empty()
            {
                text.push_str(&format!(
                    "
 {hint_line}"
                ));
            }
            if grade.verdict == ambition_core::Verdict::Pending {
                // 裁决高亮（arbitration_sel 0=判对 1=判错）+ 键位取统一键位表口径
                let (a, b) = if state.arbitration_sel == 0 {
                    ("[❯判对]", "[ 判错]")
                } else {
                    ("[ 判对]", "[❯判错]")
                };
                text.push_str(&format!(
                    "\n ⏳ 待裁定：{a} {b} · ←/→ 选择 · Enter 确认 · Esc 跳过（裁决回流规则层）\n"
                ));
            }
        }
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
}

#[cfg(test)]
mod tests {
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
        let out = crate::common::render_sized(&s, 76, 24);
        assert!(out.contains("模拟考试"), "模考横幅: {out}");
        assert!(out.contains('⏱'), "倒计时可见: {out}");
        assert!(out.contains("34.0 / 40"), "得分行: {out}");
    }

    #[test]
    fn test_render_essay_shows_prompt_and_red_lines() {
        let q = Question::new(
            "Write an essay in English according to the prompt.".into(),
            QuestionKind::Essay(ambition_core::EssayPrompt {
                prompt_zh: "以“绿色校园”为题写一篇短文。".into(),
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
        let s = new_state(vec![q]);
        let out = crate::common::render_sized(&s, 72, 18);
        assert!(
            out.contains("题面"),
            "中文题面展示（仅题面，不进 state）: {out}"
        );
        assert!(out.contains("字数 120-180 词"), "字数红线可见: {out}");
        assert!(out.contains("限时 30 分钟"), "限时可见: {out}");
    }

    #[test]
    fn test_render_draws_stem_options_and_grade() {
        // 每个状态用全新终端渲染；按显示宽度跳过宽字符续格（ratatui 续格不落笔）
        fn render_to(s: &AppState) -> String {
            // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
            crate::common::render_sized(s, 72, 18)
        }

        let mut s = new_state(vec![sample_question()]);
        let plain = render_to(&s);
        assert!(plain.contains("university"), "题干可见");
        assert!(plain.contains("❯"), "选中光标可见");
        apply_grade(&mut s, grade(Verdict::Correct));
        let graded = render_to(&s);
        assert!(graded.contains("✔"), "判分浮层可见");
        s.finished = true;
        let final_buf = render_to(&s);
        assert!(final_buf.contains("完成"), "结束态可见: {final_buf:?}");
    }

    #[test]
    fn test_render_shows_error_line_and_fill_input_separately() {
        fn render_to(s: &AppState) -> String {
            // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
            crate::common::render_sized(s, 72, 20)
        }

        // 错误行独立可见，且没有判分浮层
        let mut s = new_state(vec![sample_question()]);
        apply_error(&mut s, "rule layer failed".into());
        let buf = render_to(&s);
        assert!(buf.contains("提交失败"), "错误行可见: {buf:?}");
        assert!(buf.contains("rule layer failed"), "错误内容可见");
        assert!(!buf.contains("得分"), "错误态无判分浮层");

        // 填空输入行
        let mut f = new_state(vec![fill_question(1)]);
        update(&mut f, Msg::Char('a'));
        update(&mut f, Msg::Char('m'));
        let fbuf = render_to(&f);
        assert!(fbuf.contains("答案:"), "输入行标签可见");
        assert!(fbuf.contains("am"), "输入内容可见");
        assert!(fbuf.contains("退出编辑"), "提示行取 hint(ctx)：编辑态");
        update(&mut f, Msg::StopEdit);
        let bbuf = render_to(&f);
        assert!(bbuf.contains("打字恢复编辑"), "提示随 ctx 变化: {bbuf:?}");
        // 多空：每空一位独立行 + 编号
        let mut m = new_state(vec![fill_question(2)]);
        update(&mut m, Msg::Char('a'));
        update(&mut m, Msg::MoveDown);
        update(&mut m, Msg::Char('b'));
        let mbuf = render_to(&m);
        assert!(mbuf.contains("答案 1"), "多空编号 1: {mbuf:?}");
        assert!(mbuf.contains("答案 2"), "多空编号 2: {mbuf:?}");
    }

    #[test]
    fn test_render_marks_checked_option_for_multi_select() {
        fn render_to(s: &AppState) -> String {
            // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
            crate::common::render_sized(s, 72, 18)
        }

        let mut s = new_state(vec![multi_question()]);
        update(&mut s, Msg::Select);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select);
        let buf = render_to(&s);
        assert!(buf.contains("✓"), "选中标记可见: {buf:?}");
        assert_eq!(s.checked, vec![0, 1], "多选两枚");
    }

    #[test]
    fn test_render_shows_pending_arbitration_and_error_tags() {
        fn render_to(s: &AppState) -> String {
            // 宽度感知收集收口到 common::fixtures::render_sized（P-2 续格口径）
            crate::common::render_sized(s, 76, 24)
        }

        let mut s = new_state(vec![sample_question()]);
        // 自评提示与档位
        let hint = render_to(&s);
        assert!(hint.contains("1-4 自评把握"), "自评入口可见: {hint:?}");
        update(&mut s, Msg::Confidence(3));
        let hinted = render_to(&s);
        assert!(hinted.contains("3/4"), "档位可见: {hinted:?}");
        // 待裁定：箭头选择高亮 + 键位提示 + 错因标签 + 人工裁决标记
        let mut pending = grade(Verdict::Pending);
        pending.error_tags = vec![ambition_core::ErrorTag::Collocation];
        pending.confidence = 0.42;
        apply_grade(&mut s, pending);
        let buf = render_to(&s);
        assert!(buf.contains("待裁定"), "待裁定行可见: {buf:?}");
        assert!(buf.contains("❯判对"), "默认高亮判对: {buf:?}");
        assert!(buf.contains("Enter 确认"), "裁决键提示: {buf:?}");
        update(&mut s, Msg::MoveDown);
        let flipped = render_to(&s);
        assert!(flipped.contains("❯判错"), "箭头切换高亮: {flipped:?}");
        assert!(flipped.contains("错因: 搭配"), "错因中文标签: {flipped:?}");
        assert!(flipped.contains("0.42"), "结构化讲解含概率: {flipped:?}");
        // SRS 提示行（入卡处置回传）
        apply_srs_hint(&mut s, Some(ambition_engine::CardAction::Create));
        let srs_buf = render_to(&s);
        assert!(
            srs_buf.contains("错词复习队列"),
            "SRS 提示行可见: {srs_buf:?}"
        );
        // 人工裁决标记
        let mut human = grade(Verdict::Correct);
        human.graded_by = ambition_core::GradedBy::Human;
        apply_grade(&mut s, human);
        let human_buf = render_to(&s);
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
        let out = crate::common::render_sized(&s, 76, 24);
        assert!(out.contains("批注（1 条）"), "批注区标题: {out}");
        assert!(out.contains("✎ 升级"), "升级图标: {out}");
        assert!(
            out.contains("Nowadays I think this is good."),
            "原句: {out}"
        );
        assert!(out.contains("This measure is beneficial."), "建议句: {out}");
        assert!(out.contains("短评"), "短评标签: {out}");
        assert!(out.contains("入卡目标"), "入卡目标: {out}");
    }
}
