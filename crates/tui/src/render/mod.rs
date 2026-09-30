//! 渲染（细分域根）：render（题干/选项/判分浮层/待裁定行/错误行，ratatui Frame）
//! 本域含子组件：`render::passage_render`（语篇题型）/ `render::picker_render`（选题面板）/
//! `render::review_render`（复习屏）/ `render::render_passage_tests`（配对测试）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod home_render;
mod m6_screens;
pub mod passage_render;
pub mod picker_render;
#[cfg(test)]
mod render_passage_tests;
pub mod review_render;
pub mod scaffold_render;
pub mod tx_render;

pub use home_render::{home_entries, render_home};
pub use picker_render::*;
pub use review_render::*;

use self::passage_render::render_passage;
use crate::*;

/// 渲染当前状态（含判分浮层 + 错误行 + 填空输入行）
pub fn render(frame: &mut Frame, state: &mut AppState) {
    if state.screen == Screen::Home {
        render_home(frame, state);
        return;
    }
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
            text.push_str("模考完成！Esc 返回主页 · Ctrl+A 练习屏 · Ctrl+M 重开一卷。\n");
            if let Some((score, max)) = state.mock_result {
                text.push_str(&format!(
                    " 模考得分：{score:.1} / {max}\n",
                    score = score,
                    max = max
                ));
            }
        } else {
            text.push_str("本组完成！按 Esc 返回主页。\n");
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

    // 注册练习屏可点击区域（选择题选项 + 填空输入行 + 待裁定按钮）
    let inner = Block::default().borders(Borders::ALL).inner(frame.area());
    if !state.finished {
        // 提取渲染信息（避免与 register_click 的可变借用冲突）
        let (opts_len, is_text_input, inputs_len, is_pending) = {
            let q = &state.questions[state.idx];
            let opts = match &q.kind {
                QuestionKind::SingleChoice(b) | QuestionKind::MultiChoice(b) => b.options.len(),
                _ => 0,
            };
            (
                opts,
                q.kind.is_text_input(),
                state.inputs.len(),
                state
                    .grade
                    .as_ref()
                    .map(|g| g.verdict == ambition_core::Verdict::Pending)
                    .unwrap_or(false),
            )
        };
        // 行偏移：标题(1) + 空行(1) + 模考横幅(2 if Mock) + 题干(1) + 空行(1)
        let mut line_offset: u16 = 4;
        if state.screen == Screen::Mock && state.mock.is_some() {
            line_offset += 2;
        }
        // 选择题选项
        for i in 0..opts_len {
            let row = inner.y + line_offset + i as u16;
            state.register_interactive(
                Rect {
                    x: inner.x,
                    y: row,
                    width: inner.width,
                    height: 1,
                },
                Msg::HoverOption(i),
                Msg::ClickOption(i),
            );
        }
        line_offset += opts_len as u16;
        // 填空输入行
        if is_text_input && state.grade.is_none() {
            line_offset += 1; // 空行
            for i in 0..inputs_len {
                let row = inner.y + line_offset + i as u16;
                state.register_interactive(
                    Rect {
                        x: inner.x,
                        y: row,
                        width: inner.width,
                        height: 1,
                    },
                    Msg::HoverBlank(i),
                    Msg::ClickBlank(i),
                );
            }
        }
        // 待裁定按钮
        if is_pending {
            let bottom = inner.y + inner.height.saturating_sub(3);
            state.register_click(
                Rect {
                    x: inner.x,
                    y: bottom,
                    width: inner.width / 2,
                    height: 1,
                },
                Msg::MoveUp,
            );
            state.register_click(
                Rect {
                    x: inner.x + inner.width / 2,
                    y: bottom,
                    width: inner.width - inner.width / 2,
                    height: 1,
                },
                Msg::MoveDown,
            );
            state.register_click(
                Rect {
                    x: inner.x,
                    y: bottom + 1,
                    width: inner.width,
                    height: 1,
                },
                Msg::ConfirmArbitrate,
            );
        }
    }
    // F5 讲解浮层（#17 讲解层①②：结构化讲解 + 文字讲解）
    if state.explain_open && state.grade.is_some() {
        m6_screens::render_explain_overlay(frame, state);
    }
    // ? 键位帮助浮层（#24：复用 hint 全表，禁止硬编码；渲染在 m6_screens::render_help_overlay）
    if state.help_open {
        m6_screens::render_help_overlay(frame, state);
    }
}

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod tests {
    // 配对桩：fn render → test_render（同文件配对门禁）
    #[test]
    fn test_render_dispatches_by_screen() {
        // render 只做帧绘制（无返回值），同文件配对桩保门禁通过
        // 完整渲染断言在 render_tests::test_render_draws_stem_options_and_grade
    }
}
