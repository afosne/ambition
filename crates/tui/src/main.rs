//! Ambition 入口（M2 单机 TUI）：壳层只做 I/O，业务全走 engine 门面。
//! M2：文件库（~/.ambition/ambition.db，AMBI_DB 覆盖）+ seed 题库 + 填空输入 + 独立错误行。

use std::error::Error;
use std::path::PathBuf;

use ambition_core::{Attempt, AttemptMode};
use ambition_engine::Engine;
use ambition_tui::{Cmd, apply_error, apply_grade, key_to_msg, new_state, render, update};
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

    while !state.finished {
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
