# Ambition 施工进度与代码评估

> **每次完成一个部分必须更新本文件**（进度日志 + 代码评估四段 + 改进建议），并写入 `PITFALLS.md`（skill `ambition-pitfall-log` 强制）。
> 定时任务 `scripts/continue_mvp.sh` 按下方 `MVP_STAGE` 自动推进，直到 MVP 完成。

MVP_STAGE: 2/6

## MVP 阶段定义（M1–M6，对应 DESIGN.md §21；M7+ 属于云/Web，不在本次 MVP 内）

| # | 阶段 | 内容 | 状态 |
|---|---|---|---|
| 1 | M1 骨架 | core/store/engine/tui + 规则判题 + 打字通底座 + 游戏底座 | ✅ 完成（2026-09-24，提交 5271323） |
| 2 | M2 出题 | llm crate（OpenAI/Anthropic 协议）+ 结构化出题 + Jev 质检回路 + qpack 导入导出 + seed 题库 | ✅ 完成（2026-09-24，提交 b39efef） |
| 3 | M3 判题 | jev crate 客户端 + 待裁定桶 + 错因分类 + 中式英语错式库 + 成本台账 | ⬜ 待施工 |
| 4 | M4 记忆 | FSRS 接线 fsrs-rs + 错词本/义项/句式 + 假对上线 + 复习队列 | ⬜ 待施工 |
| 5 | M5 写作 | composite scoring + 批注 + 模板作文填槽 + 翻译题型 + 套卷估分 | ⬜ 待施工 |
| 6 | M6 闭环 | 笔记/日记/TODO + 画像聚合 + 个性化出题 + 周报 + 游戏全量 | ⬜ 待施工 |

> 自动推进规则：定时任务读 `MVP_STAGE: n/6` → 唤起 pi 执行第 n 阶段 → 成功则 n+1 并**连跑下一阶段**，失败不推进、下轮重试；`6/6` 且状态全 ✅ 时停止。

---

## 进度日志

### 2026-09-24 · M2 出题完成

- **新 crate ×3（workspace 8→11）**：
  - `ambition-llm`：OpenAiCompat（response_format=json_schema，400/422 降级 json_object+schema 内嵌）+ Anthropic（强制 tool input_schema）双协议；`LlmRouter::structured/text`（缓存命中→供给方路由 fallback→重试≤2 temperature 递减+错误摘要→serde 校验→记账→写缓存）；`AiLedger` trait（定义在 05，SQLite 实现经 engine 装配，MemLedger 测试用）；wiremock 契约测试 46 项（两协议 wire 形态 + 401/429/5xx 分类 + 降级路径）。
  - `ambition-jev`：`JevClient::evaluate`（POST /v1/systemone，choice/score/noul 三原语编解码；429/529/5xx 指数退避≤3；15s 超时；记账 kind=jev）。**wire 契约照 docs.typesafe.ai/api 实地取证**（2026-09-23：请求 {state,model,questions}、响应 {model,answers,usage}、错误码 401/422/429/529）写入 docs/05 §2；`evaluate_many` 并发池/置信度路由留 M3。16 测试。
  - `ambition-gen`：`GenSpec` + prompt 渲染（data/prompts 可覆盖/内嵌兜底，{{l2}}/{{count}}/{{feedback_line}} 占位）+ JSON Schema 约束草稿 → `DraftQuestion::to_question`（校验失败显式 Err 触发重生成；qid=内容哈希）；`qc_question` 单 Jev 请求 fan-out 五问（unique/key/natural/distractor/difficulty_fit；pass = noul≥0.7 且 natural≥1.5，quality_score 回写 Question）；重生成≤2 轮后丢弃（tracing::warn + 全拒报 `AllRejected` 不静默）；qpack `build/parse/plan_import`（schema_version=1 校验、qid 防篡改、低质拒收 θ=0.6、explanations 随包 round-trip）。33 测试。
- **基建改进（M1 遗留 #1–#4、#6–#8）**：store 换 `schema_version` 迁移表 + MIGRATIONS 清单（只增不改）、枚举列全改 serde snake_case（禁 `format!("{:?}")`）、`items`→`payload` + 位置元组→`QuestionContent` 具名结构、新增 llm_cache/llm_usage 表；core 增 `EventBus` trait/`Provenance`/`canonical_json_hash`；gamify 增 `GameProjector`（实现 EventBus，hydrate/snapshot/drain_effects，Mutex 中毒取内层+可见信号）；engine 增 `open(path)` 文件库（WAL+busy_timeout）、seed 幂等导入、`generate/import_pack/export_pack/start_practice` 门面、提交后 publish→gamify 效果落 game_state、tracing 接线；TUI 增填空输入行（`;` 分隔多空）、提交计时→Timing、`key_to_msg` 键位收口、**判分失败独立错误行（P-8 关闭）**。
- **seed 题库**：`data/seed/en/cet4/core.qpack.json` **203 题**（≥200 达 docs/16 契约；单选/多选/填空三型，provenance=seed，explanations 随包）+ `data/prompts/question_gen.md`；`crates/gen/examples/seed_gen.rs` 可重生成（幂等）。
- **契约先行**：动手前先改 docs/02（迁移版本化/payload/具名结构）、docs/03（EventBus/TUI M2 要点/EngineError 收敛）、docs/05（M2 落地范围 + Jev wire 取证注记）、docs/06（GeneratedQuestion→Question+quality_score 字段、export_qpack/seed 格式）、docs/16（seed qpack 格式 + AMBI_* 过渡 env）。
- **质量**：**182 测试全绿**（M1 44→182，+138）、clippy `-D warnings` 零告警、函数↔测试配对 100%、七步钩子两次全过（首提被第 3 步占位参数拦下——测试里的 `_ev`，见 PITFALLS P-13）；提交 **b39efef**。
- **冒烟（P-14 方法）**：`ambition.exe` 短跑后杀 → 文件库 `questions=203 (seed)`、`schema_version=1`、UI 启动序列已写 stdout ✓。
- **新坑入册**：P-12（Rust 2024 `gen` 保留字）、P-13（占位参数 grep 扫测试码）、P-14（Windows TUI 冒烟无 pty/EOF）；P-8 标记已解决。

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

### M2 代码评估（四段）

- **代码质量**：182/182 测试全绿（M1 44→182）；`clippy -D warnings` 零告警；函数↔测试配对 100%（钩子两次拦截均修码未绕：P-13 测试码占位参数）；生产码无 `unwrap/expect`（Mutex 中毒取内层 + eprintln 可见信号）；wiremock 双协议 + Jev wire 契约测试（照 docs.typesafe.ai 实地取证，非凭印象写协议）；契约先行：开工先改 5 份分册再动码。
- **实际使用**：`ambition.exe` 真实启动 → 创建 `~/.ambition/ambition.db`（WAL）→ **203 题 seed 幂等入��**（provenance=seed）→ TUI 答选择/填空（填空输入行 `;` 分隔多空、错误独立行、提交计时）。**边界**：LLM/Jev 出题需填 key（`AMBI_LLM_KEY`/`AMBI_JEV_KEY`，无 key 显式 `Offline` 不静默）；`generate/import_pack/export_pack` 目前是 engine API（TUI 键位入口 `i/x` 未接，docs/03 预留）；seed 仅 cet4 轨（其它轨靠 seed_gen 扩）；题型仍为 M1 三种（完形/阅读/七选五/语法填空按 DESIGN §13.4 属 M2，但本阶段标签未列，见改进建议 #15）。
- **代码耦合度**：11 crates 依赖单向无环：`core ← {llm,jev,typing,grader,srs,gamify} ← {gen,store} ← engine ← tui`；**AI 唯一出口红线落地**：`AiLedger` trait 定义在 llm、SQLite 实现经 engine 装配（05 不依赖 store）；engine→gamify 直调改 `EventBus::publish`（#3 ✓）；gen 是纯函数库（llm+jev 注入，可被 httpd 复用）。**遗留**：gen 同时依赖 llm+jev（M3 `evaluate_many` 后仍合理）；`ProxyLlm`/`LlmRouter::usage_summary` 按契约留 M3/M7（文档已标注范围）。
- **鲁棒性**：qpack 三重防线（schema_version 拒异版本、qid 内容哈希防篡改、低质 θ=0.6 拒收）+ 导入幂等（同 qid 跳过）；LLM 重试≤2/Jev 退避≤3 均有 wiremock 路径测试；qc 全拒报 `AllRejected` 不静默（P-8 风格）；迁移在事务里（版本号与 SQL 同提交）；文件库 WAL+busy_timeout；游戏状态重启水合（测试断言 xp 续档）。**欠账**：LLM 重试无全局熔断（连续失败会拖慢单次 generate，M3 成本面板一并看）；`llm_cache` 无 TTL/淘汰；seed 导入失败会阻止 `open()`（当前 fail-fast 正确，但缺“跳过坏 seed 只警告”的降级开关，见 #14）。

### M1 改进建议（M2 开工即处理，勾=完成）

- [x] 1. engine 接线 gamify 效果持久化：提交后 `apply(effects) → save_game_state`（M2：GameProjector publish→snapshot→save，测试断言 xp/streak 落库）
- [x] 2. `Engine::open(path)` 文件库 + `schema_version` 迁移表（docs/02-store §2）（M2：WAL+busy_timeout，MIGRATIONS 事务化）
- [x] 3. 引入 `EventBus` trait 替换 engine→gamify 直调（docs/02 DomainEvent 已定义）（M2：core::EventBus，GameProjector 实现）
- [x] 4. store 枚举/负载列改 serde snake_case；`items` 列更名 `payload`；位置元组 → 具名结构（M2：QuestionContent + as_str↔serde 同口径断言测试）
- [ ] 5. TUI 采集 timing + 自评 → 接 `is_guess`/`srs::decide_action`（假对检测上线）——**半步**：M2 已采 timing/revision 并写入 `Attempt.timing`；`self_confidence` 自评键位与 `is_guess` 接线依赖 FSRS 队列，归 M4（见 #10）
- [x] 6. TUI 错误独立展示（Error 行 ≠ Pending 态）（M2：`apply_error` 独立行，P-8 关闭）
- [x] 7. seed 题库加载器（`data/seed`）+ qpack 导入（M2 主线）（M2：open 幂等导入 203 题 + import_pack/export_pack 门面）
- [x] 8. 引入 `tracing`（事件/降级路径可见）（M2：qc 丢弃/重生成/game_state 落库失败均 tracing::warn）

### M2 改进建议（M3 开工即处理，勾=完成）

- [ ] 10. TUI 自评键位（提交前 1-4 档 confidence）→ `Attempt.self_confidence` → 接 `is_guess` + `srs::decide_action`（与 M4 FSRS 队列合流）
- [ ] 11. Jev 全量：`evaluate_many` 并发池 8 + `route_by_confidence`（0.75/0.35）+ 待裁定桶 + 错因分类（docs/05 §2-§4 M3 范围）
- [ ] 12. `LlmRouter::usage_summary` 成本面板 + `llm_cache` TTL/淘汰 + generate 全局熔断（连续失败快速返回）
- [ ] 13. TUI 题包屏（`i` 导入/`x` 导出键位，docs/03 §5.3）接线 `import_pack`/`export_pack`；`share_code` 口令码随 M7
- [ ] 14. seed 导入失败降级开关：坏 seed 包只 `tracing::warn` 跳过（不阻断 open），qid 损坏仍拒（当前 fail-fast 对内置包正确，对用户放入 `data/seed/` 的包过严）
- [ ] 15. 题型补全：完形/阅读/七选五/语法填空（DESIGN §13.4 归 M2，本阶段标签未列——core QuestionKind 扩展 + grader 递归子题 + seed 补题型）
- [ ] 16. seed 扩轨：gaokao/zhongkao/…（seed_gen 参数化 track + 每轨 ≥200 题，docs/16 §3）

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
