# 08 · adaptive — 记忆调度（FSRS）+ 难度自适应（CAT）+ 假对（M4 + M15）

> 实现顺序：**M4（FSRS/假对）→ M15（CAT）**。并入来源：旧 `07-srs` + `44-cat`。
> 两个正交自适应引擎：FSRS 管**什么时候复习**，CAT 管**出多难**。排程唯一实现 / 难度唯一实现（00-overview §5）。

## 1. FSRS（crates/srs）

```rust
pub struct Card { pub card_id: CardId, pub source: CardSource, pub fsrs: FsrsState, pub mode: ReviewMode /*choice|fill|spell|compose*/,
                  pub wrong_count: u32, pub guessed_count: u32, pub due_at, pub created_at }
pub enum SrsRating { Again, Hard, Good, Easy }

impl SrsService {
    pub async fn on_attempt(&self, a: &Attempt, g: &GradeResult) -> Result<Option<CardId>>;  // 入卡/加重/假对入卡；Pending 裁决后补执行
    pub async fn schedule(&self, card_id: &CardId, rating: SrsRating) -> Result<Card>;
    pub async fn due_queue(&self, now, limit: u32, interleave: bool) -> Result<Vec<ReviewItem>>;  // 交错（同知识点不相邻）
    pub async fn bump_wrong_word(&self, sense_id: u64, tag: &ErrorTag) -> Result<CardId>;          // 错词本提权
    pub async fn enroll_words(&self, senses: &[SenseRef], daily_new: u32) -> Result<Vec<CardId>>;
    pub fn spelling_material(&self, now, limit: u32) -> Result<Vec<PracticeLine>>;                 // 到期词→04-typing 拼写素材
    pub fn weekly_quiz(&self) -> Result<Vec<Question>>;                                            // 测验效应小测
}
// on_attempt：Wrong/Partial→建卡+Again；Correct 且假对→建卡（guessed_count+1）；Correct 非假对→已有卡按 Good
pub fn next_mode(history: &[ReviewRecord]) -> ReviewMode;      // 连续 2 次 Good/Easy 升档；Again 降档
pub fn build_review_question(item: &ReviewItem, mode: ReviewMode) -> Question;  // choice→fill→spell→compose 检索难度递进
pub fn interleave(items: &mut Vec<ReviewItem>);
```

## 2. 假对检测（猜对 ≠ 会做）

```rust
pub fn is_guess(t: &Timing, self_conf: Option<u8>, b: &UserBaseline) -> bool;
// ①秒选（<p50×0.3 且零修订）②久犹豫低自评（>p90 且 conf≤2）③自评 1；baseline=近 200 次滚动
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

- [ ] FSRS 数值快照（fsrs-rs 参考输出）；假对三形态表驱动；模式升降档边界；interleave 不相邻；
- [ ] CAT 模拟作答收敛 0.7±0.1（proptest）；floor/ceiling 不出轨；假对折扣断言；维度与全局不震荡。
