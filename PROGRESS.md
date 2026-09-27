# Ambition 施工进度与代码评估

> **每次完成一个部分必须更新本文件**（进度日志 + 代码评估四段 + 改进建议），并写入 `PITFALLS.md`（skill `ambition-pitfall-log` 强制）。
> 定时任务 `scripts/continue_mvp.sh` 按下方 `MVP_STAGE` 自动推进，直到 MVP 完成。

MVP_STAGE: 4/6

## MVP 阶段定义（M1–M6，对应 DESIGN.md §21；M7+ 属于云/Web，不在本次 MVP 内）

| # | 阶段 | 内容 | 状态 |
|---|---|---|---|
| 1 | M1 骨架 | core/store/engine/tui + 规则判题 + 打字通底座 + 游戏底座 | ✅ 完成（2026-09-24，提交 5271323） |
| 2 | M2 出题 | llm crate（OpenAI/Anthropic 协议）+ 结构化出题 + Jev 质检回路 + qpack 导入导出 + seed 题库 | ✅ 完成（2026-09-24，提交 b39efef） |
| 3 | M3 判题 | jev crate 客户端 + 待裁定桶 + 错因分类 + 中式英语错式库 + 成本台账 | ✅ 完成（2026-09-24，提交 75408b0） |
| 4 | M4 记忆 | FSRS 接线 fsrs-rs + 错词本/义项/句式 + 假对上线 + 复习队列 | ⬜ 待施工 |
| 5 | M5 写作 | composite scoring + 批注 + 模板作文填槽 + 翻译题型 + 套卷估分 | ⬜ 待施工 |
| 6 | M6 闭环 | 笔记/日记/TODO + 画像聚合 + 个性化出题 + 周报 + 游戏全量 | ⬜ 待施工 |

> 自动推进规则：定时任务读 `MVP_STAGE: n/6` → 唤起 pi 执行第 n 阶段 → 成功则 n+1 并**连跑下一阶段**，失败不推进、下轮重试；`6/6` 且状态全 ✅ 时停止。

---

## 进度日志

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
- [ ] 5. TUI 采集 timing + 自评 → 接 `is_guess`/`srs::decide_action`（假对检测上线）——**半步**：M2 已采 timing/revision 并写入 `Attempt.timing`；`self_confidence` 自评键位与 `is_guess` 接线依赖 FSRS 队列，归 M4（见 #10）
- [x] 6. TUI 错误独立展示（Error 行 ≠ Pending 态）（M2：`apply_error` 独立行，P-8 关闭）
- [x] 7. seed 题库加载器（`data/seed`）+ qpack 导入（M2 主线）（M2：open 幂等导入 203 题 + import_pack/export_pack 门面）
- [x] 8. 引入 `tracing`（事件/降级路径可见）（M2：qc 丢弃/重生成/game_state 落库失败均 tracing::warn）

### M2 改进建议（M3 开工即处理，勾=完成）

- [ ] 10. TUI 自评键位（提交前 1-4 档 confidence）→ `Attempt.self_confidence` → 接 `is_guess` + `srs::decide_action`（与 M4 FSRS 队列合流）——**两步半**：M3 已落键位采集与落库（1-4 档、判分锁定、换题重置，TUI 提交带 self_confidence）；`is_guess` 需用户延迟基线（p50/p90）与 FSRS 队列，归 M4（尾已移 #19）
- [x] 11. Jev 全量：`evaluate_many` 并发池 8 + `route_by_confidence`（0.75/0.35）+ 待裁定桶 + 错因分类（docs/05 §2-§4 M3 范围）（M3：并发池保序+失败隔离测试；路由边界/非英语阈值表驱动；arbitration 迁移 v2 同事务入桶；28 类 ErrorTag + error_tags.json 语料）
- [x] 12. `LlmRouter::usage_summary` 成本面板 + `llm_cache` TTL/淘汰 + generate 全局熔断（连续失败快速返回）（M3：区间聚合双实现 + `cache_purge_before` 7 天 + 熔断 5/10/试探三态实测）
- [ ] 13. TUI 题包屏（`i` 导入/`x` 导出键位，docs/03 §5.3）接线 `import_pack`/`export_pack`；`share_code` 口令码随 M7（M3 未动，承接至 M4）
- [ ] 14. seed 导入失败降级开关：坏 seed 包只 `tracing::warn` 跳过（不阻断 open），qid 损坏仍拒（用户 `data/seed/` 目录加载器未落地，承接至 M4）
- [ ] 15. 题型补全：完形/阅读/七选五/语法填空（DESIGN §13.4 归 M2，本阶段标签未列——core QuestionKind 扩展 + grader 递归子题 + seed 补题型）（承接至 M4）
- [ ] 16. seed 扩轨：gaokao/zhongkao/…（seed_gen 参数化 track + 每轨 ≥200 题，docs/16 §3）（承接至 M4）

### M3 改进建议（M4 开工即处理，勾=完成）

- [ ] 17. 讲解层①②：`GradeResult` 加 `detail`（Jev 概率分布/规则 diff 持久化）+ TUI 讲解键接 `06::explain`（文本讲解，需 LLM key；无 key 只显结构化讲解——docs/07 §5；键位按 docs/03 §5.3 总则用非字母键，禁单字母）
- [ ] 18. 成本面板与题包屏入 TUI：`cost_summary` → Profile/Settings 展示（M3 已有门面）；题包 `Ctrl+O` 导入 / `Ctrl+X` 导出 键位（承接 #13；docs/03 §5.3 禁单字母）
- [ ] 19. 假对检测合流：从 attempts.timing 算用户延迟基线（p50/p90 滚动）→ `is_guess(timing, self_confidence, baseline)` 接 `srs::decide_action`（M3 已采自评键位，承接 #10 尾巴）
- [ ] 20. 裁决增强：选择题人工裁决回流 `ChoiceBody.answer` 集（当前只回流填空 acceptable）；`arbitration.reason` 分值区分灰区/离线两态
- [ ] 21. 资源 loader 统一：`crates/grader/assets/jev`、`data/chinglish`、`crates/{gen,grader}/assets/prompts` 三处 include_str 收口到 16-config 单入口（**仅内容类资源 chinglish/seed 允许用户目录覆盖；prompt 与判分语料永不放开**——2026-09-27 已从 data/ 移出并删除运行时覆盖；选版函数三处同构（l2 match+回退 en），一并收口）
- [ ] 23. 质检指令本地化：gen `qc.rs` 五问 instructions 仍英文硬编码（l2≠en 时 Jev 质检指令不随语种；与 arbitrate/error_cause 同法按 l2 分版即可）
- [ ] 24. TUI 帮助屏：`?` 弹出键位帮助（docs/03 §5.3 承诺未实现；文案直接复用 `keys::hint` 全表，禁止硬编码）
- [ ] 22. `evaluate_many` 消费方：M5 套卷批量判题接入（当前仅有契约测试）

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
