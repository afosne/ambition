# 15 · speech — 语音引擎 + 口语陪练 / 口述作文（M8 + M11）

> 实现顺序：**M8（引擎/听写/跟读）→ M11（多轮陪练/口述作文）**。并入来源：旧 `17-speech` + `32-speaking-lab`。
> 分工（00-overview §5）：本册做**引擎与口语流程**；音频数据资产归 17-content-hub（hash 共用）；判分归 07。

## 1. 引擎适配（多后端）

```rust
pub trait Tts { async fn synth(&self, req: TtsReq{text, voice, speed}) -> Result<Audio>; }
pub trait Asr { async fn transcribe(&self, audio: &Audio) -> Result<Transcript{text, words: Vec<WordTiming>}>; }
// 后端：piper（本地离线）/ edge-tts（在线）；whisper.cpp（本地）/ 云 ASR；VoiceSet per LanguagePack（16-config-i18n）；phonics 仅 en
pub fn audio_asset_key(text: &str, voice: &VoiceStyle) -> String;   // 内容寻址 → media_assets（同句多题只合成一次）
```

## 2. 听力 / 听写 / 跟读 / 口语卡

```rust
pub async fn speak(&self, text: &str, style: VoiceStyle) -> Result<Audio>;
pub async fn dictation_item(&self, q: &DictationBody) -> Result<Audio>;        // 输入复用 04-typing；判分 07-规则 diff
pub async fn listening_item(&self, m: &MaterialRef) -> Result<ListeningItem>;  // TTS→听力理解/听写题（18-learning-loop 精听供料）
pub async fn shadowing(&self, target: &str, user_audio: &Audio) -> Result<ShadowingScore>;  // ASR→07 内容判断 + 发音节奏专项
pub fn speaking_card(track: &TrackId) -> SpeakingCard;                          // 雅思口语卡（06 生成+缓存）
pub async fn review_speech(&self, card, user_audio) -> Result<SpeechReview>;
pub fn phonics_drill(zone: PhonicsZone) -> Vec<PracticeLine>;                   // 自然拼读（少儿）
```

## 3. 多轮口语陪练 + 口述作文（原 32）

```rust
pub struct DialogueScene { id, lang, role_card, goals, level }   // 机场/面试/学术讨论…（data/scenes/<lang>/）
pub struct Turn { reply_audio, reply_text, transcript, corrections: Vec<Correction>, turn_score }
pub async fn start_scene(&self, scene: &SceneId) -> Result<Session>;
pub async fn turn(&self, s: &Session, user_audio: &Audio) -> Result<Turn>;   // ASR→实时纠错（规则+Jev noul）→LLM 角色应答→TTS 回话
pub async fn finish(&self, s: &Session) -> Result<DialogueReview>;           // 07 composite（流利/词汇/语法/任务）+节奏专项；发 DialogueFinished
pub async fn oral_compose(&self, audio: &Audio, prompt: &EssayPrompt) -> Result<OralCompose>;  // 说→转写→10 同口径批改
pub fn corrections(&self, transcript: &str) -> Vec<Correction>;
// 上下文窗口=场景卡+近 6 轮+纠错摘要（控成本）；非英语目标语评分标参考值（05 §多语）
```

## 4. 红线与降级

- 音频**不进 Jev**（只收文本）：一律 ASR 转写后判内容；节奏/漏读用 per_word timings 规则打分；
- 转写文落孪生 SourceDoc（17-content-hub）可再派生翻译/题目；
- 降级：TTS 不可用→文本占位；ASR 不可用→跟读/口语卡隐藏、听写降级文本填空（17 旧 §4 语义）。

## 5. 验证点

- [ ] TTS 资产复用断言（同句不重复合成）；听写 diff 一致；shadowing 对慢速/漏读 fixture 稳定；
- [ ] 多轮上下文截断；口述作文与手写评分一致（±0.5 band）；纠错不误伤；ASR/TTS 失败降级；音频不出现在任何 Jev 请求。
