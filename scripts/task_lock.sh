#!/usr/bin/env bash
# Ambition 施工任务锁：mkdir 原子抢占 + 心跳 + 过期接管
# 用法：task_lock.sh acquire | release | status
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOCK="$ROOT/.pi-task"
HEART="$LOCK/heartbeat"
STALE_SECONDS="${AMBI_TASK_STALE:-3600}"

acquire() {
  if mkdir "$LOCK" 2>/dev/null; then
    echo "$$" > "$LOCK/pid"
    date +%s > "$HEART"
    # 后台心跳：持锁进程存活期间每 30s 刷新
    (
      while kill -0 "$$" 2>/dev/null; do
        date +%s > "$HEART" 2>/dev/null || break
        sleep 30
      done
    ) >/dev/null 2>&1 &
    return 0
  fi
  # 已有锁：判定是否过期
  local last
  last="$(stat -c %Y "$HEART" 2>/dev/null || echo 0)"
  local now
  now="$(date +%s)"
  if [ $((now - last)) -gt "$STALE_SECONDS" ]; then
    echo "[lock] 检测到过期锁（$((now - last))s 无心跳），接管"
    rm -rf "$LOCK"
    mkdir "$LOCK" || return 1
    echo "$$" > "$LOCK/pid"
    date +%s > "$HEART"
    return 0
  fi
  echo "[lock] 已有任务在进行（心跳 $((now - last))s 前，pid=$(cat "$LOCK/pid" 2>/dev/null)）→ 等待"
  return 1
}

release() {
  rm -rf "$LOCK" 2>/dev/null
  echo "[lock] 已释放"
}

status() {
  if [ -d "$LOCK" ]; then
    echo "[lock] 持有中 pid=$(cat "$LOCK/pid" 2>/dev/null) 心跳=$(stat -c %Y "$HEART" 2>/dev/null)"
  else
    echo "[lock] 空闲"
  fi
}

case "${1:-status}" in
  acquire) acquire ;;
  release) release ;;
  status) status ;;
  *) echo "usage: $0 acquire|release|status"; exit 2 ;;
esac
