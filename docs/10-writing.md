# 10 · writing — 写作全家桶：模板作文 / 加分项 / 批改 / 观点素材 / 卷面模拟（M5 + M16）

> 实现顺序：**M5 → M16（观点素材）**。并入来源：旧 `10-writing` + `47-argument-bank` + `36-exam-realism(卷面模拟)`（干扰项引擎在 06-generation）。
> 分数由 07-grading（Jev composite）出；本册出文本与流程。翻译工作台见 11-translate。

## 1. 模板作文（脚手架三阶段，最终脱模板）

```rust
// —— M5-W4 精确化契约（2026-09-28；实现 crates/writing，内置库 data/templates/*.json、个人模板 SQLite templates 表）——
pub enum TemplateSource { Builtin, Personal }
/// 段落功能（骨架一环；lines = 该段句模板，含 {{slot_id}} 占位）
pub struct ParaFunc { pub key: String, pub label_zh: String, pub lines: Vec<String> }
/// 内容槽位（{{id}} 出现在 ParaFunc.lines；options_en = 槽位备选高级句）
pub struct Slot { pub id: String, pub label_zh: String, pub options_en: Vec<String> }
/// 亮点词汇/句式（语域分档）
pub struct Phrase { pub text: String, pub register: String, pub level: u8 }
/// 反模板黑名单（模板痕迹句式）
pub struct AntiPattern { pub pattern_en: String, pub note_zh: String, pub replacement_en: String }  // 升级替换句（upgrade_sentence 本地命中即换，M5-W5）
pub struct Template { pub template_id: String, pub track: String, pub genre: String,
                      pub skeleton: Vec<ParaFunc>, pub slots: Vec<Slot>, pub phrases: Vec<Phrase>,
                      pub anti_patterns: Vec<AntiPattern>, pub source: TemplateSource }
pub enum ScaffoldStage { Full, Half, Free }
pub struct ScaffoldPara { pub label_zh: String, pub lines: Vec<String> }  // Full=句模板；Half/Free 隐藏 lines
pub struct ScaffoldView { pub template_id: String, pub stage: ScaffoldStage, pub paragraphs: Vec<ScaffoldPara> }
pub fn scaffold(t: &Template, stage: ScaffoldStage) -> ScaffoldView;      // 三阶段渐退：Full=句模板含占位；Half=只段落功能提示；Free=空骨架
pub fn render(t: &Template, fills: &BTreeMap<String,String>) -> String;    // 套槽成文；缺槽保留 {{id}} 并 tracing::warn（不静默）
pub struct ScaffoldCheck { pub meaning_kept: f32, pub fits_prompt: f32, pub passed: bool }  // W4 落地：Jev noul×2、双 prob ≥0.5 过（passed）；LLM 模板痕迹检测随 W5
pub async fn check_scaffold(&self, essay, t) -> Result<ScaffoldCheck>;    // Jev noul×2（meaning_kept/fits_prompt）+ LLM 模板痕迹检测
pub async fn personalize(&self, t, user_style) -> Result<Template>;       // 个人化变体（防被判模板）
pub async fn promote_to_personal(&self, sentence, essay_id) -> Result<Template>;  // 佳句升格个人模板 → 08-adaptive(Phrase 卡)
```
模板库覆盖：高考应用文/读后续写、四六级议论、考研图画三段式、雅思 Task1 图表/Task2 四题型、商务邮件。反模板黑名单 + 地道替换。

## 2. 批改闭环 + 加分项

```rust
// —— M5-W5 落地契约（2026-09-28）；W8 留位项（polish/long_sentence_explain/merge_split_items/register_check）签名不变、随语域波次实现 ——
pub struct EssaySubmission { pub body: String, pub prompt_zh: String, pub genre: String, pub rubric_id: String, pub scaffold_stage: String, pub template_id: String }
pub async fn grade_essay(&self, sub: &EssaySubmission) -> Result<GradeResult, EngineError>;
    // 07 composite → 档位（不另造 EssayGrade：出口 = GradeResult，与 02 §2 essay_grades.payload 同构）；落 essays+essay_grades 两表 + DomainEvent::EssayGraded
pub async fn annotate(&self, essay: &str, grade: &GradeResult, rubric_id: &str) -> Result<Comments, EngineError>;
    // 逐句批注/升级/亮点（LlmKind::Explain + writing 资产 prompts/annotate + comments_schema）；缓存 = llm run_loop 按 effective_cache_key 自动 get/put（key 含 instr+essay+rubric 上下文 = hash(body+rubric) 口径）
pub fn recommend_after(g: &GradeResult, lib: &[Phrase]) -> Vec<Recommendation>;  // 缺陷→模板/句式/搭配补练（纯函数；lib = phrase_bank(track) 结果）
pub async fn upgrade_sentence(&self, s: &str) -> Result<Vec<Upgrade>, EngineError>;  // 低级句→高级句候选（本地：AntiPattern.replacement_en 整句命中 + 同义表 good→beneficial…）
pub async fn verify_upgrade(&self, from: &str, to: &str) -> Result<bool, EngineError>;  // Jev noul 原意未变（≥0.5 过；<0.5 = 语义走样必须拦）
pub fn phrase_bank(track: &str) -> Vec<Phrase>;                          // 亮点词汇（语域分档；内置模板 phrases 汇聚，08 补个人库）
pub async fn model_essays(&self, prompt: &str) -> Result<ModelEssaySet, EngineError>;  // 三档范文+逐句赏析（llm structured + writing schema；缓存同 run_loop）
pub async fn polish(&self, text, register) -> Result<String>;             // 语域润色（W8）
pub async fn long_sentence_explain(&self, s) -> Result<SyntaxTree>;       // 长难句图解（W8）
pub fn merge_split_items(track) -> Vec<Question>;                         // 合并/拆分/概要（Drill，W8）
pub async fn register_check(&self, text, target) -> Result<RegisterReport>;   // 语域检查（W8）
// —— 类型补全（M5-W5 docs-first 定义，实现 crates/writing）——
pub enum CommentKind { Upgrade, Highlight }
pub struct Annotation { pub sentence: String, pub kind: CommentKind, pub suggested: Option<String>, pub brief_reason: String, pub target: Option<SrsTarget> }
pub struct Comments { pub annotations: Vec<Annotation> }
pub enum WordKind { Word, Phrase, Expression }
pub struct SrsTarget { pub kind: WordKind, pub text: String }   // 升级句可入卡目标（Phrase 卡随 08-adaptive）
pub struct Upgrade { pub from: String, pub to: String, pub why_zh: String }
pub enum RecommendKind { Template, Phrase, Collocation }
pub struct Recommendation { pub kind: RecommendKind, pub text_zh: String, pub reason_zh: String }
pub struct ModelEssaySet { pub essays: Vec<ModelEssay> }
pub struct ModelEssay { pub word_target: u32, pub text: String, pub sentences: Vec<SentenceNote> }   // 200/400/600 三档
pub struct SentenceNote { pub sentence: String, pub note_zh: String }
```

## 3. 翻译题型（考试向；工作台在 11）

```rust
pub async fn grade_translation(&self, a: &TranslationBody, ans: &str) -> Result<TranslationGrade>;  // 4 维 + veto
pub fn culture_expressions(topic: &str) -> Vec<(String,String)>;          // 中国文化表达库
```

> **M5-W2 落地形态**：考试翻译走 `Grader::grade` 的 Translation 分支（pipeline::composite 单源 rubric + `GradeResult{detail::Dims}`，判分唯一实现 00 §5）；`TranslationGrade` 独立包装随批改闭环（W4/加分项）一并补；`culture_expressions`（中国文化表达库）随素材中台（M10）数据落位。

## 4. 观点素材库 Argument Bank（原 47）

```rust
pub struct Argument { id, topic: TopicTag, claim_l1/l2, examples, stats /*数据句必带 caveat*/, stance: Pro|Con|Nuanced, quality, source }
pub struct ArgumentBank;
// by_topic / build_arg（qc fact_plausible 拦编造数据）/ cite_into（融入 draft 保持学生语气）/ debate_drill（正反对辩）/ personal_bank（用户语料例子）
```
数据：`data/arguments/<lang>/<topic>.json` + 表 `arguments`。

## 5. 卷面模拟（原 36 §2）

```rust
pub struct ExamSheet { prompt, body, started_at, deadline, pasted }
pub fn exam_sheet(&self, prompt: &EssayPrompt) -> ExamSheet;               // 字数红线(word_range)+计时+收卷铃
pub fn sheet_violations(&self, sheet) -> Vec<Violation>;                   // Under/Over|Overtime|Pasted（应试禁粘贴）
pub async fn hand_in(&self, sheet) -> Result<EssayGrade>;                  // violations 进批注与档位说明
pub fn typing_speed(&self, sheet) -> f32;                                  // 04-typing 复用
```

## 6. 验证点

- [ ] 三阶段渲染快照；verify_upgrade 拦语义走样；veto 降档；annotate 缓存；个人模板→SRS 卡；
- [ ] 数据句 caveat 必填 + fact_plausible 拦截；正反论点成对；cite_into 语气一致；
- [ ] 字数红线与 rubric word_range 一致；粘贴/超时检测；收卷批改链与普通作文一致。
