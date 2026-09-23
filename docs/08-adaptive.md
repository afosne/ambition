# 08 · adaptive — 记忆调度（FSRS）+ 难度自适应（CAT）+ 假对（M4 + M15）

> 实现顺序：**M4（FSRS/假对）→ M15（CAT）**。并入来源：旧 `07-srs` + `44-cat`。
> 两个正交自适应引擎：FSRS 管**什么时候复习**，CAT 管**出多难**。排程唯一实现 / 难度唯一实现（00-overview §5）。

## 1. FSRS（crates/srs）

> **M4 落地**：08 是**纯函数库**（无 IO，不依赖 store）；取卡/落库/事件编排在 03-engine（`due_list` / `review_answer` / `wrong_wordbook`）。SrsService 签名废弃，以纯函数代之（排程唯一实现仍是 08，00-overview §5）。

```rust
pub struct FsrsState { pub stability: f32, pub difficulty: f32 }   // fsrs-rs MemoryState 持久化视图（Card.fsrs=None = 新卡未复习）
pub struct Card { pub card_id: CardId /*source 内容寻址 = 幂等入卡*/, pub source: CardSource, pub fsrs: Option<FsrsState>,
                  pub mode: ReviewMode /*choice|fill|spell*/, pub wrong_count: u32, pub guessed_count: u32,
                  pub error_tags: Vec<ErrorTag> /*错词本错因标签*/, pub ok_streak: u32 /*升档连续计数*/,
                  pub due_at: i64 /*unix 秒*/, pub last_review_at: Option<i64>, pub created_at: i64 }
pub enum Rating { Again, Hard, Good, Easy }                      // 即原 SrsRating；as_str 与 ReviewDone.rating 同口径
pub struct ReviewItem { pub card: Card, pub front: String, pub back: String, pub group: String /*交错分组=知识点*/ }

pub fn new_card(source: CardSource, now: i64) -> Card;                        // 新卡 due_at=now：错词/假对立即进队列（提权）
pub fn schedule(card: Card, rating: Rating, now: i64) -> Result<Card, SrsError>;  // fsrs-rs next_states（desired_retention=0.9）；mode 按 next_mode 升降
pub fn due_queue(items: Vec<ReviewItem>, now: i64, limit: u32) -> Vec<ReviewItem>; // 到期过滤 → 排序 → 交错 → 截断
pub fn interleave(items: &mut Vec<ReviewItem>);                              // 交错：同 group 尽不相邻（组耗尽后尾部相邻不可避免）
pub fn decide_action(verdict: Verdict, is_guess: bool, has_card: bool) -> CardAction;  // 作答→处置（M1 起既有）
pub fn next_mode(ok_streak: u32, failed_last: bool) -> ReviewMode;           // 连续 2 次 Good/Easy 升档；Again 降回 choice
// 编排（03-engine on_attempt）：Wrong/Partial→建卡/提权 + wrong_count + 错因标签；Correct 且假对→建卡（guessed_count+1）；
//   Correct 非假对→已有卡按 Good（schedule）；Pending→不动（裁决后 resolve_pending 补执行）
// 后续（M5+）：build_review_question 检索递进组题、weekly_quiz 测验效应、spelling_material→04-typing
// FSRS 数值快照（fsrs-rs 6.6.2 默认参数，DR=0.9，新卡第 0 天）：again(0.212/6.4133) hard(1.2931/5.1122) good(2.3065/2.1181) easy(8.2956/1.0)
```

## 2. 假对检测（猜对 ≠ 会做）

```rust
pub fn is_guess(t: &Timing, self_conf: Option<u8>, b: &UserBaseline) -> bool;
// ①秒选（<p50×0.3 且零修订）②久犹豫低自评（>p90 且 conf≤2）③自评 1；baseline=近 200 次滚动
// M4 落地：UserBaseline = core::baseline_from_timings(近 200 次 total_ms) 滚动 p50/p90；空历史用默认 15s/60s（防冷启动误判）；
// engine 在 insert_attempt **前**取样（不含本次）→ is_guess → decide_action（与 core 三形态测试共口径）
```

## 3. CAT（Elo 式可解释难度估计）

```rust
pub struct AbilityEstimate { pub track: TrackId, pub theta: f32 /*-3..3*/, pub dims: BTreeMap<Tag, f32>, pub confidence: f32 }
impl CatEngine {
    pub fn update(&self, est: &mut AbilityEstimate, item_difficulty: f32, outcome: Outcome);  // 假对按"实际低于表象"折扣
    pub fn next_difficulty(&self, est: &AbilityEstimate) -> (u8, u8);   // 喂 GenSpec.difficulty（唯一来源；39 DSL 可锁定）
    pub fn pick_from_pool(&self, est, pool: &[Question]) -> Option<Question>;   // 难度最近邻 + 知识点加权
    pub fn flow_report(&self) -> FlowStats;    // 心流命中率（目标 70%±10%，偏离调步长）
    pub fn cold_start(&self, track: &TrackId) -> AbilityEstimate;      // 词汇量测试 + 近 N 题锚定（16-config 首启向导 step_placement 调用）
}
```

## 4. 协作

- 06-generation：difficulty/distractor 强度；04-typing：spell 档素材；03-engine：review_answer 后 schedule + mode 升降；
- 12-notes-profile：假对/错词进画像；13-gamify-growth：ReviewDone 事件；18-learning-loop：微复习时机 + 预警读 stability；
- 表：`srs_queue`、`cat_state`（单写者见 00-overview §4）。

## 5. 验证点

- [x] FSRS 数值快照（fsrs-rs 6.6.2 参考输出，test_schedule_matches_fsrs_snapshot）
- [x] 假对三形态表驱动（core test_is_guess_covers_three_forms + baseline 冷启动默认）
- [x] 模式升降档边界（test_next_mode_streak_escalation_and_reset + schedule 内升/降档）
- [x] interleave 不相邻（2A2B/3A3B 无相邻；单组尾部相邻已注记）
- [ ] CAT 模拟作答收敛 0.7±0.1（proptest）；floor/ceiling 不出轨；假对折扣断言；维度与全局不震荡（M15）。
