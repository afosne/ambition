# 05 · ai-adapters — LLM + Jev 适配层（M2 / M3）

> 实现顺序：**M2（LLM 全量 + Jev 最小 `evaluate`，供 06 质检回路）→ M3（Jev 全量：fan-out 并发池、置信度路由、错因分类 + 成本台账）——M3 已落地**。并入来源：旧 `03-llm` + `04-jev`。
> 外部 AI 的唯一出口：回包只经本层 → 被 06/07/10/11/15/18 消费 → engine 落库（00-overview §2 红线）。

## 1. LLM（OpenAI / Anthropic 协议 + 代管代理）

```rust
pub struct GenRequest { pub kind: LlmKind, pub system: String, pub user: String, pub schema: Option<JsonSchema>,
                        pub temperature: f32, pub max_tokens: u32, pub cache_key: Option<String> }
pub trait LlmProvider: Send + Sync { async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError>; }
// 实现：OpenAiCompat（含 DeepSeek/Qwen/Moonshot/Ollama；response_format=json_schema，降级 json_object+内嵌 schema）
//      Anthropic（Messages API，强制 tool input_schema）
//      ProxyLlm（POST {server_url}/proxy/llm，402/429→QuotaExceeded）
pub struct LlmRouter { providers, default, fallback, ledger, proxy }
impl LlmRouter {
    pub async fn structured<T: DeserializeOwned>(&self, req: GenRequest) -> Result<T>;  // 缓存→路由→schema 校验→重试≤2（temperature 递减+错误摘要）→记账→写缓存
    pub async fn text(&self, req: GenRequest) -> Result<String>;
    pub async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary;    // 成本面板（M3 落地：按 kind 聚合，区间 [from_unix, to_unix) unix 秒）
}
pub enum LlmError { Http{status, retryable}, InvalidOutput{attempts}, QuotaExceeded, Offline, BreakerOpen /*M3 熔断快速失败*/ }
```
- prompt 模板按 **l2 目标语分版**：`assets/prompts/<kind>/<l2>.md`（gen=`crates/gen/assets/prompts/question_gen/{en,zh}.md`、grader=`crates/grader/assets/prompts/arbitrate/{en,zh}.md`），**缺版回退 `en`**（16 语言包缺失降级同款）；编译期 `include_str!` 内嵌、**无运行时文件读取——用户不可修改**；`data/` 只放用户可导入内容（词表/题包等，docs/16 §3）。渲染时 `{{l1}}/{{l2}}` 替换为**语言名**（用模板自身语言书写：`LangTag::name_in`，en 版出 “English/Chinese”、zh 版出 “英语/中文”，未收录语种回退标签本身）；模板正文只写指令，语言事实一律走占位（禁硬编码 English/Chinese）；`{{l1}}` 现取 `core::DEFAULT_L1`（zh-Hans，TODO 16-config 接入后随 config 注入）；判分内容字段必须 l2 原文；
- `llm_cache` key=hash(content+kind+params)（素材/讲解/范文共用，00-overview S7）；`llm_usage` 记账——两表由 store 提供读写函数，**trait `AiLedger` 定义在 llm，SQLite 实现经 engine 装配**（保持 05 不依赖 store 的方向）。
- **M2 落地范围**：`OpenAiCompat`/`Anthropic` 两实现 + `LlmRouter::structured/text` + wiremock 双协议契约测试（记账落 `llm_usage` 行）。
- **M3 落地范围**：`usage_summary(from_unix, to_unix)`（数据源 `AiLedger::usage_summary`，SQLite 实现 = engine::StoreLedger 经 `Engine::cost_summary` 门面）；**生成全局熔断**——连续失败 ≥`BREAKER_FAIL_LIMIT`(5) → 快速失败 `BREAKER_SKIP_LIMIT`(10) 次不发请求 → 放行 1 次试探，成功即关闸，返回 `LlmError::BreakerOpen`（`Offline` 不计入、缓存命中不计数）；`LlmKind::Arbitrate`（07 三级复核的判断型 prompt）；`llm_cache` TTL 淘汰 = store `cache_purge_before`（TTL 7 天，engine open 时执行一次）；`ProxyLlm` 仍随 M7。

## 2. Jev（TypeSafe System One：choice/score/noul）

```rust
pub struct JevClient { http, base: "https://api.typesafe.ai/v1", key, proxy }
pub struct JevState(pub serde_json::Value);                 // 只放 l2 文本（母语/中文永不进 state）
pub enum JevQuestion { Choice{instructions, criteria}, Score{instructions, levels}, Noul{instructions} }
pub enum JevAnswer { Choice(ChoiceAnswer{choice, probabilities, confidence}),
                     Score(ScoreAnswer{score, probabilities, confidence}), Noul(NoulAnswer{yes_prob}) }
impl JevClient {
    pub async fn evaluate(&self, state: JevState, qs: JevQuestions) -> Result<JevAnswers>;  // POST /v1/systemone，model=jev-latest
    pub async fn evaluate_many(&self, jobs: Vec<JevJob>) -> Vec<Result<JevAnswers, JevError>>; // M3 落地：并发池 8（JEV_CONCURRENCY）、保序、单题失败不中断
}
pub struct JevJob { pub state: JevState, pub questions: JevQuestions }   // fan-out 作业（每作答一单）
// 重试：429/529/5xx 指数退避 ≤3；超时 15s；记 llm_usage(kind=jev)
// wire 契约（docs.typesafe.ai/api，M2 取证）：请求 {state, model, questions:{id:{type, instructions, criteria}}}；
//   criteria：choice=map<opt,desc>、score=有序 levels 数组（2–10 档）、noul={"true","false"} 描述；
//   响应 {model, answers:{id:{type, noul | choice+probabilities+confidence | score+legend+probabilities+confidence}}, usage}；
//   错误：401 key 无效 / 422 body 校验失败 / 429 限流 / 529 过载。
// M2 落地范围：`JevClient::evaluate`（重试+超时）供 06::qc；evaluate_many/route_by_confidence 属 M3。

pub enum Route { AutoAccept, AutoReject, LlmReview, HumanReview }
pub fn route_by_confidence(ans: &JevAnswer, accept: f32, reject: f32) -> Route;   // 默认 0.75/0.35；noul 用 yes_prob 边际
// 路由语义（M3 定案）：margin ≥ accept → AutoAccept；margin ≤ reject → AutoReject；灰区 → LlmReview。
//   Noul：margin = yes_prob（有向：≥accept 采信「是」、≤reject 采信「否」——两端都是确定性判断）；
//   Choice/Score：margin = confidence（AutoAccept = 采信模型判断；AutoReject = 不采信、按该判断的否定口径落定）；
//   LlmReview 不可用（无 LLM key/失败）→ 07 降级 HumanReview → arbitration 待裁定桶。
```

## 3. 多语判分策略（16-config-i18n 引用）

| l2 | 阈值 | 流水线 |
|---|---|---|
| en | 0.75 / 0.35 | 规则 → Jev → LLM 复核 |
| 其它 | 0.85 / 0.20 | 规则 → Jev（仅参考）→ LLM 粗判 + 低置信待裁定；分数 UI 明示"参考值"、只同 lang 比较 |

- 固定判断语料（`crates/grader/assets/jev/<l2>.json`，en/zh 双版、缺版回退 en；rubric `levels_en`）**措辞即判分口径**，变更必升 version（**全部语言文件与代码常量同步升**——测试逐版断言）；语料**不进 data/**（编译期内嵌、用户不可改，docs/16 §3）。
  **M3 落地 + i18n 分版**：每版含 `schema_version=1`、`version`、`lang`、`instructions`（error_cause 判断指令，随语料进 Jev 请求）与 `criteria`（28 键 = `core::ErrorTag` snake_case 全集，description 语言 = 该版 l2——**l1 永不进 Jev 请求**，en 版全英文；`lang` 字段与文件名一致）；`error_criteria(lang)/error_cause_question(lang)` 按题目 l2 选版加载；改任何语言版措辞必须同时升 `version` 常量（当前 1.1.0）。

## 4. 验证点

- [x] 两协议 wiremock 契约（json_schema/tool 两形态）；schema 校验重试/fallback/缓存三路径（M2）；
- [x] request 快照断言 state 无 CJK 母语内容；三种 answer 反序列化 fixture（M2）；
- [x] route 阈值边界（=阈值）表驱动；非英语阈值切换断言；fan-out 单题失败隔离（M3）。
