# 09 · vocabulary — 词库 / 义项 / 句式 / 深训练（M4 + M13）

> 实现顺序：**M4（词库/义项/句式）→ M13（幽灵义项/辨析卡）**。并入来源：旧 `08-vocab` + `20-patterns-polysemy` + `37-vocab-depth`。
> 边界：本册 = 词汇与句式的数据+训练；记忆材料（谐音/词源）在 19-memory-cards。
> **M4 落地（最小可用）**：`crates/vocab` 纯函数库——内置义项 `data/lexicon/en/senses.json` + 句式 `data/patterns/en/patterns.json`（根带 schema_version=1）加载与校验 + 经 `CardSource::WordSense/Pattern` 入 08 复习队列（engine.assemble 幂等入卡）。EC-DICT 全量导入 / LexiconSource 插件 / 五模式训练 / 词汇量测试 / 幽灵义项 随 M5+。

## 1. 词库数据（LexiconSource 插件，16-config-i18n）

```rust
pub struct Word { word_id, lemma, phonetic, freq, level_tags }   // level_tags：k12primary/k12junior/gaokao3500/cet4/cet6/kaoyan/ielts/toefl/work_*
pub struct Sense { word_id, sense_id, pos, def_zh, def_en, examples }   // 一词多义按义项拆行（bank₁/bank₂）
pub trait LexiconSource { fn lang(&self) -> &LangTag; async fn import(&self, path) -> Result<u32>; async fn lookup(&self, lemma, ctx) -> Result<Vec<LexEntry>>; async fn senses_of(&self, lemma) -> Result<Vec<Sense>>; }
// EcDictSource(en，37 万词) | 后续 ja/ko…；import_ecdict / import_wordlists / import_relations / split_senses（构建资源文件，幂等）
```
**存储**：词库 = **只读资源文件**（`data/lexicon/<lang>/`：EC-DICT 构建产物 JSONL + 哈希索引，mmap 懒加载），**不进 SQLite**；M4 内置义项文件 `data/lexicon/en/senses.json`（`word_id` = lemma 内容定名，EC-DICT 导入后换稳定 id；词库升级不破坏 srs 引用靠 CardSource 内容寻址）；无服务端/S3 时用本地 `data/`，配置后可从资源托管拉取（16-config-i18n §3.1）。
```
查询服务：`senses/word/confusables/synonyms/antonyms/collocations/roots_of/distractors`；词汇量自适应测试 `next_probe_item/estimate_size`（staircase 30 题收敛）。

## 2. 义项级训练（原 20 §2 + 37）

| 模式 | 说明 | 判分 |
|---|---|---|
| 义项辨析 / 回填 | 语境↔义项（干扰项=同词它义优先） | 规则/Jev |
| 熟词僻义 | `familiar_rare_sense` 专项（kaoyan） | 规则/Jev |
| 义项干扰对 | bank₁/bank₂ 交替快判 + 假对检测 | 规则 |
| 全义项清卡 | 一词所有义项过卡才"毕业" | 卡片汇总 |
| 幽灵义项（原 37） | 语境才激活的义项变体（bank→保险库）；`SenseVariant{trigger, def_l1, example, rarity}`；qc 拦生造义 | 规则/Jev |
| 近义辨析卡（原 37） | 语域×感情色彩×搭配三栏（small/little/tiny）+ 语境选词 | qc 唯一答案 |

```rust
pub struct PolysemyService;   // sense_items / rare_sense_queue / interference_pairs / sense_drill / graduated / mastery
pub struct VocabDepth;        // ghost_of / ghost_drill / enrich / synonym_card / cloze_for / contrast_drill
```
出题口全部走 06-generation `sense_question`（qc 拦语境歧义/双正确）；错义项单独加重（`word_sense`）不拖累已会义项。

## 3. 固定句式（原 20 §1）

```rust
pub struct Pattern { pub pattern_id, pub form: "not only {A} but also {B}", pub slots: Vec<SlotSpec>,
                     pub kind: PatternKind /*Connect|Emphasize|Invert|Subjunctive|Compare|…*/, pub level_tags,
                     pub note_zh, pub examples: Vec<ExamplePair> /*plain↔patterned*/, pub traps: Vec<Trap> /*典型误用*/ }
pub struct PatternService;   // library(track,kind) / next_drill(weak) / check_use(sentence,pid)【Jev noul×2】/ suggest_for(写作推荐) / pattern_of(text)【句式多样性统计】
```
训练五模式（tag `pattern:<id>`）：句式辨析 / 槽位填空 / 句式转换（Rewrite）/ 造句迁移（SRS compose 档）/ 错式改错（联动 16-config 的 chinglish 库）。写作侧：批改统计句式多样性 → 低则 `suggest_for` 推荐（目标 9 量化）。

**M4 内置**：`data/patterns/en/patterns.json`（根带 schema_version=1：kind 分类 + slots + note_zh + plain↔patterned 例对 + traps 误用反例）；句式卡以 `CardSource::Pattern` 入 08 队列（复习翻卡：正面=中文讲解+普通句，背面=句式+例对+traps）；五模式训练/check_use/suggest_for 随 M5+。

## 4. 数据与单写者

表：`words/word_relations/collocations/roots/patterns/sense_variants`（导入器写入，00-overview §4）。
数据文件：`data/wordlists/`、`data/patterns/<lang>/`、`data/lexicon/<lang>/senses.json`（M4 义项）、词源见 19。

## 5. 验证点

- [x] 义项拆分抽检（bank/light/run：每词 ≥2 义项、def 互异——M4 内置数据，test_builtin_senses_split_polysemy）
- [ ] EC-DICT 导入幂等；干扰项防双正确；
- [ ] 幽灵义项真实性（qc 拦生造义）；辨析卡三栏齐全；毕业判定（部分义项过 ≠ 毕业）；
- [ ] 句式 traps 反例判负（`*I suggest him to go`）；check_use 拦原意走样；pattern_of 与人工标注一致；词汇量测试收敛。
