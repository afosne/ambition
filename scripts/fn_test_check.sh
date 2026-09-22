#!/usr/bin/env bash
# 函数↔测试配对检查：非测试代码中的每个 fn 必须存在同前缀测试 fn test_<name>_*
set -euo pipefail
cd "$(dirname "$0")/.."
shopt -s nullglob
fail=0

for f in crates/*/src/*.rs crates/*/src/**/*.rs; do
  [ -f "$f" ] || continue
  # 跳过 mod tests 块（从列 0 的 mod tests 到列 0 的 }）
  names=$(awk '
    /^mod tests/ { skip = 1; next }
    skip && /^\}/ { skip = 0; next }
    skip { next }
    match($0, /(pub )?(async )?fn [a-z0-9_]+/) {
      s = substr($0, RSTART, RLENGTH)
      sub(/.*fn /, "", s)
      if (s != "main" && s !~ /^test_/) print s
    }' "$f" | sort -u)

  for n in $names; do
    if ! grep -qE "fn test_${n}(_|[ (])" "$f"; then
      echo "  ✗ 缺少测试: $f :: fn $n  →  需要 fn test_${n}_..."
      fail=1
    fi
  done
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "  ✓ 所有函数均有配对测试"
