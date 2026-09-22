# 06 · generation — 出题 / 质检回路 / DSL / 出课 / 干扰项引擎（M2 + M13）

> 实现顺序：**M2**（出题+质检+qpack）→ **M13**（DSL/一键出课/真题级干扰项）。并入来源：旧 `06-gen` + `39-studio` + `36-exam-realism(干扰项引擎)`。
> 唯一出题生成口（00-overview §5）：DSL/微课/辨析/对战出题全部收口于此或其参数化插件。
> **M2 落地范围**：`crates/gen` = GenSpec + `generate` + `qc_question`（Jev 质检 fan-out，05::evaluate）+ `build/parse/import/export qpack` + seed 加载；变式/讲解/干扰项引擎/出课随 M3+/M13。

## 1. 生成规格与主流程

```rust
pub struct GenSpec { pub track: TrackId, pub kinds: Vec<QuestionKindTag>, pub count: u32, pub difficulty: (u8,u8) /*必来自 08::cat*/,
                     pub topic: Option<String>, pub source_words: Option<WordSet>, pub weak_points: Vec<ErrorTag>, pub seed_qid: Option<Qid> }
pub async fn generate(&self, spec: &GenSpec) -> Result<Vec<Question>>;   // prompt → llm.structured → 校验(qid=内容哈希) → 质检回路(≤2 轮) → 交 engine 入库
// quality_score 已是 Question 字段（qc_report 回写）；引擎落库时存 quality 列（00-overview §2：回包经适配层→engine 落库）
// M2：结构化输出 = JSON Schema 约束的题草稿数组（DraftQuestion，字段同统一模型）；重生成 ≤2 轮后仍不合格 → 丢弃并记 tracing::warn
```

## 2. 质检回路（出题质量护城河，fan-out 单请求）

```rust
pub async fn qc_question(&self, q: &Question) -> Result<QcReport>;
// unique_answer(noul) / key_correct(noul) / natural(score) / distractor_valid(noul) / difficulty_fit(score)
// pass：noul ≥0.7 且 natural ≥1.5；低质题不进组卷；中文剥离后只判英文部分
// M2：一次 Jev 请求 fan-out 上述问题（05::evaluate）；state 只含题干/选项/答案（l2），母语/提示永不进 state
```

## 3. 真题级干扰项引擎（原 36 §1）

```rust
pub struct DistractorEngine { vocab, profile, repo }
pub async fn distractors(&self, sense: &SenseRef, n: u32) -> Result<Vec<String>>;
// 三层加权：①学生真实错答共现（含假对）②易混/近义（09-vocab）③同词性同频词 → 统一过 qc（distractor_valid + unique_answer）
pub fn confusion_stats(&self, sense: &SenseRef) -> Result<ConfusionStats>;   // 误选共现反哺画像
```

## 4. 变式 / 讲解 / 造句 / 深水出题

```rust
pub async fn variant_of(&self, qid: &Qid, tag: &ErrorTag) -> Result<Question>;       // 错题变式（question_links kind=variant）
pub async fn explain(&self, q: &Question, a: &Answer, tag: &ErrorTag) -> Result<Explanation>;  // 讲解唯一文本源（18 可玩化/口播包装）
pub async fn make_sentence(&self, sense: &Sense, corpus: &UserCorpus) -> Result<String>;       // 用户语料造句
pub async fn writing_drill(&self, issue: &WritingIssue) -> Result<Drill>;
pub async fn pattern_questions(&self, pid: &PatternId, mode: PatternDrillMode, n: u32) -> Result<Vec<Question>>;   // 句式五模式
pub async fn sense_question(&self, sense: &SenseRef, force_non_first: bool) -> Result<Question>; // 义项定向（幽灵/熟词僻义）
pub async fn from_material(&self, m: &MaterialRef, spec: GenSpec) -> Result<Vec<Question>>;      // 素材出题（17-hub 派生注册）
```

## 5. 出课工作室（原 39）

```rust
pub async fn one_click_lesson(&self, m: &MaterialRef) -> Result<Lesson>;   // 素材→生词+句式+仿写+小测（派生注册）
pub async fn teach_me(&self, m: &MaterialRef) -> Result<CoachedLesson>;    // +讲义（llm 缓存）
pub fn load_spec(path: &Path) -> Result<GenSpec>;                          // YAML DSL → GenSpec（schema 校验+行号报错）
pub async fn run_spec(&self, spec: &GenSpec) -> Result<Vec<Question>>;     // stem_template 给定时 LLM 不得改写 stem
pub async fn run_dir(&self, dir: &Path) -> Result<BulkReport>;             // 批量命题（限速+断点续跑）→ qpack → 班级作业
```

## 6. qpack 打包导入 / 组卷

```rust
pub fn build_qpack(questions: &[Question], manifest: Manifest) -> QPack;   // 格式见 14-cloud §qpack
pub fn parse_qpack(bytes: &[u8]) -> Result<QPack>;                          // schema_version 校验 + 内容哈希
pub async fn import_qpack(&self, pack: &QPack) -> Result<ImportReport>;    // 去重/低质拒绝（quality_score < θ 丢弃）
pub async fn export_qpack(&self, tags: &[String]) -> Result<QPack>;        // 从题库按标签导出（空标签=全量）
pub async fn assemble_paper(&self, spec: &PaperSpec) -> Result<Paper>;     // 只取 quality_score ≥ θ（M5 套卷）
// seed：data/seed/<lang>/<track>/*.qpack.json（内置离线内容包，16-config §3）；open() 时幂等导入
```

## 7. 判题沉淀（自动保存为题库，可导出可导入）

> “自动判题的题目 + 生成的解析”自动成为**个人题库资产**，随 qpack 流通——题库越用越厚，零人工整理。

```rust
pub async fn auto_collect(&self, a: &Attempt, g: &GradeResult) -> Result<()>;   // 判题即沉淀（规则/Jev/LLM 任一路径，幂等）；provenance=Collected
pub async fn save_explanation(&self, qid: &Qid, e: &Explanation) -> Result<()>;  // 讲解升格为持久资产（llm_cache 保留缓存语义）
pub fn explanations_for(&self, qid: &Qid) -> Result<Vec<Explanation>>;           // 多来源并存（llm|user|teacher），按 lang 取
```

- 表 `explanations(qid, kind, lang, text, source, created_at)`（02-core-store）；单写者=本册（00-overview §4）；
- **qpack 随包资产**：每题带 `explanations[] + acceptable（含裁决回流增强）+ quality_score` —— 别人导入即得讲解；导入者可**追加**自己的讲解（多来源并存）；
- 导出范围：整包 / **我的题库** / 错题本 / 标签过滤 → `build_qpack`；导入=06 §6 原有链路（幂等）；
- 幂等：同 qid 重复判题不重复收集；讲解按 (qid, kind, lang) 去重；裁决回流的 acceptable 增强随题传播（同类题全局受益）。

## 8. 验证点

- [ ] 生成→质检→入库全链（含两轮重生成）；qc 拦截率与 DSL 批量一致；stem_template 逐字保持；
- [ ] 干扰项防双正确（qc 断言）+ 假对样本入共现统计；变式考点保持（noul）；
- [ ] explain 缓存命中不重复调用；qpack round-trip hash 稳定；组卷低质题过滤；
- [ ] **判题即沉淀幂等**（同题多次作答只收一题）；qpack 带讲解 round-trip 无损；导入后可追加讲解且多来源并存。
- [x] M2：qc 全 fan-out 一轮请求（mock Jev）；重生成 ≤2 轮后丢弃；导入去重（同 qid 跳过）+ 低质拒绝；seed 幂等导入。
