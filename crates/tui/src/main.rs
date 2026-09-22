//! Ambition 入口（M1 单机 TUI）：壳层只做 I/O，业务全走 engine 门面。

use std::error::Error;

use ambition_core::{Attempt, AttemptMode, GradeResult, GradedBy, Verdict};
use ambition_engine::Engine;
use ambition_tui::{Cmd, Msg, apply_grade, new_state, render, sample_question, update};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let engine = Engine::open_memory().await?;
    engine.add_question(&sample_question()).await?;
    let mut state = new_state(vec![sample_question()]);
    let mut attempt_seq = 0i64;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    while !state.finished {
        terminal.draw(|f| render(f, &state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let msg = match key.code {
            KeyCode::Char('q') => Msg::Quit,
            KeyCode::Down | KeyCode::Char('j') => Msg::MoveDown,
            KeyCode::Up | KeyCode::Char('k') => Msg::MoveUp,
            KeyCode::Enter if state.grade.is_some() => Msg::Next,
            KeyCode::Enter => Msg::Submit,
            _ => continue,
        };
        match update(&mut state, msg) {
            Cmd::Quit => break,
            Cmd::Submit(answer) => {
                attempt_seq += 1;
                let attempt = Attempt {
                    attempt_id: format!("m1-{attempt_seq}"),
                    qid: state.questions[state.idx].qid.clone(),
                    answer,
                    timing: None,
                    self_confidence: None,
                    mode: AttemptMode::Practice,
                };
                match engine.submit_answer(&attempt).await {
                    Ok(outcome) => apply_grade(&mut state, outcome.grade),
                    Err(err) => {
                        eprintln!("判题失败: {err}");
                        apply_grade(
                            &mut state,
                            GradeResult {
                                verdict: Verdict::Pending,
                                score: 0.0,
                                max: 1.0,
                                confidence: 0.0,
                                graded_by: GradedBy::Rule,
                                items: vec![],
                                error_tags: vec![],
                            },
                        );
                    }
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
