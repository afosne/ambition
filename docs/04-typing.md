# 04 · typing — 打字通「键胜」（M1）

> 实现顺序：**M1**（零 AI 依赖，先行攒人气）。并入来源：旧 `09-typing`。
> 打字 = 拼写回忆 + 键盘技能 + 应试输入能力。纯逻辑 crate，UI 渲染在 03-engine-ui。

## 1. 类型

```rust
pub struct KeyStroke { pub key: char, pub t_ms: u64, pub expected: char, pub correct: bool, pub corrected: bool }
pub struct TypingSession { pub session_id: String, pub mode: TypingMode, pub material: Vec<PracticeLine>,
                           pub keystrokes: Vec<KeyStroke>, pub stats: TypingStats }
pub enum TypingMode { Lesson(u8), Drill(DrillKind), Copy(ModelText), SpellFromZh, Game(GameKind), MockInput }
pub struct PracticeLine { pub id: String, pub target: String, pub hint_zh: Option<String>, pub source: LineSource }
pub enum LineSource { SrsSense(SenseRef), WordList(String), ModelEssay(EssayId), UserNote(NoteId), Dictation(String), Material(MaterialRef) }
pub struct TypingStats { pub wpm: f32, pub cpm: f32, pub accuracy: f32, pub rhythm_var: f32, pub first_key_ms: u64 }
pub struct HeatMap([u32; 128]);   // 按期望字符的错误计数
```

## 2. 指标（纯函数）

```rust
pub fn stats(ks: &[KeyStroke]) -> TypingStats;      // wpm=correct/5/min；rhythm_var=击键间隔变异系数
pub fn heat(ks: &[KeyStroke]) -> HeatMap;
pub fn bottleneck_keys(h: &HeatMap, threshold: u32) -> Vec<char>;
pub fn drill_for_keys(keys: &[char], words: &[String]) -> Vec<PracticeLine>;
pub fn baseline_wpm(history: &[TypingSession]) -> f32;
```

## 3. 课程与素材

```rust
pub struct Lesson { pub id: u8, pub key_zone: KeyZone /* HomeRow→…→All 分级解锁 */, pub lines: Vec<PracticeLine>, pub pass: PassCriteria }
pub fn next_lesson(progress: &CourseProgress) -> Option<Lesson>;          // 8 级，WPM+准确率双条件
pub fn ime_warning(typed: &str) -> Option<String>;                        // 全角/输入法提示
pub fn keyboard_layout(lang: &LangTag) -> KeyboardLayout;                 // 多语种键位（16-config-i18n）
pub fn material_bundle(opts: MaterialOpts) -> Vec<PracticeLine>;          // ①SRS 到期词拼写 ②词表/课文 ③范文临摹(10-writing) ④笔记/日记 ⑤素材中枢选区
```

## 4. 游戏接口与落库

```rust
pub struct ShootingRound { pub falling: Vec<FallingWord>, pub target: String }   // 打字射击（13-gamify-growth 消费）
pub fn shooting_round(words: &[String], speed: f32) -> ShootingRound;
pub fn round_result(ks: &[KeyStroke], r: &ShootingRound) -> RoundResult;
```
- 会话结束写 `typing_sessions`（击键流可回放）→ 发 `DomainEvent::TypingFinished`；
- 听写/作文模考输入复用本引擎组件（07-grading Dictation / 10-writing 卷面计时共用）。

## 5. 验证点

- [ ] stats 数值测试（已知序列→期望 WPM/准确率）；热力/瓶颈提取；课程解锁状态机；
- [ ] material_bundle 优先级（SRS 最前）；全角/IME 提示触发；多语种键位切换。
