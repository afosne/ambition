#!/usr/bin/env bash
# Ambition 施工任务锁：mkdir 原子抢占 + 心跳 + 过期接管
# 用法：task_lock.sh acquire | release | status
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOCK="$ROOT/.pi-task"
HEART="$LOCK/heartbeat"
STALE_SECONDS="${AMBI_TASK_STALE:-3600}"

acquire() {
  # 外层任务（continue_mvp.sh）已持锁时，其子进程（pi agent 按 skill 抢锁）直接继承，
  # 否则脚本锁与 agent 锁互斥死锁：agent 永远抢不到、只能“等待中”退出（P-10）
  if [ "${AMBI_LOCK_HELD:-}" = "1" ]; then
    echo "[lock] 外层任务已持锁（AMBI_LOCK_HELD），子进程继承"
    return 0
  fi
  if mkdir "$LOCK" 2>/dev/null; then
    # 锁主人 = 调用方外层脚本（如 continue_mvp.sh），不是本脚本自己：
    # 本脚本执行完就退出，绑 $$ 的心跳循环会立刻死、心跳永不刷新（P-10 附带坑）
    local owner="$PPID"
    echo "$owner" > "$LOCK/pid"
    date +%s > "$HEART"
    # 后台心跳：主人存活期间每 30s 刷新；主人死亡 → 停止刷新 → 3600s 后可被接管
    (
      while kill -0 "$owner" 2>/dev/null; do
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
    echo "$PPID" > "$LOCK/pid"
    date +%s > "$HEART"
    return 0
  fi
  echo "[lock] 已有任务在进行（心跳 $((now - last))s 前，pid=$(cat "$LOCK/pid" 2>/dev/null)）→ 等待"
  return 1
}

release() {
  # 子进程不释放外层任务的锁（锁生命周期归 continue_mvp.sh 的 trap）
  if [ "${AMBI_LOCK_HELD:-}" = "1" ]; then
    echo "[lock] 锁归外层任务所有，release 跳过（由外层 trap 释放）"
    return 0
  fi
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
