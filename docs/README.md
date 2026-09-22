# Ambition（俺必胜） — 分册设计文档索引

> **License**：PolyForm Noncommercial 1.0 —— **非商业免费使用**（Free for non-commercial use）；商业授权另行协商

总纲见 [`../DESIGN.md`](../DESIGN.md)。**20 份分册按实现顺序编号 00–19**（平行功能已合并、冗余已删除；新旧映射见总纲 §24）。
阅读顺序：`00-overview`（数据流/归属/去重）→ `01-engineering`（工程规范）→ 按里程碑推进。

| # | 分册 | 实现 | 内容（并入来源） |
|---|---|---|---|
| 00 | [overview](00-overview.md) | 先读 | 数据流/单写者矩阵/功能归属/去重门禁（← dataflow + ownership） |
| 01 | [engineering](01-engineering.md) | 先读 | 代码风格/命名/质量门禁/协作流程（← 工程规范四件套） |
| 02 | [core-store](02-core-store.md) | M1 | 领域模型 + 持久化 DDL/Repo（← core + store） |
| 03 | [engine-ui](03-engine-ui.md) | M1/M7 | engine 门面 + TUI/Web 双端（← engine + tui-web） |
| 04 | [typing](04-typing.md) | M1 | 打字通「键胜」（← typing） |
| 05 | [ai-adapters](05-ai-adapters.md) | M2/M3 | LLM（OpenAI/Anthropic/代管）+ Jev 适配（← llm + jev） |
| 06 | [generation](06-generation.md) | M2/M13 | 出题/质检回路/DSL/出课/干扰项引擎（← gen + studio + exam-realism·干扰项） |
| 07 | [grading](07-grading.md) | M3 | 三级判题/待裁定/错因（← grader） |
| 08 | [adaptive](08-adaptive.md) | M4/M15 | FSRS 排程 + 假对 + CAT 难度（← srs + cat） |
| 09 | [vocabulary](09-vocabulary.md) | M4/M13 | 词库/义项/句式/幽灵义项/辨析卡（← vocab + patterns-polysemy + vocab-depth） |
| 10 | [writing](10-writing.md) | M5/M16 | 模板作文/加分项/批改/观点素材/卷面模拟（← writing + argument-bank + exam-realism·卷面） |
| 11 | [translate](11-translate.md) | M5 | 翻译工作台（← translate） |
| 12 | [notes-profile](12-notes-profile.md) | M6/M16 | 笔记/日记/TODO + 画像/估分/根因溯源（← notes + profile + traceback） |
| 13 | [gamify-growth](13-gamify-growth.md) | M6+ | 游戏/成长展示/对战/小组件/年报/家长（← gamify + growth-social + family + widget + annual + battle） |
| 14 | [cloud](14-cloud.md) | M7 | qpack 共享 + 可选服务端/Web 融合（← pack-share + httpd-server） |
| 15 | [speech](15-speech.md) | M8/M11 | 语音引擎 + 口语陪练/口述作文（← speech + speaking-lab） |
| 16 | [config-i18n](16-config-i18n.md) | M9 | 配置/数据文件/分阶段/多语种（← stages-config + i18n） |
| 17 | [content-hub](17-content-hub.md) | M10/M13/M15 | 素材中枢 + OCR + 导入导出 + 系统级取词（← material-hub + ocr + importers + global-lookup） |
| 18 | [learning-loop](18-learning-loop.md) | M11–M16 | 语法树/AI 私教/讲解练习化/每日流/精听电台/预警（← grammar-tree + ai-coach + practice + study-loop + listening-radio + early-warning） |
| 19 | [memory-cards](19-memory-cards.md) | M11/M14 | 谐音梗 + 词源故事（← mnemonic + etymology） |
