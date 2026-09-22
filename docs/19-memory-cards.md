# 19 · memory-cards — 记忆材料：谐音梗 + 词源文化故事（M11 + M14）

> 实现顺序：**M11（谐音梗）→ M14（词源故事）**。并入来源：旧 `29-mnemonic` + `41-etymology`。
> 两者挂**同一张 SRS 网**交错复习；记忆法=联想锚，词源=文化锚。生成必过质检（拦编造/低俗）。

## 1. 谐音梗记忆法（品牌梗产品化：Ambition = 俺必胜）

```rust
pub struct Mnemonic { id, sense: SenseRef, kind: Homophone | Story(联想故事) | Acronym /*词根/词源类归 §2*/,
                      text_l1, phonetic_hook: "俺必胜", author: Llm(model)|User(id), votes, quality }
pub struct MnemonicService;
generate(sense) -> [3 候选]      // 谐音优先；缓存 key=hash(sense+lang)
qc(m) -> QcReport                // Jev noul×2：pronunciation_hook_valid / content_appropriate
pick(sense, id)                  // 选用→入 SRS 卡背 / submit(sense, text, kind) 用户投稿（同样过 qc）
best_for(sense)                  // 本人已选 > 票选最高 > 重新生成
share(ids) -> PackRef            // 谐音梗合集 → 14-cloud qpack 广场
```
表 `mnemonics`；联动：09-vocabulary 词书、04-typing 拼写提示、18 钩子引用；记忆法答对标 `mnemo_used` 统计命中率。

## 2. 词源文化故事卡（原 41）

```rust
pub struct EtymologyCard { id, kind: Myth(神话) | Root(词根) | History(词史), title_l1, story_l1 /*100-200 字*/,
                           pivot_word: "echo", derived: ["echolocation"], audio: Option<MediaRef> }
pub struct EtymologyService;
story_for(lemma) / story_tree(root)（muse→music/museum/amuse 族谱串讲）/ daily_story(track)（接 18 每日流）
qc(c)（Jev noul `etymology_accurate` 拦编造/张冠李戴）/ to_card(c)（入同一 SRS 网，与谐音梗卡交错）/ story_audio(c)（15 TTS 资产共用）
```
表 `etymology_cards`；数据源：09-vocab roots + LLM 补充 + 用户标注。

## 3. 记忆材料边界（00-overview §6）

| 内容 | 归属 |
|---|---|
| 谐音/联想/缩写记忆法 | 本册 §1 |
| 真实词源/神话/词史/词根笔记 | 本册 §2 |
| 词根词缀**数据**（查询/构词） | 09-vocabulary |
| 卡片排程 | 08-adaptive（CardSource 复用） |

## 4. 验证点

- [ ] 谐音候选"发音联想成立"人工基准 20 例；低俗内容拦截；生成缓存命中；票选排序稳定（同票按时间）；
- [ ] 词源真实性抽检 ≥30 例（qc ≤0.7 丢弃）；族谱成员与 09 roots 一致；
- [ ] 两类卡复习队列交错（interleave 断言）；口播音频资产复用。
