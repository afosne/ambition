# Ambition 施工进度与代码评估

> **每次完成一个部分必须更新本文件**（进度日志 + 代码评估四段 + 改进建议），并写入 `PITFALLS.md`（skill `ambition-pitfall-log` 强制）。
> 定时任务 `scripts/continue_mvp.sh` 按下方 `MVP_STAGE` 自动推进，直到 MVP 完成。

MVP_STAGE: 2/6

## MVP 阶段定义（M1–M6，对应 DESIGN.md §21；M7+ 属于云/Web，不在本次 MVP 内）

| # | 阶段 | 内容 | 状态 |
|---|---|---|---|
| 1 | M1 骨架 | core/store/engine/tui + 规则判题 + 打字通底座 + 游戏底座 | ✅ 完成（2026-09-24，提交 5271323） |
| 2 | M2 出题 | llm crate（OpenAI/Anthropic 协议）+ 结构化出题 + Jev 质检回路 + qpack 导入导出 + seed 题库 | ⬜ 待施工 |
| 3 | M3 判题 | jev crate 客户端 + 待裁定桶 + 错因分类 + 中式英语错式库 + 成本台账 | ⬜ 待施工 |
| 4 | M4 记忆 | FSRS 接线 fsrs-rs + 错词本/义项/句式 + 假对上线 + 复习队列 | ⬜ 待施工 |
| 5 | M5 写作 | composite scoring + 批注 + 模板作文填槽 + 翻译题型 + 套卷估分 | ⬜ 待施工 |
| 6 | M6 闭环 | 笔记/日记/TODO + 画像聚合 + 个性化出题 + 周报 + 游戏全量 | ⬜ 待施工 |

> 自动推进规则：定时任务读 `MVP_STAGE: n/6` → 唤起 pi 执行第 n 阶段 → 成功则 n+1 并**连跑下一阶段**，失败不推进、下轮重试；`6/6` 且状态全 ✅ 时停止。

---

## 进度日志

### 2026-09-24 · M1 骨架完成

- workspace 8 crates：`ambition-core / typing / gamify / grader / srs / store / engine / tui`（bin `ambition`，单机答题流：j/k/Enter/q + 判分浮层）；
- 质量门禁落地：`.githooks/pre-commit` 七步（fmt / clippy -D / 禁占位参数 / 函数↔测试配对脚本 / cargo test / machete / 通过）+ `commit-msg` Conventional；
- 44 测试全绿、clippy 零告警、函数↔测试 100% 配对；提交 `5271323`、`3863dc8`；
- 基建：定时施工任务（schtasks 15min + 连跑循环 + 任务锁）、雷点 skill、本文档与 PITFALLS.md。

### M1 代码评估（四段）

- **代码质量**：44/44 测试通过；`clippy -D warnings` 零告警；每个非测试函数都有 `test_` 前缀配对测试（钩子强制，脚本校验）；生产代码无 `unwrap/expect`（解析失败走显式兜底）；命名与文档分册同步（docs/01-engineering 规范已进钩子）。
- **实际使用**：`cargo build` 后 `ambition.exe` 可进 TUI 答题（1 道 seed 题、选择题可提交/判分/下一题）。**边界**：仅内存库（进程退出即失，`open(path)` 文件库属 M2 基建）；填空题输入行未做（TODO M2）；无 Jev/LLM（按计划 M2/M3）。
- **代码耦合度**：依赖严格单向无环：`core ← {typing,grader,srs,gamify} ← store ← engine ← tui`；grader/srs/typing 均为纯函数库（可被 httpd 复用）。**两处欠账**：①engine 直调 `gamify::on_event`（未走 docs/02 定义的 `EventBus` trait，旁路解耦不彻底）；②gamify 效果只返回**不落库**（`save_game_state` 已有能力但 engine 没接线）。
- **鲁棒性**：作答落库单事务（attempts+grades+error_tags 原子）✓；内存库测试隔离（每测试独立 pool）✓；**欠账**：事件 `Mutex` 中毒按空静默吞（无日志）；TUI 判题失败降级成 `Pending` 展示（错误与业务态混淆，见 PITFALLS P-8）；store 枚举列用 `format!("{:?}")` 序列化（Debug 字符串改名即破，应 serde snake_case）；`question_content` 位置元组（schema 演化脆弱，应具名结构）。

### M1 改进建议（M2 开工即处理，勾=完成）

- [ ] 1. engine 接线 gamify 效果持久化：提交后 `apply(effects) → save_game_state`
- [ ] 2. `Engine::open(path)` 文件库 + `schema_version` 迁移表（docs/02-store §2）
- [ ] 3. 引入 `EventBus` trait 替换 engine→gamify 直调（docs/02 DomainEvent 已定义）
- [ ] 4. store 枚举/负载列改 serde snake_case；`items` 列更名 `payload`；位置元组 → 具名结构
- [ ] 5. TUI 采集 timing + 自评 → 接 `is_guess`/`srs::decide_action`（假对检测上线）
- [ ] 6. TUI 错误独立展示（Error 行 ≠ Pending 态）
- [ ] 7. seed 题库加载器（`data/seed`）+ qpack 导入（M2 主线）
- [ ] 8. 引入 `tracing`（事件/降级路径可见）

### 2026-09-23 · 定时施工链路修复（自动化基建，非 MVP 阶段）

- **背景**：手动跑 `scripts\continue_mvp.cmd` 报 `'（Windows' is not recognized`；日志 03:54 阶段 2 唤起后 pi 静默 exit=1；03:57 复现会话发现 agent 永远“等待中”退出（详见 PITFALLS P-7 镜像坑 / P-10 / P-11）。
- **修复**：①`.gitattributes` 增 `*.cmd text eol=crlf`、两个 .cmd 转 CRLF（cmd 解析报错消失）；②锁继承 `AMBI_LOCK_HELD=1`（acquire 继承 / release 跳过）解开脚本锁与 agent 锁死锁；③空烧防护：pi rc=0 但 `git HEAD` 无变化不推进阶段；④心跳 owner 由 `$$` 改 `$PPID`（孤儿锁/误接管两类隐患一并消除）；⑤pi 加 `--verbose`、清 settings.json 死模型条目（每轮 Warning 消失）；⑥释放 03:57 遗留孤儿锁。
- **验证**：`bash -n` ×2、cmd DRY_RUN 全路径、锁继承/release 跳过/陌生人拒锁 ×3、心跳 delta=30s、settings JSON 合法；提交过七步钩子 7/7。提交 `4103637`。
- **残余风险**：03:54 那次 pi 启动静默 exit=1 根因未定位（无会话无输出，疑瞬时网络/供应商故障）——`--verbose` 已就位，下次真实唤起取证。

#### 本项代码评估（四段）

- **代码质量**：无 Rust 改动，钩子七步 7/7 全过；bash 语法 `bash -n` 全绿；修复点均交叉引用 PITFALLS（P-7/P-10/P-11），脚本注释同步。
- **实际使用**：手动入口与 schtasks 定时入口共用同一 .cmd，已实测 dry-run 无报错且走到“DRY_RUN ok”；真实施工链路待下一次定时触发（04:29）首验。
- **代码耦合度**：改动收敛在 `scripts/` 与 `.gitattributes`，业务 crate 零接触；锁协议仍单点 `task_lock.sh`（唯一 owner=外层脚本），skill/prompt 只经它交互，无新增耦合。
- **鲁棒性**：新增“rc=0 但无提交=失败”错误路径；锁三态（刷新/过期接管/拒锁）实测闭环；`unset AMBI_LOCK_HELD` 防环境泄漏误继承。未覆盖：pi 启动静默失败仍无结构化退出信息（靠 --verbose 日志补盲）。

**后续观察（下次真实唤起时做）**：
- [ ] 9. 用 `--verbose` 日志定位 03:54 型 pi 启动静默 exit=1 根因；确认阶段 2 正常推进到 3/6。

---

（后续阶段完成后按同格式追加：进度日志 + 四段评估 + 改进建议勾选。）
