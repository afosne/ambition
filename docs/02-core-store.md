# 02 · core-store — 领域模型 + 持久化（M1）

> 实现顺序：**M1**。并入来源：旧 `01-core` + `02-store`。纯类型 + 持久化契约，全体 crate 的底座。

## 1. 领域类型（crates/core，纯类型+纯函数）

```rust
// ID：Qid(内容哈希)/CardId(source 内容寻址——M4 定案：同源同卡=幂等入卡，不走 uuid)/AttemptId/EssayId/NoteId/DocId（uuid v7）/ContentHash(sha256)
pub struct LangTag(pub String);                  // BCP-47："en"/"zh-Hans"/"ja"…（16-config-i18n）
pub struct Stem { pub text_l2: String, pub text_l1_hint: Option<String>, pub lang: LangTag } // l2 进 Jev；l1 永不进
pub struct MediaRef { pub kind: MediaKind /* Image|Audio */, pub path: String, pub alt_zh: String }

pub struct Question { pub qid: Qid, pub kind: QuestionKind, pub stem: Stem, pub media: Vec<MediaRef>,
                      pub difficulty: u8, pub tags: Vec<Tag>, pub provenance: Provenance }
pub enum QuestionKind { SingleChoice(ChoiceBody), MultiChoice(ChoiceBody), FillBlank(FillBody), Cloze(ClozeBody),
                        Reading(ReadingBody), SevenOfFive(SevenOfFiveBody), GrammarFill(GrammarFillBody),
                        Match(MatchBody), Translation(TranslationBody), Essay(EssayPrompt),
                        Dictation(DictationBody), Rewrite(RewriteBody) }
// Body 族：ChoiceBody{options, answer} / FillBody{sentence, blanks:Vec<Blank>} / Blank{index, acceptable, hint_zh}
// ClozeBody{passage, blanks:Vec<Blank>}（完形：语篇 + 逐空可接受表；空位 = passage 中 `___` 标记按序；
//   作答 = Answer::Fill(逐空文本)，规则层逐空规范化比 acceptable，与 FillBlank 同路）
// GrammarFillBody{passage, blanks:Vec<Blank>}（语法填空：同 Cloze，空更密、多考语法变形）
// ReadingBody{passage, items:Vec<ReadingItem>}（阅读理解：子题=单选；
//   ReadingItem{question, options:Vec<OptionItem>, answer:Vec<u32>}；作答 = Answer::Group(逐子题 Choice)——
//   子题选项 id 各自独立 0..n，扁平 Vec<u32> 无法表达 → 新增 Answer::Group(Vec<Answer>)，
//   规则层/Jev/LLM 三级流水线逐子题递归后汇总（docs/07 §1 M4 注记）
// SevenOfFiveBody{passage, options:Vec<String>, answer:Vec<u32>}（七选五：段落 5 个缺口（passage 中 `___` 序）+ 7 候选句，
//   answer = 按缺口顺序的选中句下标（长度 5）；作答 = Answer::Choice(逐缺口选中项，长度 = 缺口数)）
// MatchBody / TranslationBody{direction: TranslationDirection{from,to}, source_text, reference} / EssayPrompt{prompt_zh, genre, track, rubric_id, word_range, minutes}
//   翻译（M5-W2）：source_text=源文（l1，仅题面展示、**永不进 Jev state**）；reference=目标语参考译文（l2，进 state 作判分参照）；作答 = Answer::Text

pub struct Attempt { pub attempt_id: AttemptId, pub qid: Qid, pub card_id: Option<CardId>, pub answer: Answer,
                     pub timing: Option<Timing>, pub self_confidence: Option<u8>, pub mode: AttemptMode, pub created_at }
pub enum Answer { Choice(Vec<u32>), Fill(Vec<String>), Group(Vec<Answer>) /*复合题子题答案（M4 题型补全：Reading 等）*/, Match(Vec<(u32,u32)>), Text(String), Typing{text, keystrokes} }

pub struct GradeResult { pub attempt_id: AttemptId, pub verdict: Verdict, pub score: f32, pub max: f32,
                         pub confidence: f32, pub graded_by: GradedBy, pub error_tags: Vec<Tagged>, pub detail: GradeDetail }
pub enum Verdict { Correct, Partial, Wrong, Acceptable /*Jev 模糊通过*/, Pending /*待裁定*/ }
pub enum GradedBy { Rule, Jev, LlmReview, Human }
pub enum GradeDetail { Items(Vec<ItemResult>), Dims(Vec<DimScore>), Plain }
pub struct DimScore { pub key: String, pub score: f32 /* 0..1 归一化 */, pub confidence: f32 }   // M5：composite 维度分（GradeDetail::Dims 载体）

pub enum ErrorTag { WordSense, Confusable, Collocation, Preposition, Spelling, PosMix, Article, Countability,
                    Tense, Voice, Sva, ClauseLinker, NonFinite, Subjunctive, InversionEmphasis, SentenceStructure,
                    Cohesion, Reference, Logic, Chinglish, Register, Redundancy, PatternMisuse,
                    Misread, TimePressure, Careless, NoError, Other }

pub enum CardSource { WordSense{word_id, sense_id}, Question{qid}, Note{note_id}, Phrase{phrase_id}, Pattern{pattern_id} }

pub enum DomainEvent { AttemptGraded{..}, ReviewDone{..}, TypingFinished{..}, EssayGraded{..}, DiaryReviewed{..},
                       PaperFinished{..}, PackShared{..}, PackImported{..}, DrillCleared{..}, QuizPassed{..},
                       DialogueFinished{..} }   // 不可变事实，payload 自包含
pub trait EventBus: Send + Sync { fn publish(&self, ev: DomainEvent); }   // 订阅者：gamify/sync 投影/profile（旁路幂等）

// 纯函数
pub fn normalize_text(s: &str) -> String;                 // 小写/全角/空白/标点/可选冠词（按语种 normalizer 注入）
pub fn canonical_json_hash<T: Serialize>(v: &T) -> ContentHash;
pub fn is_guess(t: &Timing, c: Option<u8>, b: &UserBaseline) -> bool;   // 假对三形态（08-adaptive §假对）
pub fn baseline_from_timings(&[Timing]) -> UserBaseline;                // 近 N 次 total_ms 滚动 p50/p90（空历史=默认 15s/60s，M4 落地）
```

## 2. 持久化（crates/store：sqlx 双后端同一迁移）

- **存储分层（关键）**：SQLite = **业务数据**（用户动态 + 生成内容）；**词库与内置内容 = 只读资源文件**（`data/lexicon` 等，16-config-i18n §3.1）——**无服务端/S3 时资源就保存/使用本地 `data/` 目录**，配置后可从托管拉取；words 系列**不进 SQLite**；客户端形态下**本地 SQLite 始终是主存储**（配了服务端也不变，14-cloud §1.1）；
- **DDL 全表**（`*_json` 列 / 时间 `*_at` / 单写者见 00-overview §4）：
  `patterns/sense_variants/etymology_cards/arguments/mnemonics`（用户/生成内容；内置同类在 data/ 资源，运行时叠加）、
  `questions/question_links/papers/paper_runs`（题）、`attempts/grades/error_tags/arbitration`（作答判分）、
  `srs_queue`、`essays/essay_grades/templates/translations/tx_favorites`（写作翻译）、
  `notes/diary/todos`、`typing_sessions`、`profile_snapshots/warnings/cat_state/radio_episodes/annual_reports`（学情节奏）、
  `game_state/game_events`、`packs/shared_packs/sync_log/accounts`（云）、`source_docs/media_assets/derivations`（素材）、
  `llm_cache/llm_usage`、`explanations`（讲解资产：判题/讲解自动沉淀，随 qpack 导出导入，06-generation §7）；
- 迁移 `NNNN_desc.sql` 只增不改；SQLite/Postgres shim；静态数据（词表/模板/错式库）由 16-config 导入器幂等 upsert，不进迁移。

### Repo trait（engine 只依赖这些；测试用 fake）

```rust
QuestionRepo{upsert/get/by_tags/link} · AttemptRepo{insert_attempt_tx/get_arbitration/resolve_arbitration/add_blank_acceptable/get_attempt/recent_timings}
CardRepo{upsert_card/get_card/due_cards/wrong_cards} · EssayRepo{insert/history/upsert_template/templates} · NoteRepo{upsert/backlinks/search/diary_range/todo_open}
TypingRepo{insert/history} · ProfileRepo{save_snapshot/latest/error_heat} · GameRepo{load/save/log_event}
PackRepo{upsert_pack/upsert_shared/followed} · SyncRepo{append/unpushed/mark_pushed} · AiLedgerRepo{cache_get/cache_put/record_usage/usage_summary}
LexiconReader{senses/relations/lookup/confusables}（**读词库资源文件**，09-vocabulary） · PatternRepo{upsert/by_track} · TranslateRepo{insert/search/favorite} · MaterialRepo{ingest/get/register/usage_of/set_status/asset_*}
```

### 事务与导出

- **提交作答 = 单事务**（attempts+grades+error_tags+arbitration）；srs/gamify/sync/profile 后置旁路（幂等可重放）；全部业务写入经 engine 钩子投影 `sync_log`（**完备变更日志**，14-cloud §1.2 全量恢复的基础）；
- SQLite WAL + busy_timeout；`export_all_json / import_all_json / export_notes_md / db_backup`（数据所有权）。
- **迁移版本化（M2 落地）**：`schema_version` 单行表 + 代码内迁移清单 `MIGRATIONS: [(version, SQL)]`（只增不改，应用到最大版本）；枚举列一律 serde `snake_case` 字符串（禁 `format!("{:?}")`）；通用 JSON 负载列（questions.payload、grades.payload）统一命名 `payload`（attempts.answer 是语义列保留原名）；题目内容 = core 具名结构 `QuestionContent{stem, kind, difficulty, tags}`（不是位置元组），provenance/quality_score/lang 独立列。
- **迁移 v2（M3 落地）**：`arbitration` 待裁定唯一桶（`attempt_id` 主键 + `status(pending|resolved)` + `reason` + 裁决后 `verdict/resolved_at`）——`insert_attempt_tx` 在 `verdict=pending` 时**同事务**写入（§事务含 arbitration）；`list_arbitration(status)` / `resolve_arbitration(attempt_id, verdict)`（同时改 grades：`graded_by=human`）；裁决回流 = `add_blank_acceptable(qid, index, value)`（读改写 questions.payload，同值幂等）。`cache_purge_before(unix)`（llm_cache TTL 淘汰）与 `usage_summary(from_unix, to_unix)`（llm_usage 按 kind 区间聚合，成本面板数据源）同版落地。
- `quality_score REAL NOT NULL DEFAULT 1.0` 随 questions 表（06 质检回路产物；组卷/导入用 θ 过滤）。
- **迁移 v4（M5-W4 落地）**：写作三表——`templates`（`template_id` PK + `track/genre/source` 列 + `payload` 整模板 JSON，16-config §4 schema）、`essays`（`essay_id` PK + `qid/prompt/genre/body/word_count/scaffold_stage/template_id/created_at`）、`essay_grades`（`essay_id` PK + `payload` 整 GradeResult JSON + `graded_at`）。`EssayRepo{upsert_template/templates}` 随 W4b 落地，`insert/history`（作文批改闭环）随 W5；个人模板同 id 覆盖内置（engine 合并口径 docs/10 §1）。
- **迁移 v5（M5-W6 落地）**：套卷两表——`papers`（`paper_id` PK + `track/title` 列 + `payload` 整 Paper JSON（spec+qids）+ `created_at`）、`paper_runs`（`run_id` PK + `paper_id` + `started_at/deadline/finished_at` + `score/max`）。`PaperRepo{save_paper/load_paper/start_run/finish_run/load_run/paper_score}`；`paper_score(qids)` = 每 qid **最新一次**作答（attempts.rowid 降序窗口）求和——作答经 Practice 正常流落 attempts/grades，历史 run 即 12-notes-profile::predict_score 的回归样本（估分唯一在 12，M6；M5 只落数据与本次得分）。
- **迁移 v3（M4 落地）**：`srs_queue` 复习卡——`card_id` PK（= core CardId，source 内容寻址，幂等入卡）+ `payload`（整张 Card JSON：FSRS 状态/模式/错词计数/假对计数/错因标签）+ **`due_at` 整数 unix 秒索引列**（避开 `datetime()` NULL 静默坑，P-15）+ `wrong_count` 查询列（错词本入口）；单写者 08-adaptive（经 engine：on_attempt/review_answer）。同版新增 `get_attempt(attempt_id)`（读回整条 Attempt——attempts.answer 列存的是完整 Attempt JSON）与 `recent_timings(limit)`（近 N 次 total_ms 滚动样本，假对基线原料）。

## 3. 验证点

- [ ] 迁移双后端跑通；submit 事务异常注入无半写；每 Repo 有 fake；
- [ ] export→import 行数逐表一致（仅业务表）；**words 系列不在 SQLite**（schema 断言）；词库资源加载查询等价；word_id/sense_id 引用稳定（词库升级不破坏 srs/mnemonics 引用）；
- [ ] QuestionKind×Answer 配对穷尽（编译期）；Qid 对同内容稳定；DomainEvent 序列化向后兼容；normalize_text ≥30 表驱动用例。
