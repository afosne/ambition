//! Ambition 入口（M2 单机 TUI）：壳层只做 I/O，业务全走 engine 门面。
//! M2：文件库（~/.ambition/ambition.db，AMBI_DB 覆盖）+ seed 题库 + 填空输入 + 独立错误行。

mod main_support;
use main_support::{db_path, game_summary};

use std::collections::BTreeMap;
use std::error::Error;

use ambition_core::{Attempt, AttemptMode, now_unix};
use ambition_engine::{Engine, Rating};
use ambition_tui::{
    Cmd, ReviewCard, Screen, apply_error, apply_grade, apply_mock_finish, apply_mock_start,
    apply_review_cards, apply_review_rated, apply_scaffold_check, apply_scaffold_result,
    apply_scaffold_templates, apply_srs_hint, apply_tx_align, apply_tx_compare, apply_tx_history,
    apply_tx_practice, apply_tx_result, key_to_msg, new_state, render, tx_opts, update,
};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

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

    // finished 后仍可驻留非答题屏（复习/模板/模考/翻译工作台族，M5-W4..W7），直到显式退出
    while !state.finished
        || matches!(
            state.screen,
            Screen::Review
                | Screen::Scaffold
                | Screen::Mock
                | Screen::Tx
                | Screen::TxAlign
                | Screen::TxMyTurn
                | Screen::TxHistory
                | Screen::Notes
                | Screen::Diary
                | Screen::Todos
                | Screen::Profile
                | Screen::Games
                | Screen::Wordbook
        )
    {
        terminal.draw(|f| render(f, &state))?;
        let event = event::read()?;
        if matches!(event, Event::Paste(_)) {
            // 纸笔口径禁粘贴（M5-W8 docs/10 §5：Pasted 违规先行反馈；卷面扫描随手交屏 hand_in）
            state.error = Some("检测到粘贴：纸笔口径禁用（卷面 Pasted 违规）".to_string());
            continue;
        }
        let Event::Key(key) = event else {
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
            Cmd::TxTranslate => {
                let opts = tx_opts();
                match engine.translate(&state.tx_source, &opts).await {
                    Ok(tx) => apply_tx_result(&mut state, tx),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::TxAlign => {
                // 参考译文 = 同句出译（llm 缓存命中即回）→ 段级对齐（docs/11 §2）
                let opts = tx_opts();
                let source = state.tx_source.clone();
                match engine.translate(&source, &opts).await {
                    Ok(reference) => {
                        match engine
                            .translate_align(&source, &reference.target, &opts)
                            .await
                        {
                            Ok(alignment) => apply_tx_align(&mut state, alignment),
                            Err(err) => apply_error(&mut state, err.to_string()),
                        }
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::TxMyTurn => {
                let opts = tx_opts();
                let source = state.tx_source.clone();
                let mine = state.tx_mine.clone();
                match engine.translate_my_turn(&source, &mine, &opts).await {
                    Ok(compare) => apply_tx_compare(&mut state, compare),
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::TxHistory => match engine.translate_history(None, 50).await {
                Ok(list) => match engine.translate_favorites().await {
                    Ok(favorites) => apply_tx_history(&mut state, list, favorites),
                    Err(err) => apply_error(&mut state, err.to_string()),
                },
                Err(err) => apply_error(&mut state, err.to_string()),
            },
            Cmd::TxFavorite => {
                let Some(tx) = state.tx_history.get(state.tx_sel).cloned() else {
                    continue;
                };
                let on = !state.tx_favorites.contains(&tx.tx_id);
                match engine.translate_favorite(&tx.tx_id, on).await {
                    Ok(()) => {
                        if on {
                            state.tx_favorites.push(tx.tx_id);
                        } else {
                            state.tx_favorites.retain(|id| *id != tx.tx_id);
                        }
                        state.error = None;
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::TxToPractice => {
                let Some(tx) = state.tx_history.get(state.tx_sel).cloned() else {
                    continue;
                };
                match engine.translate_to_practice(&tx.tx_id).await {
                    Ok(question) => apply_tx_practice(&mut state, question),
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
            Cmd::WorkspaceLoad => {
                state.workspace_draft.clear();
                state.error = None;
                let rows = match state.screen {
                    Screen::Notes => engine.note_search("", 20).await.map(|xs| {
                        xs.into_iter()
                            .map(|n| format!("{} · {:?}", n.title, n.kind))
                            .collect()
                    }),
                    Screen::Diary => engine.diary_list(20).await.map(|xs| {
                        xs.into_iter()
                            .map(|d| {
                                format!(
                                    "{} · {}",
                                    d.date,
                                    d.body.lines().next().unwrap_or_default()
                                )
                            })
                            .collect()
                    }),
                    _ => engine.todo_list(true, 20).await.map(|xs| {
                        xs.into_iter()
                            .map(|t| format!("{} · {:?}", t.title, t.status))
                            .collect()
                    }),
                };
                match rows {
                    Ok(items) => state.workspace_items = items,
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::ProfileLoad => match engine.refresh().await {
                Ok(snapshot) => {
                    state.error = None;
                    state.workspace_items = if snapshot.weak_points.is_empty() {
                        vec![format!(
                            "统计作答 {} 次，当前范围暂无错因记录。",
                            snapshot.attempts
                        )]
                    } else {
                        snapshot
                            .weak_points
                            .iter()
                            .map(|w| {
                                format!(
                                    "{} · {} · 密度 {:.2} · {:?}",
                                    w.tag.label_zh(),
                                    w.knowledge,
                                    w.density,
                                    w.trend
                                )
                            })
                            .collect()
                    };
                }
                Err(err) => apply_error(&mut state, err.to_string()),
            },
            Cmd::GameLoad => {
                state.workspace_items = game_summary(&engine.game_state());
                state.error = None;
            }
            Cmd::VocabLoad => match engine.vocab_wordbook().await {
                Ok(words) => {
                    state.workspace_draft.clear();
                    state.workspace_items = words
                        .into_iter()
                        .map(|w| format!("{w} · 生词卡已入队"))
                        .collect();
                    state.error = None;
                }
                Err(err) => apply_error(&mut state, err.to_string()),
            },
            Cmd::VocabLookup(query) => {
                let (word, context) = query
                    .split_once('|')
                    .map(|(w, c)| (w.trim(), Some(c.trim())))
                    .unwrap_or((query.trim(), None));
                match engine.lookup(word, context) {
                    Ok(result) => {
                        state.error = None;
                        state.workspace_items = result
                            .senses
                            .iter()
                            .map(|s| format!("{} · {} · {}", s.pos, s.def_zh, s.def_en))
                            .collect();
                        if let Some(s) = result.selected {
                            state.workspace_items.insert(
                                0,
                                format!(
                                    "语境匹配：{}（置信度 {:.0}%）",
                                    s.def_zh,
                                    result.confidence * 100.0
                                ),
                            );
                        }
                        if result.senses.is_empty() {
                            state.workspace_items = vec![format!("{} 暂无本地义项", word)];
                        }
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::VocabEnroll(text) => match engine.vocab_new_words(&text, &[]).await {
                Ok(words) => {
                    state.error = None;
                    state.workspace_items = words
                        .iter()
                        .map(|w| format!("{w} · 已加入生词复习"))
                        .collect();
                    if words.is_empty() {
                        state.workspace_items = vec!["文本中没有识别到尚未掌握的本地词条。".into()];
                    }
                }
                Err(err) => apply_error(&mut state, err.to_string()),
            },
            Cmd::GameAction(action) => {
                use ambition_gamify::GameItem as I;
                let items = [
                    I::Hint,
                    I::Eliminate,
                    I::DoubleXp,
                    I::StreakFreeze,
                    I::Makeup,
                ];
                let result = if action == 12 {
                    engine.game_set_enabled(!engine.game_state().enabled).await
                } else if (1..=5).contains(&action) {
                    engine
                        .game_use_item(items[(action - 1) as usize])
                        .await
                        .map(|_| ())
                } else if (6..=10).contains(&action) {
                    engine
                        .game_purchase_item(items[(action - 6) as usize], 1)
                        .await
                } else {
                    continue;
                };
                match result {
                    Ok(()) => {
                        state.workspace_items = game_summary(&engine.game_state());
                        state.error = None;
                    }
                    Err(err) => apply_error(&mut state, err.to_string()),
                }
            }
            Cmd::WorkspaceSave(body) => {
                let result = match state.screen {
                    Screen::Notes => {
                        let title = body.lines().next().unwrap_or("新笔记").trim().to_string();
                        engine
                            .note_upsert(title, body.clone(), ambition_core::NoteKind::Free, vec![])
                            .await
                            .map(|_| ())
                    }
                    Screen::Diary => engine
                        .diary_write(&ambition_engine::today(), &body)
                        .await
                        .map(|_| ()),
                    _ => engine
                        .todo_add(
                            ambition_core::TodoKind::ExamPrep,
                            body.lines().next().unwrap_or("待办").to_string(),
                            None,
                        )
                        .await
                        .map(|_| ()),
                };
                match result {
                    Ok(()) => {
                        let preview = body.lines().next().unwrap_or_default().to_string();
                        state.workspace_draft.clear();
                        state.workspace_items.insert(0, preview);
                        state.error = None;
                    }
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
