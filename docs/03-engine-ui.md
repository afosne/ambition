# 03 · engine-ui — engine 门面 + TUI / Web 双端（M1 / M7）

> 实现顺序：**M1（TUI）/ M7（Web）**。并入来源：旧 `19-engine` + `16-tui-web`。
> 两壳只调 engine；本册函数表 = 功能全集（需求→函数对照在 §4）。

## 1. 生命周期与事件旁路

```rust
pub async fn open(cfg: Config) -> Result<Engine>;   // 开库/迁移/静态数据幂等导入/装配服务
// M3 落地：Engine::open(path) 内部取 AiConfig::from_env()（AMBI_JEV_BASE/AMBI_JEV_KEY/TYPESAFE_API_KEY/
//   AMBI_LLM_BASE/AMBI_LLM_MODEL/AMBI_LLM_KEY，key 缺省 = 离线形态）；测试/定制走 open_with_ai(path, AiConfig)
//   / open_memory_with_ai(AiConfig)。StoreLedger（AiLedger 的 SQLite 实现）在 engine 装配（05 不依赖 store）。
// EventBus → 订阅：GameService（可关）/ SyncProjector（server_url 非空）/ ProfileAggregator
// 落库规则：主事务提交后旁路逐个执行，各自幂等（旁路失败不回滚，tracing 记录可补偿）
// M2：core::EventBus trait 落地，engine 只 publish 不直调 gamify（解耦改进 #3）；gamify 效果经 save_game_state 落库（#1）
// M4：submit 后 SRS 旁路（insert 前取基线 → is_guess → decide_action → 建卡/提权/调度；旁路失败 warn 不回滚）
//   + assemble 幂等入卡内置义项/句式（data/lexicon、data/patterns，坏文件仅 warn 不阻断 open——已落地 enroll_builtin_cards）+ ReviewDone 经 settle 落 game_state（已落地）
```

## 2. use-case 门面（函数全集，按域）

```rust
// 答题/判分（S1）：start_practice / submit_answer / explain_mistake / resolve_pending / pending_bucket
//   M3 落地：submit_answer = 三级流水线（规则→Jev→桶）；pending_bucket() 待裁定列表；
//   resolve_pending(attempt_id, accept) 人工裁决（grades→human + 填空可接受答案回流 + gamify 事件）；
//   explain_mistake（文字讲解）随 06::explain 讲解层（M4+）；
//   cost_summary(from_unix, to_unix) -> UsageSummary（成本台账门面，数据源 llm_usage）；
//   add_chinglish_drills(limit)（中式英语改错训练题注入，data/chinglish/，DESIGN §13.6 归 M3）
//   M4 落地：SubmitOutcome{grade, effects, srs: Option<CardAction>}（入卡处置回传 UI 提示）；
//   due_list(now_unix, limit) -> Vec<ReviewItem>（到期+交错）；review_answer(card_id, rating) -> Card（FSRS 调度 + ReviewDone）；
//   wrong_wordbook() -> Vec<WrongWordRow>（错词本：错因标签+次数+题干，已落地）；word_detail(lemma) -> Vec<Sense>（09 义项查询，已落地：engine::lexicon）；
//   （weekly_quiz / vocab_probe_* / spelling_material 随 M5+，docs/08 §1 注记）
// 复习/词汇（S2）：due_list / review_answer / enroll_words / wrong_wordbook / word_detail / vocab_probe_* / weekly_quiz
// 出题/题包（S4/S6）：generate / variant_drill / export_pack / import_pack / share_code / pack_updates
// 写作（S3）：scaffold / templates / save_template / check_scaffold / render_template / submit_essay / model_essays / upgrade_sentence / translate_grade / writing_drills / drill_submit / long_sentence / polish
// 笔记/日记/TODO：note_* / diary_write / diary_review / plan_for_exam / todo_complete
// 打字/游戏/听说：typing_start / typing_finish / game_round / game_settle / speak / shadowing
// 词汇深水/句式：pattern_* / sense_drill / rare_sense_queue / word_mastery
// 翻译工作台：translate / translate_align / translate_my_turn / translate_harvest / translate_history
// 素材中枢：material_ingest_text / material_ingest_image / material_proofread / material_derive / material_usage / material_drop
// 画像/模考：profile / weekly_report / export_report / mock_start / mock_finish
// 云（server_url 空则 EngineError::CloudDisabled）：login_pair_start / login_pair_finish / sync_now / cloud_generate
```
（完整签名见 git 历史旧 19-engine §2-§9；语义不变。）

## 3. 双端 UI（tui / httpd+web）

```rust
// TUI（ratatui+crossterm）：App{state, screen, engine}；Msg/ update(纯) / draw；Screen=Home|Practice|Review|Wordbook|Typing|Writing|Notes|Diary|Todo|Games|Pack|Profile|Settings|Mock
// 要点：unicode-width CJK 折行；答题组件（题干/选项/填空光标/判分视图）；打字组件复用 04-typing；游戏全 TUI 可玩
// M2：判题失败 → 独立错误行（≠ Pending 态，P-8）；填空题输入行（采集→ Answer::Fill）；提交计时 → timing（假对检测输入，#5 半步）
// M4：Tab 切 Practice↔Review 复习屏（Space/Enter 翻面 · 1-4 = Again/Hard/Good/Easy · Esc/Tab 返回；无 Home 屏故 `3` 直达键随 M6）——已落地：Screen::Review + KeyCtx::Review + Cmd::ReviewLoad/ReviewRate（due_list limit 20/轮，评分失败独立错误行）；
//   判分后 SRS 提示行（错词本入册/假对进卡/FSRS 已安排，apply_srs_hint）——已落地
// Web（React+TS，OpenAPI 生成客户端）：页面↔Screen 一一对应；TanStack Query
```

| use-case | TUI | Web | 备注 |
|---|---|---|---|
| 答题/复习/单词/打字/写作/笔记/日记/TODO/游戏/题包/画像/模考 | Practice…Mock | /practice…/mock | 同权 |
| 图片/听力/富文本批注/图表报表/扫码/OCR 校对 | 文本降级（alt_zh 占位） | 原生 | 唯一不等价点 |
| 管理面板/看板/班级 | —（TUI 纯客户端） | /admin /me /classes | 服务端功能（14-cloud） |

- 离线降级矩阵（07-grading §6）两端共享；TUI 只做客户端（配对码登录），无任何服务端职责；
- 首启向导（16-config-i18n §5）：TUI 首启屏 / Web `/setup`；Settings 可随时重设目标/难度（`reconfigure`）；
- Settings 屏：**参数总表见 16-config-i18n §6**（TUI Settings 分区 = Web `/settings` 同构；🔒 机密项只显掩码；⚙️ 判分口径类变更弹历史可比警示）。

## 4. 需求 → 函数对照（总纲 11 目标 + 扩展）

| 需求 | 函数 |
|---|---|
| 1 出题 / 2-4 判题 | generate/cloud_generate；submit_answer/review_answer/submit_essay/translate_grade |
| 5 共享导入 | export_pack/import_pack/share_code |
| 6 错题/笔记/日记/TODO | due_list/note_*/diary_*/todo_* |
| 7 错因解释 / 8 背单词 | explain_mistake；enroll_words/due_list/wrong_wordbook/word_detail |
| 9 范文加分 / 10 专属练习 / 11 写作加强 | model_essays/upgrade_sentence；variant_drill/generate(weak)；writing_drills/drill_submit |
| 打字/模板作文/游戏/翻译/素材 | typing_*/scaffold/game_*/translate*/material_* |

## 5. TUI 单机模式使用指南（默认形态，零依赖）

> 单机模式 = 不填 `server_url` 的**默认形态**：所有学习功能可用，数据全在 `~/.ambition/ambition.db`。**内容三级保障**：①内置 seed 离线内容包（16-config-i18n `data/seed`）②qpack 导入/共享 ③AI 生成（可选）——无 AI 有 ①② 核心功能完整。无 AI key = 纯规则模式（即 M1 形态）；有 key（BYOK）则解锁 AI 生成/讲解；联网（填 server_url）随时可选升级。

### 5.1 启动与首启

```
ambition                 # 进入 TUI：无需账号、无需服务、无需联网
```
- 首启向导五步（16-config-i18n §5）：目标轨 → 目标分/考试日期 → 摸底定难度 → 每日预算 →（可跳过）填 key。**全部可跳过**，跳过即纯本地规则模式；
- 单机最小配置：config.toml 只需 `[sync] server_url = ""`（其余全默认）。

### 5.2 无 key / 离线可用面（M1 全量）

| 零 AI / 零网可用 | 需 AI key（BYOK 或服务端代管） |
|---|---|
| 本地题库答题（**内置 seed + 导入包 + 自动沉淀**）、规则判题、待裁定人工裁决 | LLM 出题/讲解/范文/翻译/造句 |
| 判题**解析自动入题库**、qpack 导出/导入（含已生成讲解） | 讲解的**首次生成**需 key（生成后即本地资产） |
| FSRS 复习/错题本/错词本/词汇量测试 | Jev 模糊判分/错因/作文 composite 分 |
| 打字通「键胜」全量、游戏全量 | AI 私教、口语陪练、私人电台 |
| 笔记/日记/TODO（日记不批改仅本地保存） | 日记批改、批注、口播 |
| qpack 文件/口令码导入导出（**离线共享**） | S3/广场分享、同步、看板、家长推送 |
| **词库/词表（data/ 本地资源，零配置可用）**、素材 OCR（本地 paddleocr） | 视觉 LLM 兜底 OCR |

### 5.3 屏幕与默认键位（可配，`?` 打开帮助屏）

> **键位总则（2026-09-27 用户裁定）**：**禁止单字母快捷键**——字母键只做填空输入文本；命令一律走「非字母键（方向 / Enter / Space / Esc / Tab / 数字）+ `Ctrl` 组合」。屏幕直达统一 `Ctrl+字母`（避开保留：`Ctrl+C`/`Ctrl+Q`=退出；终端不可达：`Ctrl+I`=Tab、`Ctrl+H`=Backspace）。

| 屏 | 键 | 屏 | 键 |
|---|---|---|---|
| Home 今日 | `Ctrl+B` | Games 游戏 | `Ctrl+G` |
| Practice 答题 | `Ctrl+A` | Notes/Diary/Todo | `Ctrl+N`/`Ctrl+D`/`Ctrl+K` |
| Review 复习 | `Ctrl+R`（M4：`Tab` 从答题屏切入） | Pack 题包 | `Ctrl+O` |
| Wordbook 单词 | `Ctrl+V` | Profile 画像 | `Ctrl+P` |
| Typing 打字 | `Ctrl+T` | Mock 模考 | `Ctrl+M` |
| Writing 写作 | `Ctrl+E` | Settings | `Ctrl+S` |

- 全局：**`Ctrl+Q`/`Ctrl+C` 恒退出（任何状态，含填空编辑中；单字母 `q` 退出已废除）** · `Ctrl+字母` 屏幕直达（**M4 落地 `Msg::Screen`：`Ctrl+A`=Practice、`Ctrl+R`=Review**，其余屏随后续阶段） · `?` 帮助 · `Tab`/`Shift+Tab` 切面板或字段 · `Esc` 返回/退出/跳过；
- **键位唯一入口（2026-09-27 统一重构）**：`crates/tui/src/keys/mod.rs::key_to_msg(KeyCtx, key, mods)` —— `KeyCtx`（Choice / Fill{editing} / Pending / Graded / Finished / Review / Picker）由 `AppState::key_ctx()` 计算，键位表只此一份；提示行 `hint(ctx)` 与路由同源（render 不得硬编码键位文案）；main 不做任何键位改写（路径 2026-09-27 随 src/ 目录细分改为 `keys/mod.rs`）；
- 答题（Choice ctx，**2026-09-27 选择体验修订**）：`↑/↓` 移动 · **`Space` 勾选/取消（单选=单元素勾选替换；多选=连按多选；七选五/阅读=确认当前缺口/子题并前进）——勾选状态选项前 `✓` 可见** · `Enter` 提交（**单选提交勾选项、未勾回退光标位；多选未勾任何项 → 错误行「请先 Space 选中」不提交**）· **`→` = 跳过/下一题（不判分）· `←` = 上一题（状态复位，已作答不保留）** · **`F1` = 自定义选题面板**（全屏列表：↑↓ 选 · Enter 跳转 · Esc/F1 返回）· `1-4` 自评把握（数字，→ Attempt.self_confidence；仅作答态）；判分浮层显示错因标签 + graded_by + **「你的选择 / 正确答案」对照（选择题，结构化讲解①）**；
- 填空（Fill ctx，**2026-09-27 键位修订**）：**打字即编辑（无需进入键，任何可打印字符含空格/数字直接进当前空位）** · **多空 = 每空一位独立输入行**（`Tab`/`→`/`↓` 下一空，`Shift+Tab`/`←`/`↑` 上一空，循环；**废除 `;` 分隔**） · `Enter` 提交 · **`Esc` = 退出编辑但保留已输入**（再打字直接恢复编辑；未编辑时 `Esc` 才退出程序）——修订前 `Esc` 直接退程序丢答案是坑；
- 待裁定（Pending ctx，**2026-09-27 裁决键修订**）：**`←/→`（或 `↑/↓`）高亮选择「判对/判错」· `Enter` 确认裁决 · `Esc` 跳过（保持待裁定桶）**；裁决回流规则层（同类题下次直判，resolve 后浮层变 graded_by=human）；单字母 `y/n` 已废除；
- 复习翻卡：`Space`/`Enter` 翻面 · `1/2/3/4` = Again/Hard/Good/Easy（FSRS，M4 落地：`Tab` 进入/退出复习屏，空队列显「暂无到期卡片」）；
- 选题面板（`F1`，2026-09-27）：全屏列出题库（序号 + 题型标签 + 题干摘要），`↑/↓` 选择 · `Enter` 跳转到该题（状态复位）· `Esc`/`F1` 返回；任何有题状态（Choice/Graded/Finished）可呼出；
- 模板作文屏（`F2` 呼出，M5-W4b，docs/10 §1）：屏顶模板列表（`↑/↓` 选 · track/体裁可见）→ 中部三阶段脚手架视图 · **`Space` 阶段循环（全模板→半模板→脱模板）** · `Tab`/`Shift+Tab`、`←/→` 切换槽位 · 字符直接填当前槽 · **`Enter` 套槽成文**（缺槽保留 `{{id}}` 不静默）· **`Ctrl+S` 草稿自查**（Jev noul×2 双 prob≥0.5 过；无 key → 离线配置提示）· 屏底成文预览与自查结果（原意/贴合分项）· `Esc` 返回答题；任何有题状态可呼出；
- 打字：直接输入，`Esc` 结束结算 WPM；
- 题包：`Ctrl+O` 导入（文件/口令码）· `Ctrl+X` 导出 · `Ctrl+F` 关注更新。

### 5.4 单机的一天（walkthrough）

1. `ambition` → Home 今日清单（新词 + 复习 + 专项 + 日记，每日流见 18-learning-loop）；
2. Review 复习（1-4 评分；错题/假对自动入卡）→ Typing 打到期词拼写（复习即打字）；
3. Practice 用本地题库/导入题包练（无 key 规则判题；Pending 由你 `y/n` 裁决）；
4. Diary 写几句（无 key 仅本地保存）；Games 玩两把（玩的就是复习）；
5. Profile 看热力/周报；Pack 用口令码和同学交换题包（离线可用）；
6. 任意时候 `s` 改目标/预算——**数据不丢**（16-config-i18n §5.1）。

### 5.5 升级到联网（可选、随时）

Settings 填 `server_url` → 配对码登录 → 解锁同步/广场/代管 key/看板；本地数据原样保留（00-overview S5）。

### 5.6 数据与备份

`~/.ambition/ambition.db` + `~/.ambition/notes/*.md`（笔记是纯 Markdown，可直接同步网盘）；Settings→数据：全量 JSON 导出 / DB 备份 / 清库（唯一删除口）。

## 6. 错误与验证

`EngineError{Store, Grader, Llm, Jev, Pack, CloudDisabled, Offline, Gen, Seed, NotFound, Srs, Vocab}`（Srs/Vocab 为 M4 新增）；外部失败可恢复降级。
- [ ] S1–S8 端到端（fake AI + 真 SQLite）；旁路失败注入主事务仍成功；CloudDisabled 显式报错；
- [ ] 同权映射表逐行走查；TUI 中文宽度快照；媒体降级可作答；游戏 TUI 全可玩；SPA 深链 fallback；
- [ ] **单机全流程走通**（无 key 无网：答题/复习/打字/游戏/笔记/口令码题包）；键位表与 `?` 帮助屏一致。
