# 01 · engineering — 工程规范（风格 / 命名 / 质量门禁 / 协作流程）

> 实现顺序：**先读**。并入来源：旧 `25-code-style` + `26-naming` + `27-code-quality` + `28-dev-workflow`。

## 1. 代码风格

- 格式化是 CI 门禁：`cargo fmt --check`（max_width=100）/ `prettier --check`；配置入库；
- 组织：`lib.rs` 薄壳；模块 `model/service/repo/error`；文件 ≤500 行、函数 ≤80 行、参数 ≤5（多则组 struct）；可见性最小化；
- 惯用法：不可变优先、迭代器优先、禁裸 bool 参数、魔法数入 config；
- **占位/预留参数规则**：签名禁止未使用参数与 `_xxx` 占位（pre-commit grep 强制）；未来需要的参数写 `// TODO(name): 实现该功能时加入 xx 形参`，功能落地时再加——**函数里不留无用形参**；
- 错误：每 crate 一个 `XxxError`（thiserror），跨层 `EngineError`；外部错误带 `retryable: bool`；库禁 anyhow；**禁 unwrap/expect/panic（非测试）**；
- 异步：禁阻塞 IO、禁持锁跨 await、并发上限命名常量、spawn 必收 JoinHandle；
- `#![forbid(unsafe_code)]`（FFI 特例单列 + miri）；
- 文档：公共 API 必须 rustdoc（`# Errors`/`# Examples`）+ doc test；注释讲"为什么"；**契约变更同 PR 更新分册**；
- TS：strict、禁 any/`@ts-ignore`、API 类型只从 OpenAPI 生成。

## 2. 命名规范

- **品牌命名（定案）**：产品 **Ambition（俺必胜）**（Ambition≈俺必胜）；口号 "Ambition, 俺必胜！"；CLI `ambition`（仅两入口：`ambition` / `ambition web`）；env 前缀 `AMBI_`；数据目录 `~/.ambition/`；题包 scheme `ambi://pack/<hash>`；keychain service `ambition`；Mascot 小蜜蜂「必必」；打字通 UI 名「键胜」。**开源协议 PolyForm Noncommercial 1.0（非商业免费使用；商业授权另行协商）**；
- 总则：英文命名、见名知义、禁拼音、禁裸词（data/info/temp/util/manager）、谓词 is_/has_/can_、缩写白名单 `id qid db ui api url http json tx fsrs jev llm ocr tts asr srs`；
- Rust：crate snake_case 短名词；类型 PascalCase；trait 名词/形容词（`XxxProvider`/`XxxRepo`）；服务 `XxxService`；错误 `XxxError`；ID `XxxId` newtype；函数动词开头；常量 SCREAMING_SNAKE；枚举持久化 snake_case 字符串；
- SQL：表复数 snake_case；时间 `*_at`、JSON 列 `*_json`、分类 `kind`、状态 `status`、语言 `lang`；索引 `idx_<表>_<列>`；迁移 `NNNN_desc.sql` 只增不改；
- JSON/协议：snake_case 字段、根带 `schema_version`、semver；
- TS：组件 `PascalCase.tsx`、hooks `useXxx`、路由 kebab-case；
- Git：分支 `<type>/<desc>`；commit Conventional Commits（`type(scope): 祈使句`，scope=crate 名）；tag `vX.Y.Z`；rubric/qpack/sync 协议独立版本号（**判分口径变更必升 version**）；
- 反模式（直接打回）：Manager/Info/Data 裸词、`new_xxx/flag1`、一词多叫（统一 question）、否定式动词、拼音。

## 3. 质量检测与门禁

- 本地 pre-commit（fmt/clippy 快检/gitleaks）→ PR CI → 发布；clippy `-D warnings` + deny `unwrap_used/expect_used/panic/todo!/dbg!`；`clippy.toml`：too-many-arguments=5、判分路径禁无序 HashMap；
- **pre-commit 七步门禁**（`.githooks/pre-commit`，全过才允许提交）：①`cargo fmt --check` ②`cargo clippy -D warnings`（含未使用参数/未使用导入）③禁占位参数 grep ④**函数↔测试配对**（`scripts/fn_test_check.sh`：每个非测试 fn 必须存在 `fn test_<fn名>_…`）⑤`cargo test` 全绿 ⑥未使用包（cargo-machete，可选）⑦通过；`commit-msg` 强制 Conventional Commits；
- 测试金字塔：单元（核心库 grader/adaptive/vocab/typing 覆盖率 ≥80%，全仓 ≥70%）/ 属性（proptest：qpack hash、canonical JSON、SRS 单调、CAT 收敛）/ 契约（wiremock + fixture：Jev 三形态、schema 重试）/ 集成（S1–S8）/ E2E（TUI 快照 insta、Web Playwright）；**测试需求来源 = 各分册「验证点」**；doc test 必过；`cargo-llvm-cov` 门禁；
- 供应链：cargo-deny 许可证白名单（MIT/Apache-2.0/BSD-3；与 PolyForm-NC 兼容、禁 GPL/AGPL）、cargo-audit、lockfile 入库、gitleaks、SQL 一律参数化；
- 资源/安全：图 ≤20MB、qpack ≤50MB、入 LLM ≤32k tokens 显式截断；日志脱敏（key/日记正文禁入 log）；
- 性能：criterion（判题 P95、SRS 10k 取队、qpack 1k 解析、打字 10k 击键）；±10% 告警、±25% 阻断；
- CI 矩阵：fmt → clippy → test(SQLite+Postgres) → doc/doctest → proptest → coverage → deny → audit → gitleaks → web(prettier/eslint/tsc/vitest) → build(双平台内嵌 SPA)。

## 4. 协作与流程

- **文档先行铁律**：动契约（core 类型/DDL/rubric/qpack schema/API）先改分册、同 PR 提交；新功能必须有分册验证点 + 测试；**新增分册必须在 00-overview §6 登记归属**；
- 分支 trunk-based（短分支 ≤3 天、squash 进 main）；PR ≤400 行，模板四段（动机/改动/验证/风险回滚）；grader/jev/rubric 变更需判分负责人额外 +1；
- 版本：0.x MINOR 可破坏（CHANGELOG 标注）；1.0 后严格 semver；rubric/qpack/sync 协议独立版本（兼容窗口 ≥2 MINOR）；
- 发布：CHANGELOG → tag → CI 双平台二进制（内嵌 SPA + `--web-dist` 说明）+ SHA256；产物带 PolyForm-NC 许可标注；
- 回滚：判分事故优先回退 rubric version；迁移事故走全量导出/幂等导入；密钥泄漏走 keychain 轮换 + audit；
- onboarding 顺序：00-overview → 本册 → 对应功能分册；
- **进度与雷点**（skill `ambition-pitfall-log` 强制）：完成一个部分 → `PROGRESS.md`（进度日志 + 四段代码评估：质量/实际使用/耦合度/鲁棒性 + 改进建议）；遇坑 → `PITFALLS.md`（五要素，开工前必读，同根因更新不新开）；
- **定时施工任务**：`scripts/continue_mvp.sh`（Windows 任务 AmbitionMVP，每 15 分钟）读 `MVP_STAGE` 唤起 pi 逐阶段推进到 M6，阶段内连跑下一阶段；`task_lock.sh` 互斥（已有任务则等待）；**停用/删除**：`scripts/uninstall_task.sh`（或双击 `.cmd`；`AMBI_DRY_RUN=1` 可演练）。

## 5. 检查清单（评审）

- [ ] 无 unwrap/panic；错误有上下文与 retryable；无持锁跨 await
- [ ] 契约变更同步分册；新功能登记 00-overview §6 归属；无平行实现（00-overview §5）
- [ ] 新公共 API 有 rustdoc + doc test；对应验证点有测试
- [ ] 判分口径变更升 rubric version + CHANGELOG
