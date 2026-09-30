# docs/20 — Home 菜单重构 + 练习分类选择（M7）

> **阶段归属**：DESIGN.md §21 路线图 M7 之前置子阶段（TUI 本地增强，不依赖云/Web）。
> **前置条件**：M1–M6 已完成（619 测试全绿）；本分册只改 TUI + engine + store 三层。
> **文档先行**：本文件先于代码落地（docs/01 §4 红线）；代码改动必须与本分册函数签名一致。

---

## 1. 目标

将 Home 屏从 13 项扁平菜单重构为 **7 大模块 + 多级子菜单** 结构，并新增「练习选择屏」实现按题型/标签筛选练习。

### 1.1 用户故事

1. 启动 → Home 屏显示 7 大模块（练习 / 考试 / 词汇 / 对话 / 游戏 / 设置 / 退出）
2. 选「练习」→ 进入练习选择屏 → 选分类（基础练习 / 专项训练 / 考试模拟 / 自定义练习）→ 选具体题型 → 开始练习
3. 选「退出」→ 退出程序（不弹确认；其余模块 Enter 进入对应屏）
4. Esc 在非 Home 屏 → 返回 Home；在 Home 屏 → 退出

### 1.2 不做的事（本阶段边界）

- 不新增 engine 业务逻辑（不出题、不改判分）
- 不新增 store 表（不持久化设置）
- 不实现「对话」「设置」模块的完整功能（仅占位菜单项）
- 不改 `new_state` 默认 screen（P-24 红线）

---

## 2. 架构设计

### 2.1 Home 菜单结构（7 大模块）

```
Home
├── 练习        → PracticeSelect 屏（分类选择 → 题型 → 开始）
├── 考试        → Mock 屏（已有，直接进入）
├── 词汇        → Wordbook 屏（已有）
├── 对话        → 占位（Screen::Dialog，显示"即将推出"）
├── 游戏        → Games 屏（已有）
├── 设置        → 占位（Screen::Settings，显示"即将推出"）
└── 退出        → Cmd::Quit
```

### 2.2 练习选择屏（PracticeSelect）结构

```
PracticeSelect
├── 基础练习
│   ├── 单词      → filter tags=["vocab"] 开始练习
│   ├── 单选题    → filter kind=SingleChoice 开始练习
│   ├── 多选题    → filter kind=MultiChoice 开始练习
│   ├── 完形填空  → filter kind=Cloze 开始练习
│   ├── 阅读理解  → filter kind=Reading 开始练习
│   ├── 七选五    → filter kind=SevenOfFive 开始练习
│   ├── 语法填空  → filter kind=GrammarFill 开始练习
│   ├── 翻译      → filter kind=Translation 开始练习
│   └── 写作      → filter kind=Essay 开始练习
├── 专项训练
│   └── （占位——后续按 tags 筛选，本阶段不可选）
├── 考试模拟
│   └── → Mock 屏
└── 自定义练习
    └── 全部题库  → start_practice(0) 开始练习（等同原"开始练习"）
```

### 2.3 屏幕层级与 Esc 行为

```
Home ──Enter──▶ PracticeSelect ──Enter──▶ Practice（答题）
  │                  │                      │
  │                  │Esc                   │Esc
  │                  ▼                      ▼
  │              Home                  Home
  │
  ├──Enter──▶ Mock / Wordbook / Games / ... ──Esc──▶ Home
  └──Enter──▶ 退出
```

---

## 3. 数据模型变更

### 3.1 新增 Screen 变体

```rust
// state/mod.rs :: enum Screen
+ PracticeSelect,  // 练习选择屏（分类 → 题型 → 开始练习）
+ Dialog,          // 对话模块（占位）
+ Settings,        // 设置模块（占位）
```

### 3.2 新增 KeyCtx 变体

```rust
// keys/mod.rs :: enum KeyCtx
+ PracticeSelect,  // 练习选择屏：↑↓ 导航 · Enter 选择 · Esc 返回 Home
```

### 3.3 新增 AppState 字段

```rust
// state/mod.rs :: struct AppState
+ practice_filter: Option<PracticeFilter>,  // 练习筛选条件（None = 全部）
```

### 3.4 新增 PracticeFilter 枚举

```rust
// state/mod.rs（或 state/practice_filter.rs 子模块）
+ pub enum PracticeFilter {
+     All,                    // 全部题库
+     Kind(QuestionKindKey),  // 按题型筛选
+     Tags(Vec<String>),      // 按 tags 筛选（预留）
+ }
+
+ pub enum QuestionKindKey {
+     SingleChoice, MultiChoice, Cloze, Reading,
+     SevenOfFive, GrammarFill, Translation, Essay,
+ }
```

### 3.5 新增 Msg 变体

```rust
// state/msg.rs :: enum Msg
+ PracticeSelect,     // Home → PracticeSelect 屏
+ PracticeStart,      // PracticeSelect → 开始练习（用 state.practice_filter）
```

### 3.6 新增 Cmd 变体

```rust
// state/msg.rs :: enum Cmd
+ PracticeLoadFiltered(PracticeFilter),  // main 按筛选条件加载题目
```

---

## 4. 函数级拆分（按文件组织）

### 4.1 `state/mod.rs`

| 函数 | 改动类型 | 签名 |
|---|---|---|
| `enum Screen` | 修改（新增 3 变体） | `+ PracticeSelect, Dialog, Settings` |
| `struct AppState` | 修改（新增字段） | `+ practice_filter: Option<PracticeFilter>` |
| `new_state()` | 修改（初始化新字段） | `+ practice_filter: None` |
| `enum PracticeFilter` | 新增 | 见 §3.4 |
| `enum QuestionKindKey` | 新增 | 见 §3.4 |
| `QuestionKindKey::to_kind_matcher()` | 新增 | `fn to_kind_matcher(&self) -> fn(&QuestionKind) -> bool` |

### 4.2 `state/key_ctx.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `AppState::key_ctx()` | 修改 | 新增 `Screen::PracticeSelect → KeyCtx::PracticeSelect`、`Screen::Dialog → KeyCtx::Dialog`、`Screen::Settings → KeyCtx::Settings` |

### 4.3 `state/msg.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `enum Msg` | 修改 | `+ PracticeSelect, PracticeStart` |
| `enum Cmd` | 修改 | `+ PracticeLoadFiltered(PracticeFilter)` |

### 4.4 `keys/mod.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `enum KeyCtx` | 修改 | `+ PracticeSelect, Dialog, Settings` |
| `key_to_msg()` | 修改 | Home Enter 路由按 cursor 分化（练习→PracticeSelect，退出→Quit）；PracticeSelect Enter→PracticeStart；PracticeSelect ↑↓→MoveUp/Down |

### 4.5 `keys/hints.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `hint()` | 修改 | 新增 `KeyCtx::PracticeSelect / Dialog / Settings` 提示文案 |

### 4.6 `render/home_render.rs`

| 函数 | 改动类型 | 签名 |
|---|---|---|
| `home_entries()` | 修改 | 返回 7 项（练习/考试/词汇/对话/游戏/设置/退出） |
| `render_home()` | 修改（内部） | cursor 路由表适配 7 项 |
| `render_practice_select()` | 新增 | `pub fn render_practice_select(frame: &mut Frame, state: &mut AppState)` |

### 4.7 `render/mod.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `render()` | 修改 | 新增 `Screen::PracticeSelect → render_practice_select`、`Screen::Dialog/Settings → 占位渲染` |

### 4.8 `update/home_update.rs`

| 函数 | 改动类型 | 签名 |
|---|---|---|
| `home_menu_len()` | 修改 | 返回 7（原 13） |
| `home_enter()` | 修改 | cursor 0→PracticeSelect（非 PracticeLoad）；1→Mock；2→Wordbook；3→Dialog；4→Games；5→Settings；6→Quit |
| `practice_select_entries()` | 新增 | `pub fn practice_select_entries() -> &'static [(&'static str, &'static str, Option<PracticeFilter>)]` |
| `practice_select_len()` | 新增 | `pub fn practice_select_len() -> usize` |
| `practice_select_enter()` | 新增 | `pub fn practice_select_enter(state: &mut AppState) -> Cmd` |

### 4.9 `update/mod.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `update()` | 修改 | Home 分支新增 `Msg::PracticeSelect → Screen::PracticeSelect`；新增 PracticeSelect 分支（↑↓/Enter/Esc） |
| `pub use home_update::{...}` | 修改 | 导出新函数 |

### 4.10 `update/m6.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `update_m6()` | 修改 | 新增 `Screen::Dialog / Settings` 的 Quit/None 分支 |
| `update_screen()` | 修改 | 新增 `Screen::PracticeSelect / Dialog / Settings` 路由 |

### 4.11 `update/practice.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| `practice_msg()` | 修改 | unhandled 列表补 `PracticeSelect / PracticeStart` |

### 4.12 `main.rs`

| 函数 | 改动类型 | 说明 |
|---|---|---|
| 事件循环 match | 修改 | `Cmd::PracticeLoad` → `Cmd::PracticeLoadFiltered(filter)`；main 调 `engine.start_practice_filtered(filter)` |
| `Cmd::PracticeLoadFiltered(filter)` | 新增分支 | 按 filter 筛选题目后设 state |

### 4.13 `engine/src/practice/mod.rs`

| 函数 | 改动类型 | 签名 |
|---|---|---|
| `start_practice()` | 保留（向后兼容） | 不改 |
| `start_practice_filtered()` | 新增 | `pub async fn start_practice_filtered(&self, filter: &PracticeFilter) -> Result<Vec<Question>, EngineError>` |

### 4.14 `store/src/repo/question_repo.rs`

| 函数 | 改动类型 | 签名 |
|---|---|---|
| `list_questions()` | 保留 | 不改 |
| `list_questions_by_kind()` | 新增 | `pub async fn list_questions_by_kind(pool: &SqlitePool, matcher: fn(&QuestionKind) -> bool) -> Result<Vec<Question>, sqlx::Error>` |

### 4.15 测试文件

| 文件 | 改动类型 | 说明 |
|---|---|---|
| `update/home_update.rs :: tests` | 修改 | `home_menu_len` 7；`home_enter` 路由表适配 |
| `update/update_mod_tests.rs` | 修改 | Home 导航测试适配 7 项 |
| `keys/keys_tests.rs` | 修改 | Home Enter 路由测试适配 |
| `render/render_tests.rs` | 修改 | Home 渲染测试高度适配 |
| `render/home_render.rs :: tests` | 修改 | `home_entries` 数量 7 |
| `state/key_ctx.rs :: tests` | 修改 | 新增 PracticeSelect/Dialog/Settings 路由 |
| `store/src/repo/question_repo.rs :: tests` | 修改 | 新增 `list_questions_by_kind` 测试 |

---

## 5. M7 阶段 → W 周计划

### W1：Home 菜单重构（7 模块）+ Screen/KeyCtx/Msg 新增

**改动范围**：`state/mod.rs`、`state/msg.rs`、`state/key_ctx.rs`、`keys/mod.rs`、`keys/hints.rs`、`update/home_update.rs`、`update/mod.rs`、`update/m6.rs`、`update/practice.rs`、`render/home_render.rs`、`render/mod.rs`

**交付物**：
- Home 屏显示 7 大模块（练习/考试/词汇/对话/游戏/设置/退出）
- cursor 0→PracticeSelect、6→Quit，其余映射到已有屏
- Dialog/Settings 占位屏（"即将推出" + Esc 返回）
- Esc 在 Home → Quit；非 Home → Home
- 所有 match 穷尽（P-28 红线）
- 全仓测试绿

**函数清单**：
1. `Screen` 新增 `PracticeSelect, Dialog, Settings`
2. `KeyCtx` 新增 `PracticeSelect, Dialog, Settings`
3. `Msg` 新增 `PracticeSelect`
4. `home_entries()` 改返回 7 项
5. `home_menu_len()` 改返回 7
6. `home_enter()` cursor 路由表改 7 项
7. `key_ctx()` 新增 3 屏路由
8. `key_to_msg()` Home Enter 分化 + Esc 行为
9. `hint()` 新增 3 个 ctx 提示
10. `update()` Home 分支 + PracticeSelect/Dialog/Settings 分支
11. `update_m6()` 新增 Dialog/Settings 处理
12. `update_screen()` 新增 3 屏路由
13. `practice_msg()` unhandled 补全
14. `render_home()` cursor 路由适配
15. `render()` 新增 3 屏分发
16. 测试文件全量适配

### W2：练习选择屏（PracticeSelect）+ 渲染 + 导航

**改动范围**：`state/mod.rs`（PracticeFilter）、`render/home_render.rs`（`render_practice_select`）、`update/home_update.rs`（练习选择条目 + enter）、`update/mod.rs`（PracticeSelect 分支）、`keys/mod.rs`

**交付物**：
- PracticeSelect 屏：列表展示「基础练习」分类下的题型选项 + 「自定义练习 → 全部题库」
- ↑↓ 导航 · Enter 选择 → 设 `state.practice_filter` 并触发 `Cmd::PracticeLoadFiltered`
- Esc 返回 Home
- 专项训练/考试模拟为占位项（灰显或不可选）

**函数清单**：
1. `PracticeFilter` 枚举 + `QuestionKindKey` 枚举
2. `practice_select_entries()` 返回条目列表
3. `practice_select_len()` 返回条目数
4. `practice_select_enter()` 设 filter + 返回 Cmd
5. `render_practice_select()` 渲染列表 + cursor
6. `key_to_msg()` PracticeSelect 上下文路由
7. `AppState.practice_filter` 字段 + `new_state` 初始化
8. `Msg::PracticeStart` 变体
9. `update()` PracticeSelect 分支处理

### W3：engine + store 层筛选取题

**改动范围**：`store/src/repo/question_repo.rs`、`engine/src/practice/mod.rs`、`tui/src/main.rs`

**交付物**：
- `list_questions_by_kind()` store 层按题型筛选
- `start_practice_filtered()` engine 层门面
- `main.rs` 事件循环 `Cmd::PracticeLoadFiltered` 分支

**函数清单**：
1. `list_questions_by_kind()` store 仓储
2. `start_practice_filtered()` engine 门面
3. `main.rs` match 新增 `PracticeLoadFiltered` 分支
4. store/engine 测试

### W4：测试全量适配 + 验收

**改动范围**：所有测试文件

**交付物**：
- 全仓 fmt/clippy/test 三连绿
- pre-commit 七步钩子全过
- PITFALLS.md + PROGRESS.md 更新

---

## 6. 约束与红线

| 约束 | 来源 | 说明 |
|---|---|---|
| `new_state` 默认 screen 不改 | P-24 | 保持 `Screen::Practice`，main.rs 手动设 Home |
| 新增枚举变体全仓 match 穷尽 | P-28 | Screen/KeyCtx/Msg/Cmd 新增后 `cargo check` 定位 |
| Ctrl+字母是直达表保留字 | P-20 | 本阶段不新增 Ctrl 快捷键 |
| 文件 ≤500 行 | 门禁 | 新增代码可能超限 → 拆子模块 |
| 配对测试同文件 | 门禁 | 新增 `fn` 必须同文件 `test_<fn>` |
| 键位文案与路由同源 | docs/03 §5.3 | hint() 取 keys.rs 同源，禁硬编码 |
| 不新增 _ 占位参数 | P-13 | 所有函数参数必须使用 |

---

## 7. 验收标准

1. **Home 屏**：7 大模块，↑↓ 导航，Enter 进入对应屏，Esc 退出
2. **PracticeSelect 屏**：基础练习 9 题型 + 自定义练习，↑↓ 导航，Enter 按筛选开始练习，Esc 返回
3. **练习筛选**：选「单选题」→ 只出 SingleChoice 题；选「全部题库」→ 出全部
4. **Dialog/Settings**：占位屏显示"即将推出"，Esc 返回
5. **三连全绿**：fmt 0 diff / clippy 0 warning / test 0 failed
6. **七步钩子**：pre-commit 7/7 通过
