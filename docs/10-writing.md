# 10 · writing — 写作全家桶：模板作文 / 加分项 / 批改 / 观点素材 / 卷面模拟（M5 + M16）

> 实现顺序：**M5 → M16（观点素材）**。并入来源：旧 `10-writing` + `47-argument-bank` + `36-exam-realism(卷面模拟)`（干扰项引擎在 06-generation）。
> 分数由 07-grading（Jev composite）出；本册出文本与流程。翻译工作台见 11-translate。

## 1. 模板作文（脚手架三阶段，最终脱模板）

```rust
pub struct Template { template_id, track, genre /*Letter|Notice|Continuation|Summary|Argument|Chart|Email…*/,
                      skeleton: Vec<ParaFunc>, slots: Vec<Slot>, phrases: Vec<Phrase>, anti_patterns: Vec<AntiPattern>, source: Builtin|Personal }
pub enum ScaffoldStage { Full, Half, Free }
pub fn scaffold(t: &Template, stage: ScaffoldStage) -> ScaffoldView;
pub fn render(t: &Template, fills: &BTreeMap<String,String>) -> String;
pub async fn check_scaffold(&self, essay, t) -> Result<ScaffoldCheck>;    // Jev noul×2（meaning_kept/fits_prompt）+ LLM 模板痕迹检测
pub async fn personalize(&self, t, user_style) -> Result<Template>;       // 个人化变体（防被判模板）
pub async fn promote_to_personal(&self, sentence, essay_id) -> Result<Template>;  // 佳句升格个人模板 → 08-adaptive(Phrase 卡)
```
模板库覆盖：高考应用文/读后续写、四六级议论、考研图画三段式、雅思 Task1 图表/Task2 四题型、商务邮件。反模板黑名单 + 地道替换。

## 2. 批改闭环 + 加分项

```rust
pub async fn grade_essay(&self, essay, rubric) -> Result<EssayGrade>;     // 07 composite → band
pub async fn annotate(&self, essay, g) -> Result<Comments>;               // 逐句批注/升级/亮点（llm 缓存 key=hash(body+rubric)）
pub fn recommend_after(g, lib) -> Vec<Recommendation>;                    // 缺陷→模板/句式/搭配补练
pub async fn upgrade_sentence(&self, s) -> Result<Vec<Upgrade>>;          // 低级句→高级句（good→beneficial…）
pub async fn verify_upgrade(&self, from, to) -> Result<bool>;             // Jev noul 原意未变
pub fn phrase_bank(track) -> Vec<Phrase>;                                 // 亮点词汇（语域分档）
pub async fn model_essays(&self, prompt) -> Result<ModelEssaySet>;        // 三档范文+逐句赏析（qc.natural）
pub async fn polish(&self, text, register) -> Result<String>;             // 语域润色
pub async fn long_sentence_explain(&self, s) -> Result<SyntaxTree>;       // 长难句图解
pub fn merge_split_items(track) -> Vec<Question>;                         // 合并/拆分/概要（Drill）
pub async fn register_check(&self, text, target) -> Result<RegisterReport>;
```

## 3. 翻译题型（考试向；工作台在 11）

```rust
pub async fn grade_translation(&self, a: &TranslationBody, ans: &str) -> Result<TranslationGrade>;  // 4 维 + veto
pub fn culture_expressions(topic: &str) -> Vec<(String,String)>;          // 中国文化表达库
```

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
