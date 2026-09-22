# 18 · learning-loop — 语法树 / AI 私教 / 讲解练习化 / 每日流 / 精听电台 / 学情预警（M11-M16）

> 实现顺序：**M11（语法树/AI 私教）→ M12（每日流）→ M13（讲解练习化）→ M15（精听电台）→ M16（预警）**。
> 并入来源：旧 `30-grammar-tree` + `31-ai-coach` + `38-practice-loop` + `34-study-loop` + `45-listening-radio` + `50-early-warning`。
> **每日流编排唯一入口**（00-overview §5）：各"每日 X"都是本册供料环节。

## 1. 语法体系树 + 微课（原 30）

```rust
pub struct GrammarNode { id: NodeId /*tense.present_perfect*/, name_zh, parent, prereqs, track_tags, error_tags }
pub struct MicroLesson { node, minutes: u8, explain_l1, examples, drill: Vec<Question> }
tree(lang)（拓扑校验）/ next_node(mastery)（前置满足+错因加权）/ micro_lesson(node, track)（LLM 讲解缓存+06 出 3 题）
mastery_of(node) / weak_branch(heat)   // 错因热力→节点（ErrorTag↔node 映射全覆盖语法类）
```
学习地图关卡（13）= GrammarNode。

## 2. AI 私教面板（原 31，含讲解口播）

```rust
pub struct CoachCtx { weak_points, recent, today_plan, sel }   // 摘要化：不发 diary/笔记全文（隐私红线）
pub struct CoachReply { text_l1, actions: Vec<CoachAction>, refs }   // actions=可执行按钮 deep link（03 门面）
ask(q, ctx) / daily_plan()（=12::interleaved_plan + 教练口吻解释）/ weekly_review() / explain_orally(reply)（15 TTS，资产共用）
```

## 3. 讲解练习化 + 习惯日课（原 38）

```rust
pub struct ExplainGame { attempt_id, explain: Explanation, plays: Vec<PlayItem> }   // 讲解变 3 个"下一题"
pub enum PlayItem { FillWrongSpan{…}, PickFix{…}, Reorder{…} }
playful_explain(attempt)（按错因选 play）/ settle(g, ans)（07 规则/Jev 复用；玩错→08 FSRS 掉档，玩对→发 AttemptGraded）
pub enum HabitTag { TenseDrift, SvaLapse, ArticleDrop, ChinglishReflex, RunOn, WordOrder }
habit_profile()（error_tags 时序共现挖习惯）/ habit_daily(habit)（5 题纠错：chinglish 库+用户历史错句；习惯消退结课+徽章）
```

## 4. 每日流编排（原 34）+ 精听/私人电台（原 45）

```rust
// StudyLoop = 每日流唯一编排：精读 → 精听 → 电台；微复习/日课/今日词卡/每日方案都是供料环节
daily_reading(track) -> ReadingPiece;      // i+1（超纲词密度≤5%）；点击查词→11/08 沉淀；读后 3 题
daily_listening(track) -> ListeningPiece;  // data/listening/<lang>/；语速 0.75x–1.2x；听力理解+听写（04 输入）
today_episode() -> RadioEpisode;           // 今日错词/到期词→LLM 双主持对话脚本→qc（目标词全出现）→TTS 双音色（资产共用）；尾部 3 词快问快答
best_review_time(user) / micro_review(now) // ≤5 分钟微复习（FSRS due×活跃时段；推送 opt-in；未成年人晚间静默）
sprint_mode(track, exam_date) -> SprintPlan;   // 30/7/1 作战室：清单+模考节奏+错题清零
exam_strategy_drill(track) -> StrategyDrill;   // 时间分配模拟/放弃决策/蒙题率分析（复用假对数据）
```

## 5. 学情预警（原 50）

```rust
pub enum WarningKind { ForgettingRisk{card}, WeakSpotRisk{node}, ExamRisk{area}, StreakRisk }
forgetting_risk(horizon)（FSRS stability 衰减外推 <0.7）/ weak_spot_risk()（趋势外推 Rising×密度）
exam_risk(track)（调用 12::predict_score 分区外推——估分唯一在 12）/ streak_risk()
intervene(w) -> CoachAction（→31 下发，deep link 03）/ dismiss(id, wrong)（误报反馈调参）
// 红线：只给趋势区间不给断言；宁漏报；basis 证据链可解释；同节点每日至多 1 条；可整体关闭
```

## 6. 验证点

- [ ] 语法树拓扑无环/映射全覆盖/推荐不越前置；coach 出站无 diary 正文、daily_plan 与 12 一致、口播资产复用；
- [ ] play 与错因对应、玩错掉档断言、习惯识别/消退稳定、日课不超预算；
- [ ] i+1 难度断言、推送 opt-in、冲刺不超预算、电台目标词全出现/断网可播；
- [ ] 遗忘预测与 FSRS 理论一致（proptest）、误报率<20%（回放）、Warning 可解释、intervene deep link 有效。
