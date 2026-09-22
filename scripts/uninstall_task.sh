#!/usr/bin/env bash
# 删除 Ambition 定时施工任务（Windows 任务计划程序 AmbitionMVP）
# 用法：uninstall_task.sh                 （真实删除）
#       AMBI_DRY_RUN=1 uninstall_task.sh  （只演练，不删）
set -u
export PATH="$HOME/.cargo/bin:/d/nodejs:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
TASK="AmbitionMVP"
LOG="$ROOT/.pi-task.log"
log() { echo "[$(date '+%F %T')] $*" >>"$LOG"; }

# 1) 任务在不在？
if ! schtasks.exe //Query //TN "$TASK" >/dev/null 2>&1; then
  echo "定时任务 $TASK 不存在，无需删除"
  log "uninstall：任务不存在，跳过"
  bash scripts/task_lock.sh status
  exit 0
fi

schtasks.exe //Query //TN "$TASK" //FO LIST 2>/dev/null | grep -E "TaskName|Next Run" | sed 's/^/  /'

# 2) 演练模式：只说会做什么
if [ "${AMBI_DRY_RUN:-0}" = "1" ]; then
  echo "DRY_RUN ok：将执行 schtasks //Delete //TN $TASK //F（未删除）"
  log "uninstall DRY_RUN：未删除"
  bash scripts/task_lock.sh status
  exit 0
fi

# 3) 删除（退出码判断成败；不 grep 输出文案，避免本地化差异）
if schtasks.exe //Delete //TN "$TASK" //F >/dev/null 2>&1; then
  echo "✓ 已删除定时任务：$TASK"
  log "uninstall：已删除任务 $TASK"
else
  echo "✗ 删除失败（权限？请以当前用户重开终端再试）"
  log "uninstall：删除失败"
  exit 1
fi

# 4) 复核
if schtasks.exe //Query //TN "$TASK" >/dev/null 2>&1; then
  echo "✗ 复核：任务仍存在（异常）"
  exit 1
fi
echo "✓ 复核：任务已不存在"

# 5) 现场说明
echo "说明："
echo "  · 删除只停止未来触发；**正在运行的施工循环不会被中断**（会跑完当前阶段链）"
echo "  · 如需中止运行中的任务：结束对应 bash 进程后执行 bash scripts/task_lock.sh release"
bash scripts/task_lock.sh status
