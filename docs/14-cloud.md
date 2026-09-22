# 14 · cloud — 题包共享 + 可选服务端 / Web 融合（M7）

> 实现顺序：**M7**。并入来源：旧 `14-pack-share` + `15-httpd-server`。
> 云全可选：`server_url` 留空 = 纯本地全功能；填入 = 云增强。**只有两个入口**：`ambition`（TUI 纯客户端）/ `ambition web`（用户端 + 管理面板）。

## 1. qpack 题包（唯一内容格式）+ PackStore 三后端

```rust
pub struct QPack { schema_version, manifest: Manifest{pack_id, name, version, author, license, track, tags, signature}, questions }
pub struct PackRef(pub String);   // ambi://pack/<hash> | https://… | 口令码（gzip+base64，≤4KB IM 传播）
pub trait PackStore { async fn put(&self, pack) -> Result<PackRef>; async fn get(&self, r) -> Result<QPack>; async fn search(&self, q) -> Result<Vec<PackMeta>>; }
// LocalDirStore（文件/剪贴板/口令码）| S3Store（MinIO/OSS/COS/R2/AWS + index.json 乐观锁）| ServerStore（检索/评分/推荐/审核）
// 每题可随包资产：explanations[]（讲解）/ acceptable（裁决回流增强）/ quality_score —— 别人导入即得讲解（06-generation §7）
// resolve：本地→S3→服务端，hash 校验防篡改；ed25519 签名防冒名；follow 关注更新（作答按 qid hash 关联不丢）
// import：schema 校验+去重+低质拒收（06::qc 分展示）
```

### 1.1 本地 SQLite 的角色（有服务端也永远用）

| 形态 | 业务数据存哪 | 说明 |
|---|---|---|
| TUI / `ambition web` 本机（客户端） | **本地 SQLite 始终是主存储/事实源** | 服务端只是同步/广场/看板/代管增强；断网续学、联网补同步 |
| 纯浏览器托管用户（不装客户端） | 服务端库（多租户，**事件日志为事实源**） | 无本地 SQLite；随时全量导出（JSON/qpack）**迁回本地客户端** |

- 分工：服务端库只存账号 / `sync_events` 事件日志 / 广场 / 班级 / 配额 / 代管 key —— **聚合与中转，不替代客户端库**；
- 同步语义（00-overview S5）：本地为 source of truth **上行**（append-only `sync_log`），**下行**幂等 apply（FSRS LWW）——两端各持副本，互不依赖存活；
- 数据所有权 / 重设不丢（16-config-i18n §5.1）/ 全量导出——全部锚定“本地 SQLite 始终存在”；
- 云端计算（代管出题/判题）的**结果仍落本地库**；词库等只读资源不受影响（16-config-i18n §3.1）。

### 1.2 全量恢复（换机/重装：填 `server_url` 即得全部历史）

> 契约：**填入服务器 = 可恢复所有历史内容**。同步不是“部分镜像”，是**完备备份**。

- **完备性基础**：`sync_log` = 业务写入的**通用变更日志**（entity/op/payload；engine 写入钩子统一投影，DomainEvent 只是其中语义化子集）——覆盖 attempts/grades/srs_queue/essays/notes/diary/todos/typing/explanations/mnemonics/game/沉淀题库等**全部业务实体**；
- **恢复流程（新设备/重装）**：
  1. 安装 → 首启向导⑤填 `server_url`（或 Settings）→ 配对码登录；
  2. 首次 `sync_now` 进入**全量恢复模式**：`pull(cursor=0)` 分页拉全部事件 → `apply` 幂等重建业务库；
  3. **快照加速**（可选）：服务端保留周期全量快照（`export_all_json` 产物）→ 下快照 + 增量事件 = 秒级恢复；快照只是加速，**事件日志才是历史事实**；
  4. 完成后本地即完整历史 → 转入常态增量同步；
- **词库也同步（两层）**：①**词库资源包**从服务端/S3 托管拉取（包带 `version`，多设备**锁定同版**→ word_id/sense_id 跨设备稳定，02-core-store 引用约束）；②**用户词库增量**（自定义词条/释义修正/个人标注 sense_variants）作为业务数据走 sync_log；
- **不同步的边界（安全设计）**：密钥（keychain 本机）、config 本机项（bind/web_dist）；可选 E2EE：payload 客户端加密上行，服务端只存密文，恢复时本地解密；
- 冲突沿用 §1.1（append-only + FSRS LWW）；恢复边界 = 账号（同一用户的所有设备）。

## 2. 服务端能力（`ambition web` → 管理面板「服务端功能」开关）

- 开关持久化（重启保持）；子开关：注册/同步/广场/代管 key/班级；默认关 + 默认绑定 loopback；首启管理员口令 + 审计日志；
- 模块（httpd 内 server 路由组）：

| 模块 | 职责 |
|---|---|
| auth | 邮箱/OAuth + **设备配对**（TUI 6 位码→网页确认→device token）+ token 轮换 |
| sync | `pull(cursor)`/`push(batch)` 幂等（event_id 去重）+ WS 在线推送；本地 append-only `sync_log` 上行；FSRS 时间戳 LWW |
| registry | 广场：上传/检索/版本/评分/举报/审核/关注更新 |
| proxy | LLM/Jev 代管（key 加密存储 + 配额/计费/usage_ledger/缓存）；BYOK 直连不经代理（S6） |
| analytics | 学习看板（画像/周报/时长——从 sync 事件重放，同一套 12 聚合函数）；grant_parent 授权 |
| class | 班级/教师/作业（qpack 指派）/班级共性错点 |
| admin | 服务端功能开关、用户/封禁/审核（举报下架）、审计 |

- 服务端数据（Postgres/SQLite 同代码）：`users/devices/sessions/sync_events/packs_registry/classes/memberships/assignments/submissions/usage_ledger/quotas/provider_keys/audit_log`；qpack 本体走 S3（复用 S3Store）。

## 3. Web 融合（一个部署单元 = 完整 Web 产品）

- `web/dist` rust-embed 内嵌同一二进制：API + WS + 静态 SPA + fallback；开启服务端功能后浏览器打开即完整产品；
- Web 独占体验：图片/听力/富文本批注/图表报表/扫码/OCR 校对（03-engine-ui §3 映射）；
- 开发期 `--web-dist` 外挂热更；CI 先 build web 再 cargo build；OpenAPI 生成 TS 客户端；
- 「自建服务端」= 任意机器跑 `ambition web` → 管理面板开开关；官方托管 = 常开 + Postgres/S3。

## 4. API 分组

`/auth`（pair_start/pair_confirm/login/refresh/logout）· `/sync`（push/pull/live-WS）· `/packs`（CRUD/rate/report）· `/proxy/{llm,jev}` · `/me`（dashboard/report）· `/classes` · `/admin` · 本地单用户 `/api/local/*` + SPA fallback。

## 5. 验证点

- [ ] qpack round-trip/hash 防篡改/口令码 4KB 边界/S3 乐观锁重试/apply_update 保作答；
- [ ] 开关关闭时 server 路由 404；sync push 幂等 + pull 无遗漏；proxy 配额 402 映射；
- [ ] **连服务端后断网续学**（本地库完整可用）；同步不覆盖本地未传数据；托管用户导出可迁回本地（§1.1）；
- [ ] **全量恢复对账**：空库 + server_url → 历史逐表行数/内容一致；快照+增量 ≡ 纯事件重放；E2EE 下恢复可解密；密钥/config 本机项不出网（§1.2）；
- [ ] **词库同步**：多设备拉同版词库包后 word_id/sense_id 引用一致；用户词库增量（自定义词条/标注）随 sync_log 往返不丢。
- [ ] 配对码一次性/防爆破；审计覆盖开关/授权/审核；OpenAPI→TS 编译通过；`--web-dist` 热更路径。
