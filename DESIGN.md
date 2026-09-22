# Ambition（俺必胜） — 英语学习 TUI / Web 程序 设计文档（总纲）

> **产品名：Ambition（俺必胜）**（Ambition [æmˈbɪʃn] ≈「俺必胜」）· CLI：`ambition` · 口号：Ambition, 俺必胜！
> **开源协议：PolyForm Noncommercial 1.0** —— **非商业免费使用**（Free for non-commercial use）；商业授权另行协商，与代码仓解耦
> 目标用户：不同阶段的中国人（小学 → 中考 → 高考 → 四六级/考研 → 雅思/托福 → 职场），多语种目标语可扩展（§17）
> 形态：**只有两个入口**——TUI（`ambition`，纯用户端）与 Web（`ambition web`，用户端 + 管理面板内开关服务端功能，M7）。
> AI 分工：OpenAI / Anthropic 协议 LLM 负责**出题、讲解、范文、翻译**；TypeSafe **Jev** 负责**结构化判题**；分数只由规则 + Jev 出。
>
> **文档体系**：本文件是总纲；`docs/` **20 份分册按实现顺序 00–19 编号**（平行功能已合并、冗余已删除；新旧分册映射见 §24）；每章结尾标注对应分册；全库数据流与闭环验证见 `docs/00-overview.md`（含场景走查 S1–S8、单写者矩阵），需求→函数对照见 `docs/03-engine-ui.md` §10。章节号是跨文档引用锚点，只增不改。

---

## 1. 设计原则

1. **代码在控，模型出判断**（TypeSafe 的 System One 哲学）：能用规则判的绝不用模型；模型只返回类型化判断（choice/score/noul），权重、阈值、路由全部留在代码里。
2. **三级判题流水线**：`确定性规则 → Jev → LLM 复核/人工裁定`，逐级升级，成本递增、调用递减。
3. **原子化判断**：一题一问（一个判断点），复杂评分拆多个原子问题在代码里组合（composite scoring）。
4. **本地优先，云可选**：数据全在本地 SQLite + Markdown 导出；只有判题/生成/翻译时把必要文本发给模型；密钥进 keychain。云（服务端 / S3）是**可选增强**——是否启用任何云行为，只取决于用户是否填入 `server_url` / S3 配置（§15）。**无 AI 时核心功能照常**：内容三级保障（内置离线 seed 包 + qpack 导入 + AI 可选）；判题与解析**自动沉淀题库**，随 qpack 导出/导入（§4.3）。
5. **一切错误都进入间隔重复**：错题、错词、笔记、句式、作文弱点全部走 FSRS 复习队列，而不是静态列表。
6. **游戏化放大动机，不替代学习科学**：XP/徽章/游戏只做反馈与乐趣，排程、出题、评分永远由学习科学与判题引擎决定；游戏层可整体关闭（§11.5）。
7. **素材只存一份，派生全部注册**：OCR/音频/文本素材统一进素材中枢，各功能 span 引用 + 派生注册，零复制共用（§19）。

> 分册：docs/00（数据流红线、单写者矩阵）。

---

## 2. 总体架构

```
rust/                        # Cargo workspace
├── crates/
│   ├── core/               # 领域类型：题目/作答/判分/错因/DomainEvent（纯类型+纯函数）
│   ├── store/              # sqlx（SQLite/Postgres 双后端同一迁移）+ Repo trait + Markdown/JSON 导出
│   ├── llm/                # LlmProvider：OpenAI 兼容 + Anthropic + 代管代理出口；结构化输出/缓存/用量台账
│   ├── jev/                # TypeSafe Jev HTTP 客户端 + 置信度路由（多语策略 §17）
│   ├── grader/             # 三级判题流水线 + 错因归类 + 待裁定桶
│   ├── gen/                # 出题 + Jev 质检回路 + qpack 打包导入 + 错题变式/讲解
│   ├── srs/                # FSRS 调度 + 假对检测 + 复习队列 + 检索难度递进
│   ├── vocab/              # 词库（LexiconSource 插件）：义项/搭配/易混/词根/同反义/词汇量测试
│   ├── typing/             # 打字通「键胜」：课程/指标/热力图/素材/打字游戏接口
│   ├── writing/            # 模板作文 + 加分项 + 批改闭环 + 语域/长难句专项
│   ├── translate/          # 翻译工作台：即时翻译/对齐/我来翻/学习沉淀
│   ├── ocr/                # OCR 图文提取：多引擎投票 + 版面保持 + 校对回路
│   ├── material/           # 素材中枢：SourceDoc/MaterialRef/派生注册/资产共用
│   ├── gamify/             # 游戏层：订阅 DomainEvent → XP/成就/道具/排行（可关）
│   ├── sync/               # 本地 append-only 事件日志 + 可选云同步（§15）
│   ├── speech/             # TTS / ASR 适配（听说闭环，M8）
│   ├── engine/             # use-case 门面：两壳唯一消费入口（docs/19 函数全集）
│   ├── tui/                # ratatui + crossterm（一期，纯用户端）
│   └── httpd/              # axum：API + WS + 内嵌 Web SPA；「服务端功能」按管理面板开关挂路由
├── web/                    # React + TS（M7）；dist 内嵌进二进制（开发期 `--web-dist` 外挂热更）
└── data/                   # 词表/词库源/rubric/模板/句式/错式库/patterns/prompts/typing 课程（schema 见 docs/16-config-i18n）
```

- 关键 crate：`ratatui`/`crossterm`、`tokio`、`serde`、`sqlx`(sqlite/postgres)、`reqwest`、`fsrs`、`rust-embed`、`unicode-width`（CJK 排版）、`thiserror`/`anyhow`、`tracing`、`keyring`；OCR/ASR 引擎可选（paddleocr/tesseract/whisper.cpp/piper）。
- **两壳一引擎**：`tui` / `httpd` 只调 `engine` 门面，不直连 store/AI；核心库 ~90% 代码。
- **事件总线**：engine 发 `core::DomainEvent`（不可变事实）→ gamify / sync 投影 / profile 增量订阅（旁路、可关、幂等）。
- **单写者原则**：每张业务表唯一写入方（矩阵见 docs/00 §3）。

> 分册：docs/02-core-store、docs/02-core-store、docs/03-engine-ui、docs/03-engine-ui。

---

## 3. 判题体系（核心）

### 3.1 三级流水线 + 置信度路由

```
作答 ──▶ ① 规则判题（规范化 + 精确/可接受答案表）
            │ 命中 ──▶ 完成（0 成本、0 延迟）
            ▼ 未命中 / 有歧义
        ② Jev 判题（一次请求多原子判断 + probabilities + confidence）
            │ confidence ≥ θ ──▶ 完成（自动判分 + 错因标签）
            ▼ 低于阈值
        ③ LLM 粗判（只出判断不出分）或进「待裁定」桶 → 人工裁决 → 可接受答案回流规则层
```

- **规则层**：大小写/全角/空白/标点/冠词容错的规范化（按语种注入 normalizer，§17）；可接受答案表；选项比对。
- **Jev 层**：模糊可接受性、多选逐项部分分、翻译/作文分维度打分、**错因分类**。
- **LLM 层**：只生成**给人看的文本**（错因讲解、更优译文、逐句批注、范文），不负责给分——分数以规则 + Jev 为准（校准、可跨时间比较）。

### 3.2 各题型判题矩阵

| 题型 | 判分 | 错因 / 讲解 | 说明 |
|---|---|---|---|
| 单词四选一 | 规则（选项比对） | Jev `choice` 错因 + LLM 讲解（按需） | 出题侧 Jev 质检（§4.2） |
| 多项选择 | 规则全对/全错 + Jev 逐项 `noul` 部分分 | Jev | 逐项低置信 → 待裁定 |
| 填空/完形/语法填空 | 规则规范化 → Jev `noul` 逐空可接受性 | Jev `choice` + LLM | 同义变体走 Jev |
| 阅读/七选五/匹配 | 内部子题按 kind 递归 | 同左 | |
| 听写 | 转写/输入词序列 diff | Jev `noul`（漏词小错容忍） | 输入复用打字引擎 |
| 翻译 | Jev `score` 组合：准确/完整/流畅/地道 | LLM 逐句对照 + 更优译文 | composite，权重在代码 |
| 作文 | Jev `score` 多维（切题/结构/语法/词汇/连贯/规范）按考纲加权 | LLM 逐句批注 + 加分项 + 三档范文 | rubric 版本化 |
| 句式转换/造句 | Jev `noul`（formed/meaning_kept） | LLM 反馈 | §5.4 |

### 3.3 Jev 请求打包示例（填空）

```json
{
  "state": { "sentence": "She has a ___ for languages (talent).", "student_answer": "gift",
             "reference_answers": ["talent", "gift", "knack"], "level": "CET-4" },
  "model": "jev-latest",
  "questions": {
    "acceptable":  { "type": "noul",  "instructions": "The student's answer fits the sentence grammatically and preserves the intended meaning." },
    "error_cause": { "type": "choice","instructions": "Classify the mistake in the student's answer.", "criteria": { "word_sense": "…", "collocation": "…", "grammar": "…", "spelling": "…", "no_error": "…" } },
    "severity":    { "type": "score", "instructions": "How serious is the mistake?", "criteria": ["minor / meaning intact", "noticeable error", "meaning broken"] }
  }
}
```

一次请求 = 一个 state 的一次作答；多题并发（受限并发池）。作文/翻译单请求内 fan-out 多维度。

### 3.4 作文 / 翻译的 composite scoring

- 每维度一个原子 `score`，levels 用**考试评分标准原文**（`data/rubrics/*.json`，版本化）；代码加权合成档位分；
- `noul` 兜底一票降档：离题 / 意义走样 / 严重中式英语；
- **假对检测**：答对但秒选/久犹豫低自评 → 同样进复习队列（猜对 ≠ 会做，§5.3）。

### 3.5 ⚠️ Jev 落地注意事项（官方文档）

- **只收文本**：图片/听力不可直接判 → ASR 转写后判内容（§13）；
- **主训练语言是英语（CJK 低准确率）**：state 只放学习目标语 `l2` 原文，母语/中文内容永不进 state；非英语目标语降级策略见 §17；
- **无 Rust SDK**（仅 Python/TS）→ 自研薄 HTTP 客户端 `crates/jev`（`POST /v1/systemone`）；
- choice/score/noul 可混一次请求，并行求值、互不污染。

> 分册：docs/05-ai-adapters、docs/05-ai-adapters、docs/07-grading。

---

## 4. 题目生成（OpenAI / Anthropic 协议）

### 4.1 LlmProvider 抽象

```rust
trait LlmProvider: Send + Sync { async fn complete(&self, req: GenRequest) -> Result<LlmOut>; }
```
- 实现：`OpenAiCompat`（OpenAI/DeepSeek/Qwen/Moonshot/Ollama）、`Anthropic`（Messages）、`ProxyLlm`（服务端代管 `/proxy/llm`）；JSON Schema 约束 + serde 校验 + 重试 ≤2；`llm_cache`（key 含内容 hash）+ `llm_usage` 成本面板。

### 4.2 生成参数 & 质检回路（出题质量护城河）

生成请求携带：目标阶段/语种、词表范围、题型、难度、主题、来源素材（生词本/错词本/画像薄弱点/素材中枢选区）。
LLM 出题最大风险是**题目本身有错** → 生成后过 Jev 质检 fan-out：

| 质检问题 | 类型 |
|---|---|
| 四个选项是否只有一个正确 | noul |
| 标准答案是否确实正确 | noul |
| 句子是否自然、无歧义、难度适中 | score |
| 干扰项是否有区分度 | noul |

不合格自动丢弃重生成（≤2 轮）；入库记 `quality_score`，低质题不进组卷。

### 4.3 题目共享 / 导入：`.qpack` 题包

- 纯 JSON、版本化 schema：`manifest`（作者/来源/许可/考纲/schema 版本）+ 统一题目模型（`stem / lang / media? / type / options? / blanks? / acceptable_answers? / rubric_ref? / tags[] / provenance`）；
- 导入 schema 校验 + 内容哈希去重；分发通道（纯本地/口令码/S3/官方服务端）见 §14。
- **判题与解析自动沉淀**：作答题自动入“我的题库”，讲解/可接受答案增强随包携带（`explanations[] / acceptable / quality_score`），导入即得讲解；

> 分册：docs/06-generation、docs/14-cloud（qpack 格式）。

---

## 5. 背单词 & 记忆系统

### 5.1 词表体系（按阶段）

新课标小学 ~800 → 初中 1600 → 高考 3500 → 四级 4500 → 六级 6000 → 考研 5500 → 雅思/托福 8000+ → 行业词包。词库底座 EC-DICT（en），多语种走 `LexiconSource` 插件（§17）。

### 5.2 词条深度数据（中国学生痛点全覆盖）

- **一词多义按义项拆行**（bank₁/bank₂ 分开记忆出题，训练侧见 §5.5）；
- 搭配/介词、易混词辨析、同义词/反义词（既是记忆关系也是题型）、词根词缀；
- **词汇量自适应测试**（staircase 30 题收敛）定位词表起点。

### 5.3 复习引擎

- **FSRS 统一调度**：词义项/错题/假对题/笔记/模板功能句/固定句式共用队列；
- 错词本：答错自动入册（错因标签 + 次数），队列提权；
- **假对检测**：秒选 / 久犹豫低自评 / 自评完全没把握 → 进卡；
- 检索难度递进：选择 → 填空 → 拼写 → 造句（答对升档、答错降档）；交错练习 + 每周小测（测验效应）。

### 5.4 固定句式系统（句型级习惯用法）

- **句式库**（`data/patterns/<lang>/`）：句型骨架 + 槽位（`not only {A} but also {B}`）+ 分类（连接/强调/倒装/虚拟/比较/建议…）+ 中文讲解 + 普通句↔句式化对比 + **典型误用 traps**；
- **训练五模式**：句式辨析 / 槽位填空 / 句式转换（Rewrite）/ 造句迁移（SRS compose 档）/ 错式改错（联动 §13.6 错式库）；
- **写作迁移**：批改统计**句式多样性**（目标 9 的量化指标），低则推荐可套句式；句式卡进 SRS。

### 5.5 一词多义专项（义项级训练）

- **训练五模式**：义项辨析（语境↔义项）、熟词僻义（考研 `familiar_rare_sense`）、易混义项对交替快判（+ 假对检测）、**全义项清卡毕业**（所有义项过卡才算掌握）；
- 错义项单独加重（错因 `word_sense`）；出题强制目标义项语境（含非首义项，qc 拦语境歧义）。

> 分册：docs/08-adaptive、docs/09-vocabulary、docs/09-vocabulary。

---

## 6. 笔记 / 日记 / TODO

- **笔记**：词/题/语法点/句式/自由统一 Markdown（front-matter 进 SQLite 索引，FTS 检索），双向链接 `词 ↔ 题 ↔ 语法点 ↔ 作文弱点 ↔ 素材`；一键转 SRS 卡；
- **日记**：英语日记 → LLM 鼓励式批改（只改错 + 1-2 个升级建议 + 一句鼓励）；**隐私红线：日记仅在用户请求批改时外发**（HTTP 断言测试）；
- **TODO/备考计划**：每日任务（新词 N + 复习 M + 专项 1 + 日记）；目标考试倒计时自动排期，<30 天切模考周。

> 分册：docs/12-notes-profile。

---

## 7. 画像 & 个性化（目标 10 / 11）

### 7.1 错因标签体系（画像原料）

- 词汇：词义混淆/错义项/搭配/介词/拼写/词性/冠词/可数性
- 语法：时态/语态/主谓一致/从句引导词/非谓语/虚拟/倒装强调/句子结构
- 语篇：衔接/指代/逻辑
- 表达：中式英语负迁移/语域不当/冗余重复/**句式误用**
- 应试：审题偏差/时间分配/粗心

Jev `choice` 按此表打 1-2 个加权标签 → 热力图（错误密度 = Σ权重 × 时间衰减）。

### 7.2 从画像到练习

- **错题变式复仇战**：同点不同句变式再练，连对 2 次出队；
- 薄弱点出题：画像权重进 `GenSpec.weak_points`（含句式 `pattern_misuse`、义项 `word_sense`）；
- **写作弱项加强**（目标 11）：作文批注聚类 → 专项 drills（升级改写/合并拆分/概要/翻译造句）→ 复批闭环；
- 周报/报告导出（Markdown）；套卷**估分**（历史 run 回归）；素材利用统计（derivations）。

> 分册：docs/12-notes-profile。

---

## 8. 作文 / 翻译加分项系统（目标 9）

1. **高级句式库**：强调句/倒装/独立主格/with 复合/非谓语开头/虚拟/比喻——每式配「原句 → 升级句 → 适用场景」；
2. **升级改写训练**：低级句 → 高级句，Jev `noul` 校验原意未变；
3. 亮点词汇替换表（按语域分档，"准确 > 花哨"）+ **反模板黑名单**（阅卷反感句及替换）；
4. **范文体系**：每题三档范文（及格/良好/满分）+ 逐句赏析 + 范文听写临摹（联动 §9）；
5. **翻译专项**（考试题型）：中译英段落（CET/考研）、中国文化主题表达库、"直译死板"→ 地道对照。

> **翻译工作台**（随手翻译/双语对照/我来翻/学习沉淀）是独立功能，见 §16。

> 分册：docs/10-writing。

---

## 9. 打字通（键盘拼写训练系统，UI 名「键胜」）

> 打字 = 拼写回忆 + 键盘技能 + 应试输入能力。零 AI 依赖，M1 先行。

### 9.1 指法课程体系

- F/J 定位、八指分区、键区分级解锁（中排→上排→下排→数字符号→Shift）8 级通关；
- 全角/半角与输入法切换提醒；少儿模式配自然拼读；多语种键盘布局 per lang（§17）。

### 9.2 训练素材（优先级即复习队列）

1. **SRS 到期词拼写打字**（复习即打字，检索强度高于选择）；2. 词表/课文/真题句；3. **范文临摹打字**（§10）；4. 用户笔记/日记；5. 素材中枢选区（`MaterialRef`）。

### 9.3 指标与专项

WPM/CPM、准确率、节奏方差、**错误键位热力图** → 瓶颈键位专项；WPM 曲线进周报。

### 9.4 与其它系统联动

- 游戏（§11）：打字射击/限时冲刺/Boss 打字模式；
- 考试联动：作文模考计时输入、听写题复用打字引擎组件（§13）。

> 分册：docs/04-typing。

---

## 10. 模板作文系统

> 模板是脚手架，最终目标**脱模板**（填槽三阶段）。与 §8 互补：§8 管句式，本节管篇章。

### 10.1 模板模型（`data/templates/*.json`）

= **结构骨架（段落功能序列）+ 内容槽 + 功能句槽位（备选高级句式）+ 亮点词汇 + 反模板提示 + 适用考纲**。支持个人模板（用户佳句升格）。

### 10.2 模板库覆盖

高考应用文（建议/邀请/感谢/通知/求助）+ 读后续写情节模板；四六级议论（现象/对比/解决）；考研图画三段式 + 小作文；雅思 Task1 图表句型 / Task2 四题型；职场商务邮件（询价/致歉/催单/求职）。

### 10.3 填槽三阶段（脚手架渐退）

1. 全模板填槽 → 2. 半模板（只给段落功能提示）→ 3. 脱模板限时自由写。
护栏：Jev `noul`（保留原意/贴合题目）+ LLM 检测**模板痕迹过重** → 生成个人化变体。

### 10.4 闭环与联动

批改后按缺陷推荐模板/句式；句式进 SRS + 范文临摹打字；句式多样性统计（§5.4）。

> 分册：docs/10-writing、docs/09-vocabulary（句式）。

---

## 11. 游戏系统

> 原则（§1.6）：游戏放大动机不替代学习科学；健康与防沉迷优先。

### 11.1 架构：事件总线解耦

engine 发 `DomainEvent` → gamify 订阅 → XP/金币/成就/连击；游戏层可整体关闭（家长/极简模式），学习数据零影响（`game_events` 事件溯源，幂等可重放）。

### 11.2 进度系统

XP 等级、streak（冻结保护 + 补签卡，断签惩罚从轻）、徽章（词汇量里程碑/错题清零/模考上岸/盲打通关/写作连优）；Mascot「必必」成长换装。

### 11.3 学习地图（阶段 = 大陆）

拼读湾 → 中考岛 → 高考山 → 四六级桥 → 考研峰 → 留学海 → 职场城；知识点 = 关卡，**错题变式 = 复仇战**，阶段模考 = Boss。

### 11.4 小游戏（TUI 全部可玩）

单词翻牌/消消乐（SRS 队列驱动）、打字射击（§9）、限时连击冲刺、词根合成器、Boss 战（10 题限时评级掉落）。**玩的就是复习。**

### 11.5 经济、排行与健康

金币 → 道具（提示/排除/双倍 XP/补签卡）+ 主题装扮；排行榜按**努力分**（学习量 + 进步幅度）而非正确率；每日上限/久坐提醒/未成年人模式（脱敏排行）。

> 分册：docs/13-gamify-growth。

---

## 12. 分阶段学习者支持矩阵

| 阶段 | 词表 | 重点题型 | 写作标准 | 阶段特色 |
|---|---|---|---|---|
| **小学** | 新课标 ~800 | 图词卡、拼写、听写、趣味选择 | 句子仿写 | 自然拼读、TTS、游戏化、全中文 UI |
| **初中·中考** | 1600 | 语法单选、完形、阅读、首字母填空 | 应用文/看图写话 80 词 | 语法树打底、真题 |
| **高中·高考** | 3500 | 语法填空、七选五、完形、**读后续写/概要** | 高考 25 分档 rubric | 义项深度、长难句、倒计时计划 |
| **四六级** | 4500/6000 | **段落翻译**、选词填空、长阅读 | 议论文 120-180 词 | 中国文化翻译专题、真题估分 |
| **考研** | 5500 | 完形 20、阅读、新题型、**英译汉** | 图画大作文 + 小作文 | 长难句图解、熟词僻义 |
| **雅思/托福** | 8000+ | 图表小作文、议论文、综合写作 | band descriptors | 口语话题卡、计时模考 |
| **职场/成人** | 行业词包 | 邮件、会议纪要、简历、面试 | 商务写作 rubric | 语域评分、一键润色、托业/BEC |
| **其它语种** | per lang | jlpt/topik/delf/daf/dele 可插拔 | per lang rubric | §17 分级开放 |

差异全部收敛到 `data/stages/<lang>/<track>.json`（词表、rubric、题型开关、错因权重、每日预算）。

> 分册：docs/16-config-i18n。

---

## 13. 扩展功能（全部纳入路线图 §21）

> 每项标注【责任模块 · 里程碑】。已在 §23 落册的以分册定义为准（功能归属见 docs/51）。

**听说闭环**（speech）
1. 全题目 TTS（piper/edge-tts）+ **听写题型**（输入复用打字引擎）【speech/typing · M8】
2. 影子跟读 + ASR 评分（转写后 Jev 判内容 + 发音节奏评估）【speech · M8】
3. 雅思口语话题卡 + 口语批改【speech/gen · M8】

**考试题型补全**（core/gen）
4. 完形、阅读、七选五、语法填空【core/gen · M2】
5. 真题套卷 + 计时模考 + 估分【engine · M5】

**中式英语专项**
6. **L1 负迁移错式库**（`data/chinglish/`）改错训练，Jev 归类错式【grader/gen · M3】
7. 语域训练：口语↔书面切换【writing · M5】

**记忆科学**
8. 假对检测（§5.3）【srs · M4】
9. 交错练习 + 测验效应【srs · M4】
10. 个性化造句：用户日记/错题语料造例句【gen · M6】

**写作深水区**
11. 长难句图解专项【writing · M6】
12. 概要写作 + 句子合并/拆分【writing · M6】

**协作与角色**（依赖 §15）
13. 班级/教师：布置题包作业、班级共性错点【httpd/web · M7】
14. 题包共享广场【§14 · M7】

**产品化**
15. 学习报告导出（Markdown/PDF）【engine · M6】
16. 成本控制面板（token/调用量；讲解按需 + 缓存）【llm · M3】
17. 无障碍与 CJK 排版（unicode-width）【tui/web · M1 起】

> 分册：见各【】对应分册；全景对照 docs/03-engine-ui §10。

---

## 14. 题包共享系统（纯本地 / 口令码 / S3 / 官方服务端）

### 14.1 内容与引用

- `.qpack` 是**唯一内容格式**；共享 = 内容自寻址 `ambi://pack/<sha256>` + semver + 作者/许可；
- `PackRef` 解析顺序：本地库 → S3 → 服务端；拉取后 hash 校验防篡改；
- **口令短包**：gzip + base64 口令码，IM/剪贴板传播，纯离线可用。

### 14.2 存储后端抽象（三档可混用）

```rust
trait PackStore { async fn put(&self, pack: &QPack) -> Result<PackRef>;
                  async fn get(&self, r: &PackRef) -> Result<QPack>;
                  async fn search(&self, q: &Query) -> Result<Vec<PackMeta>>; }
```
`LocalDirStore`（文件/剪贴板/口令码）｜`S3Store`（用户自配 MinIO/OSS/COS/R2/AWS + index.json 清单）｜`ServerStore`（服务端检索/评分/推荐/审核）。
分享 = 上传 + 短链（Web 附二维码）；**关注更新**：follow 的包新版提示，本地作答按 qid hash 关联不丢。

### 14.3 安全与治理

导入前 schema 校验 + 质检分展示；ed25519 作者签名防冒名；官方服务端审核/举报下架。

> 分册：docs/14-cloud。

---

## 15. 可选服务端与 Web 全功能（云可选、本地优先）

### 15.1 配置决定行为（关键原则）

- `server_url` **留空 = 纯本地**：无账号无同步；共享走文件/口令码/S3；LLM/Jev 用用户自己的 key（BYOK）。**所有学习功能照常可用**；
- **填入 = 云增强**：账号、多端同步、题包广场、班级/教师、**代管 LLM/Jev key（服务端出题/判题，用户无需配 key）**；
- 数据所有权始终在本地 SQLite，一键全量导出。

### 15.2 只有两个入口：TUI 纯用户端，Web = 用户端 + 管理面板

```
ambition        # 入口一：TUI —— 纯用户端（本地优先，可选连接服务端）
ambition web    # 入口二：Web  —— 用户端 + 管理面板（面板内开关「服务端功能」）
```

- TUI **只能是用户端**（配对码登录连服务端；无任何服务端职责）；
- Web 默认单用户本地应用；**管理面板「服务端功能」总开关**打开后同进程激活多用户路由，实例即变为服务端（子开关：注册/同步/广场/代管 key/班级；持久化、重启保持）；
- 「自建服务端」= 任意机器跑 `ambition web` → 打开开关；官方托管 = 常开 + Postgres/S3；
- 运维全部收进 Web 管理面板（用户/配额/审核/审计/备份），不设第三个入口。

### 15.3 服务端能力（开关开启后生效）

账号 + **多端同步**（本地 append-only `sync_log` 上行合并；FSRS 时间戳 LWW）；**学习情况看板**（本人/家长/教师）；**出题判题代理**（代管 key + 配额计费 + 用量面板）；**题包广场 + 班级/教师**（作业、班级共性错点）；REST + WS，OpenAPI 生成 TS 客户端。

### 15.4 服务端内部设计（httpd 内 server 模块）

| 模块 | 职责 |
|---|---|
| `auth` | 邮箱/OAuth + **设备配对**（TUI 6 位配对码，网页确认即登录）+ token 轮换 |
| `sync` | `pull(cursor)`/`push(batch)` 幂等合并 + WS 在线推送 |
| `registry` | 广场：上传/检索/版本/评分/举报/审核/关注更新 |
| `proxy` | LLM/Jev 代管：加密 key + 配额/计费/缓存记账（BYOK 不走代理） |
| `analytics` | 看板聚合（同一套画像函数，从 sync 事件重放） |
| `class` | 班级/教师/作业/班级共性错点 |
| `admin` | 服务端功能开关、用户/封禁/审核、审计日志（强鉴权 + 首启管理员口令） |

服务端数据（Postgres/SQLite 同代码 sqlx）：`users/devices/sessions/sync_events/packs_registry/classes/memberships/assignments/submissions/usage_ledger/quotas/provider_keys/audit_log`；qpack 本体走 S3（复用 `S3Store`）。

### 15.5 Web 端与服务端融合：一个部署单元 = 完整 Web 产品

`web/dist` 用 `rust-embed` 内嵌进同一二进制，同一 axum 应用托管 API + WS + 静态 SPA + fallback。开启服务端功能后浏览器打开即完整产品（Web 独占：图片题/听力音频/富文本批注/图表报表/扫码/OCR 校对）；TUI 媒体题文本降级。开发期 `--web-dist` 外挂热更；CI 先 build web 再 cargo build。

> 分册：docs/14-cloud、docs/03-engine-ui、docs/14-cloud（ServerStore）。

---

## 16. 翻译工作台（独立功能）

> 与考试翻译题（§8.5）互补：工作台管**随手翻译与对照学习**，考试题管**判分**。LLM 出译文，评分仍走 grader。

- **即时翻译**：任意文本 L1↔L2（LLM + 缓存 + 术语表一致）；划词/整段/素材选区（`MaterialRef`）；
- **双语对齐**：句/短语对齐高亮（直译/意译/语序重排标注）；
- **我来翻**：用户先译 → 参考译文 → 差异对比 → 可选评分（grader；非英语目标语降级 §17）→ 差异点生成学习卡；
- **语境查词**：划词 → 义项 + 语境义项判定；**学习沉淀**：生词本/笔记/个人模板/句式库一键沉淀，已学词高亮复现；
- 与题库互链：工作台句一键转翻译/改写练习题。

> 分册：docs/11-translate。

---

## 17. 多语种兼容（i18n，横向约束）

- **三个语言维度**：`ui_lang` 界面语 / `l1` 母语 / **`l2` 学习目标语**（默认 en）；`LangTag`（BCP-47）贯穿全部内容类型；
- **LanguagePack 插件**：按语种打包「词库源 `LexiconSource` / 文本 normalizer / 语音 VoiceSet / 键盘布局 / 考试轨 / 句式库」；缺包按能力降级（无词库语种只开翻译/写作）；
- **Jev 多语策略**：英语全流水线（阈值 0.75/0.35）；其它语种阈值提高（0.85/0.2）+ LLM 粗判 + 待裁定占比提高，分数 UI 明示**参考值**、只同 lang 比较；`l1` 永不进 state；
- 考试轨可插拔（`data/stages/<lang>/<track>.json`：jlpt/topik/delf/daf/dele…）；
- 分级开放：架构留位（M1 起）→ 翻译+写作先行 → 词库到位开全量（M9，建议首开 ja）。

> 分册：docs/16-config-i18n。

---

## 18. OCR 图文提取（素材入口）

- 图片（拍照/截图/粘贴/文件）→ 文本 → **素材中枢**（§19），下游共用：翻译、选词填空、出作文题、打字通素材、听写、笔记；
- 多引擎（本地 PaddleOCR/Tesseract + 视觉 LLM 兜底手写/公式/复杂版面）按块投票 + 版面保持（表格→Markdown、行尾连字合并）+ **低置信校对回路**；
- 原 OCR 文本保留 provenance（可 diff），校对修正回流；多语种识别（配合 §17）。

> 分册：docs/17-content-hub。

---

## 19. 素材中枢（数据共用方法）

- **素材只存一份，派生全部注册**：OCR 文本、TTS 音频/转写、粘贴文本、网页剪藏、导入书籍统一 `SourceDoc`；选区即 `MaterialRef{doc_id, span}`（引用不复制）；
- **共用五方法**：①单份存储（内容寻址去重）②span 引用③派生注册 `derivations`（可追溯/可重建 `rebuild`/失效策略）④缓存共用（`llm_cache` key 含素材 hash）⑤参数哈希 `params_hash`（同参数派生复用）；
- **TTS/听力共用面**：合成音频按 `hash(text+voice)` 存 `media_assets`（同句多题只合成一次）；ASR 转写文是孪生 SourceDoc 可再派生；
- 生命周期：素材删除默认派生标 orphan（学习数据不丢），可选级联；素材带 rights 版权标记（分享合规）。

> 分册：docs/17-content-hub、docs/00-overview（S7/S8 场景走查）。

---

## 20. 数据模型（SQLite 主要表；完整 DDL 见 docs/02-core-store）

```
words / word_relations / collocations / roots            -- **只读资源文件**（data/lexicon 或服务端/S3 托管；**不进 SQLite**，无云时用本地 data/，02-core-store 存储分层）
patterns                                               -- 固定句式库（lang，含 traps）
questions / question_links / papers / paper_runs        -- 题目/变式链/套卷/作答
attempts / grades / error_tags / arbitration            -- 作答/判分/错因/待裁定桶
srs_queue                                              -- FSRS 卡片（词义项/错题/笔记/句式/模板句）
essays / essay_grades / templates                       -- 写作（含个人模板）
notes / diary / todos                                   -- 笔记/日记/TODO
typing_sessions                                         -- 打字（击键流可回放）
profile_snapshots                                       -- 画像快照
game_state / game_events                                -- 游戏（事件溯源，可关）
packs / shared_packs                                    -- 题包/共享（source: local|s3|server）
sync_log / accounts                                     -- 同步事件日志 / 可选云账号
llm_cache / llm_usage                                   -- 讲解缓存 / 成本面板
translations / tx_favorites                             -- 翻译工作台历史/收藏
source_docs / media_assets / derivations                -- 素材中枢（单份存储/资产共用/派生注册）
```

> 服务端独有表（users/sync_events/…）见 docs/15 §4；导出备份（全量 JSON/Markdown/DB 备份）见 docs/02-core-store §5。

---

## 21. 路线图（MVP → 完整）

| 里程碑 | 内容 |
|---|---|
| **M1 骨架** | core + store + engine + TUI 答题流；单词四选一/填空**规则判题**；本地题库；词表导入；**打字通 v1**；游戏层底座（事件总线 + streak/XP/徽章）；多语种字段留位 |
| **M2 出题** | LlmProvider + 结构化出题 + Jev 质检回路 + qpack 导入导出；**题型补全**（完形/阅读/七选五/语法填空） |
| **M3 Jev 判题** | jev 客户端；填空模糊判分、多选部分分、错因分类；置信度路由 + 待裁定桶；**中式英语错式库** + 成本面板 |
| **M4 背单词** | FSRS、错词本、同反义词、**一词多义专项**（辨析/熟词僻义/干扰对/全义项毕业）、**固定句式库 + 五模式**、假对检测、交错练习/小测；**小游戏**（翻牌/消消乐、打字射击） |
| **M5 写作** | 翻译/作文 composite + LLM 批注 + 加分项 + 三档范文；**模板作文**（填槽三阶段 + 反模板库 + 个人模板）；真题套卷 + 计时模考 + 估分；语域训练；**翻译工作台** |
| **M6 闭环** | 笔记/日记/TODO + 错因画像 + 个性化出题（语料造句）+ 写作加强计划 + 周报/报告导出；长难句 + 句子合并/拆分/概要；**游戏全量**（学习地图/Boss/词根合成器/经济） |
| **M7 云可选** | httpd 服务端功能（管理面板开关：账号/同步/看板/**出题判题代理**）+ 题包共享（口令码/S3/服务端）+ 班级/教师 + **Web 全功能**（内嵌 SPA） |
| **M8 听说** | TTS 全题目朗读、听写题型、影子跟读 + ASR、雅思口语卡 |
| **M9 多语种** | LanguagePack 插件层 + 首个新语种（建议 ja）词库/句式/考试轨 + Jev 多语策略全量落地 |
| **M10 素材中台** | OCR 图文提取（多引擎 + 校对回路）+ 素材中枢（五方法）+ 听力素材流水线（TTS 资产共用） |
| **M11 增强包 P0** | 谐音梗记忆法生成器 + 语法体系树/微课 + AI 私教面板 + 多轮口语陪练/口述作文 + 生态导入导出（§23.1–23.5） |
| **M12 增强包 P1** | 每日精读/睡前微复习/冲刺模式 + 学习年历/作文档案馆/分享卡/自习室/周赛/家长周报（§23.6–23.7） |
| **M13 深水区包** | 真题级干扰项 + 卷面模拟 + 幽灵义项/辨析卡 + 讲解可玩化/习惯日课 + 出课工作室（一键出课/DSL）+ 家长控制台（§23.8–23.12） |
| **M14 触点与回顾** | 词源文化故事卡 + 桌面/锁屏背词小组件 + 年度学习报告（§23.13–23.15） |
| **M15 心流与捕词** | CAT 自适应难度引擎 + 每日精听/私人电台 + 系统级划词取词（§23.16–23.18） |
| **M16 深智包** | 写作观点素材库 + 错题溯源根因 + 实时对战答题 + 学情预警（§23.19–23.22） |

---

## 22. 风险与对策

| 风险 | 对策 |
|---|---|
| Jev 中文（CJK）准确率低 | state 只放 l2 原文；中文/母语讲解走 LLM/规则 |
| Jev 无 Rust SDK | 自研薄 HTTP 客户端（含重试/并发池） |
| LLM 出题质量不稳 | §4.2 Jev 质检回路 + 合格入库门槛 |
| 评分可比性 | 分数只由规则 + Jev 出；rubric/措辞版本化，改口径必升 version |
| LLM 成本 | 判题主力 Jev 扇出；讲解按需 + 缓存（key 含内容 hash） |
| 题包格式变动 | schema 版本号，导入时迁移/拒绝并提示 |
| 云/本地行为不一致 | 云全程可选；`server_url` 未配置全走本地路径；双路径 feature 开关统一测试 |
| 同步冲突 | attempt/日记 append-only 合并；FSRS 时间戳 LWW；双写笔记提示手动合并 |
| 隐私边界 | 默认纯本地；同步可选端到端加密；日记仅请求批改时外发（断言测试）；家长/教师看板需授权；未成年人脱敏 |
| S3/服务端治理 | S3 用户自带 bucket；官方服务端限额、审核、举报下架 |
| 单二进制维护复杂度 | 逻辑全在库 crate，`tui`/`httpd` 两壳只做交互与路由；sqlx 双后端同一迁移 |
| 运行时开启服务端功能扩大攻击面 | 管理面板强鉴权（首启口令/TOTP）；开关变更审计；默认关 + 默认绑定 loopback |
| Web 产物内嵌更新需重发二进制 | `--web-dist` 外挂静态目录；CI 产物含前端构建 |
| 多语种 Jev 准确率不足 | 非英语阈值提高 + LLM 粗判/待裁定；分数明示参考值（§17） |
| 多语种词典数据缺口 | `LexiconSource` 插件化 + 分级开放（无词库语种只开翻译/写作） |
| OCR 准确率（拍照/手写/表格） | 多引擎投票 + 视觉 LLM 兜底 + 低置信校对回路；原 OCR 文本可 diff（§18） |
| 素材版权 | rights 标记（UserOwned/FairUse/Unknown）；分享含素材题包需用户确认 |
| 派生数据一致性 | 派生注册 + params_hash；素材修正 → rebuild；删除默认 orphan 保学习数据（§19） |
| 私教/陪练的 LLM 成本 | ctx 摘要化 + 截断；回合数上限；讲解/口播全走缓存与资产共用（docs/18-learning-loop/15-speech） |
| 谐音梗质量/内容风险 | Jev 双检（联想成立/内容得体）+ 票选排序；投稿同过质检（docs/29） |
| 社交功能的服务端依赖 | 无 `server_url` 降级：单人番茄钟/本地人机赛/本地周报（docs/35） |
| 第三方卡片格式变动 | importer 魔数嗅探 + fixture 契约测试；解析失败给 CSV 模板兜底（docs/33） |
| 自定义 DSL 出题质量 | DSL 生成同样过 §4.2 质检回路；模板白名单 + schema 校验；批量限速与断点续跑（docs/39） |
| 小组件锁屏隐私 | 锁屏默认只显示词卡不含学习明细；可整体关闭；反馈仅本地回传（docs/42） |
| CAT 冷启动/难度漂移 | 词汇量测试锚定 + Elo 步长退火 + floor/ceiling；心流命中率周报监控（docs/44） |
| 系统级取词隐私 | 本地词库优先零出站；带语境查询显式外发；捕词默认只存词；辅助功能授权用途明示（docs/46） |
| 学情预警误报 | 只给趋势区间不给断言；阈值保守宁漏报；误报反馈闭环调参；可整体关闭（docs/50） |

---

## 23. 增强功能包（P0 / P1 / 深水区 / 追加）

> 36 项增强特性已**归并进实现顺序分册**（docs/00–19）；下表保留原编号（§23.x）作为路线图锚点（M11–M16 引用）。
> 归属与去重规则见 docs/00-overview §5–§6。暂缓池：多语种对照卡、口述日记、口语考官、对赌契约、费曼模式、零 AI 纯净模式、学习伙伴人格。

| # | 特性簇 | 归并入 | 里程碑 |
|---|---|---|---|
| 23.1 | 谐音梗记忆法生成器 | docs/19-memory-cards | M11 |
| 23.2 | 语法体系树 + 微课 | docs/18-learning-loop | M11 |
| 23.3 | AI 私教面板 | docs/18-learning-loop | M11 |
| 23.4 | 多轮口语陪练 / 口述作文 | docs/15-speech | M11 |
| 23.5 | 生态导入导出 | docs/17-content-hub | M11 |
| 23.6 | 学习节奏包（精读/微复习/冲刺） | docs/18-learning-loop | M12 |
| 23.7 | 成长展示与共学 | docs/13-gamify-growth | M12 |
| 23.8 | 应试真实感（干扰项/卷面） | docs/06-generation + docs/10-writing | M13 |
| 23.9 | 词汇深水区（幽灵义项/辨析卡） | docs/09-vocabulary | M13 |
| 23.10 | 讲解练习化（可玩化/习惯日课） | docs/18-learning-loop | M13 |
| 23.11 | 出课工作室（一键出课/DSL） | docs/06-generation | M13 |
| 23.12 | 家长控制台 | docs/13-gamify-growth | M13 |
| 23.13 | 词源文化故事卡 | docs/19-memory-cards | M14 |
| 23.14 | 背词小组件 | docs/13-gamify-growth | M14 |
| 23.15 | 年度学习报告 | docs/13-gamify-growth | M14 |
| 23.16 | CAT 自适应难度 | docs/08-adaptive | M15 |
| 23.17 | 每日精听 + 私人电台 | docs/18-learning-loop | M15 |
| 23.18 | 系统级划词取词 | docs/17-content-hub | M15 |
| 23.19 | 写作观点素材库 | docs/10-writing | M16 |
| 23.20 | 错题溯源根因 | docs/12-notes-profile | M16 |
| 23.21 | 实时对战答题 | docs/13-gamify-growth | M16 |
| 23.22 | 学情预警 | docs/18-learning-loop | M16 |

---

## 24. 分册文档索引（按实现顺序 00–19；平行功能已合并）

| # | 分册 | 实现 | 并入来源（旧分册） |
|---|---|---|---|
| 00 | [overview](docs/00-overview.md) | 先读 | 00-dataflow + 51-ownership |
| 01 | [engineering](docs/01-engineering.md) | 先读 | 25-code-style + 26-naming + 27-code-quality + 28-dev-workflow |
| 02 | [core-store](docs/02-core-store.md) | M1 | 01-core + 02-store |
| 03 | [engine-ui](docs/03-engine-ui.md) | M1/M7 | 19-engine + 16-tui-web |
| 04 | [typing](docs/04-typing.md) | M1 | 09-typing |
| 05 | [ai-adapters](docs/05-ai-adapters.md) | M2/M3 | 03-llm + 04-jev |
| 06 | [generation](docs/06-generation.md) | M2/M13 | 06-gen + 39-studio + 36-exam-realism（干扰项引擎） |
| 07 | [grading](docs/07-grading.md) | M3 | 05-grader |
| 08 | [adaptive](docs/08-adaptive.md) | M4/M15 | 07-srs + 44-cat |
| 09 | [vocabulary](docs/09-vocabulary.md) | M4/M13 | 08-vocab + 20-patterns-polysemy + 37-vocab-depth |
| 10 | [writing](docs/10-writing.md) | M5/M16 | 10-writing + 47-argument-bank + 36-exam-realism（卷面模拟） |
| 11 | [translate](docs/11-translate.md) | M5 | 21-translate |
| 12 | [notes-profile](docs/12-notes-profile.md) | M6/M16 | 13-notes + 12-profile + 48-traceback |
| 13 | [gamify-growth](docs/13-gamify-growth.md) | M6+ | 11-gamify + 35-growth-social + 40-family-console + 42-daily-widget + 43-annual-report + 49-quiz-battle |
| 14 | [cloud](docs/14-cloud.md) | M7 | 14-pack-share + 15-httpd-server |
| 15 | [speech](docs/15-speech.md) | M8/M11 | 17-speech + 32-speaking-lab |
| 16 | [config-i18n](docs/16-config-i18n.md) | M9 | 18-stages-config + 22-i18n |
| 17 | [content-hub](docs/17-content-hub.md) | M10/M13/M15 | 24-material-hub + 23-ocr + 33-importers + 46-global-lookup |
| 18 | [learning-loop](docs/18-learning-loop.md) | M11–M16 | 30-grammar-tree + 31-ai-coach + 38-practice-loop + 34-study-loop + 45-listening-radio + 50-early-warning |
| 19 | [memory-cards](docs/19-memory-cards.md) | M11/M14 | 29-mnemonic + 41-etymology |
