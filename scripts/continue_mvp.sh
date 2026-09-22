#!/usr/bin/env bash
# Ambition 定时施工任务：按 PROGRESS.md 的 MVP_STAGE 逐阶段唤起 pi 施工，
# 直到 MVP（M1–M6）全部完成。互斥：已有任务在跑则等待（下一轮定时再试）；
# 本任务自身在阶段完成后**连跑下一个阶段**（唤醒逻辑）。
# 用法：continue_mvp.sh            （真实执行）
#       AMBI_DRY_RUN=1 continue_mvp.sh  （只演练锁/解析，不唤起 pi）
set -u
export PATH="$HOME/.cargo/bin:/d/nodejs:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
LOG="$ROOT/.pi-task.log"
LOCK="$ROOT/.pi-task"

log() { echo "[$(date '+%F %T')] $*" >>"$LOG"; }

# 0) 已完成？
if ! grep -q '^MVP_STAGE: ' PROGRESS.md; then
  log "PROGRESS.md 缺少 MVP_STAGE 行，中止"
  exit 1
fi
stage_line="$(grep '^MVP_STAGE: ' PROGRESS.md)"
cur="${stage_line#MVP_STAGE: }"
n="${cur%%/*}"
total="${cur##*/}"
if [ "$n" -gt "$total" ] 2>/dev/null; then
  log "MVP 全部完成（$cur），无需执行"
  echo "MVP 已全部完成（$cur）"
  exit 0
fi

declare -a LABELS=(
  "M1 骨架"
  "M2 出题：llm 协议 + 结构化出题 + 质检回路 + qpack + seed 题库"
  "M3 判题：jev 客户端 + 待裁定桶 + 错因分类 + 成本台账"
  "M4 记忆：FSRS 接线 + 错词本/义项/句式 + 假对上线 + 复习队列"
  "M5 写作：composite scoring + 批注 + 模板填槽 + 翻译题型 + 套卷估分"
  "M6 闭环：笔记/日记/TODO + 画像 + 个性化出题 + 周报 + 游戏全量"
)

# 1) 抢锁（已有任务 → 等待）
if ! bash scripts/task_lock.sh acquire >>"$LOG" 2>&1; then
  log "阶段 $n 等待中（其它任务持有锁）"
  echo "已有 ambition 任务在进行，等待完成后的下一轮"
  exit 0
fi
trap 'bash scripts/task_lock.sh release >>"$LOG" 2>&1' EXIT

# 2) 连跑循环：每轮唤起 pi 完成一个阶段，成功则推进并继续（任务自身唤醒下一阶段）
while true; do
  stage_line="$(grep '^MVP_STAGE: ' PROGRESS.md)"
  cur="${stage_line#MVP_STAGE: }"
  n="${cur%%/*}"
  total="${cur##*/}"
  if [ "$n" -gt "$total" ] 2>/dev/null; then
    log "MVP 全部完成"
    echo "MVP 已全部完成"
    break
  fi
  label="${LABELS[$((n - 1))]}"

  if [ "${AMBI_DRY_RUN:-0}" = "1" ]; then
    log "DRY_RUN：将唤起 pi 执行阶段 $n/6（$label），不实际运行"
    echo "DRY_RUN ok：下一阶段 = $label"
    break
  fi

  log "唤起 pi 执行阶段 $n/6：$label"
  pi -p --approve --name "ambition-stage-$n" \
    "你是 Ambition 项目的施工者（当前阶段：$label）。

必做顺序：
1) read .pi/skills/ambition-pitfall-log/SKILL.md 并遵守（记坑/抢锁/进度评估守则）；
2) read PITFALLS.md（同样的坑不许踩第二次）与 PROGRESS.md（当前进度与改进建议）；
3) read DESIGN.md 对应章节与相关 docs/ 分册（契约先于实现；文档先行）；
4) 执行本阶段最小可用实现；提交必须过七步钩子（不要绕过/放宽钩子）；
5) 完成后更新 PROGRESS.md：勾状态、写『进度日志』、写四段『代码评估』（质量/实际使用/耦合度/鲁棒性）、更新『改进建议』；遇到新坑写入 PITFALLS.md；
6) 完成标准：本阶段功能可运行 + 测试全绿 + git 提交成功。" >>"$LOG" 2>&1
  rc=$?

  if [ $rc -ne 0 ]; then
    log "阶段 $n 失败（exit=$rc），不推进；下一轮定时任务重试"
    echo "阶段 $n 执行失败（exit=$rc），已记录日志 $LOG"
    exit $rc
  fi

  # 推进阶段号
  next=$((n + 1))
  sed -i "s/^MVP_STAGE: .*/MVP_STAGE: $next\/$total/" PROGRESS.md
  log "阶段 $n 完成 → 推进到 $next/$total，连跑下一阶段"
  # 不退出：循环立即进入下一阶段（任务自身唤醒 pi 继续）
done

log "本轮施工结束"
