# 17 · content-hub — 素材中枢 + OCR + 生态导入 + 系统级取词（M10 + M13 + M15）

> 实现顺序：**M10（素材中枢/OCR）→ M13（导入导出）→ M15（全局取词）**。并入来源：旧 `24-material-hub` + `23-ocr` + `33-importers` + `46-global-lookup`。
> **数据共用五方法**：素材只存一份，派生全部注册。`harvest` 唯一收口（00-overview §5）。

## 1. 素材中枢（核心）

```rust
pub struct SourceDoc { doc_id, kind: Text|OcrImage|Audio|Transcript|WebClip|Book, lang, body, media, provenance, rights: UserOwned|FairUse|Unknown, hash }
pub struct MaterialRef { doc_id, span: Option<(usize,usize)>, lang }   // 引用不复制：词/句/段级
pub struct Derivation { src: MaterialRef, kind: Translate|Questions|EssayPrompt|TypingLines|Listening|Note|SrsCard, target_id, params_hash }

impl MaterialHub {
    ingest_text / ingest_image(→OCR 流水线) / ingest_audio(→ASR 孪生 doc) / clip_web / span_of
    usage_of(doc_id) -> Vec<Derivation>     // 素材被谁用了
    rebuild(doc_id, kind)                   // 素材修正→按 derivations+params_hash 重派生
    drop(doc_id, cascade)                   // 默认 orphan（学习数据不丢）/ 级联删
    // 派生函数（执行后写 derivations）：derive_translation / derive_questions / derive_essay_prompt / derive_typing_lines / derive_listening / harvest(to: Wordbook|Note|Template|Pattern)
}
```
共用五方法：①单份存储（内容寻址）②span 引用③派生注册（可追溯/rebuild/orphan）④缓存共用（llm_cache key 含素材 hash）⑤params_hash（同参数派生复用）。TTS 音频=`media_assets`（hash(text+voice)）同句多题只合成一次。

## 2. OCR（图片→文本→素材）

```rust
pub trait OcrEngine { fn langs(&self) -> Vec<LangTag>; async fn recognize(&self, img, opts) -> Result<OcrRaw>; }
// PaddleOcr | TesseractOcr | VisionLlmOcr（手写/公式/复杂版面兜底）| SystemOcr
impl OcrService {
    ingest_image（EXIF 纠正/切块）→ recognize（多引擎按块 conf 加权投票）→ postprocess（段落/表格→Markdown/行尾连字合并）
    → proofread_marks（conf<θ 高亮校对，修正回流，原 OCR 留 provenance）→ finalize（→ hub ingest 唯一落库口）
    llm_rescue（低置信块兜底，diff 可查）
}
```

## 3. 生态导入导出

```rust
pub trait Importer { fn kind(&self) -> ImportKind; fn sniff(&self, bytes) -> bool; async fn parse(&self, bytes) -> Result<ImportBatch>; }
// AnkiApkgImporter（deck→level_tags，复习史→FSRS 初始态）| CsvImporter（墨墨/百词斩/Quizlet profile；熟词→FSRS 高稳定态跳过初级复习）| WebClipper（clip_web）
pub async fn import(&self, batch) -> Result<ImportReport>;   // 幂等 upsert；batch_id 可整体回滚
pub fn export_anki_csv(&self, scope) -> Result<Vec<u8>>;      // 迁出不锁人
pub fn export_wrongbook_pdf(&self, track, range) -> Result<Vec<u8>>;   // 错题本打印（错题+错因+正解+讲解摘要）
pub async fn clip_endpoint(self) -> Router;                   // loopback POST /clip（浏览器扩展）
```

## 4. 系统级划词取词（全局捕词网）

```rust
impl GlobalLookup {
    lookup(sel, ctx: Option<&str>) -> Result<LookupCard>;   // 义项+谐音梗(19)+发音；ctx 空=纯本地零出站，ctx 非空（显式）→ 11-translate::lookup
    harvest(sel) -> Result<SenseRef>;                       // 委托 hub::harvest（幂等：重复划词不重复建卡）
    capture_log(since) -> Result<Vec<Capture>>;             // 默认只存词；capture_span 开启才存句入 hub
}
// 系统托盘 + 悬浮球/热键（辅助功能授权用途明示）；端点复用 13-widget 的 loopback 服务；选区语言嗅探→LanguagePack
```

## 5. 验证点

- [ ] 同素材多派生存一份（source_docs+1/derivations+n）；修正→rebuild 更新且作答保留；drop orphan/级联两策略；
- [ ] 缓存/资产共用调用计数断言；OCR 多引擎投票 fixture、表格→MD、连字合并、校对回流、失败无半写；
- [ ] apkg/CSV 解析 fixture、幂等导入、熟词不进新词队列、batch 回滚；PDF 内容快照；
- [ ] 取词零出站断言（无 ctx）、harvest 幂等、中英混排提取、停用监听无事件。
