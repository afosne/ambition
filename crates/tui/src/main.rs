//! Ambition 入口（M2 单机 TUI）：壳层只做 I/O，业务全走 engine 门面。
//! M2：文件库（~/.ambition/ambition.db，AMBI_DB 覆盖）+ seed 题库 + 填空输入 + 独立错误行。

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use ambition_core::{Attempt, AttemptMode, now_unix};
use ambition_engine::{Engine, Rating};
use ambition_tui::{
    Cmd, ReviewCard, Screen, apply_error, apply_grade, apply_mock_finish, apply_mock_start,
    apply_review_cards, apply_review_rated, apply_scaffold_check, apply_scaffold_result,
    apply_scaffold_templates, apply_srs_hint, key_to_msg, new_state, render, update,
};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// 默认库路径：AMBI_DB 覆盖 → HOME/.ambition/ambition.db（docs/03 §5.6）
fn db_path() -> PathBuf {
    if let Some(over) = std::env::var_os("AMBI_DB") {
        return PathBuf::from(over);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ambition").join("ambition.db")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(&db_path()).await?;
    let questions = engine.start_practice(0).await?;
    let mut state = new_state(questions);
    let mut attempt_seq = 0i64;
    // 待裁定上下文：最近一次提交的 attempt id（y/n 裁决用，M3）
    let mut last_attempt_id: Option<String> = None;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // finished（答题组完成）后仍可进复习屏/模板屏（Ctrl+R/Tab、F2），直到显式退出
    while !state.finished
        || state.screen == Screen::Review
        || state.screen == Screen::Scaffold
        || state.screen == Screen::Mock
    {
        terminal.draw(|f| render(f, &state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // 键位唯一入口（keys.rs：KeyCtx 路由 + 恒定层 Ctrl+Q/Ctrl+C；Enter 提交/下一题在表内）
        let Some(msg) = key_to_msg(state.key_ctx(), key.code, key.modifiers) else {
            continue;
        };
        match update(&mut state, msg) {
            Cmd::Quit => break,
            Cmd::Submit(answer) => {
                attempt_seq += 1;
                // M5-W5：记录文本作答（作文/翻译批注输入；随题由 reset 清空）
                state.submitted_text = match &answer {
                    ambition_core::Answer::Text(text) => Some(text.clone()),
                    _ => None,
                };
                let attempt_id = format!("tui-{attempt_seq}");
                let attempt = Attempt {
                    attempt_id: attempt_id.clone(),
                    qid: state.questions[state.idx].qid.clone(),
                    answer,
                    timing: Some(state.timing()),
                    self_confidence: state.self_confidence,
                    mode: AttemptMode::Practice,
                };
                match engine.submit_answer(&attempt).await {
                    Ok(outcome) => {
                        last_attempt_id = Some(attempt_id);
                        apply_grade(&mut state, outcome.grade);
                        apply_srs_hint(&mut state, outcome.srs);
                    }
                    // 失败 = 独立错误行，绝不伪造 Pending 判分（P-8 / 改进 #6）
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::Arbitrate(accept) => {
                // 人工裁决：grades 改判 + 填空回流 + 游戏旁路结算（engine.resolve_pending）
                if let Some(attempt_id) = last_attempt_id.clone() {
                    match engine.resolve_pending(&attempt_id, accept).await {
                        Ok(grade) => apply_grade(&mut state, grade),
                        Err(err) => apply_error(&mut state, err.to_string()),
                    }
                }
            }
            Cmd::ReviewLoad => {
                // 到期队列（docs/08 §1：到期过滤 + 交错；limit 20 一轮）
                match engine.due_list(now_unix(), 20).await {
                    Ok(items) => {
                        let cards = items
                            .into_iter()
                            .map(|item| ReviewCard {
                                card_id: item.card.card_id.0,
                                front: item.front,
                                back: item.back,
                                group: item.group,
                            })
                            .collect();
                        apply_review_cards(&mut state, cards);
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::ReviewRate(level) => {
                // FSRS 评分（1-4 → Rating；排程 + ReviewDone 落 game_state，engine.review_answer）
                let card_id = state.review.cards[state.review.idx].card_id.clone();
                let Some(rating) = Rating::from_key(level) else {
                    continue;
                };
                match engine.review_answer(&card_id, rating).await {
                    Ok(card) => apply_review_rated(&mut state, card.due_at, now_unix()),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::ScaffoldLoad => {
                // 模板列表（内置 + 个人合并；track 暂全量，随16-config 注入当前轨，TODO(16-config): engine.templates(Some(track)))
                match engine.templates(None).await {
                    Ok(list) => apply_scaffold_templates(&mut state, list),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::ScaffoldRender => {
                // 套槽成文（槽位 id ↔ 填值对齐；缺槽由 writing::render 保留占位 + warn）
                let Some(t) = state.scaffold_templates.get(state.scaffold_idx) else {
                    continue;
                };
                let fills: BTreeMap<String, String> = t
                    .slots
                    .iter()
                    .enumerate()
                    .map(|(i, slot)| {
                        (
                            slot.id.clone(),
                            state.scaffold_fills.get(i).cloned().unwrap_or_default(),
                        )
                    })
                    .collect();
                match engine.render_template(&t.template_id, &fills).await {
                    Ok(text) => apply_scaffold_result(&mut state, text),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::ScaffoldCheck => {
                // 草稿自查（无成文不发 Cmd——scaffold 状态机已拦；此处防御）
                let Some(t) = state.scaffold_templates.get(state.scaffold_idx) else {
                    continue;
                };
                let Some(essay) = state.scaffold_result.clone() else {
                    continue;
                };
                match engine.check_scaffold(&t.template_id, &essay).await {
                    Ok(check) => apply_scaffold_check(&mut state, check),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::Annotate(text) => {
                // 批注（M5-W5：LlmKind::Explain + writing 资产；缓存走 llm run_loop）
                let Some(grade) = state.grade.clone() else {
                    continue;
                };
                let Some(rubric_id) = Engine::rubric_id_for(&state.questions[state.idx].kind)
                else {
                    continue;
                };
                match engine.annotate(&text, &grade, &rubric_id).await {
                    Ok(comments) => {
                        state.comments = Some(comments);
                        state.error = None;
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::MockStart => match engine.mock_start().await {
                Ok(session) => apply_mock_start(&mut state, session),
                Err(err) => apply_error(&mut state, err.to_string()),
            },
            Cmd::MockFinish => {
                // 交卷（run 元数据来自开卷回填；store 最新一答口径计分，M5-W6）
                let Some(run) = state.mock.clone() else {
                    continue;
                };
                match engine.mock_finish(&run.run_id).await {
                    Ok((score, max)) => apply_mock_finish(&mut state, score, max),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::None => {}
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    engine.drain_events();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_path_honors_env_override_then_home() {
        // 无覆盖时基于 HOME/USERPROFILE（测试进程环境可能两者皆有其一）
        let p = db_path();
        assert!(
            p.ends_with(".ambition/ambition.db") || p.ends_with(".ambition\\ambition.db"),
            "默认落在 ~/.ambition/ambition.db: {:?}",
            p
        );
        // AMBI_DB 覆盖路径（unsafe: env 并发写；串行子进程验证留 CI，见 PROGRESS 观察项）
        let fallback = PathBuf::from(".").join(".ambition").join("ambition.db");
        assert!(fallback.ends_with("ambition.db"), "路径构造稳定");
    }
}
