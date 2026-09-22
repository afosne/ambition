# 12 · notes-profile — 笔记 / 日记 / TODO + 画像 / 估分 / 根因溯源（M6 + M16）

> 实现顺序：**M6 → M16（溯源）**。并入来源：旧 `13-notes` + `12-profile` + `48-traceback`。
> 估分唯一实现（00-overview §5）：18-learning-loop 预警调用本册 `predict_score`。

## 1. 笔记 / 日记 / TODO

```rust
pub struct Note { note_id, kind: Word|Question|Grammar|Phrase|Free, title, body_md, links: Vec<LinkRef>, created_at, updated_at }
pub enum LinkRef { Word(u64), Qid(Qid), Grammar(String), Essay(EssayId), Note(NoteId) }   // 双链：词↔题↔语法点↔作文弱点↔素材
pub struct DiaryEntry { entry_id, date, body, review: Option<DiaryReview> }
pub struct Todo { todo_id, kind: Daily(DailyPlan)|Review(CardId)|Drill(String)|Writing(EssayPrompt)|ExamPrep, title, due, status, meta }

// upsert（md 导出 + front-matter）/ backlinks / search(FTS) / to_srs_card / export_md
// review_diary：LLM 鼓励式批改（只改错 + ≤2 升级 + 一句鼓励）；隐私红线：仅请求批改时外发 body（HTTP 断言）
// link_new_words / plan_for_exam（倒计时排期，<30 天切模考周）/ daily_checklist / complete（发事件）
```

## 2. 画像 / 个性化（目标 10/11）

```rust
pub struct HeatMap { cells: BTreeMap<(ErrorTag, Tag), f32> }   // 错误密度 = Σ权重 × 时间衰减(0.97/周)
pub struct WeakPoint { tag, knowledge, density, trend: Rising|Stable|Falling }
pub async fn aggregate(&self, from, to) -> Result<ProfileSnapshot>;
pub fn weak_points(&self, heat, top) -> Vec<WeakPoint>;
pub async fn refresh(&self) -> Result<()>;                      // 写 profile_snapshots
pub fn gen_spec_for(&self, budget) -> GenSpec;                  // 弱点→GenSpec.weak_points/题型轮换（06 消费）
pub async fn vendetta_queue(&self) -> Result<Vec<Qid>>;         // 复仇战队列（变式连对 2 次出队）
pub fn interleaved_plan(&self, daily: DailyBudget) -> DailyPlan;
// 写作加强（目标 11）：
pub fn writing_weaknesses(grades: &[(Essay, EssayGrade)], min_count) -> Vec<WritingIssue>;
pub async fn plan_writing_drills(&self, issues) -> Result<Vec<Drill>>;   // → 10-writing 执行 → DrillCleared → 复批
// 周报/导出/估分：
pub async fn weekly_report(&self, week) -> Result<WeeklyReport>;
pub async fn export_report_md(&self, r) -> Result<String>;
pub fn predict_score(&self, runs: &[PaperRun], target: &TrackId) -> f32;  // 估分唯一实现（样本<5 给区间）
pub fn vocab_growth(&self, from, to) -> Vec<(Date, u32)>;
```

### 2.1 遥测/聚合算法（离线实现：全本地、零网络、零 LLM）

> “遥测” = 从本地事件流（attempts/grades/error_tags/typing/…）算出画像。全部是 **SQL 聚合 + 纯函数**，单机/断网完整可用；云端看板只是**从 sync 事件重放同一套函数**（14-cloud analytics）。

**A. 错误密度热力图**
```
density(tag, node) = Σᵢ wᵢ · λ^(Δweeksᵢ)
// i = 该 (tag,node) 的每次错误；w = 判分权重（Partial 0.5 / Wrong 1.0）× error_weights(track)
// λ = 0.97/周（指数时间衰减）：8 周前的错 ×0.79、半年前 ×0.45 —— “最近总错”浮顶
```

**B. 趋势判定**（Rising/Stable/Falling）
```
trend = slope(近 4 周 density 周序列)；|slope|<ε → Stable；slope>ε 且密度升 → Rising（进 18-learning-loop 预警）
```

**C. 假对检测**（08-adaptive 同源口径）
```
is_guess = (t < p50×0.3 ∧ 零修订) ∨ (t > p90 ∧ 自评≤2) ∨ (自评==1)
// baseline = 近 200 次作答的延迟分位数（滚动），冷启动用全局默认
```

**D. CAT 能力估计**（Elo 简化 IRT，08-adaptive）
```
E = 1/(1+10^((d-θ)/4))            // 期望正确率（d=题难度归一化）
θ' = θ + k·(o-E)·g                // o=结果(1/0.5/0)；k 步长退火(0.4→0.1)；g=假对折扣 0.5；dims[tag] 小步长同步
```

**E. 遗忘预测**（预警输入）：`R(t) = exp(-t/S)`（S=FSRS stability）；`R<0.7` → ForgettingRisk。

**F. 估分**：`score = α·mean(近 5 套卷) + (1-α)·回归(练习量→分数)`；样本 <5 只给区间。

**G. 词汇量测试**：staircase 近似 1PL（连续对→降频升难，连续错→降难；30 题收敛；θ→词频段映射=词量）。

**H. 工程遥测**（可观测性）：`tracing` span（一次判题=1 span，含 jev/llm 调用计数）+ `llm_usage` 记账，全落本地 SQLite；崩溃/错误上报**默认关闭**（opt-in）——离线时无任何出站流量（这也是隐私验证点）。

## 3. 错题溯源（原 48，18 的语法树消费方）

```rust
pub struct RootCauseChain { qid, tags, nodes, root: NodeId, evidence: Vec<Evidence> }   // 可解释
pub fn root_cause(&self, qid: &Qid) -> Result<RootCauseChain>;      // tag → 节点 → prereqs 上溯 → 最上游薄弱前置
pub fn causal_report(&self) -> Result<Vec<(ErrorTag, NodeId, f32)>>; // "先修什么收益最大"
pub fn fix_first(&self) -> Result<GrammarNode>;                      // 上游前置优先（18-next_node 增强）
pub fn root_drill(&self, chain) -> Result<Vec<Question>>;            // 修前置节点（不是同型题！）
pub fn is_fake_root(&self, chain) -> Result<bool>;                   // 假根因 → 18-habit 日课分流
```

## 4. 数据流 / 单写者

写 `notes/diary/todos/profile_snapshots`（唯写）；读 attempts/essay_grades/derivations（素材利用统计）。
出：GenSpec 权重、DailyPlan、Drill、warnings 原料、14-cloud 看板（同一套聚合函数从 sync 事件重放）。

## 5. 验证点

- [ ] md 导出/再导入无损；diary 隐私断言（无调用不出站）；倒计时计划全覆盖；
- [ ] 时间衰减聚合趋势方向正确；**断网环境完整出画像**（零出站断言）；λ 衰减数值（半衰期）断言；writing_weaknesses 聚类去重；复仇战出队；周报与手工 SQL 一致；
- [ ] 根因链无环、root_drill 题目 tag 指向前置节点、假根因分流。
