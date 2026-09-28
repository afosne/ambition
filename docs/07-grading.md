# 07 · grading — 三级判题流水线（M3；M1 先落规则层）

> 实现顺序：**M1（规则层）→ M3（Jev/LLM 全流水线，已落地：三级 grade、待裁定桶、错因分类、中式英语错式库）**。并入来源：旧 `05-grader`。
> 判分唯一实现（00-overview §5）：06/10/13/15/18 的一切"判断对错"都调本册。LLM 不打分。

## 1. 总控与路由

```rust
pub struct Thresholds { pub accept: f32, pub reject: f32 }          // M3：for_lang(en)=0.75/0.35；其它=0.85/0.20（docs/05 §3）
pub struct Grader { jev: Option<Arc<JevClient>>, llm: Option<Arc<LlmRouter>>, thresholds: Thresholds }  // AI 全可空 = 离线形态
pub async fn grade(&self, q: &Question, answer: &Answer) -> GradeResult;   // 永不 Err：离线/网络失败 → Pending 进桶（tracing 可见）
// ①规则命中→完；②Jev（margin≥θ）→完+错因；③LLM 粗判（只出判断）或进 arbitration 待裁定（唯一桶）
// M4 题型补全（DESIGN §13.4）：复合题型（Cloze/Reading/SevenOfFive/GrammarFill）拆子题递归——
//   `sub_questions(kind)` / `sub_answers(kind, answer)`（子题 qid = 父 qid + 子下标，不走内容哈希防碰撞）；
//   子题各自走完整三级流水线，汇总 score/max=Σ、verdict=全对 Correct/全错 Wrong/否则 Partial（任一 Pending 整体 Pending）、
//   items 带子下标、error_tags 合并去重、graded_by 取子题最重路径（Human > LlmReview > Jev > Rule）。
```

| Kind | ①规则 | ②Jev | ③兜底 |
|---|---|---|---|
| SingleChoice | 选项比对 | 错因 choice | — |
| MultiChoice | 全对/全错 | 逐项 noul 部分分 + 错因 | 低置信→Pending |
| FillBlank | normalize + acceptable 表 | 逐空 noul + 错因 + 严重度 | Pending |
| Cloze | 子题递归（每空 = FillBlank 子题） | 同左 | 同左 |
| GrammarFill | 子题递归（每空 = FillBlank 子题） | 同左 | 同左 |
| Reading | 子题递归（每题 = SingleChoice 子题） | 同左 | 同左 |
| SevenOfFive | 子题递归（每空 = MultiChoice 子题，选项=缺口候选句） | 同左 | 同左 |
| Match | 子题递归 | 同左 | 同左 |
| Dictation | 词序列 diff | noul 容忍小错 | Pending |
| Rewrite | — | noul formed/meaning_kept | LLM 复核 |
| Translation | — | composite 4 维 + veto | LLM 复核 |
| Essay | 词数/时长硬校验 | composite 6 维 + veto | LLM 复核 |
| Typing | 击键统计（04） | — | — |

## 2. 规则层（纯函数）

```rust
grade_single_choice / grade_multi_choice_rule / grade_fill_blank_rule / grade_dictation_rule
grade_rule(kind, answers, fills) -> Option<GradeResult>   // 复合题型不在此层判（None → 拆子题递归，docs/07 §1 M4 注记）
sub_questions(kind) -> Vec<(String, Question)>           // 复合题拆子题（子题 qid = 父 qid + 下标）
sub_answers(kind, &Answer) -> Vec<Answer>                // 复合作答拆分（index 对齐 sub_questions）
normalize_for_compare(s)   // core::normalize_text + 按 track 可选忽略冠词/单复数
```

## 3. Jev 升级（单请求 fan-out 多判断）

```rust
pub async fn jev_blank(&self, q, blank, typed) -> Result<BlankJudge>;    // acceptable(noul)+error_cause(choice)（M3 落地；severity 展示随讲解层 GradeDetail）
pub async fn jev_multi_items(&self, q, sel) -> Result<MultiJudge>;    // 逐项 noul 部分分（M3：只判漏选项 answer\sel；MultiJudge{items: Vec<ItemJudge{index, belongs, margin}>, error_tags}；无 Jev → Offline）
pub async fn jev_composite(&self, state: &JevState, rubric: &Rubric) -> Result<CompositeJudge>;  // M5 落地：rubric 单源携带 dims+veto；每维 Score + veto×3（Noul，正面表述、noul<0.5 触发）
pub fn compose_scores(dims: &[DimScore], rubric: &Rubric) -> f32;   // M5 落地：按 key 取 weight 加权 → 档位（band_map 下界 floor；rubric 版本化）
pub fn down_band(band: f32, band_map: &[f32]) -> f32;               // M5 落地：veto 一票降一档（按 band_map 降到下一档；已是最低/低于首档 → 0）
pub fn tag_error_cause(answers: &JevAnswers) -> Vec<ErrorTag>;        // 唯一错因归类口（M3：error_cause choice → ErrorTag；未知 key tracing::warn 跳过）
pub fn error_cause_question(lang: &str) -> JevQuestion;               // error_cause 判断集：criteria+instructions 按题目 l2 选版、缺版回退 en（05 §3，M3→i18n）
```

**composite 语义（M5 落地）**：rubric 资产 = `crates/grader/assets/rubrics/<id>.json`（**判分语料红线 #19：不进 `data/`**、include_str 内嵌、按 rubric_id/version 版本化）；每维 Jev `score` ∈ 0..levels-1 → 归一化 `score/(levels-1)` ∈ 0..1（维度 instructions 由代码按 dim key 生成，state 放 prompt+essay，levels = 考纲原文）；`CompositeJudge{dims, triggered_vetoes, confidence=min(dims.confidence)}`；veto 问题**正面表述**（如 "stays on topic"）→ `noul < 0.5` 触发 → `down_band` 降一档；`compose_scores` = Σ(dim.score×weight)×max → 向下取档（band_map 中 ≤ raw 的最大值，raw < 首档 → 0）；confidence < thresholds.accept → 走既有 Pending 进桶（M3 复用）。

## 4. 第三级：LLM 复核与待裁定桶

```rust
pub async fn llm_arbitrate(&self, q, a) -> Result<Arbitration>;            // acceptable+error_tag+brief_reason（判断，不打分，M3 落地：LlmKind::Arbitrate；prompt 按题目 l2 分版、缺版回退 en——05 §1）
// 仍不定夺/离线 → arbitration 表（唯一桶，store 迁移 v2）；人工裁决 resolve → grades 更新（graded_by=human）
//   + 填空题 typed 答案回流 Blank.acceptable（add_blank_acceptable）→ 同类题规则直判（规则增强闭环）
// engine 门面（M3 落地）：pending_bucket() / resolve_pending(attempt_id, accept)
```

## 5. 判题后讲解（Jev 结果 → 三层讲解输出）

> Jev 是判断模型（**不出文本**）；但判题后**完全可以讲解**，且无 LLM 也有第一层。

| 层 | 内容 | 来源 | 依赖 |
|---|---|---|---|
| ① 结构化讲解（零成本、离线可用） | verdict、**错因标签+权重**、severity、**概率分布**（正确项 85% / 你选的 15%——“模型视角为什么”）、规则 diff（“只差大小写/多冠词”） | 规则层 + Jev 原始 answers（probabilities/confidence 直接展示） | 无 |
| ② 文字讲解 | 错因讲解/规则点/更好答案/类似例句（Explanation） | 06-generation::explain（**唯一文本源**；输入 = 题目+作答+**Jev 错因标签**） | LLM key（缓存按需，`e` 键唤出） |
| ③ 讲解深加工 | 可玩化 3 题（18-learning-loop::playful_explain）、口播（18::explain_orally）、一键存笔记/入 SRS | ②的文本 + Jev 标签 | LLM / TTS |

数据流：`grade() → GradeResult{error_tags, detail(含 Jev probabilities)} → UI 立即渲染① → 按 e → 06::explain(qid, tag) → llm_cache → ② → ③`。

- 无 LLM key：①照常（错因+概率分布就是“为什么”的骨架）；②③隐藏/提示配置；待裁定题裁决后补错因 → ①②照常；
- Jev 概率分布兼做**干扰项分析**（哪个错项把你骗了，进 06-generation::confusion_stats）。

## 6. 离线/降级矩阵

| 情况 | 行为 |
|---|---|
| Jev 离线 | 规则可判（单选/全对全错/填空全对或错）→ 规则结果（graded_by=rule，无错因标签）；规则不可判（多选漏选）→ 第三级：有 LLM key 则粗判（graded_by=llm_review），否则 Pending 进 arbitration 桶（confidence=0 标低置信，人工 y/n 可裁决）；Jev 请求**中途失败**（非 key 缺席的网络错）= Jev 离线 → 保持当前已得判定（规则结果/已判项），warn 可见 |
| LLM 离线 | 无复核（route=LlmReview 直接降级 HumanReview 进桶）；讲解不生成 |
| 全离线 | 本地题库+规则判题（M1 形态）+ 待裁定人工裁决（零 key 可用，docs/03 §5.2） |

## 7. 验证点

- [x] M1 三题型三路径（fake/wiremock AI）；route 灰区 → Pending；离线矩阵（Jev 无 key / LLM 无 key / 全离线）；错因归类概率阈值测试；
- [x] 裁决回流 → 二次作答规则直判闭环（M3）；
- [x] 复合题型递归判分（M4：Cloze/Reading/SevenOfFive/GrammarFill 子题拆解 + 汇总口径 + 任一 Pending 整体 Pending）；
- [x] composite 加权/档位边界（高考 25 分档/雅思 band）；veto 降档（M5——W1 落地：rubric 资产内嵌 + compose/down_band + wiremock composite 全链，79 测试）；
- [ ] **无 LLM key 时结构化讲解完整**（错因+概率分布+规则 diff）；explain 的 prompt payload 含 Jev 错因标签（断言）（随讲解层，06::explain）。
