---
name: ambition-verify
description: Ambition 项目测试与验收流程。凡改完代码需要验证、提交前把关、或用户说"测试、验收、跑一遍、过门禁、检查一下"时必用本 skill——三连验证（fmt/clippy/test）、七步钩子、结构门禁、TUI 冒烟、按改动类型的专项检查、失败归档 PITFALLS。接手 ambition 任何编码任务的"验证/交付"环节先读本文件（记坑与进度纪律见 ambition-pitfall-log）。
---

# Ambition 测试与验收流程

你在 `D:/rust/ambition`（Rust workspace，11 crates：core/llm/jev/grader/gen/store/engine/srs/gamify/typing/tui）。本 skill 约束**怎么算测过、怎么算能交**——验收不通过不许标完成、不许提交。

## 0. 开工前

1. `read PITFALLS.md`（同根因坑不许踩第二次）与 `.pi/skills/ambition-pitfall-log/SKILL.md`（记坑/进度纪律）；
2. 确认本次改动范围（哪几个 crate、是代码/文档/资源哪类）→ 决定跑下面哪几项；
3. 基线：`cargo test` 全绿（近期约 313 用例、13 组）——**测试数只增不减是纪律**，红一个都不算过。

## 1. 快速三连（每次改动必跑，顺序固定）

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # P-3：Windows git-bash 默认 PATH 没有 cargo
cargo fmt && cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test 2>&1 | grep -E "^test result|FAILED|^error" | awk '{p+=$4; f+=$6} END {print "passed="p, "failed="f}'
```

- 任何一项红 → 停下修到全绿；**不许**为了过钩子放松钩子（禁删检查、禁加 allow）；
- 测试输出用 grep/awk 汇总成 `passed=N failed=0`，别肉眼扫几百行。

## 2. 提交前 = 七步钩子（唯一合法门禁）

```bash
bash .githooks/pre-commit      # 或直接 git commit（钩子自动执行）
```

七步：①`fmt --check` ②`clippy -D warnings` ③占位参数 grep ④**结构门禁** ⑤`cargo test` ⑥machete ⑦通过；`commit-msg` 强制 Conventional Commits。
结构门禁自查（最容易被拦）：

- 全仓 `.rs` **≤500 行**（src 与 examples）；
- 每个 src 非测试 `fn` 必须存在 `fn test_<fn名>_…`（**前缀匹配**：`test_key_ctx_routes_states` 能配 `key_ctx`，但 `test_is_…_key_ctx…` 不行）；测试 helper 放 `mod tests` 内豁免。

## 3. 专项检查（按改动类型追加，不是三连就完）

| 改动类型 | 必查 |
|---|---|
| TUI 键位/渲染 | §4 真机冒烟 + 键位总则断言（docs/03 §5.3：非填空 ctx 逐字母 a-z → None、提示行取 `keys::hint` 不硬编码） |
| grader 判分 | `error_tags` 各语言版 `version` == `ERROR_TAGS_VERSION`；错因中文标签渲染；改措辞必升 version |
| gen 出题 / i18n | prompt 占位全替换（断言 `!contains("{{")`）；l2 选版缺版回退 en；`{{l1}}/{{l2}}` 渲染为语言名 |
| seed 题库 | 改题目内容后必重跑 `cargo run -p ambition-gen --example seed_gen` 再提交（幂等覆盖 core.qpack.json） |
| 资源/数据目录 | `data/` 只留用户可导入内容（seed/chinglish）；提示词与判分语料必须在 crate `assets/` 且纯 `include_str!`（无任何运行时读取） |
| store/迁移 | 迁移只增不改；export→import 行数一致 |

## 4. TUI 冒烟（Windows，P-14 产物断言法）

```bash
cargo build -p ambition-tui
( ./target/debug/ambition.exe > /tmp/tui_smoke.log 2>&1 & ); sleep 3
taskkill //F //IM ambition.exe
grep -c panic /tmp/tui_smoke.log          # 必须 0
grep -o "<本次预期的提示文案>" /tmp/tui_smoke.log   # 渲染产物断言（未改文案时抽旧文案确认在场）
```

- **禁**：`ambition.exe </dev/null` 逼 EOF（crossterm 不吃管道关闭，卡死 120s+）；python `pty`（Windows 无 termios）；
- 交互键位无法自动化 → 靠单测（`TestBackend` 快照，宽字符按 unicode-width 跳续格，P-2）+ 手动 `cargo run -p ambition-tui` 抽查；
- 日志里文案被光标定位符打断（`[x;H`），grep **短词**（如 `Space`、`Ctrl+Q`）而非整句。

## 5. 环境坑速查（详见 PITFALLS.md）

- **check 与 test 结果矛盾**（源码明明有、check 说找不到，test 反而绿）→ `cargo clean -p <crate>`，反复复现就全量 `cargo clean`；**别去改源码**（P-17）；
- 编辑器飘红但 CLI 绿 → 重启 rust-analyzer（旧分析缓存）；
- 编辑报 `Could not find the exact text` → 重新 `sed -n` 取当下原文再写 oldText，尤其自己几轮前改过的文件（P-5）。

## 6. 收尾（验收通过后的双归档）

1. **坑**：已解决的坑追加 `PITFALLS.md`（五要素：ID/症状/根因/解决/预防；同根因更新原条目不新开）；
2. **进度**：`PROGRESS.md` 进度日志 + 四段评估（代码质量/实际使用/代码耦合度/鲁棒性）+ 改进建议勾选；**绝不改 `MVP_STAGE` 行**（阶段号归外层推进）；
3. **提交**：Conventional Commits；代码与 `docs(progress)` 分两笔；确认 `git status` 干净。

## 测试用例处置

- 验证 **skill 本身** 的测试用例（prompt 文件）：**测试成功即删除**，不留在仓库；
- 验证 **函数/方法** 的单元测试：属于代码资产，**随函数保留**——改动/删除函数时必须同步改动对应测试（结构门禁会拦）。
