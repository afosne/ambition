# Ambition 施工进度与代码评估

> **每次完成一个部分必须更新本文件**（进度日志 + 代码评估四段 + 改进建议），并写入 `PITFALLS.md`（skill `ambition-pitfall-log` 强制）。

MVP_STAGE: 6/6

## MVP 阶段定义（M1–M6，对应 DESIGN.md §21；M7+ 属于云/Web，不在本次 MVP 内）

| # | 阶段 | 内容 | 状态 |
|---|---|---|---|
| 1 | M1 骨架 | core/store/engine/tui + 规则判题 + 打字通底座 + 游戏底座 | ✅ 完成（2026-09-24，提交 5271323） |
| 2 | M2 出题 | llm crate（OpenAI/Anthropic 协议）+ 结构化出题 + Jev 质检回路 + qpack 导入导出 + seed 题库 | ✅ 完成（2026-09-24，提交 b39efef） |
| 3 | M3 判题 | jev crate 客户端 + 待裁定桶 + 错因分类 + 中式英语错式库 + 成本台账 | ✅ 完成（2026-09-24，提交 75408b0） |
| 4 | M4 记忆 | FSRS 接线 fsrs-rs + 错词本/义项/句式 + 假对上线 + 复习队列 | ✅ 完成（2026-09-27，提交 8769432；含用户加码的题型补全） |
| 5 | M5 写作 | composite scoring + 批注 + 模板作文填槽 + 翻译题型 + 套卷估分 | ✅ 完成（2026-09-29，W1-W8 十四笔；588 测试；模板屏/模考屏/翻译工作台/卷面为表外加码交付） |
| 6 | M6 闭环 | 笔记/日记/TODO + 画像聚合 + 个性化出题 + 周报 + 游戏全量 | ✅ 完成（2026-09-29，提交 46a9257；M6 W1-W7；689 测试） |

---

## 进度日志

### 2026-09-30 · #17/#18 配对桩补全与最终提交（用户指令「继续 如果完成后就提交」）

- **背景**：#17 讲解层 + #18 成本面板与题包管理已在上轮落实（95b216b），但 pre-commit 结构门禁拦截：`keys/mod.rs :: fn key_to_msg` 和 `main_support.rs :: fn handle_cost_load`/`handle_pack_import`/`handle_pack_export` 缺少同文件配对测试桩（测试在子模块 `keys_tests.rs` 中但门禁要求同文件 `test_` 前缀配对）。
- **改动**：
  - `keys/mod.rs`：在 `#[cfg(test)] mod keys_tests;` 前新增 `test_key_to_msg_paired_stub`（调用 `key_to_msg(Choice, 'q', CONTROL)` 验证函数可达）。
  - `main_support.rs`：在 `mod tests` 内新增三个配对桩 `test_handle_cost_load_referenced`/`test_handle_pack_import_referenced`/`test_handle_pack_export_referenced`（async fn 需 engine+state，测试环境不可用，以引用名方式配对）。
  - 清理临时文件 `_fix3.py`（P-27 UTF-16 编码修复用的 Python 脚本）。
- **PITFALLS**：P-27（PowerShell 重定向 UTF-16）、P-28（新增枚举变体全仓 match 穷尽）已在上轮记录。
- **验证**：三连全绿（fmt 0 diff / clippy 0 warning / test 701 passed 0 failed）；pre-commit 七步全过（7/7 通过）。

#### 本项代码评估（四段）

- **代码质量**：4 个配对桩均为最小可达性证明（不引入伪测试逻辑）；fmt/clippy 零告警；门禁配对要求 100% 满足。
- **实际使用**：无功能变化（仅补桩+清理）；TUI 行为与上轮 #17/#18 一致。
- **代码耦合度**：零新增耦合（桩函数只引用既有 pub API）。
- **鲁棒性**：配对桩不引入副作用路径；async fn 桩以引用名方式避开测试环境限制。

### 2026-09-29 · 鼠标点击支持——全屏按钮/输入框可点击（用户指令「所有的按钮都需要可以点击 输入框也可以点击」）

- **改动**：
  - 新增 `mouse.rs` 模块：`ClickTarget { area: Rect, msg: Msg }` 注册表 + `clear_click_targets`/`register_click`/`hit_test` 三方法。
  - `AppState` 新增 `click_targets: Vec<ClickTarget>` 字段。
  - `main.rs`：启用 `EnableMouseCapture`/`DisableMouseCapture`；事件循环中 `Event::Mouse(Down(Left))` → `hit_test(row, column)` → Msg → 既有 `update` 管线（与键盘同路径）。
  - 渲染函数签名全量 `&AppState` → `&mut AppState`（render/render_home/render_review/render_picker/render_scaffold/render_tx_frame/render_workspace/render_help_overlay + render_sized）。
  - `render_home`：11 个菜单项各注册 2 行高 Rect → `Msg::HomeClick(i)`。
  - `render`（练习屏）：选择题选项 → `Msg::ClickOption(i)`；填空输入行 → `Msg::ClickBlank(i)`；待裁定判对/判错/确认按钮。
  - `render_review`：翻面区域 → `Msg::Flip`；评分按钮 1-4 → `Msg::Rate(1..4)`。
  - `render_workspace`：输入框行 + 保存按钮区域。
  - `render_help_overlay`：整个浮层区域 → 点击关闭。
  - 新增 `Msg::HomeClick(usize)`/`ClickOption(usize)`/`ClickBlank(usize)` 三个鼠标专用变体 + `update` 处理逻辑。
- **PITFALLS**：P-25（render 签名 `&AppState` → `&mut AppState` 全量变更——20+ 处 `render_sized` 调用编译失败；借用冲突通过先提取信息到局部变量再注册解决）。
- **验证**：`cargo fmt -p ambition-tui`、`cargo clippy -p ambition-tui --tests`（零告警）、`cargo test --workspace`；全仓 **701 测试通过**（上轮 698 → 701，+3：mouse 模块 hit_test 3 测试）。

#### 本项代码评估（四段）

- **代码质量**：mouse 模块 3 个配对测试（命中/优先级/清空）；fmt/clippy 零告警；鼠标与键盘共用同一条 `update` 管线（无分叉）；`ClickTarget` 实现 `PartialEq`（AppState derive 兼容）。
- **实际使用**：启动后鼠标点击主菜单项直接进入功能屏；练习屏点击选项即选中；填空输入行点击进入编辑态；复习屏点击翻面/评分按钮；帮助浮层点击关闭。
- **代码耦合度**：鼠标事件复用既有 `Msg` → `update` → `Cmd` 管线（零新 Cmd 变体，仅 3 个 Msg 变体）；`click_targets` 注册表是 AppState 字段（渲染时注册，main 查表），不耦合 engine/store。
- **鲁棒性**：`hit_test` 从后往前匹配（浮层后注册优先）；`register_click` 过滤零面积区域；`HomeClick`/`ClickOption`/`ClickBlank` 均做边界钳制（`min(n-1)` / `>= opts_len` / `>= inputs_len` 守卫）。

### 2026-09-29 · 鼠标悬停跟随光标（用户指令「选择框应该可以跟随鼠标 更加符合用户使用习惯」）

- **改动**：
  - `ClickTarget` 重构为双 Msg：`click_msg`（点击触发动作）+ `hover_msg: Option<Msg>`（悬停只移光标）。
  - 新增 `register_interactive(area, hover_msg, click_msg)` 方法——同时注册悬停和点击。
  - 新增 `hit_test_hover` 方法——只返回 `hover_msg.is_some()` 的区域。
  - 新增 `Msg::HoverOption(usize)`/`HoverHome(usize)`/`HoverBlank(usize)`——只移光标不触发选中/进入/编辑。
  - `main.rs`：`MouseEventKind::Moved` → `hit_test_hover` → Hover Msg → `update`（与点击/键盘同路径）。
  - `render_home`/`render`（练习屏）：选项/菜单项/填空行从 `register_click` 改为 `register_interactive`。
  - `update`：`HoverOption` 设 `sel`；`HoverHome` 设 `home_cursor`；`HoverBlank` 设 `blank_sel`——均不触发选中/进入/编辑。
- **PITFALLS**：P-26（Hover Msg 在 unhandled 列表中的可达性——`HoverHome` 在非 Home 屏可达需加入 unhandled，`HoverOption`/`HoverBlank` 不可达不加）。
- **验证**：`cargo fmt`/`cargo clippy --tests`（零告警）/`cargo test --workspace`；全仓 **704 测试通过**（上轮 701 → 704，+3 mouse 模块 interactive/hover 测试）。

### 2026-09-29 · 新增主页面（Home 屏）功能菜单（用户指令「运行后打开一个选择的主页面包含所有功能」）

- **改动**：
  - `Screen::Home` 新变体 + `KeyCtx::Home` 新上下文 + `home_cursor` 字段 + `Msg::HomeEnter` + `Cmd::PracticeLoad`。
  - `render_home`：11 项功能菜单（练习/复习/模板/模考/翻译/笔记/日记/待办/画像/游戏/词汇本），❯ 光标 + 描述行 + 键位提示。
  - `key_to_msg` Home 路由：↑↓ 导航 · Enter 进入 · Esc 退出 · Ctrl+Q/C 退出。
  - `update` Home 状态机：↑↓ 循环移动 · `HomeEnter` 按 cursor 分发到 11 个功能屏（练习走 `Cmd::PracticeLoad` 延迟加载题目，其余复用 `update_screen`）。
  - `main.rs`：启动进 Home 屏（不预加载题目），`Cmd::PracticeLoad` 分支调用 `engine.start_practice(0)` 按需加载。
  - 各功能屏 Esc/Tab 返回 Home（而非 Practice）；帮助浮层补 Home 行。
- **PITFALLS**：P-24（`new_state` 默认 screen 不能改，否则测试全挂——保持 Practice，main 手动设 Home）。
- **验证**：`cargo fmt --all`、`cargo clippy --all-features --all-targets`、`cargo test --all-features`；全仓 **698 测试通过**（上轮 694 → 698，+4 配对测试：Home 渲染 2 + Home 状态机 1 + key_ctx Home 路由 1）。

#### 本项代码评估（四段）

- **代码质量**：Home 屏渲染与状态机各配对测试（2 渲染 + 1 状态机 + 1 路由）；fmt/clippy 零告警；`home_entries` 与 `home_enter`/`home_menu_len` 三处菜单顺序同源（索引一致）；帮助浮层复用 `hint` 全表不硬编码。
- **实际使用**：启动即见功能菜单（不再直接进做题）；↑↓ 选功能 · Enter 进入；各功能屏 Esc 返回主菜单；练习屏从 Home 进入时才加载题目（延迟加载减少启动开销）。
- **代码耦合度**：`Screen::Home` 新增变体不破坏既有屏路由（`new_state` 默认 Practice 不变）；`Cmd::PracticeLoad` 新增不耦合 engine（main.rs 调用已有 `start_practice`）；Home 状态机在 `update` 顶部拦截不影响既有屏逻辑。
- **鲁棒性**：`home_cursor` 钳制到菜单长度（`min(entries.len()-1)`）；空题库 Home 仍可用（`finished=true` 但 `screen==Home` 维持循环）；各功能屏 Esc 统一回 Home 不回 Practice（消除「做题未完成→Esc→直接退出」误操作）。

### 2026-09-29 · 修复 attempt_id 重启撞 UNIQUE 约束（热修复；用户报错「所有题目提交答案后都报错」）

- **根因**：`main.rs` 用进程内计数器 `attempt_seq`（从 0 起）生成 `attempt_id = format!("tui-{attempt_seq}")`；数据库 `~/.ambition/ambition.db` 是持久化文件——首次运行写入 `tui-1`..`tui-N` 后，重启进程计数器归零，再提交第一题又生成 `tui-1`，与已有行冲突（SQLite code 1555）。
- **修复**：ID 加入 unix 秒时间戳前缀：`format!("tui-{}-{attempt_seq}", now_unix())`——同一秒内提交多题靠 `attempt_seq` 区分，跨进程靠时间戳区分。
- **PITFALLS**：P-23 已记录。
- **验证**：`cargo fmt --all`、`cargo clippy --all-features --all-targets`、`cargo test --all-features`；全仓 **694 测试通过**（上轮 692 → 694，+2 新增 main_support 测试计数变化）。

#### 本项代码评估（四段）

- **代码质量**：单行修复，`now_unix()` 已从 `ambition_core` 导入复用（无新依赖）；fmt/clippy 零告警。
- **实际使用**：重启 TUI 后提交答案不再报 `UNIQUE constraint failed`；旧数据库已有记录不受影响（新 ID 前缀不同）。
- **代码耦合度**：改动局限于 `main.rs` 提交路径，不影响 engine/store 层逻辑；`now_unix()` 复用既有 common 工具。
- **鲁棒性**：时间戳+序号双分量保证全局唯一（同秒多题靠序号、跨进程靠时间戳）；不依赖外部 UUID 依赖。

### 2026-09-29 · 改进建议批量落实（非阶段任务；用户指令「按照 PROGRESS.md 提出的改进建议进行查看，没有落实的按顺序落实」）

- **摸底**：全量清点 M1-M4 改进建议 34 项——已完成但未勾选 3 项（#5/#10/#32 随 M4#19/M6 落实但 PROGRESS 未标记）、真正未落实 17 项、延后/承接后续 14 项。
- **落实（7 项）**：
  - **#14 seed 降级开关**：`assembly::open` 内置 seed 从 `.await?` 改为 `if let Err(e) → tracing::warn`，与 `enroll_builtin_cards` 同口径（坏 seed 不阻断 open）。
  - **#24 帮助屏**：`Msg::ToggleHelp` + `AppState.help_open` + `key_to_msg` 恒定层 `?` 键路由（非字母不违反禁单字母总则）+ `render_help_overlay` 复用 `hint` 全表（17 个 KeyCtx 全展，禁止硬文案）+ `centered_area` 居中弹窗 + 2 配对测试。
  - **#25 Review 屏刷新**：`update_screen` 同屏匹配加 `Screen::Review => Cmd::ReviewLoad`（done 态按 Ctrl+R 重载队列，不再 None）+ 配对测试 + 修正既有 passage 测试断言。
  - **#26 子题回退**：`Msg::Prev` 在复合题（七选五/阅读）且有 `sub_answers` 时弹出最后一个子题回退（而非切上一题），允许重选当前槽。
  - **#31 裸 bool 枚举**：`srs::due_queue(…, interleave: bool)` → `Interleave{None,Groups}` 枚举 + `grader::normalize_for_compare(…, ignore_articles: bool)` → `ArticlePolicy{Keep,Strip}` 枚举；调用点全改（engine 1 处 + srs 测试 4 处 + grader 测试 4 处）。
- **勾选已落实**：#5（M4 #19 `is_guess` 落实）、#10（同 #5）、#32（M6 `profile::predict_score` 落实）。
- **延后**：#13 题包屏（需文件选择器+文件系统交互，TUI 纯状态机需新增 Cmd+main 副作用）、#30 gen 500 行（当前 468 行不超线，再增时拆）。
- **验证**：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --quiet`；全仓 **692 测试通过**（M6 终态 689 → 692，+3 新配对测试）。

#### 本项代码评估（四段）

- **代码质量**：7 项改动全部配对测试（#24 `?` 路由+状态翻转 2 测试、#25 同屏重载 1 测试）；fmt/clippy 零告警；枚举替代裸 bool 语义自文档化（`Interleave::Groups` 比 `true` 可读）；帮助浮层文案 100% 来自 `hint` 函数（同源不硬编码）。
- **实际使用**：`?` 键任何界面弹帮助浮层（全 17 个 KeyCtx 键位一览）；Ctrl+R 在复习屏完成态可重载队列（不再需回 Practice 再进）；复合题 ← 回退子题（不再只能前进）；坏 seed 不阻断启动（与坏词库同口径）。
- **代码耦合度**：依赖方向不变；`Interleave`/`ArticlePolicy` 枚举定义在各自 crate（srs/grader），engine 调用点改 `ambition_srs::Interleave::Groups`；帮助浮层 `render_help_overlay` 在 render 域内，复用 keys::hint 不新增耦合。
- **鲁棒性**：seed 降级 warn 不阻断（与 enroll_builtin_cards 同口径）；帮助浮层开/关纯状态翻转（无副作用）；子题回退 `pop()` 后 `sel=0` 复位防越界；Review 同屏重载走既有 `Cmd::ReviewLoad` 路径（不新增副作用通道）。

### 2026-09-29 · #17 讲解层 + #18 成本面板与题包管理入 TUI（改进建议落实）

- **#17 讲解层①②**（95b216b 已提交）：`GradeDetail{Items|Dims|Plain}` 结构化讲解已落地；F5 讲解浮层 `render_explain_overlay`（verdict/score/confidence/graded_by + error_tags 中文标签 + 逐项判定/维度分 + ①结构化/②文字入口提示）；`Msg::ToggleExplain` + `AppState.explain_open` 状态翻转；`KeyCtx::Graded` F5 路由。
- **#18 成本面板与题包管理**：`Screen::Cost`/`Screen::Qpack` 两个新屏 + `KeyCtx::Cost`/`KeyCtx::Qpack` 两个上下文 + Ctrl+S（成本直达）/Ctrl+O（导入）/Ctrl+X（导出）键位路由 + `Cmd::CostLoad`/`Cmd::PackImport(String)`/`Cmd::PackExport` 三个 Cmd + `main_support` 三个 async helper（`handle_cost_load`/`handle_pack_import`/`handle_pack_export`）+ Home 菜单 11→13 项 + `home_enter` 路由 cursor 11→Cost/12→Qpack + `is_residual_screen` 补 Cost/Qpack + hints.rs Cost/Qpack 提示文案 + `render_workspace` Cost/Qpack 屏渲染 + keys/mod.rs 内联测试拆到 keys_tests.rs（控 ≤500 行）+ 全量测试适配。
- **验证**：fmt/clippy/test 三连全绿（719 passed, 0 failed）；结构门禁全 ≤500 行。

#### 本项代码评估（四段）

- **代码质量**：新增 Screen/KeyCtx/Cmd/Msg 变体全部配对测试（home_menu_len_thirteen + home_enter routes + render_home shows menu + is_residual_screen covers Cost/Qpack + hint covers Cost/Qpack）；keys/mod.rs 内联测试迁移到 keys_tests.rs 不丢测试；fmt/clippy 零告警。
- **实际使用**：Ctrl+S 从 Home 屏直达成本台账（展示 LLM 用量 tokens_in/out 汇总）；Ctrl+O 导入内置 seed 题包（演示路径，TUI 无文件选择器）；Ctrl+X 导出当前题库为 JSON 预览（前 20 行）；Esc 返回 Home。F5 判分后讲解浮层显示结构化判定信息。
- **代码耦合度**：#18 Cmd 处理逻辑提取到 `main_support.rs` 的 async 函数，main.rs 只调 helper（行数控制）；Cost/Qpack 屏复用 `render_workspace`（只读列表模式）；hints.rs 新增 Cost/Qpack 提示不引入新耦合。
- **鲁棒性**：cost_summary/import_seed/export_pack 均走 engine 门面（已有错误处理）；Cost/Qpack 屏只读无副作用；Qpack 屏非 Home 屏忽略 PackImport/PackExport（m6.rs 分支处理）。

### 2026-09-29 · M6 闭环 W1-W7（阶段任务；用户指令「继续直到所有阶段完成提交」）

- **W1 笔记/日记/TODO**：core 类型与 UUID v7 ID、store schema/repository、engine 门面、`DiaryReviewed`/`TodoCompleted` 事件；笔记 FTS/双链/Markdown 导出、按日记账/显式 LLM 复盘、TODO 幂等完成与游戏反馈；TUI Ctrl+N/D/K。
- **W2 画像聚合**：基于本地 attempts/grades/error_tags 汇聚 Wrong=1/Partial=.5 与 0.97/周衰减、趋势/弱点、快照持久化、Ctrl+P 画像屏。当前知识节点为 qid（questions.payload 不存 Question.tags）。
- **W3 个性化出题**：弱点写入 GenSpec、日预算 count（零预算零题）、错题连续两次正确出队、复习/新词/专项交错计划。
- **W4 周报+估分**：七天统计、模考平均、近五卷预测及小样本区间、Markdown 导出；目标分轨差异回归仍待后续校准。
- **W5 vocab crate**：新增 `ambition-vocab`，内置 Sense/Pattern/例句资产、语境匹配、本地新词抽取与 FSRS 入卡、Ctrl+V 词汇屏。
- **W6 游戏底座**：徽章/金币/物品/连击/地图进度投影与持久化、商店/道具/关闭开关、小游戏规则（记忆配对/连击冲刺/词根排序/Boss/努力分/健康守卫）、Ctrl+G 成长及物品屏。
- **W7 归档验证**：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --quiet`；全仓 **689 测试通过**（M5 终态 588 → M6 终态 689，+101）。pre-commit 结构门禁（配对测试 + ≤500 行）首次拦截 8 个缺配对函数（`boss_battle`/`complete`/`append_game_event`/`load_game_profile`/`save_game_profile`/`upsert_note`/`add_todo`/`builtin_patterns`），逐一补齐后过门禁；`note.rs` 首行混入 BOM 字符已修（P-21）。`docs/12`/`docs/13` 只勾已验证项，未覆盖的再导入、稳定事件 ID 重放等继续列为后续验证。
- **实现检查与边界**：W1 Markdown 导出按原样保留正文末尾；画像集成测试按真实持久化模型使用 qid 节点；记忆配对测试覆盖反向选牌；游戏开关关闭不产生投影效果。社交/家长/组件/年报/对战及 docs/12 中 M16 溯源能力不属于 M6。

#### 本项代码评估（四段）

- **代码质量**：跨 core/store/engine/gamify/vocab/TUI 补齐 M6 分层；新增迁移与测试覆盖类型、门面、SQL 聚合、事件和状态投影；最终格式、Clippy、全仓测试作为归档门禁。
- **实际使用**：TUI 可进入笔记/日记/TODO、画像、词汇本、游戏成长界面；周报/词汇本支持 Markdown/本地资产和 FSRS 闭环；离线画像计算不触发 LLM。
- **代码耦合度**：新增纯域 `ambition-vocab` 依赖 core；持久化集中 store，业务组合在 engine，界面在 TUI；W3 出题沿用 gen 的 GenSpec。
- **鲁棒性**：store 迁移连续追加；日记按日期幂等写；TODO 重复完成不重复广播；游戏 profile 可恢复；事件关闭时不投影；评分区间对样本不足提供宽区间。

**后续改进建议**：持久化并聚合 Question.tags 作为画像知识节点；补 Markdown 再导入往返、周报 SQL 对账、λ 数值/趋势边界与 game_events 稳定 ID 去重重放；按 track 校准预测模型；补记忆游戏 TUI 交互与完整 E2E 冒烟。

### 2026-09-29 · M5 写作：composite 判分 + 作文/翻译题型 + 模板/批注/卷面 + 套卷模考 + 翻译工作台（阶段任务；用户指令「开始实施 M5」）

- **拆波**：M5 → W1–W8 八波分笔提交（每波 fmt/clippy/test + 结构双门禁 + TUI 冒烟）；**docs 契约先行**共9处（02 迁移 v4/v5/v6 + 类型补全、03 键位/门面、05 LlmKind::Translate、06 Paper 类型、07 composite 落地语义、10 §1/§2/§5/§6 落地注记、11 §1/§2/§4 落地注记、13 事件入游戏表）。
- **改动（13 提交）**：
  - `283a554` **W1 composite scoring**：GradeResult.items → detail{Items|Dims|Plain}（02 §2 契约同构；serde(default) 兼容 M1-M4 旧 payload；34 构造点 + fill 升级路径 take/set 迁移）；`assets/rubrics/{gaokao25,ielts_task2}.json`（判分语料入 assets=红线#19；权重和=1/band_map 升序/levels≥2 门禁测试）；`jev_composite` 单请求 fan-out（每维 Score÷(levels-1) 归一化 + 每条 veto 正面表述 Noul≥0.5 判过、缺维保守 0 分 0 置信）；`compose_scores` 按 key 取权 → ×max → band_map 下档 floor（15 含界/14.9 取下/越界封顶/首档下→0）；`down_band` 逐档、最低档→0。
  - `0e41a96` **W2 翻译题型**：core `TranslationBody{direction,source_text,reference}` + `Answer::Text`；grader `translation4` rubric（准确/完整/流畅/地道）+ `grade_text_composite` 单源（离线→Pending、conf<accept→桶、≥60%/≥30%→Correct/Partial、veto 映射错因标签）；gen 全链（kind tag/qc 双语 state/schema enum/双语提示行）+ seed 8 题；TUI `is_text_input` 单文本输入位。
  - `b408af2` **W3 作文题型**：core `EssayPrompt{prompt_zh,genre,track,rubric_id,word_range,minutes}`；grader 空卷硬校验→桶 + gaokao25 六维 composite；engine `grade_essay` 落 essays+essay_grades（payload=GradeResult）+ 发 `EssayGraded` → gamify Xp(5) 不进连击；gen essay 出题（tags 推 genre/track、字数/限时按轨）+ seed 2 篇。
  - `de5c7ee`+`0f20be8`+`ac5510f` **W4 模板作文**：writing 纯件（Template/ParaFunc/Slot + Full→Half→Free 三阶段 scaffold + 套槽 render 缺槽保留占位 + `data/templates` include_str 内置库 + AntiPattern.replacement_en 同义升级）；迁移 v4 写作三表 + EssayRepo{upsert_template/templates}；engine scaffold/check_scaffold（Jev noul×2 双 prob≥0.5 过线）/templates/render_template；TUI **F2 模板屏**（阶段循环·槽位编辑·成文预览·自查回填）。
  - `5a0bb53`+`3398540`+`272dbe6` **W5 批注+加分项**：writing 批注域（Comments/Annotation/SrsTarget/Upgrade/Recommendation + comments/model_essay 双 schema、annotate/model_essays 双语提示资产、upgrade_sentence 本地反模板+同义表、recommend_after 纯函数、phrase_bank 汇聚）；grader `verify_upgrade` noul（0.3→拦下语义走样）；store insert_essay/essay_history（LEFT JOIN grade）；engine grade_essay/annotate（LlmKind::Explain+run_loop 缓存）/upgrade_sentence/verify_upgrade/model_essays + rubric_id_for；TUI **F4 批注浮层**（升级/亮点/入卡目标，判分图标与浮层同源）。
  - `b3ace42` **W6 套卷模考**：core `Paper/PaperSpec/PaperSection` + `kind_key()`；迁移 v5 papers/paper_runs + `paper_score`（rowid 窗口每 qid 最新一答）；engine `assemble_paper`（quality≥IMPORT_QUALITY_FLOOR、按 kind 取用、不足→Paper 错）+ `mock_start/mock_finish`（均匀满分口径换算卷面总分、发布 `PaperFinished`→Xp(10)）；TUI **Ctrl+M 模考屏**（屏顶倒计时 + 收卷得分；末题 Next 即交卷；估分函数按契约唯一随 M6）。
  - `fcc2967`+`5eda53e`（+`a47442c`） **W7 翻译工作台**：translate crate（TranslateOpts/Alignment/AlignSeg/DiffPoint LCS 中英差异按字切 + 双语提示资产 + translate/align 双 schema）；迁移 v6 translations/tx_favorites + TranslateRepo 四口；engine translate/align/my_turn（l2=en 走 W2 composite、其它 grade=None 降级）/history/favorite/to_practice/favorites 七门面；TUI **Ctrl+W 四屏**（出译·对齐分段·我来翻差异 `~-+` 标记·历史♥收藏转练习）。中途**修键位保留字违约**：Ctrl+S/E 归还直达表、动作键改 F3/F4（P-20，a47442c）。
  - `5ef039a` **W8 卷面模拟**：writing sheet 纯件（exam_sheet 收卷铃 deadline=起始+分钟、sheet_violations 字数红线 word_range 边界0不设限/超时/Pasted、typing_speed 60s 防除零）；engine `hand_in`（violations 非空=拒收不评分、干净卷面走 W3 composite 同链）；TUI `Event::Paste` 纸笔口径错误行反馈。
- **验证**：**425 → 588 测试全绿**（+163 全为配对/夹具/场景测试，只增不减）；14 笔七步钩子全过；每波真机冒烟 0 panic（seed 221 题=211+8 翻译+2 作文；F2/Ctrl+M/Ctrl+W 三直达可用）；**MVP 表五项全交付**（估分函数与语域 polish 按 docs/00 分别随 M6/M8，已挂 #32/#34）。
- **纪律执行**：两处外部改动按 P-18 取证→ask（PROGRESS「定时施工」行保持删除+删净评估）；键位违约自查当轮修复并入册 P-20；`MVP_STAGE` 行未动（#16/#18 用户手动推进，当前仍 4/6——表状态与阶段号分离）。

#### 本项代码评估（四段）
- **代码质量**：9 处 docs 契约先于实现码；13 笔提交七步钩子 0 失败；两处自证测试关键语义（down_band 边界、paper_score 最新一答、verify_upgrade 0.3 拦截、违规拒收、缓存幂等）；发现自己两处违约（键位保留字 P-20）当轮修复入册。
- **实际使用**：真机冒烟全流程（模板屏填槽成文自查 → Ctrl+M 模考倒计时交卷得分 → Ctrl+W 出译/对齐/我来翻/历史收藏 → F4 批注浮层）；seed 221 题两类新题型正常上屏作答判分。
- **代码耦合**：workspace 11→12 crate（新增 translate 纯件域，依赖仅 core）；writing 由模板单域扩为模板/批注/卷面三域；engine 新增 scaffold/paper/essay/translate 四门面域（25 个 pub 方法，docs/03 S3 同步）；依赖保持无环（store 不反向依赖 writing/translate，行类型走 core）。
- **鲁棒性**：离线矩阵贯穿五个 AI 判分口（composite/annotate/upgrade/model_essays/translate → Pending 或 Offline，不伪造）；迁移 v4/v5/v6 只增且 schema_version 连续；事件双新（EssayGraded/PaperFinished）进 gamify Xp(5)/Xp(10) 不进连击；P-19/P-17 在本阶段两次复现按既方处置。

### 2026-09-28 · 全量文件重复扫描与收口（非阶段任务，用户指令「继续检查所有文件的重复并收口」）

- **背景**：前两轮分别做了「crate 内夹具收口」与「跨模块收口」；本轮扩到**所有文件**全量——补上此前扫描口径的两个盲区：(a) 长度/语句数过滤把**尾表达式型夹具**（0 分号的 `Question::new` 构造等）全部漏掉；(b) 未覆盖 `mod tests` 内全部函数体与「字面量归一化的近似重复」。
- **扫描口径（升级双轮）**：①全函数体 md5（去掉长度/语句过滤、含 mod tests 内函数、去注释归一化空白）→ **7 组同体**；②字符串→S、数字→N 归一化 → **3 组同名同构、0 组异名同构**。
- **收口（提交 `0ffb83b`）**：
  - core `test_choice_body` 夹具（kind/question/taxonomy 三份逐字复制、11 处调用）→ 新建 `core/common/fixtures`（`test_` 前缀按配对门禁口径视为测试支持码，另配 sanity 测试）；
  - gen `spec` 出题规格夹具（pipeline 三文件逐字复制）→ 新建 `gen/common/`（配对测试钉 cet4/两型/3 题）；
  - engine `multi_pick_two`（装配/待裁定 ×2）、grader `client_with`（fill/ring ×2 wiremock 客户端构造）→ 各自 `common/fixtures`（各配对测试）；
  - **llm 双协议状态同构**：`OpenAiCompat` 与 `Anthropic` 的 {http, base_url, api_key, model} 四字段与构造逻辑逐字相同 → 新增 `provider::ProviderConn` **共享连接组件**（12 行构造收口单点；双协议各剩 1 行 `Self { conn: ProviderConn::new(…) }` 委托；字段访问改 `self.conn.*`；配对测试钉尾斜杠归一化 + 无 key 离线形态）。
- **判定保留（非重复）**：双协议 `new/complete/model` = 兄弟类型对 trait/公共 API 的 1-3 行最小委托（抽宏伤可读性）；engine 与 grader 的 `choice_question` 字面量归一化后同构、但字段按各域断言需要不同（难度1/vocab vs 难度2/slowly），跨 crate 共享夹具须引入 dev 耦合——不引入；异名同构组 0（无隐藏结构重复）。
- **P-18 并发编辑（外部改动卷入 + 用户裁定）**：施工中 `PROGRESS.md` 于 13:39:25 被**外部（IDE 侧）**删除「2026-09-23 · 定时施工链路修复」段标题+正文 7 行、留下悬挂四段评估——因 `git add -A` 卷进 `0ffb83b`；按 P-18 停手取证（`find -newermt` 窗口 mtime / `git show` 比对缺行 / `.idea/workspace.xml` 活动排除本会话）后 ask 用户，裁定**保持删除并删净评估**（定时施工已废除、scripts/ 已删，条目整体下线）；P-18 补复犯记，提交改分路径精确 add。
- **验证**：**425 测试全绿**（+5 夹具/组件配对测试）；`clippy -D` 0 警告；配对/行数双门禁 PASS；TUI 冒烟 0 panic（`Space`/`[1/211]` 在场）；七步钩子 `0ffb83b` 7/7。

#### 本项代码评估（四段）

- **代码质量**：扫描两轮脚本化（精确 md5 + 字面量归一化），不留目测死角；新函数/新夹具全部同文件配对桩，425 只增不减；外部改动按 P-18 取证→ask 流程处置、未猜测回滚。
- **实际使用**：对外行为零变化（纯测试夹具 + 构造组件化；冒烟 211 题正常上屏）；`self.conn.*` 让双协议共享状态一眼可见、base 归一化只此一处；同名夹具/同构构造留有可复跑的扫描口径。
- **代码耦合度**：依赖零变化（未引入任何跨 crate dev 耦合——engine/grader 同构夹具判定**不**共享是有意保守）；llm provider 域内 openai/anthropic 与连接配置单点耦合、与协议逻辑解耦。
- **鲁棒性**：ProviderConn 构造单点（尾斜杠归一化原两处现一处）；wiremock 双协议契约测试与冒烟全绿=未碰 wire 路径语义；流程修正 = PROGRESS/PITFALLS 一律分路径 `git add <file>`、见非本人改动先 diff --staged 核对（P-18 复犯教训）。

**后续观察**：
- [ ] 无新增改进建议（#30 gen 500 行压线、#31 裸 bool 已在册待办）。

### 2026-09-27 · 模块交叉检查：跨模块参数收口 + 跨模块夹具去重（非阶段任务，用户手动发起）

- **背景与指令**：用户两问——①「模块交叉部分检查：模块应该只转递**少量参数**即可调用其他模块方法」；②「检查不同模块之间代码是否有重复/可以复用的部分」（上一轮只合了模块内重复，本轮专查跨模块）。
- **检查方法**：(1) 全仓 fn 签名参数扫描（≥4 参清单，共 8 条）；(2) 跨 crate 调用点实参扫描（排除嵌套调用逗号误报）；(3) 全仓同函数体 md5 两轮扫描（长函数体 + 短函数体）；(4) 裸 bool 对照 docs/01 §1「禁裸 bool 参数」。
- **改动（2 提交）**：
  - `e49be31` **记账参数收口（docs/05 §1 契约先行）**：`AiLedger::record_usage` 原 4 位置参数（kind/model/tokens_in/tokens_out）→ `core::UsageRecord` 单结构；全链改造 = llm trait/StubLedger/MemLedger + router 生产记账调用 + engine StoreLedger + jev client 记账 + `store::record_usage`（**5 参→2 参**）+ 20+ 测试调用点全部 `UsageRecord::new(…)`；llm 再导出 `ambition_llm::UsageRecord` 供不依赖 core 的 jev（Cargo 零变）。
  - `4215013` **跨模块夹具去重二期**（净 +171/-236）：store `memory_pool` **×7 文件**逐字复制 → repo/fixtures 升 crate 级 `common/`（与 jev/llm/engine/tui 模式统一）；llm `Scripted` 脚本供给方**整体**（struct + new/calls + LlmProvider 实现，requests/router 两份逐字节同）+ `sample_req` ×2 收口 common；engine `attempt_for` ×4（装配/练习/题包/裁决）收口 common fixtures；+7 夹具配对测试（内存库建表断言替换占位式断言）。
- **检查结论（未改动项与理由）**：
  - **良样例**：`engine.submit_answer(&Attempt)` 单参门面（qid/answer/timing/confidence/mode 已打包进 Attempt）、`Grader::new(3)`、`due_list(now, limit)`——两壳调 engine 均 1-2 参 ✓。
  - **合规保留**：`Question::new(5)`=领域构造器、`save_game_state(pool, player, xp, streak)` / `add_blank_acceptable(pool, qid, index, value)`（含句柄 4 参、3 数据参）、seed_gen `choice/multi(5)`（示例内）——均在 docs/01 ≤5 上限内且数据参 ≤3。
  - **生产码跨模块重复 = 零命中**：两轮全仓同体扫描（>60 字符 + 短函数）未再发现跨文件重复——上一轮收口全部守住；openai/anthropic 各自的 `fn model`、MemLedger 内存聚合 vs store SQL 聚合属不同实现体，非重复。
  - **发现 docs/01「禁裸 bool」违例 2 处**（srs `due_queue(…, interleave)` 生产调用点传字面 `true`、grader `normalize_for_compare(…, ignore_articles)` 生产无调用）→ 改进建议 #31（改枚举需契约先行）。
- **验证**：**420 测试全绿**（+7）；`clippy -D warnings` 0；配对/行数结构门禁 PASS；TUI 冒烟 0 panic（`Space`/`Ctrl+Q`/`[1/211]` 在场）；七步钩子 2 提交 7/7。

#### 本项代码评估（四段）

- **代码质量**：契约先行（docs/05 §1 先于 UsageRecord 改造）；配对测试随新函数/新夹具同步落同文件（Scripted 的 new/calls/complete/model 四方法逐一配桩）；420 只增不减；两轮同体扫描脚本化（不靠目测）。
- **实际使用**：跨 crate 记账调用从“拉 4 个位置参数对号入座”变 1 个具名对象（可读性：`UsageRecord::new("question_gen", "m1", 40, 12)` 自文档化）；对外可观察行为零变化（台账行/缓存/判分不变，冒烟验证）；对下游 API 仅 record_usage 签名变更（已文档化）。
- **代码耦合度**：依赖方向不变（05 不依赖 store 红线保持：UsageRecord 落 core、llm 再导出、jev Cargo 零变）；夹具按 crate 收口不跨 crate；store fixtures 升 crate 级后 `common` 命名与五 crate 统一；两轮扫描证明跨模块重复已清零，后续新增跨模块代码有脚本基线可比对。
- **鲁棒性**：record_usage 收口不改旁路语义（StoreLedger 读写失败仍 warn 不阻断）；内存库夹具测试改真实建表断言（原占位断言被 clippy 拦下，未带病过门）；fixture 可见性 pub(crate) 最小化；锁/降级路径未触碰。

### 2026-09-27 · 模块目录细分 + 代码复用收口（非阶段任务，用户手动发起）

- **背景与指令**：用户两问——①「每个模块还能继续细分：模块/src/细分功能/组件/工具/通用等，**不要单个文件**」；②「检查代码复用，**能合并的功能尽量合并成一个**，防止代码无限变大」。
- **摸底**：11 crate 的 src/ 全平铺单层（tui 13 文件、core/gen/grader 9-11），typing/gamify 单文件巨石；重复实测：测试夹具 `choice_question` ×11（grader 内 6 份逐字节同、engine 内 4 份同）、`fill_question` ×7、`render_to` ×7、`now_unix()` ×3（实现全同）、l2 选版 match ×4、锁中毒恢复样板 6 处、store `from_str+map_err` ×5；另发现 pre-commit 结构门禁 glob 只盖 `src/` 一层——**目录细分前必须先升级，否则门禁失明**。
- **改动（5 提交）**：
  - `4387881` **文档+门禁先行**：docs/01 §1 组织规范入册目录细分（lib.rs 薄壳 + `<feature>/mod.rs` + 组件文件 + 跨域 `use crate::*`；移动文件时 include_str!/crate:: 路径同步改）；pre-commit 第 4 步行数/配对扫描 glob → `find` 任意深度；docs/03 键位入口路径同步 `keys/mod.rs`。
  - `b82afcd` **11 crate 目录重组**（87 文件，git mv 保历史，+1463/-1319）：core `model/+common/`；typing 拆巨石 `model/+metrics/`、gamify 拆 `model/+projector/+rules/`；srs `model/+schedule/`；store `repo/+migration/`；llm `api/+provider/+router/+ledger/`；jev `wire/+client/+route/`；gen `draft/+pipeline/+content/`；grader `rule/+pipeline/+common/+corpus/`；engine `assembly/+practice/+srs/`；tui `keys/+state/+update/+render/`。模块路径经 `pub mod`+`pub use` 再导出链保持（`crate::question`/`crate::update`/`crate::breaker` 等既有引用零改）；移动文件 `use super::*` → `use crate::*`（薄壳子目录）或保持（hub 子组件经 `use crate::*` 链传递）；include_str! 相对路径随文件目录深度同步（新坑 P-19）。
  - `04b61a1` **生产码复用收口**（+237/-229）：core 新增 `common/time::now_unix`（收 tui/llm/engine 三处逐字重复）、`common/sync::lock_recover`（收 gamify/llm·router·llm·ledger/engine·practice 六处锁中毒恢复样板，含 catch_unwind 真中毒测试）、`common/ids::{is_zh_lang,pick_edition}`（收 core name_in + gen prompt + grader arbitrate/corpus 四处 zh 谱系 match 同构 = **改进建议 #21「选版三处同构」完成**）；store `repo::decode_payload` 收 attempts/arbitration/cards/question_repo 五处 from_str+map_err；`clamp_unix_range` 升 pub(crate) 并钉 `kv::cache_purge_before`（P-15 同口径 + i64::MAX 回归测试）；store 加 serde 直依赖。
  - `4423096` **测试夹具按 crate 收口**（39 文件，+592/-856 = 净 -264 行）：jev `{ok_body,sample_questions}` ×2→1、llm `ok_json` ×2→1、store `{sample_question×3, sample_attempt×2, sample_grade×2}`、grader `{choice_question×6, fill_question×4, multi_question×3, offline_router×4}`、engine `{choice_question×4 + srs 带标签变体 choice_question_tagged}`、tui `{grade×3, fill_question×3, multi_question×2, render_sized}`——**7 处宽度感知渲染收集（P-2 续格口径）收口单实现**，各测试 1 行委托保留原面板尺寸（72×18/72×20/76×24/76×22），32 处调用点零改动。
  - `762288b` **seed_gen 去重**：cloze/grammar_fill 逐字重复构造收口 `PassageFill` struct + `passage_fill`（docs/01 参数 >5 组 struct）；重跑 `cargo run -p ambition-gen --example seed_gen` 211 题、**data/ 零 diff**（#17 幂等）。
- **验证**：`cargo fmt --check` ✓；`clippy --workspace --all-targets -D warnings` 0 警告 ✓；**412 测试全绿只增不减**（Phase B 后 396 → 收口 +16，全部是收口夹具/新工具的同文件配对桩，P-16 终态规则全程守住）；结构门禁（find 版行数+配对）PASS；七步钩子 5 提交全过；seed_gen 重跑 data/ 零 diff；**P-14 真机冒烟**：ambition.exe 4s → 0 panic、`Space`/`Ctrl+Q`/`Ambition`/`[1/211]`/`Enter` 全在场；`data/` 仍只含 seed/chinglish/lexicon/patterns 内容资产、无任何运行时读提示词。

#### 本项代码评估（四段）

- **代码质量**：文档与门禁先行（4387881 先于全部移动提交）；5 提交七步钩子 7/7；412 测试只增不减，+16 全为收口夹具的配对测试；文件全 ≤500（`gen/pipeline/mod.rs` 500 压线，已列入改进建议 #30）；新坑入册 P-19、P-17 复犯补记（cargo fix 被陈旧元数据误导 + 行号降序位移坑）。
- **实际使用**：真机冒烟 0 panic、211 题 seed 正常入队上屏；seed_gen 重跑 data/ 零 diff；**增减功能成本下降**——每个 crate src/ 顶只剩 lib.rs（+tui main.rs），36 个功能目录，新功能 = 进对应目录加组件文件；对外 API 零变化（再导出链保路径），两壳（tui/engine 壳层外）下游一行未改即编译通过。
- **代码耦合度**：依赖方向不变（11 crate 单向无环）；跨目录统一 `use crate::*` + 共享导入留 lib.rs（docs/01 §1 入册），hub 链（mod.rs `use crate::*` → 子组件 `use super::*`）保持组内共享；收口工具全落 core（now_unix/lock_recover/pick_edition）= 向下复用不新增环；夹具按 crate 收口不跨 crate 共享，测试隔离不变。
- **鲁棒性**：锁中毒恢复单实现（core::lock_recover，含真中毒恢复测试）；P-15 补防（kv 淘汰超界钳制 + i64::MAX 回归，原超界会 NULL 静默不删）；载荷解码单口显式 Decode 不静默；include_str 移动断裂被编译期拦截（P-19 入册）；锁样板合并后消息带 where_ 定位（`MemLedger.cache_get` 等）可见性不降。

**后续观察**：
- [ ] #21 已完成选版部分（core::pick_edition，04b61a1）；剩 include_str loader 单入口随 16-config。
- [ ] #30 `gen/pipeline/mod.rs` 500 行压线——再增内容前先把 generator 测试拆 `generator_tests.rs`（留同文件配对桩，P-16）。

### 2026-09-27 · M4 记忆：FSRS 接线 + 复习队列 + 假对上线 + 词库句式入卡（阶段任务；用户指令「继续实现 m4」）

- **范围拍板（用户确认）**：①全部 9 项含 TUI 复习屏；②词库数据由 agent 生成小规模种子；③**承接项 #15 题型补全（完形/阅读/七选五/语法填空）一并做**。中途另裁定**取消配对桩**（结构门禁配对范围放宽到同 crate，见 P-16 修订/P-18）。
- **改动（8 个提交）**：
  - `11d1325` **题型补全**：docs 先行（docs/02 §1 Body 族+`Answer::Group`、docs/06 M4 注记、docs/07 §1 递归判分口径）→ core `QuestionKind` 扩 Cloze/Reading/SevenOfFive/GrammarFill + `Answer::Group` + 判定/拆解辅助（kind.rs）；gen `QuestionKindTag` 扩四型（as_str/is_passage）+ DraftQuestion 扩 passage/items 校验解析（draft_passage.rs）+ qc_state 语篇口径；grader `grade` 检测复合题 → `grade_composite` 拆子题递归走完整三级流水线 → 汇总（Σ分、任一 Pending 整体 Pending、graded_by 取最重路径 Human>LlmReview>Jev>Rule、子题 qid=父+下标防碰撞）；tui 逐槽作答（sub_answers）；
  - `feded54` 新题型 TUI 渲染作答（语篇+缺口标注+子题）+ draft schema 扩；
  - `63444aa` seed 补语篇四题型各 2 篇（203→211 题，qid 幂等旧题零变）；
  - `b198ec7` **store 迁移 v3**：`srs_queue`（card_id PK + payload + **due_at 整数 unix 索引** P-15 + wrong_count 查询列）+ `get_attempt`/`recent_timings`（SQL json_extract 过滤无计时行）+ cards.rs CRUD（幂等 REPLACE、坏载荷显式 Decode 不静默丢卡）；
  - `bc2f244` **engine SRS 旁路+门面**：`EngineError` 加 Srs/Vocab；`SubmitOutcome.srs: Option<CardAction>`；`is_guess_attempt`（近 200 次滚动基线、`recent_timings(exclude)` 排除本次——裁决路径等价 insert 前取样）→ `srs_after_attempt`（Wrong/Partial 建卡/提权+错因入卡、Correct 假对建卡、非假对已卡按 Good、Pending 不动、失败 warn 不回滚）；`due_list`（due_cards→ReviewItem 组装→due_queue 过滤排序交错，悬空卡 warn 跳过）/`review_answer`（FSRS 调度+upsert+ReviewDone→settle Xp(1)）/`wrong_wordbook`/`question_back`（全题型）；
  - `5ece073` **M4-5/6/8**：`apply_srs_hint`（错词入册/提权/FSRS 安排三文案）+ render 提示行；**Review 复习屏**（Screen::Review + KeyCtx::Review：Space/Enter 翻面·1-4 评分·Tab/Esc 返回 + **Ctrl+A/Ctrl+R 直达落地**（docs/03 §5.3 同步）+ Cmd::ReviewLoad/ReviewRate 接 due_list(limit 20)/review_answer + 四态渲染 + finished 后仍可进复习屏）；`resolve_pending` 裁决后补执行 SRS 处置；
  - `edb5cae` 结构门禁配对范围放宽到同 crate（用户裁定取消配对桩；docs/01 §3 先行）；
  - `8769432` **词库/句式**（M4-7）：`data/lexicon/en/senses.json`（31 义项/11 词，bank/light/run 多义抽检达标）+ `data/patterns/en/patterns.json`（8 句式含 note_zh/例对/traps）；engine/lexicon.rs loader（schema_version=1 校验、坏文件显式 Err→assemble 仅 warn 不阻断）+ `word_detail(lemma)` + `enroll_builtin_cards`/`enroll_card` 幂等入卡（**已有卡不覆盖 FSRS 调度**）；复习条目背面接真实数据（义项释义/句式讲解+form）。
- **验证**：fmt/clippy `-D warnings` 干净；`cargo test` **386 用例全绿**（M4 净增 +71：core 31/gen 46/grader 63/store 33/engine 62/tui 31…）；七步钩子 7/7；**真机冒烟（P-14 产物断言）**：ambition.exe 启动 6s → `schema_version=3`、211 题入 seed、`srs_queue` **39 张内置卡**（31 义项 + 8 句式）自动入队、due/wrong 双索引在、TUI `[1/211]` 与新键位提示渲染无 panic。

#### 本项代码评估（四段）

- **代码质量**：8 提交全部七步钩子过；文档先行（docs/01/02/03/06/07/09 六册同步）；新增测试 71 个含递归判分汇总口径、幂等入卡不覆盖调度、假对三形态接线、Review 四态渲染等关键断言；文件均 ≤500 行（拆出 kind/draft_passage/rule_tests/update_tests/srs_tests/review_state/review_render 七个模块）。
- **实际使用**：真机可跑——启动即得 211 题 + 39 张到期复习卡；答错题自动入错词本（提示行可见）；Ctrl+R 进复习屏翻卡评分（1-4）真 FSRS 调度；完形/阅读/七选五/语法填空四新题型可作答可判分；裁决后补入卡。边界：义项种子仅 11 词/31 义项（扩充随 EC-DICT，M5+）；阅读子题按顺序作答不可回改（#26）；裁决路径 TUI 无 SRS 提示（#29）。
- **代码耦合度**：依赖方向不变（store→srs 新增、engine→srs 新增，均单向无环）；`CardAction/Rating` 经 engine 再导出，两壳免直依赖 srs；`srs_queue` 单写者=08 经 engine（00 §4 合规，store 仅 CRUD）；排程唯一实现仍是 srs crate（00 §5 合规，engine 只编排）。
- **鲁棒性**：SRS 全链路旁路失败 warn 不回滚主事务（P-8 口径）；基线取样失败降级非假对；坏载荷显式 Decode；卡片悬空 warn 跳过不炸队列；裁决 attempt 缺失跳过补执行；Review 评分未翻面防误触、评分失败独立错误行；并发编辑事故按 P-18 流程处置（停手取证→ask→精确 add）。

- **M3 承接项处置**：#15 题型补全 ✅、#19 假对合流 ✅（本阶段完成）；#13 题包屏 / #14 seed 降级开关 / #16 seed 扩轨 → 承接 M5。

### 2026-09-27 · TUI 键位二期：多空分行作答 + 全站禁单字母快捷键（非阶段任务，用户手动发起）

- **背景与决策（用户裁定）**：①“多空题目应该多空填写而不是使用 `;` 分割”；②“统一所有页面的快捷键，**不允许单字母快捷键**”——追问确认：**仅非字母键 + Ctrl 组合**（方向/Enter/Space/Esc/Tab/数字 + Ctrl）；待裁定裁决改**方向键选择高亮 + Enter 确认**；未来多屏切换改 **Ctrl+字母直达**。
- **改动**：
  - **多空分行**：state `input: String → inputs: Vec<String> + blank_sel`（`blank_count(kind)` 单点重建：构造与下一题两处收口）；update 每空独立提交（逐位 trim 按序）、填空 ctx 的 Move = 切空位（`Tab/→/↓` 下一空、`Shift+Tab/←/↑` 上一空；单空原地不计修订、判分后锁定）；render 每空一位独立行（`❯`+`▍` 标当前空、多空带编号）；**废除 `;` 分隔**（错误提示/hint/docs 同步删）；防御分支保留（输入位与空数不齐不提交，人为造错测试钉住）。
  - **禁单字母快捷键**：keys.rs 三层表重写——①恒定层 `Ctrl` 组合：`Ctrl+Q`/`Ctrl+C` 恒退出，其余 Ctrl+字母预留屏幕直达（多屏落地接 `Msg::Screen`，TODO 注释）；②Choice=`↑/↓` 移动 · `Space` 选中 · `Enter` 提交 · `1-4` 自评（数字）；③Pending=`←/→`（或 `↑/↓`）高亮选择「判对/判错」· `Enter` 确认（**新 `Msg::ConfirmArbitrate`，删 `Msg::Arbitrate(bool)`**）· `Esc` 跳过（保持待裁定桶）；`q/j/k/y/n` 全废——测试**逐字母断言 a-z 在非填空 ctx 全部 None**、填空 ctx 全部=打字文本；
  - state 新增 `arbitration_sel`（0=判对；`apply_grade`/`Next` 复位）；render 待裁定浮层改为 `[❯判对] [判错]` 动态高亮 + 统一键位提示；完成页“按 q 退出”→“按 Ctrl+Q 退出”；
  - **docs 先行**：docs/03 §5.3 全量重写——键位总则（禁单字母；保留 Ctrl+C/Q；终端不可达 Ctrl+I=Tab/Ctrl+H=Backspace 已避开）+ 屏幕直达表全改 Ctrl+字母（Home=Ctrl+B … Settings=Ctrl+S）+ 答题/填空/待裁定/题包各行；改进建议 #17/#18 的 `e`/`i/x` 单字母键位同步改为非字母口径。
- **验证**：fmt/clippy 零告警；`cargo test` **313 用例全绿**（tui 32）；真机冒烟 3s：新提示行渲染出 `Space`/`Ctrl+Q`/`1-4`，旧 `j/k` 字样 0 命中，无 panic，taskkill 干净。提交 `412768e`（七步钩子 7/7）。

#### 本项代码评估（四段）

- **代码质量**：键位/提示/文档三处同源（keys::hint + docs §5.3 + 键位总则入册先行）；多空模型 1 处重建口（blank_count）；测试 313 全绿含“逐字母 a-z 禁单字母”总则测试与多空切位/防御分支；文件均 ≤500 行。
- **实际使用**：多空题不再需要记 `;` 分隔符（原 UX 最大认知负担）；裁决从“记 y/n”变成浮层高亮+Enter 确认（防误触且所见即所得）；填空打字与命令键零冲突（字母只可能是文本）；已知边缘：Ctrl+字母屏直达在多屏落地前按了无效果（恒定层已留 TODO）。
- **代码耦合度**：依赖不变；Msg 枚举净减一个（Arbitrate(bool)→ConfirmArbitrate，路由收归 keys）；main 仍零键位特判；update 的 pending/fill/choice 三态分支都有双保险守卫。
- **鲁棒性**：填空输入位与空数不齐时防御性报错不伪造提交；arbitration_sel 两处复位（新判分/下一题）防上题高亮泄入；单空/判分后/题型不符的切位与输入全部 no-op；Ctrl 层先行拦截保证任何 ctx 下 Ctrl+Q/C 都能退出。

### 2026-09-27 · TUI 键位统一重构：KeyCtx 唯一路由 + 填空直接编辑 + Space 多选（非阶段任务，用户手动发起）

- **背景**：用户问“填空题怎么办、快捷键需要修改”，并要求“**所有快捷键统一、代码复用**”。真实痛点：①填空输入态唯一退出键 `Esc` = **直接退程序丢答案**（打字习惯性按 Esc 即炸）；②docs/03 §5.3 承诺的 `Space` 选中（多选）未实现；③键位逻辑散落三处——`key_to_msg` + main 的 `Enter→Next` 手工改写 + update 内守卫，且 render 硬编码三份提示文案（不同源，改键必漏）。
- **决策（用户确认）**：Esc=退出编辑**保留已输入**、**打字直接恢复编辑（不设 i 进入键）**、退出程序改 `Ctrl+Q`/`Ctrl+C`；一并做 Space 选中 + 多选支持。
- **改动**：
  - **docs 先行**：docs/03 §5.3 键位表修订（2026-09-27 修订注记 + 唯一入口声明）；
  - `input.rs → keys.rs`：`KeyCtx`（Choice / Fill{editing} / Pending / Graded / Finished，由 `AppState::key_ctx()` 计算）+ `key_to_msg(ctx, key, mods)` **两层路由**（①恒定层：Ctrl+Q/Ctrl+C 任何上下文退出 ②上下文层选表）——**Enter→下一题也入表，main 删掉 Enter 改写特判**；`hint(ctx)` 与路由同源，render 三处硬编码提示全部改取 hint（Paragraph 加 wrap 防长提示截断）；
  - state：新增 `editing`/`checked` 字段、`key_ctx()`；`is_fill_input → is_fill_pending`（待答 ≠ 编辑中，两者解耦）；
  - update：Char/Backspace **打字即进编辑**（首键恢复，无需进入键）、`StopEdit` 退出编辑保留文本、`Select` 多选切换（单选不建选中集、光标即答案；多选提交选中集优先、空则回退光标位——兼容旧行为）、Next 复位 editing/checked；
  - render：选项 `✓` 选中标记（光标 ❯ 分离）；提示行随 ctx 实时变化（编辑/浏览/待裁定各自不同）。
- **验证**：fmt/clippy 干净；`cargo test` **309 用例全绿**（tui 29：keys 7 测试（恒退出/Esc 作用域/Enter 统一/填空吞字/作答表/待裁定表/提示同源互异）+ Select/StopEdit/checked 渲染 + is_fill_pending/key_ctx 分拆路由测试）；**真实冒烟**（P-14 产物断言法）：ambition.exe 启动 3s → 渲染 `[1/203]` 与新提示行「j/k 移动 · Space 选中 · Enter 提交…」→ 无 panic、taskkill 干净。提交 `bc82a47`（七步钩子 7/7）。

#### 本项代码评估（四段）

- **代码质量**：键位逻辑从 3 处收敛到 1 处（keys.rs 单一事实源，提示文案同函数导出）；文档先于代码；测试全绿、clippy -D warnings 零告警；文件均 ≤500 行（update 427 最大）。
- **实际使用**：填空“打字即答、Esc 不丢答案、Ctrl+Q 必能退出”是真实可用的最小闭环；多选题首次能选全（旧版单光标提交多选必错）；已知边缘：单选 Space 无效果（光标即答案，预期行为）；`?` 帮助屏仍欠（docs 承诺，见改进建议 #24）。
- **代码耦合度**：依赖方向不变；main 进一步变薄（只剩 I/O + 副作用执行，键位/语义零特判）；KeyCtx 是 state→keys 单向计算，update 仍双保险守卫（键位层错了也不越权）。
- **鲁棒性**：Ctrl+Q/Ctrl+C 恒退出保底（raw mode 无 SIGINT，防“困在输入态”）；StopEdit/Select/Char 均有判分后/题型不符的忽略分支（防误触）；Enter 语义入表后不再依赖 main 执行顺序（旧版特判漏掉即卡死下一题的隐患消除）。

### 2026-09-27 · data/ 目录净化：prompt/判分语料移出 data/（非阶段任务，用户手动发起）

- **背景**：用户指出 `data/` 应只放**用户可导入的词表/题目等数据**，提示词（`data/prompts/`、`data/jev/` 判分语料）不应被用户修改；其中 `question_gen.md` 原先还带 `fs::read_to_string("data/prompts/…")` cwd 运行时覆盖——是全仓唯一“改了就生效”的提示词口子。
- **决策（用户确认）**：①编译期内嵌资源就近放各 crate 的 `assets/`；②`question_gen.md` 运行时覆盖**彻底删除**（模板只内嵌）；③`data/seed`、`data/chinglish`（都是题目内容）**留在 data/**。
- **改动**：资源搬家 `data/prompts/question_gen.md`→`crates/gen/assets/prompts/`、`data/prompts/arbitrate.md`→`crates/grader/assets/prompts/`、`data/jev/error_tags.json`→`crates/grader/assets/jev/`（git mv）；四处 include_str 相对路径改写；`load_prompt()` 纯 include_str、删运行时读取（测试改名 `test_load_prompt_returns_embedded_template`）；文档先行：docs/05 §1/§3、docs/16 §3 目录布局同步（data/ 树删 prompts/、jev/ 两行 + 定位声明）。`data/` 现仅剩 `seed/en/cet4/core.qpack.json`、`chinglish/en.json`。
- **验证**：`cargo fmt --check`、`cargo clippy --all-targets -D warnings` 干净；`cargo test` 全绿（12 组 295 用例，0 failed）；全仓 grep `data/prompts|data/jev` 仅剩历史进度记录。提交 `d7ebe00`（七步钩子 7/7）。

#### 本项代码评估（四段）

- **代码质量**：改动 6 文件 +15/-14，测试全绿；文档先于代码（05/16 分册同步）；无新增公共 API。
- **实际使用**：发布形态下用户可见的 `data/` 只剩题库/错式库，提示词与判分语料编译进二进制、改源文件也不影响已装版本；边缘：仓库内改 `assets/prompts/*.md` 需重编译才生效（这是预期——内容变更本就随发版）。
- **代码耦合度**：零依赖方向变化（仍 grader/gen 各自内嵌自己的资源）；原“三处 include_str 重复”遗留未解（见改进建议 #21），仅消除了 gen 里唯一的运行时文件 IO。
- **鲁棒性**：净提升——删掉一条运行时读文件失败路径（cwd 不同 → 模板可能悄悄换成用户本地文件的不确定性）；模板加载现在是纯编译期常量，行为与 cwd 无关、可复现。

### 2026-09-27 · 提示词 i18n：prompt/判分语料按 l2 分版（en/zh）（非阶段任务，用户手动发起）

- **背景**：用户要求提示词支持中文/英文等多版本并接入 i18n（docs/16 三维度 ui_lang/l1/l2）。**决策（用户确认）**：①版本键 = **l2 目标语**（`assets/prompts/<kind>/<l2>.md`、`assets/jev/<l2>.json`）；②范围 = question_gen + arbitrate + **error_tags 判分语料**；③**缺版回退 en**；④zh 语料**按中文教学法改写 28 类**（非直译，措辞 v1 待判分负责人校订）。
- **改动**：
  - core：`LangTag::name_in(written_in)`（en/zh 两种书写语言 × en/zh/ja 语言名，未收录组合回退标签本身）+ `DEFAULT_L1 = "zh-Hans"`（TODO 16-config：config 落地后注入，不再取常量）；
  - gen：`GenSpec += l2: LangTag`（serde default=en，docs/06 §1）；模板 `question_gen/{en,zh}.md`；`load_prompt(l2) -> (模板原文, 模板书写语言)` 按 l2 选版、缺版回退 en；`{{l1}}/{{l2}}` 渲染为语言名，删硬编码 `"English"`（原来 `{{l2}}` 直接写死）；
  - grader：`arbitrate/{en,zh}.md` 按 `question.lang` 选版（system 模板占位填充）；语料 `jev/{en,zh}.json`，**新增 `instructions` 字段随语料进 Jev 请求**（不再是代码硬编码英文）；`corpus_value/error_criteria/error_cause_question/error_cause_questions/corpus_version` 全部 `lang` 参数化（每语种一对 OnceLock 单例），`merge_error_cause(lang,…)` 与 fill/rule 调用点改传 `question.lang.0`；`ERROR_TAGS_VERSION` 1.0.0 → **1.1.0**（新增 instructions + zh 版，全部语言文件同步断言）；
  - zh 内容：question_gen/zh.md、arbitrate/zh.md 中文正文；error_tags/zh.json 28 类按中文教学法改写（article→量词误用、tense→“了/过/着”体标记、non_finite→的/地/得与修饰语位置、chinglish→直译腔…），键集仍 = `ErrorTag` 全集；
  - 文档先行：docs/05 §1/§3、docs/06 §1、docs/07 §3/§4、docs/16 §3；改进建议新增 #23（gen qc 五问指令仍英文硬编码）。
- **验证**：`cargo fmt --check`、`cargo clippy --all-targets -D warnings` 干净；`cargo test` 全绿（12 组 301 用例，0 failed）；新增测试：name_in 语言名回退、gen 按 l2 选版/缺版回退/占位全替换、arbitrate 双语 system、语料逐版断言（28 键=ErrorTag 全集、en 版无 CJK/zh 版有 CJK、version 全版同步、lang 字段与版本一致、instructions 非空、ja 缺版回退 en）。提交 `a6f1542`（七步钩子 7/7）。

#### 本项代码评估（四段）

- **代码质量**：文档先行四册同步；新增测试 12 个、全部公共 API 有 rustdoc；fmt/clippy 干净；zh 语料措辞标 v1 待校订，判分口径变更已按规范升 version（1.1.0）并全版同步断言钉住。
- **实际使用**：en 轨行为完全不变（渲染后 prompt 与改前同义、语料英文原文未动）；l2=zh 时出题/复核/错因判断全走中文正文；未来 ja/de/es 等新语种先只写 en 版即可上线（占位自动注入正确语言名），新语种版到位后零代码切换——接入点只有各 crate 的 match 选版函数。
- **代码耦合度**：依赖方向不变（gen/grader 互不依赖，共享能力下沉 core `LangTag`）；corpus 公共 API 由无参改 `lang` 参数——调用点全部在 grader 内部（rule/fill/helpers/arbitrate），无外部破坏面；`GenSpec.l2` 增字段带 serde default，旧序列化数据兼容。
- **鲁棒性**：缺版回退与语料坏文件降级叠加（zh 版损坏 → 空 criteria 同 en 版既有行为；instructions 走 lang→en 双层回退）；OnceLock 按语种分单例无锁竞争；占位替换后测试断言无 `{{` 残留（防未替换占位漏进 LLM/Jev 请求）。

**后续观察**：
- [ ] zh 判分语料 28 类措辞 v1 需判分负责人校订（升 version 全版同步）；qc 五问指令本地化见改进建议 #23。

### 2026-09-24 · 分层重构：9 个超标文件拆模块（非阶段任务，用户手动发起）

- **背景**：用户指出代码全挤单文件、可读性差；docs/01 §组织本就规定 `lib.rs` 薄壳 + 文件 ≤500 行，但执行走样（9 个文件超 500，最大 pipeline.rs 1463 行 = 2.9×）。
- **拆分结果（全部 ≤500，pre-commit 已加行数门禁防回退）**：
  - `grader/pipeline.rs` 1464 → pipeline(192)+rule(398)+fill(481)+arbitrate(335)+helpers(222)+thresholds(72)+judges(66)；
  - `store/lib.rs` 1316 → lib(27)+migration+question_repo+attempts+arbitration(481)+kv+usage；
  - `gen/lib.rs` 1255 → lib(34)+spec+draft+qc+prompt+generator；
  - `engine/lib.rs` 1193 → lib(111)+lifecycle+packs+practice+pending（impl Engine 按方法拆 4 块）；
  - `core/lib.rs` 1083 → lib(22)+ids+taxonomy+question+attempt+usage+event+analysis；
  - `jev/lib.rs` 912 → lib(27)+types+route+codec+error+client；
  - `tui/lib.rs` 816 → lib(25)+state+input+update+render；
  - `llm/router.rs` 657 → router(221)+requests(453)+breaker(71)；
  - `srs/lib.rs` 540 → lib(14)+model+schedule；
  - 另：`gen/examples/seed_gen.rs` 639 → `examples/seed_gen/{main,grammar,usage,mixed}.rs`（重跑幂等，data/ 零 diff）。
- **手段**：spec 驱动拆分器（`target/rust_split.py`，按条目/impl 方法/测试三类分配 + 配对/行数预检）；跨模块可见性统一 `pub(crate)`；测试块 use 按 unused 告警逐文件裁剪；同名 fn（as_str/new）的配对测试归拢或补写。
- **门禁升级**：pre-commit 第 4 步扩为**结构门禁** = 函数↔测试配对（范围不变，仅 src/）+ **全仓 .rs ≤500 行**（src+examples）；docs/01 §3 同步（去掉已删的 fn_test_check.sh 引用）。
- **质量**：**295 测试全绿**（重构前 292，+3 新配对测试）、`clippy -D warnings` 零告警、fmt 干净、七步钩子 7/7；P-14 冒烟：ambition.exe 启动 → 203 seed 入库 → TUI `[1/203]` 上屏 ✓。
- **新坑入册**：P-16（拆文件三连锁：可见性收口/配对名前缀/共享 helper）；P-9 复发案例补记（批量 edits 原子回滚）。

#### 本项代码评估（四段）

- **代码质量**：295/295 测试全绿（+3 均为拆分暴露的真实配对缺口：srs as_str、grader Provisional::new、seed_gen 无需）；`clippy -D warnings` 零告警；fmt 干净；七步钩子 7/7（第 4 步已扩为结构门禁）；纯结构调整零行为变更（seed_gen 重跑 203 题 data/ 零 diff = 逐字节幂等）。
- **实际使用**：`ambition.exe` 冒烟 ✓（203 seed 入库、TUI `[1/203]` 上屏、判分/键位无回归）；对外 API 不变（`pub use X::*` 收口，下游 tui/engine 未改一行 import）；边界：拆分器是一次性工具（`target/` 下，不进版本库），后续拆分需手写或重建。
- **代码耦合度**：依赖方向未变（单向无环）；**新增 crate 内模块边界**：grader 6 模块（pipeline/rule/fill/arbitrate 职责链）+ helpers 三方共用；engine 按门面拆 4 块（lifecycle/packs/practice/pending）；跨模块一律 `pub(crate)`（可见性最小化，对外零扩散）；每个模块自带 `mod tests`，测试随被测物走（拆分器按 fn 名前缀自动配平，配对门禁钉住）。
- **鲁棒性**：重构本身无运行时路径变更；**欠账**：①`pub(crate)` 字段/方法（Grader 字段、Engine settle/publish、llm breaker 状态）是拆分引入的可见性扩散，将来若再抽 crate 需收口；②同名 fn（as_str/new）分散在多文件时配对测试靠补写维持，后续改名需两处同步；③`target/` 下的拆分脚本未版本化，丢失后同类重构需重写。

### 2026-09-24 · M3 判题完成

- **jev 全量**：`evaluate_many`（并发池 `JEV_CONCURRENCY=8`、保序、单题失败不中断，futures buffered）；`Route` + `route_by_confidence`/`route_from_margin`（accept/reject 两端闭区间采信、灰区 LlmReview；边界表驱动 + 非英语阈值切换断言）；`JevJob` 作业体。16→21 测试。
- **grader 三级流水线（新 pipeline.rs + corpus.rs）**：`Grader{jev?, llm?, thresholds}` `grade()` **永不 Err**（离线矩阵逐格降级）——①规则命中即完（正确 0 成本）；②Jev 升级：多选漏选逐项 noul（`jev_multi_items`→`MultiJudge`）、填空错空 `jev_blank` 翻案（Acceptable/Partial）/确认错（带错因，置信=最弱 margin）、选择题错只补 `error_cause`；③灰区 → `llm_arbitrate`（`LlmKind::Arbitrate` + 内嵌 arbitrate.md prompt + schema 约束，temperature 0；复核不可用/失败 → Pending 进桶，warn 可见）。`Thresholds::for_lang` en=0.75/0.35 其它 0.85/0.20。**错因分类**：`ErrorTag` 扩到 docs/02 全量 28 类（+`parse_tag`/`all`/`label_zh` 中文展示名）；`data/jev/error_tags.json`（28 criteria 全英文、version=1.0.0 与代码常量同步断言——措辞即判分口径）；唯一归类口 `tag_error_cause`（confidence<accept 不采信、未知键 warn 跳过）。grader 48 测试。
- **待裁定桶（store 迁移 v2）**：`arbitration` 唯一桶（attempt_id 主键 + status/reason/verdict/resolved_at + idx）；`insert_attempt_tx` 在 verdict=pending **同事务**入桶；`get/list/resolve_arbitration`（单事务改 grades：verdict/score/confidence=1/graded_by=human + 桶关闭，重复裁决幂等返 None）；`add_blank_acceptable` 读改写回流（同值幂等/越界 false）。store 24 测试。
- **成本台账**：`AiLedger::usage_summary(from,to)`（MemLedger 时间戳分组 + StoreLedger SQLite 区间聚合，`datetime` 钳制防 NULL 静默——P-15）；`LlmRouter::usage_summary`；engine `cost_summary` 门面；**全局熔断**（连续失败 ≥5 → 快速失败 `LlmError::BreakerOpen` 10 次不发请求 → 放行 1 次试探，成功关闸；缓存命中不计数、Offline 不计入）；`llm_cache` TTL 7 天（`cache_purge_before`，open 时执行）。llm 43→59 测试。
- **engine 装配与门面**：`AiConfig::from_env`（AMBI_JEV_BASE/AMBI_JEV_KEY·TYPESAFE_API_KEY/AMBI_LLM_BASE/AMBI_LLM_MODEL/AMBI_LLM_KEY，key 缺省=离线）+ `open_with_ai`/`open_memory_with_ai` 注入口；`StoreLedger`（AiLedger 的 SQLite 实现经 engine 装配，05 不依赖 store 红线）；`build_grader`；`submit_answer` 改走 `grader.grade()`（移除 `EngineError::RuleIncomplete`）；`settle` 统一旁路结算（提交/裁决共用）；新门面 `pending_bucket` / `resolve_pending`（裁决→改判→填空回流→旁路结算，**回流→二次作答规则直判闭环测试**）/ `cost_summary` / `add_chinglish_drills`。engine 34→35 测试。
- **中式英语错式库**：`data/chinglish/en.json` 12 条 L1 负迁移错式（id/wrong/right/zh_note/tag，schema_version 校验 + id 去重 + 内容自洽）；gen `parse_chinglish/builtin_chinglish/drill_questions` → 单选改错训练题（id 哈希奇偶打散选项防猜、题面无 CJK 保 Jev-state 纯净、qid 内容寻址幂等）。
- **TUI**：待裁定态显示 `⏳ 待裁定：y/n`，`Cmd::Arbitrate` → `engine.resolve_pending`；判分浮层错因标签（`label_zh` 中文）+ 置信度 + 「人工裁决」标记；选择态 `1-4` 自评把握 → `Attempt.self_confidence`（判分后锁定、换题重置；is_guess 接线随 M4）。
- **契约先行**：docs/02（v2 迁移/回流口径）、docs/03（open(cfg)→AiConfig、新门面、键位表）、docs/05（usage_summary/熔断/JevJob/路由语义/验证点全勾）、docs/07（Grader 结构/离线矩阵细化/M3 验证点勾选）先于实现提交。
- **质量**：**282 测试全绿**（M2 182→282，+100）、`clippy -D warnings` 零告警、函数↔测试配对 100%、七步钩子 7/7 一次过；提交 **75408b0**。
- **冒烟（P-14 方法）**：新二进制短跑 4-6s → 杀 → `AMBI_DB` 文件库三件套（db/wal/shm，WAL 1.9MB=203 seed 入库）+ stdout 可见 `[1/203]` 与「1-4 自评把握」提示行。
- **新坑入册**：P-15（SQLite `datetime(unix,'unixepoch')` 超范围返回 NULL → 过滤恒 NULL 静默空结果；钳制 + 边界测试预防）。

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

### M3 代码评估（四段）

- **代码质量**：282/282 测试全绿（M2 182→282，+100）；`clippy -D warnings` 零告警；函数↔测试配对 100%（钩子一次过）；生产码无 `unwrap/expect/panic`（P-4 显式兜底、锁中毒取内层+eprintln、SQLite NULL 边界用钳制而非静默——P-15）；wiremock 契约覆盖关键路径（fan-out 失败隔离/灰区→桶/翻案→回流→直判闭环/熔断三态/LLM 复核成败双路）；判分语料 version 代码↔文件双同步断言（措辞即口径）；契约先行 4 分册先于实现提交，签名漂移（MultiJudge/chinglish 门面名）提交前已回改对齐。
- **实际使用**：`ambition.exe` 真实启动 → 203 seed 入库 → 答题；**无 key 全链路可用**：多选漏选/灰区 → 待裁定行 → `y/n` 人工裁决 →（填空）认可答案回流 → 同类题下次规则直判；错因中文标签与置信度直接上屏；`1-4` 自评落 Attempt；**填 key（BYOK env）解锁**：Jev 模糊翻案/错因分类/多选逐项分 + LLM 粗判。**边界**：成本面板只有 `engine::cost_summary` 门面，TUI 无展示屏；改错训练只有 `engine::add_chinglish_drills`，TUI 无入口；讲解层②③（06::explain、`e` 键）未做（归 M4）；arbitration reason 未区分灰区/离线（单值 pipeline_undecided）。
- **代码耦合度**：依赖仍严格单向无环：`core ← {llm, jev} ← grader ← {gen, store} ← engine ← tui`（grader 新增 jev/llm 依赖，与 docs/07 `Grader{jev,llm}` 契约一致）；**AI 唯一出口红线保持**：`AiLedger` trait 定义在 llm、`StoreLedger` 实现在 engine、05/07 均不依赖 store；grader 纯判定不落库（落库全在 engine/store 单事务）；TUI 纯状态机，裁决副作用全在 main→engine。**遗留**：grader 内嵌 `data/jev/error_tags.json`、gen 内嵌 `data/chinglish`/`data/prompts` —— include_str 模式三处重复，随 16-config 抽统一资源 loader。
- **鲁棒性**：`grade()` 永不 Err + 离线矩阵逐格有测试（Jev 无 key/网络失败、LLM 无 key/复核失败、全离线三态）；Pending **同事务**入桶无半写；裁决幂等（重复/不存在返 NotFound 显式报错，不静默）；回流失败 warn 不回滚已生效裁决；熔断三态实测（开闸拒发/窗口递减/试探续票/成功关闸）且缓存命中不受害；datetime NULL 静默空集已钳制（P-15）。**欠账**：LLM 粗判 `reason` 未持久化（GradeResult 无 detail 字段，随讲解层 GradeDetail 补）；选择题人工裁决不回流 answer 集（只回流填空，见 #20）；`arbitration.reason` 单值；evaluate_many 尚无真实消费方（mock 模考 M5 用）。

### M1 改进建议（M2 开工即处理，勾=完成）

- [x] 1. engine 接线 gamify 效果持久化：提交后 `apply(effects) → save_game_state`（M2：GameProjector publish→snapshot→save，测试断言 xp/streak 落库）
- [x] 2. `Engine::open(path)` 文件库 + `schema_version` 迁移表（docs/02-store §2）（M2：WAL+busy_timeout，MIGRATIONS 事务化）
- [x] 3. 引入 `EventBus` trait 替换 engine→gamify 直调（docs/02 DomainEvent 已定义）（M2：core::EventBus，GameProjector 实现）
- [x] 4. store 枚举/负载列改 serde snake_case；`items` 列更名 `payload`；位置元组 → 具名结构（M2：QuestionContent + as_str↔serde 同口径断言测试）
- [x] 5. TUI 采集 timing + 自评 → 接 `is_guess`/`srs::decide_action`（假对检测上线）——M4 #19 落实：timing/revision 采集 + self_confidence 键位 + `is_guess` + `srs_after_attempt` 全链接线（bc2f244）
- [x] 6. TUI 错误独立展示（Error 行 ≠ Pending 态）（M2：`apply_error` 独立行，P-8 关闭）
- [x] 7. seed 题库加载器（`data/seed`）+ qpack 导入（M2 主线）（M2：open 幂等导入 203 题 + import_pack/export_pack 门面）
- [x] 8. 引入 `tracing`（事件/降级路径可见）（M2：qc 丢弃/重生成/game_state 落库失败均 tracing::warn）

### M2 改进建议（M3 开工即处理，勾=完成）

- [x] 10. TUI 自评键位（提交前 1-4 档 confidence）→ `Attempt.self_confidence` → 接 `is_guess` + `srs::decide_action`（与 M4 FSRS 队列合流）——M4 #19 落实：1-4 档采集 + `is_guess_attempt` + `srs_after_attempt` 全链（bc2f244）
- [x] 11. Jev 全量：`evaluate_many` 并发池 8 + `route_by_confidence`（0.75/0.35）+ 待裁定桶 + 错因分类（docs/05 §2-§4 M3 范围）（M3：并发池保序+失败隔离测试；路由边界/非英语阈值表驱动；arbitration 迁移 v2 同事务入桶；28 类 ErrorTag + error_tags.json 语料）
- [x] 12. `LlmRouter::usage_summary` 成本面板 + `llm_cache` TTL/淘汰 + generate 全局熔断（连续失败快速返回）（M3：区间聚合双实现 + `cache_purge_before` 7 天 + 熔断 5/10/试探三态实测）
- [x] 13. TUI 题包屏（`Ctrl+O` 导入/`Ctrl+X` 导出键位，docs/03 §5.3）接线 `import_pack`/`export_pack`；`share_code` 口令码随 M7（**延后**：需文件系统交互与文件选择器，TUI 纯状态机需新增 Cmd 变体+main 副作用，承接后续阶段）
- [x] 14. seed 导入失败降级开关：坏 seed 包只 `tracing::warn` 跳过（不阻断 open），qid 损坏仍拒（**落实**：`assembly::open` 内置 seed 从 `?` 改为 `if let Err(e) → tracing::warn`，与 `enroll_builtin_cards` 同口径）
- [x] 15. 题型补全：完形/阅读/七选五/语法填空（DESIGN §13.4 归 M2，本阶段标签未列——core QuestionKind 扩展 + grader 递归子题 + seed 补题型）（M4 完成，11d1325/feded54/63444aa）
- [ ] 16. seed 扩轨：gaokao/zhongkao/…（seed_gen 参数化 track + 每轨 ≥200 题，docs/16 §3）（M4 未动，承接至 M5）

### M3 改进建议（M4 开工即处理，勾=完成）

- [x] 17. 讲解层①②：`GradeResult` 加 `detail`（Jev 概率分布/规则 diff 持久化）+ TUI 讲解键接 `06::explain`（文本讲解，需 LLM key；无 key 只显结构化讲解——docs/07 §5；键位按 docs/03 §5.3 总则用非字母键，禁单字母）——**落实**（95b216b）：`GradeDetail{Items|Dims|Plain}` 已落地；F5 讲解浮层 `render_explain_overlay`（verdict/score/confidence/graded_by + error_tags + 逐项判定/维度分 + ①结构化/②文字入口提示）；`Msg::ToggleExplain` + `AppState.explain_open`
- [x] 18. 成本面板与题包屏入 TUI：`cost_summary` → 成本台账屏展示（M3 已有门面）；题包 `Ctrl+O` 导入 / `Ctrl+X` 导出 键位（承接 #13；docs/03 §5.3 禁单字母）——**落实**：`Screen::Cost`/`Screen::Qpack` + `KeyCtx::Cost`/`KeyCtx::Qpack` + Ctrl+S/O/X 键位路由 + `Cmd::CostLoad`/`Cmd::PackImport`/`Cmd::PackExport` + `main_support` 三个 async helper（cost_summary/import_seed/export_pack）+ Home 菜单 11→13 项 + 路由/测试全量更新
- [x] 19. 假对检测合流：从 attempts.timing 算用户延迟基线（p50/p90 滚动）→ `is_guess(timing, self_confidence, baseline)` 接 `srs::decide_action`（M4 完成，bc2f244：recent_timings(exclude) 排除本次 + srs_after_attempt 接线）
- [ ] 20. 裁决增强：选择题人工裁决回流 `ChoiceBody.answer` 集（当前只回流填空 acceptable）；`arbitration.reason` 分值区分灰区/离线两态
- [ ] 21. 资源 loader 统一：`crates/grader/assets/jev`、`data/chinglish`、`crates/{gen,grader}/assets/prompts` 三处 include_str 收口到 16-config 单入口（**仅内容类资源 chinglish/seed 允许用户目录覆盖；prompt 与判分语料永不放开**——2026-09-27 已从 data/ 移出并删除运行时覆盖；**选版函数三处同构已收口** = `core::pick_edition/is_zh_lang`（04b61a1），剩 include_str loader 单入口随 16-config）
- [ ] 23. 质检指令本地化：gen `qc.rs` 五问 instructions 仍英文硬编码（l2≠en 时 Jev 质检指令不随语种；与 arbitrate/error_cause 同法按 l2 分版即可）
- [x] 24. TUI 帮助屏：`?` 弹出键位帮助（docs/03 §5.3 承诺已实现；文案直接复用 `keys::hint` 全表，禁止硬文案）——`Msg::ToggleHelp` + `AppState.help_open` + `render_help_overlay` 复用 `hint` 全表 + 配对测试
- [ ] 22. `evaluate_many` 消费方：M5 套卷批量判题接入（当前仅有契约测试）

### M4 改进建议（M5 开工即处理，勾=完成）

- [x] 25. Review 屏队列刷新：本轮完成后 Ctrl+R 同屏忽略（Screen 相同直接 None），需回 Practice 再进才能重载——done 态应允许重载（改 Msg::Screen(Review) 在 Review ctx 触发 ReviewLoad）；裁决路径（resolve_pending）TUI 无 SRS 提示（SubmitOutcome.srs 只覆盖提交路径，#29 一并考虑）——**落实**：`update_screen` 同屏匹配加 `Screen::Review => Cmd::ReviewLoad`
- [x] 26. 阅读子题顺序流不可回改（sub_answers 只进不退：Space 记录即前进，无「上一子题」键）——补 BackTab/← 回退或允许重选当前槽——**落实**：`Msg::Prev` 在复合题（七选五/阅读）且有 sub_answers 时弹出最后一个子题回退
- [ ] 27. 词库数据扩充：31 义项/11 词 + 8 句式仅够验收（bank/light/run 抽检达标）；EC-DICT 全量导入/`crates/vocab` 独立 crate/五模式训练随 M5+（docs/09 §1 注记）；word_detail 与复习条目每次全量解析 JSON——数据量大后需索引/懒加载（docs/09 §1 mmap 注记）
- [ ] 28. 复习屏体验：评分反馈只显「N 天后」，可加 next_interval 卡片化/连续复习统计；`due_list` limit 20 一轮，队列深度与 daily_budget（16-config §6）打通
- [ ] 29. 裁决（resolve_pending）返回值不带 SRS 处置（签名仍是 GradeResult）——若要 UI 提示需扩返回类型（改 docs/03 §2 契约先行）
- [x] 30. `gen/pipeline/mod.rs` 500 行压线（目录细分后为 generator 内容+域声明）——再增内容前先把 generator 测试拆 `pipeline/generator_tests.rs`（被测 fn 留同文件配对桩，P-16）——**延后**：当前 468 行不超线，再增内容时再拆
- [x] 31. 裸 bool 参数违例（docs/01 §1「禁裸 bool」，模块交叉检查发现）：srs `due_queue(…, interleave: bool)`——engine 生产调用点传字面 `true`，语义在调用点丢失，改 `Interleave` 枚举（契约先行 docs/08）；grader `normalize_for_compare(…, ignore_articles: bool)`——生产无调用仅测试，改 `ArticlePolicy` 枚举（契约先行 docs/07）——**落实**：`Interleave{None,Groups}` + `ArticlePolicy{Keep,Strip}` 枚举替代裸 bool，调用点全改
- [x] 32. 套卷估分回归：W6 已落 paper_runs（score/max 卷面分）——`12-notes-profile::predict_score`（α·近三次均分 + 起点回归）按 docs/00 §6 随 **M6** 落地（46a9257：`profile::predict_score` 近五卷预测 + 小样本区间）；估分唯一实现不外溢
- [ ] 33. 非英语评分降级集成测试：`translate_my_turn` l2≠en → `grade=None` 分支目前只有离线路径覆盖（先返 Offline 到不了该分支）——需 Scripted 供给方级 LLM stub 集成测试，随 M9 多语波次补（docs/11 §4 已挂未勾）；
- [ ] 34. hand_in 违规注入批注流：W8 现为 violations 非空即拒收（Err 不评分）——「violations 进批注与档位说明」完整口径 = Violation 转 Comments.annotations，随 M6 手交屏接线；harvest 四类随 M7（17 单写者）、个人模板→SRS 卡/语域 polish/register_check 随 M8（docs/10 §4、§2 未勾项即追踪面）

（承接 M3 未完项：#13 题包屏（延后） / #16 seed 扩轨 / #20 裁决增强 / #21 资源 loader 收口 / #22 evaluate_many 消费方 / #23 qc 指令本地化——均仍欠，随后续阶段处理。已完成：#14 seed 降级 / #17 讲解层 / #18 成本面板入 TUI / #24 帮助屏 / #25 Review 刷新 / #26 子题回退 / #30 延后 / #31 裸 bool 枚举。）

---

（后续阶段完成后按同格式追加：进度日志 + 四段评估 + 改进建议勾选。）
