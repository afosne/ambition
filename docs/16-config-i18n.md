# 16 · config-i18n — 配置 / 数据文件 / 分阶段 / 多语种（M9 + 贯穿）

> 实现顺序：**M1 起贯穿（配置/数据）→ M9（多语种全量）**。并入来源：旧 `18-stages-config` + `22-i18n`。
> 一切静态差异（词表/rubric/模板/错式/句式/听力材料）与运行配置的唯一出处。

## 1. 三个语言维度（贯穿全库）

| 维度 | 含义 | 默认 | 影响 |
|---|---|---|---|
| `ui_lang` | 界面语言 | zh | 文案资源 |
| `l1` | 母语 | zh-Hans | 讲解/翻译一侧；**永不进 Jev state** |
| `l2` | 学习目标语 | en | 词库/句式/rubric/语音/键盘/判分策略 |

## 2. LanguagePack 插件（按语种打包差异）

```rust
pub struct LanguagePack { lang: LangTag, lexicon: Arc<dyn LexiconSource>, normalizer: Arc<dyn TextNormalizer>,
                          voices: VoiceSet, keyboard: KeyboardLayout, tracks: Vec<TrackId>, patterns_dir: String }
pub fn language_pack(lang: &LangTag) -> Result<Arc<LanguagePack>>;   // 缺包→能力降级：只开翻译/写作（词库语种到位再全量）
pub trait TextNormalizer { fn normalize(&self, s: &str) -> String; }  // en 大小写冠词/ja 全角假名/fr·de·es 变音/zh 繁简
```
各册落点：core LangTag/Stem（02）→ store lang 列（02）→ prompt `{{l2}}/{{l1}}`（05）→ Jev 多语阈值（05）→ normalizer 注入（07）→ 词库/句式/语音/键盘 per lang（09/15/04）→ 考试轨可插拔（`stages/<lang>/<track>.json`：jlpt/topik/delf/daf/dele…）。
**分级开放**：架构留位（M1）→ 翻译+写作先行（任意语种）→ 词库到位开全量（M9，建议首开 ja）。

## 3. 目录布局（data/）

```
stages/<lang>/<track>.json   词表 wordlists/   rubric rubrics/   模板 templates/   句式 patterns/<lang>/
错式库 chinglish/   观点库 arguments/<lang>/   中国文化 culture/   听力材料 listening/<lang>/   场景 scenes/<lang>/
打字课程 typing/   判断措辞 jev/（error_tags.json=判分口径，改必升 version）   prompts/（{{l2}}/{{l1}} 占位）
qpack.schema.json   importers/（Anki/CSV 映射 profile）
seed/<lang>/<track>/       # 内置离线内容包：种子题库（每轨 ≥200 题）——内容三级保障①，零 AI 零网核心功能可用；M2 格式=qpack（*.qpack.json），open() 幂等导入
lexicon/<lang>/            # **词库资源**（EC-DICT 构建产物：JSONL+索引）；词库不进 SQLite（02-core-store 分层）
```

**资源解析顺序（词库/内置内容）**：`server_url` 已配置且服务端托管 → S3 → **本地 `data/`**（无服务端/S3 时，资源就保存/使用本地 `data/` 目录）。SQLite 只存业务数据（02-core-store 存储分层）。**多设备一致性**：词库包含 `version`（semver），客户端按锁定版本拉取——换机恢复自动拉同版词库（14-cloud §1.2），保证 word_id/sense_id 跨设备稳定；用户词库增量走业务同步。

## 4. 关键 schema

```jsonc
// stages/<lang>/<track>.json
{ "track": "gaokao", "wordlist": "gaokao3500", "kinds": {…题型开关…},
  "writing": { "genres": ["letter","continuation"], "rubric": "gaokao25", "word_range": [80,120], "minutes": 30 },
  "thresholds": { "jev_accept": 0.75, "jev_reject": 0.35 }, "error_weights": {…}, "daily_budget": { "new_words": 20, "reviews": 60, "drills": 1, "diary": 1 },
  "features": { "long_sentence": true, "phonics": false } }
// rubrics/<id>.json（判分口径 version 化）
{ "rubric_id": "gaokao25", "version": 1, "max": 25, "band_map": [5,10,15,20,25],
  "dims": [ { "key": "task", "weight": 0.25, "levels_en": ["…档位英文描述…"] }, … ], "veto": ["off_topic","meaning_changed","heavy_chinglish"] }
```

## 5. 首次启动向导（目标设置 + 难度定位 + 偏好）+ config/keychain

```rust
pub struct Onboarding { track: TrackId, exam_date: Option<Date>, goals: GoalSet, placement: Placement, budget: DailyBudget, features: FeaturePrefs }
pub struct GoalSet   { primary: GoalKind /*过级|提分|兴趣|工作*/, target_score: Option<u32>, vocab_target: Option<u32>, deadline: Option<Date> }
pub struct Placement { vocab_size: u32, theta: f32, weak_hints: Vec<ErrorTag> }

impl SetupWizard {
    pub async fn step_track(&self) -> Result<TrackCatalog>;      // ①目标：考试轨/用途选择（stages/<lang>/<track>）
    pub async fn step_goals(&self, track) -> Result<GoalSet>;    // ②目标：目标分/考试日期/词汇量目标 → 12::plan_for_exam 倒计时排期
    pub async fn step_placement(&self) -> Result<Placement>;     // ③难度：词汇量测试（09::vocab probe）+ 10 题摸底（08::cold_start 锚定 θ）
    pub async fn step_budget(&self) -> Result<DailyBudget>;      // ④偏好：每日预算**按目标日期倒推建议值**（可改），难度偏好三档
    pub async fn step_providers(&self) -> Result<Providers>;     // ⑤可跳过：填 key 或连服务端（跳过=纯本地）
    pub async fn finish(&self, o: Onboarding) -> Result<DailyPlan>;  // 写 config+cat_state+计划 → 首日清单
    pub async fn reconfigure(&self, scope: SetupScope) -> Result<()>;  // 随时重设：改目标/重测难度/换轨（Settings 入口，不丢历史）
}
// 难度偏好三档：Gentle(target_success 0.8) | Normal(0.7) | Hard(0.6) → 08::CatConfig；另可手动锁难度（06 DSL/练习界面）
```

- 落地映射：goals→12-notes-profile::plan_for_exam；placement→09-vocabulary+08-adaptive::cold_start；budget→stages daily_budget 覆写；providers→config.toml+keychain；
- 界面落点：03-engine-ui（TUI 首启屏/Settings；Web `/setup`）；全程可跳过（默认值不阻塞）、可重测；

### 5.1 重设语义（reconfigure：数据不丢保证）

原则：**目标是计划的输入，不是学习数据的属性**。向导/Settings 重设**永不删除学习数据**；唯一删除入口是显式动作（17-content-hub `drop(cascade)` / 手动清库）。

| 数据 | 改目标 goals | 重测难度 placement | 换轨 track | 换语种 l2 |
|---|---|---|---|---|
| attempts/grades/error_tags/essay/notes/diary/typing | 保留 | 保留 | 保留 | 保留 |
| srs_queue（卡片 + FSRS 态） | 保留 | 保留 | 保留 | 保留 |
| cat_state | 保留 | **重校准**（θ 融合更新，dims 保留） | 切到目标轨状态（旧轨留存，切回即用） | 同换轨 |
| 计划/todos（plan_for_exam 产物） | 归档旧（status=archived）→ 重生成 | 不变 | 归档 → 重生成 | 可选重生成 |
| daily_budget | 可选更新建议值 | 不变 | 按新轨默认 | 按新轨默认 |
| profile_snapshots / game_state / 素材 / 题包 / 缓存 | 保留 | 保留 | 保留（错因标签跨轨通用） | 保留（视图按 lang 过滤） |

- 旧计划归档可回看（todos `status=archived`）；历史画像/周报不受影响（从 attempts 重算口径不变）；

```toml
[ui] lang="zh"   [data] dir="data" db="~/.ambition/ambition.db"   [features] gamify=true minor_mode=false
[llm] default="openai-compat" fallback=["anthropic"]   [jev] base_url="https://api.typesafe.ai/v1" model="jev-latest"
[sync] server_url=""          # 留空=纯本地（一切云行为的总开关，14-cloud）
[s3]  endpoint="" bucket="" prefix="qpacks"    [httpd] bind="127.0.0.1:8787" web_dist=""
```
- 密钥只进 keychain（service=`ambition`）：`llm.*.api_key`/`jev.api_key`/`s3.secret`/`account.token`；env 覆盖：`AMBI_LLM_KEY`/`AMBI_JEV_KEY`/`TYPESAFE_API_KEY`（兼容）/`AMBI_SERVER_URL`；
- M2 过渡 env（config 落地前的 provider 定址，16-config 接入后收回）：`AMBI_LLM_BASE`（默认 `https://api.openai.com/v1`）、`AMBI_LLM_MODEL`（默认 `gpt-4o-mini`）、`AMBI_JEV_BASE`（默认 typesafe v1）、`AMBI_DB`（覆盖库文件路径，默认 `~/.ambition/ambition.db`）；
- 加载器：`load_stage/load_rubric/load_jev_error_tags/load_templates/load_prompt/config_load/secret_get`（幂等导入静态数据）；
- 首启向导：选 track → 词汇量测试定位 → 填 key 或连服务端（可跳过）。

## 6. Settings 参数总表（UI：TUI Settings 分区 = Web `/settings`，同构）

>  = 机密项（只存 keychain，界面只显掩码）；⚙️ = 变更即**判分口径/历史可比性**相关，带警示确认。

| 分组 | 参数（config 键） | 取值 | 默认 | 生效 |
|---|---|---|---|---
| 目标与计划 | `track` | stages 各轨（gaokao/cet4/…/work_*） | 向导选择 | 重算派生（§5.1） |
| 〃 | `exam_date` / `target_score` / `vocab_target` / `goal.primary` | 日期/数值/过级·提分·兴趣·工作 | 空 | 归档旧计划→重排 |
| 〃 | `daily_budget.{new_words,reviews,drills,diary}` | 数值 | 轨默认（向导按目标倒推） | 次日生效 |
| 〃 | `difficulty_pref` | gentle(0.8)/normal(0.7)/hard(0.6) | normal | CatConfig.target_success |
| 〃 | 重测难度（动作） | — | — | θ 重校准（dims 保留） |
| 语言 | `ui_lang` | zh/en/… | zh | 立即 |
| 〃 | `l1` / `l2` | LangTag（l2 需 LanguagePack） | zh-Hans / en | 出题/判分策略/视图 |
| AI 与判分 | `llm.default` / `llm.fallback` | provider id 序列 | openai-compat | 立即 |
| 〃 | `llm.providers.*.base_url/model` | URL / 字符串 | 内置 | 立即 |
| 〃 | `llm.*.api_key` / `jev.api_key`  | keychain | 空（空则走代管） | 立即 |
| 〃 | `jev.base_url/model` | URL | typesafe / jev-latest | 立即 |
| 〃 | 规则容错 `normalize.{ignore_articles,ignore_plurals}` ⚙️ | bool（per track） | 轨默认 | 影响历史判分可比（改必升口径 version） |
| 〃 | `difficulty_lock` ⚙️ | off \| (min,max) | off | 手动锁难度（覆盖 CAT，06 DSL 同款） |
| 功能开关 | `features.gamify` / `features.widget` | bool | true | 立即（游戏层可关） |
| 〃 | `features.minor_mode` | bool | false | 1h 时长上限/晚间静默/排行脱敏 |
| 〃 | 提醒：`micro_review_push` / `sedentary` / `daily_limit` | bool/bool/时长 | off/on/4h | 推送严格 opt-in |
| 〃 | 讲解口播默认开 / `voice_style` / `speech_speed` / 精听语速 | bool/enum/0.75–1.2x | off/default/1.0/1.0x | 15-speech TTS |
| 云与同步 | `sync.server_url`（云总开关） | URL（空=纯本地） | 空 | 能力开关（14-cloud） |
| 〃 | 账号：登录/设备配对/登出（动作） | — | — | 立即 |
| 〃 | `sync.enabled` / `sync.e2ee` | bool | server 非空时 on / off | 端到端加密可选 |
| 〃 | `s3.{endpoint,bucket,prefix,secret}`   | — | 空 | 题包 S3 后端 |
| 〃 | `pack_share_default` | local/s3/server | local | 分享默认通道；follows 管理 |
| 素材与隐私 | 取词监听 `capture_listen` / 存整句 `capture_span` | bool/bool | on/off | 17-content-hub；可一键停用 |
| 〃 | 数据导出/备份/清库（动作） | — | — | 导出=所有权；清库=唯一显式删除口（§5.1） |
| 界面 | `theme`（含 ASCII 装扮）/ `font_size`(Web) / TTS 自动朗读 | id/px/bool | default/12/off | 立即 |
| 服务端 | 管理面板：`server_features.{master,registration,sync,plaza,proxy,classroom}` | bool 组 | 全关 | 仅 Web 管理面板（14-cloud）；审计留痕 |

- 参数 ↔ config.toml 键一一对应（CI 断言文档同步）；向导可改项 ⊆ 本表（`reconfigure` scope = 目标/难度/语种）；

## 7. 验证点

- [ ] 向导五步可跳过（纯本地全流程可完成）；placement 新用户 θ 误差 ±0.5（模拟）；目标日期倒推预算在合理区间；
- [ ] **reconfigure 数据不丢**：各 scope 重设后学习类表逐表行数不变；旧计划 archived 可回看；换轨后切回旧轨 cat_state 还在；
- [ ] data/*.json 全过 schema 校验（CI）；rubric version 升级不重算历史；
- [ ] Settings 表与 config.toml 键双向一致（文档同步断言）；🔒 项只存 keychain（界面掩码）；⚙️ 变更弹历史可比警示；
- [ ] env 覆盖优先级（env>config>默认）；keychain 缺失引导不 panic；
- [ ] 语言包缺失降级（只开翻译/写作）；normalizer 按语种表驱动；`stages/<lang>/` 缺轨报错不崩溃；words 表 en+ja 不串数据。
