# 00 · overview — 总览与治理（数据流 / 单写者 / 功能归属 / 去重）

> 实现顺序：**先读**。并入来源：旧 `00-dataflow` + `51-ownership`。
> 内容契约：全库数据流方向、单写者矩阵、端到端场景、功能归属唯一实现、去重门禁。

## 1. 架构一句话

两壳一引擎：`tui`（纯用户端）+ `httpd`（Web 用户端 + 管理面板「服务端功能」开关）只调 `engine` 门面（03-engine-ui）；AI 出口三个（LLM/Jev/S3·服务端），外部永不直写业务表。

## 2. 数据流方向（红线）

```
静态数据(16-config) → gen/writing/…     作答 → grader(07) → store(02) → 旁路事件 → gamify/sync/profile
UI(03) ⇄ engine(03) → 落库唯一通道       外部(05-ai-adapters) 只被 gen/grader/writing 调用，回包经适配层
素材入口(17-content-hub：OCR/导入/划词/剪藏) → source_docs 单份存储 → 派生注册
```

- 外部红线：LLM/Jev/S3 永不直接写业务表；回包先进适配层（05）→ 被 06/07/10 消费 → engine 统一落库。
- **存储分层**：SQLite = 业务数据；词库/内置内容 = **只读资源文件**——无服务端/S3 时在本地 `data/` 目录，配置后从托管拉取（16-config-i18n §3.1）。

## 3. 端到端场景（验证基线 S1–S8）

| # | 场景 | 关键链路 |
|---|---|---|
| S1 | 做题（含假对） | engine.submit → 07 三级判分（规则→Jev→待裁定）→ 单事务落库 → srs/gamify/sync/profile 旁路 |
| S2 | 复习 | 08.due_queue → 组题（模式升档）→ S1 → 08.schedule |
| S3 | 作文闭环 | 10.scaffold → 提交 → 07 composite + 10 批注 → 12 画像 → 专项 drills → 复批 |
| S4 | 分享题包 | 06.build_qpack → 14.PackStore.put（本地/口令码/S3/服务端）→ hash 校验导入 |
| S5 | 双端同步 | sync 投影 sync_log（完备变更日志）→ /sync/push → 另端 pull → apply_remote（append-only / FSRS LWW） |
| S9 | **全量恢复**（换机/重装） | 填 server_url → 配对登录 → `pull(cursor=0)` 全量重放（或快照+增量）→ 本地重建**全部历史** + 拉取同版词库包（14-cloud §1.2） |
| S6 | 代管出题判题 | engine → /proxy/{llm,jev}（服务端 key）→ 同款 gen/grader 代码 → usage_ledger |
| S7 | 拍照素材六路派生 | 17-content-hub ingest → source_docs+1 → span 派生（翻译/出题/打字/听力/笔记）→ derivations+n |
| S8 | 听力资产共用 | derive_listening → hash(text+voice) 查 media_assets → 合成一次多处复用 |

## 4. 单写者矩阵（每表唯一写入方）

| 表 | 写入方 | 读方 |
|---|---|---|
| questions/question_links/papers | 06-generation | 07/12/UI |
| attempts/grades/error_tags/arbitration | 07-grading（经 engine） | 08/12/gamify/sync |
| srs_queue | 08-adaptive | 04/18/UI |
| 词库（words 系列） | **只读资源文件**（16-config 构建；不进 SQLite） | 09-vocabulary 查询服务 |
| patterns/sense_variants/etymology_cards/arguments/mnemonics（用户/生成部分） | 对应服务（09/10/19） | 全体 |
| essays/essay_grades/templates | 10-writing | 12/UI |
| notes/diary/todos | 12-notes-profile | 06/04 |
| typing_sessions | 04-typing | 12/gamify |
| profile_snapshots/warnings/cat_state | 12-notes-profile / 18-learning-loop / 08-adaptive | 06/14/server |
| game_state/game_events | 13-gamify-growth | UI/排行 |
| packs/shared_packs/accounts | 14-cloud | UI |
| sync_log | engine 写入钩子 + 事件投影（**完备变更日志**：覆盖全部业务实体写操作） | 14-cloud 上行 / 全量恢复重放 |
| source_docs/media_assets/derivations/translations | 17-content-hub / 11-translate | 全体消费方（span 引用不复制） |
| llm_cache/llm_usage | 05-ai-adapters | 成本面板 |
| explanations | 06-generation（判题/讲解沉淀） | qpack 导出导入、10/18 讲解展示 |
| radio_episodes/annual_reports/typing 相关 | 18-learning-loop / 13-gamify-growth | UI |

## 5. 横切能力唯一实现（禁止平行实现）

| 能力 | 唯一实现 | 其它只能 |
|---|---|---|
| 判分（composite 同） | 07-grading | 调用 |
| 难度/选题 | 08-adaptive::cat | GenSpec.difficulty 必须来自 CAT |
| 间隔排程 | 08-adaptive::srs | 调用 |
| 游戏结算 | 13-gamify-growth::round_settle | 调用 |
| 素材沉淀 harvest | 17-content-hub | 委托 |
| 语境查词 | 11-translate | 17 只做 UI 面分流 |
| 估分 | 12-notes-profile::predict_score | 调用 |
| 讲解文本 | 06-generation::explain | 18 可玩化/口播包装 |
| TTS/ASR | 15-speech | 调用；音频资产走 17 |
| 生成质检 | 06-generation::qc | 必过 |
| 每日流编排 | 18-learning-loop | 各"每日 X"只供料 |
| 待裁定桶 | 07-grading（arbitration） | 唯一 |

## 6. 功能域归属（防重复）

记忆材料=19（谐音/词源）；词汇=09；写作=10（含观点素材/卷面）；翻译=11；学情=12（含溯源）；游戏成长=13（含小组件/年报/对战/家长）；云=14（共享+服务端）；语音=15（引擎+口语流程）；配置/多语种=16；素材入口=17（OCR/导入/划词）；练习编排=18（语法树/讲解练习化/每日流/精听电台/预警/AI 私教）。**新增功能先查本节定归属，只能委托不能平行实现；新分册必须登记归属（01-engineering PR 清单）。**

## 7. 验证基线

S1–S8 各 ≥1 条集成测试；§5 表 grep 断言（同名跨册必标"委托"）；单写者矩阵断言（双写测试为负例）。
