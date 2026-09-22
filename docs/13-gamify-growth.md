# 13 · gamify-growth — 游戏层 / 成长展示 / 对战 / 小组件 / 年报 / 家长（M6 + M11-M16）

> 实现顺序：**M6（游戏底座）→ M11-M16（社交/展示/对战/家长）**。并入来源：旧 `11-gamify` + `35-growth-social` + `40-family-console` + `42-daily-widget` + `43-annual-report` + `49-quiz-battle`。
> 游戏结算唯一实现 `round_settle`（00-overview §5）。原则（总纲 §1.6）：只做反馈，不影响排程/评分；可整体关闭。

## 1. 游戏层（事件订阅）

```rust
pub enum GameEffect { Xp(i32), Coins(i32), ComboUp, ComboBreak, StreakAdvanced, StreakFreezeUsed, UnlockBadge(BadgeId), GiveItem(ItemId,u32), MapProgress(NodeId), Notify(..) }
pub fn on_event(&self, ev: &DomainEvent) -> Vec<GameEffect>;   // 事件→效果映射表；apply 幂等（event_id 去重）
pub fn round_settle(&self, r: &RoundResult) -> Vec<GameEffect>;   // 所有游戏/对战统一结算口
// 进度：level_of(xp) / streak_update(Advance|FreezeUsed|Reset 从轻) / check_badges（词汇里程碑/错题清零/模考上岸/盲打通关/写作连优）
// 学习地图：node_status（知识点=关卡，错题变式=复仇战，模考=Boss）/ boss_round(paper)
// 小游戏：memory_match(SRS 队列) / combo_sprint / root_composer / boss_battle——玩的就是复习
// 经济/健康：use_item(Hint|Eliminate|DoubleXp|StreakFreeze|Makeup) / effort_score(学习量0.6+进步0.4，防劝退排行) / playtime_guard(每日上限/久坐/未成年人脱敏)
```
表：`game_state/game_events`（事件溯源，可重放）。

## 2. 成长展示（原 35）

```rust
pub fn learning_calendar(&self, year) -> Result<HeatCalendar>;        // 年历热力（TUI 字符/Web 色块）
pub fn essay_portfolio(&self, track) -> Result<Vec<(Essay, EssayGrade)>>;
pub fn rewrite_diff(&self, old, new) -> Result<DiffPoint>;            // 同题重写对比
pub fn share_card(&self, kind: VocabSize|BandUp|Streak|MockScore|Badge) -> Result<ShareCard>;  // 只含汇总+标语（隐私审计）
// 共学：room_join（WS 房间：在线人数/番茄钟/静默自习）/ bee_weekly（Spelling Bee 周赛，努力分排行）
// 无 server_url 降级：单人番茄钟 + 本地人机挑战
```

## 3. 家长控制台（原 40，家长能力唯一归属）

```rust
pub async fn weekly_brief(&self, child) -> Result<FamilyBrief>;      // 大白话亮点/薄弱/一条建议
pub async fn quiz_cards(&self, child, n) -> Result<Vec<FamilyQuiz>>; // 亲子问答卡（本周新词→口语提问脚本）
pub async fn quiz_audio(&self, q) -> Result<Audio>;                  // TTS 口播（15-speech，资产共用）
pub async fn goals(&self, child) -> Result<Vec<Goal>>;
pub async fn push_weekly(&self, parent) -> Result<()>;               // 邮件/Webhook；数据=weekly_brief 摘要
// 边界：只读脱敏（无日记/笔记正文，只到知识点粒度）；grant_parent 授权+审计，撤回即停 403；无服务端=本机同屏
```

## 4. 桌面/锁屏小组件（原 42）+ 年报（原 43）

```rust
pub async fn today_card(&self) -> Result<WidgetCard>;    // 每日一词+谐音梗轮换（错词>到期>新词）；离线预取
pub fn flip(&self, card, known: bool) -> Result<()>;     // 轻反馈→08-adaptive::schedule
pub async fn countdown(&self) -> Result<ExamCountdown>;
pub fn widget_page(&self) -> Router;                     // loopback 端点（46 复用）；锁屏只显词卡（脱敏）
pub async fn annual_report(&self, year) -> Result<AnnualReport>;  // 数字与12 同源聚合；fun_facts 只喂汇总数字
pub async fn unwrap_page(&self, year) -> Result<Unwrapped>;       // 分步炫页（Web 动画/TUI 文本）
pub async fn share(&self, year) -> Result<ShareCard>;              // 复用 share_card
```

## 5. 实时对战（原 49）

```rust
pub async fn quick_match(&self, friend: Option<&UserId>) -> Result<BattleRoom>;   // 双方 weak_points 交集选题 + CAT θ 平衡
pub fn battle_items(&self, both: &[AbilityEstimate]) -> Result<Vec<Qid>>;
pub async fn play(&self, room, ans) -> Result<RoundTick>;   // 规则秒判/Jev 异步补；15s/题、乱序防窥屏
pub async fn settle(&self, room) -> Result<BattleResult>;   // 结算委托 round_settle
pub async fn homework_check(&self, h: &BattleHomework) -> Result<()>;   // 输方出题义务（06 qc 必过）
pub async fn bot_battle(&self) -> Result<BattleResult>;     // 无服务端降级
```

## 6. 验证点

- [ ] 事件→效果映射表全变体；游戏关闭零写入；effort_score 防劝退用例；streak 边界；未成年人脱敏；
- [ ] 分享卡隐私审计；房间消息短 TTL；家长无授权 403 + 断言无正文；widget 锁屏脱敏/反馈进 FSRS/离线可播；
- [ ] 年报数字对账 + fun_facts 零隐私；对战交集选题/乱序限时/homework 过 qc/降级可玩。
