@echo off
REM 删除 Ambition 定时施工任务（Windows 入口，双击即删）
"C:\Program Files\Git\bin\bash.exe" -lc "exec bash /d/rust/ambition/scripts/uninstall_task.sh"
pause
