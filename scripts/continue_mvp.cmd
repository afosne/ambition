@echo off
REM Ambition 定时施工任务入口（Windows 任务计划程序调用）
"C:\Program Files\Git\bin\bash.exe" -lc "export PATH=/d/nodejs:$HOME/.cargo/bin:$PATH; exec bash /d/rust/ambition/scripts/continue_mvp.sh"
