# 07 · grading — 三级判题流水线（M3；M1 先落规则层）

> 实现顺序：**M1（规则层）→ M3（Jev/LLM 全流水线）**。并入来源：旧 `05-grader`。
> 判分唯一实现（00-overview §5）：06/10/13/15/18 的一切"判断对错"都调本册。LLM 不打分。

## 1. 总控与路由

```rust
pub struct Grader { jev: Arc<JevClient>, llm: Arc<LlmRouter>, rules: RuleSet, thresholds: Thresholds }
pub async fn grade(&self, q: &Question, a: &Answer, ctx: &GradeCtx) -> GradeResult;
// ①规则命中→完；②Jev（confidence≥θ）→完+错因；③LLM 粗判（只出判断）或进 arbitration 待裁定（唯一桶）
```

| Kind | ①规则 | ②Jev | ③兜底 |
|---|---|---|---|
| SingleChoice | 选项比对 | 错因 choice | — |
| MultiChoice | 全对/全错 | 逐项 noul 部分分 + 错因 | 低置信→Pending |
| FillBlank/Cloze | normalize + acceptable 表 | 逐空 noul + 错因 + 严重度 | Pending |
| Reading/Match | 子题递归 | 同左 | 同左 |
| Dictation | 词序列 diff | noul 容忍小错 | Pending |
| Rewrite | — | noul formed/meaning_kept | LLM 复核 |
| Translation | — | composite 4 维 + veto | LLM 复核 |
| Essay | 词数/时长硬校验 | composite 6 维 + veto | LLM 复核 |
| Typing | 击键统计（04） | — | — |

## 2. 规则层（纯函数）

```rust
grade_single_choice / grade_multi_choice_rule / grade_fill_blank_rule / grade_dictation_rule
normalize_for_compare(s)   // core::normalize_text + 按 track 可选忽略冠词/单复数
```

## 3. Jev 升级（单请求 fan-out 多判断）

```rust
pub async fn jev_blank(&self, q, blank, typed) -> Result<BlankJudge>;      // acceptable(noul)+error_cause(choice)+severity(score)
pub async fn jev_multi_items(&self, q, sel) -> Result<Vec<ItemResult>>;    // 逐项 noul 部分分
pub async fn jev_composite(&self, state, dims: &[RubricDim]) -> Result<CompositeJudge>;  // 每维 score + veto×3（off_topic/meaning_changed/heavy_chinglish 一票降档）
pub fn compose_scores(dims: &[DimScore], weights, band_map) -> f32;        // 加权→档位（rubric 版本化）
pub async fn tag_error_cause(&self, q, a, judge) -> Vec<Tagged>;           // 唯一错因归类口（48 溯源消费）
```

## 4. 第三级：LLM 复核与待裁定桶

```rust
pub async fn llm_arbitrate(&self, q, a) -> Result<Arbitration>;            // acceptable+error_tag+brief_reason（判断，不打分）
// 仍不定夺/离线 → arbitration 表（唯一桶）；人工裁决 resolve → 可接受答案回流 Blank.acceptable → 同类题规则直判（规则增强闭环）
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
| Jev 离线 | 客观题规则结果+low_confidence；主观题 Pending |
| LLM 离线 | 无复核/讲解（直接展示 Jev 结果） |
| 全离线 | 本地题库+规则判题（M1 形态），UI 明示 |

## 7. 验证点

- [ ] 每 Kind 三路径集成测试（fake AI）；composite 加权/档位边界（高考 25 分档/雅思 band）；veto 降档；
- [ ] 裁决回流 → 二次作答规则直判闭环；离线矩阵逐格；错因归类概率阈值测试；
- [ ] **无 LLM key 时结构化讲解完整**（错因+概率分布+规则 diff）；explain 的 prompt payload 含 Jev 错因标签（断言）。
