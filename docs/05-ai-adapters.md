# 05 · ai-adapters — LLM + Jev 适配层（M2 / M3）

> 实现顺序：**M2（LLM 全量 + Jev 最小 `evaluate`，供 06 质检回路）/ M3（Jev 全量：fan-out 并发池、置信度路由、错因分类）**。并入来源：旧 `03-llm` + `04-jev`。
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
    pub async fn usage_summary(&self, from: Date, to: Date) -> Result<UsageSummary>;     // 成本面板
}
pub enum LlmError { Http{status, retryable}, InvalidOutput{attempts}, QuotaExceeded, Offline }
```
- prompt 模板 `data/prompts/<kind>.md`（`{{l2}}/{{l1}}` 占位，M2 内嵌默认 + data/ 目录可覆盖，用户目录覆盖随 16-config）；判分内容字段必须 l2 原文；
- `llm_cache` key=hash(content+kind+params)（素材/讲解/范文共用，00-overview S7）；`llm_usage` 记账——两表由 store 提供读写函数，**trait `AiLedger` 定义在 llm，SQLite 实现经 engine 装配**（保持 05 不依赖 store 的方向）。
- **M2 落地范围**：`OpenAiCompat`/`Anthropic` 两实现 + `LlmRouter::structured/text` + wiremock 双协议契约测试；`usage_summary` 成本面板与 `ProxyLlm` 随 M3/M7（记账 M2 先落 `llm_usage` 行）。

## 2. Jev（TypeSafe System One：choice/score/noul）

```rust
pub struct JevClient { http, base: "https://api.typesafe.ai/v1", key, proxy }
pub struct JevState(pub serde_json::Value);                 // 只放 l2 文本（母语/中文永不进 state）
pub enum JevQuestion { Choice{instructions, criteria}, Score{instructions, levels}, Noul{instructions} }
pub enum JevAnswer { Choice(ChoiceAnswer{choice, probabilities, confidence}),
                     Score(ScoreAnswer{score, probabilities, confidence}), Noul(NoulAnswer{yes_prob}) }
impl JevClient {
    pub async fn evaluate(&self, state: JevState, qs: JevQuestions) -> Result<JevAnswers>;  // POST /v1/systemone，model=jev-latest
    pub async fn evaluate_many(&self, jobs: Vec<…>) -> Vec<Result<JevAnswers>>;             // M3：并发池 8，单题失败不中断
}
// 重试：429/529/5xx 指数退避 ≤3；超时 15s；记 llm_usage(kind=jev)
// wire 契约（docs.typesafe.ai/api，M2 取证）：请求 {state, model, questions:{id:{type, instructions, criteria}}}；
//   criteria：choice=map<opt,desc>、score=有序 levels 数组（2–10 档）、noul={"true","false"} 描述；
//   响应 {model, answers:{id:{type, noul | choice+probabilities+confidence | score+legend+probabilities+confidence}}, usage}；
//   错误：401 key 无效 / 422 body 校验失败 / 429 限流 / 529 过载。
// M2 落地范围：`JevClient::evaluate`（重试+超时）供 06::qc；evaluate_many/route_by_confidence 属 M3。

pub enum Route { AutoAccept, AutoReject, LlmReview, HumanReview }
pub fn route_by_confidence(ans: &JevAnswer, accept: f32, reject: f32) -> Route;   // 默认 0.75/0.35；noul 用 yes_prob 边际
```

## 3. 多语判分策略（16-config-i18n 引用）

| l2 | 阈值 | 流水线 |
|---|---|---|
| en | 0.75 / 0.35 | 规则 → Jev → LLM 复核 |
| 其它 | 0.85 / 0.20 | 规则 → Jev（仅参考）→ LLM 粗判 + 低置信待裁定；分数 UI 明示"参考值"、只同 lang 比较 |

- 固定判断语料（`data/jev/error_tags.json`、rubric `levels_en`）**措辞即判分口径**，变更必升 version。

## 4. 验证点

- [ ] 两协议 wiremock 契约（json_schema/tool 两形态）；schema 校验重试/fallback/缓存三路径；
- [ ] request 快照断言 state 无 CJK 母语内容；三种 answer 反序列化 fixture；
- [ ] route 阈值边界（=阈值）表驱动；非英语阈值切换断言；fan-out 单题失败隔离。
