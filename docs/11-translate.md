# 11 · translate — 翻译工作台（M5）

> 实现顺序：**M5**。并入来源：旧 `21-translate`。
> 独立于考试翻译题（10-writing §3）：工作台管**随手翻译与对照学习**；LLM 出译文，评分仍走 07-grading。语境查词唯一实现（00-overview §5）。

## 1. 类型

```rust
pub struct TranslateOpts { l1: LangTag, l2: LangTag, register: Register, literal_first: bool, glossary: Vec<(String,String)> }
pub struct Translation { tx_id, source, target, dir, model, glossary_hits }
pub struct Alignment { segments: Vec<AlignSeg{src, tgt, kind: Literal|Idiomatic|Reordered|Omitted}> }
pub struct CompareResult { reference: Translation, diff: Vec<DiffPoint>, grade: Option<GradeResult> }   // cards: Vec<CardDraft> 随 08-adaptive 落地（M8 注记，M5 不建）
pub struct DiffPoint { src: String, tgt: String, kind: DiffKind }      // DiffKind: Same|Changed|Missing|Extra（word 级 LCS；CJK 按字）
pub enum HarvestTarget { Wordbook(SenseRef), Note(NoteId), Template, Pattern(PatternId) }
```

## 2. 函数

```rust
pub async fn translate(&self, text: &str, opts: TranslateOpts) -> Result<Translation>;            // 缓存 key=hash(text+dir+model+glossary)；glossary 强制术语一致
pub async fn translate_span(&self, m: &MaterialRef, opts) -> Result<Translation>;                 // 素材选区（缓存含素材 hash）
pub async fn align(&self, source, target, opts) -> Result<Alignment>;                             // 句/短语对齐（LLM 结构化）
pub async fn my_turn(&self, source, mine, opts) -> Result<CompareResult>;                         // 我来翻：参考译文+差异+可选评分+学习卡
// 评分：l2=en → 07::jev_composite（准确/完整/地道）；其它语种 → LLM 粗判 + 待裁定（05 §多语）
// —— M5 落地边界（2026-09-28）：本波次落地 translate/align/my_turn/history/favorite/to_practice；
//    lookup（依赖 46/17 的 ContextSense/SenseRef——M6 语境查词波次）、harvest（依赖 17 单写者——M7）、
//    highlight_learned（依赖生词本 SenseRef——M6）随后续阶段；Direction 复用 core::TranslationDirection；Register 类型在 core（02 §1）。
pub async fn lookup(&self, word: &str, ctx: Option<&str>) -> Result<ContextSense>;               // 义项+语境义项判定（46/17 的 UI 面委托此处）
pub async fn harvest(&self, sel: TextSelection, to: HarvestTarget) -> Result<()>;                 // 委托 17-content-hub::harvest（唯一收口）
pub async fn history(&self, q: Option<&str>, limit: u32) -> Result<Vec<Translation>>;
pub async fn favorite(&self, tx_id: &str, on: bool) -> Result<()>;
pub fn highlight_learned(&self, text: &str, learned: &HashSet<SenseRef>) -> Vec<(usize,usize,bool)>;  // 已学词高亮
pub async fn to_practice(&self, tx_id: &str) -> Result<Question>;                                  // 工作台句→Translation/Rewrite 练习（tag from:tx）
```

## 3. 数据流

- 出：llm_cache（共用五方法 §缓存共用）→ `translations/tx_favorites` 落库；
- 入：harvest → 08-adaptive 卡 / 12-notes-profile 笔记 / 10-writing 模板 / 09-vocabulary 句式（全走单写者）；
- 事件：my_turn 评分复用 `AttemptGraded`，无新事件。

## 4. 验证点

- [ ] 缓存命中（同文本同方向不重复调 LLM）；glossary 术语一致；
- [ ] my_turn 差异对齐（中英/中日 fixture）；非英语评分降级路径；
- [ ] 四类 harvest 落库正确（单写者不变）。
