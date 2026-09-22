# Ambition 雷点与错误日志（PITFALLS）

> 每解决一个坑记一条；开工前必读（skill `ambition-pitfall-log` 强制）。同根因更新原条目，不新开。

## P-1 sqlx 内存库测试必须开 tokio 定时器

- **症状**：`panicked at sqlx-core/src/rt/mod.rs: "A Tokio 1.x context was found, but timers are disabled"`，整组测试全挂。
- **根因**：`Builder::new_current_thread()` 默认不开 time driver；sqlx 连接池心跳/超时需要定时器。
- **解决**：测试运行时加 `.enable_time()`，并给 tokio dev-dep 加 `"time"` feature。
- **预防**：新 crate 里凡 sqlx/带超时的异步测试，一律 `#[tokio::test]` 或 builder + `enable_time()`。

## P-2 ratatui TestBackend 宽字符续格不落笔（残留上一帧）

- **症状**：快照断言 `contains("完成")` 失败，缓冲里是 `"本1组1完 成h！"`——新旧帧字符交错。
- **根因**：宽字符（CJK）占 2 格，第二格（续格）ratatui **不写**；复用 buffer 时续格留上一帧残迹；空格续格还会让 `contains("完成")` 被拆开。
- **解决**：每个渲染状态用**全新 TestBackend**；收集字符串时**按 `unicode-width` 跳过续格**（`i += 1 + usize::from(sym.width()==2)`）。
- **预防**：凡 TUI 快照测试都走 `render_to()` 这种宽度感知收集；不要在同一终端连续 draw 后断言含 CJK 的整行。

## P-3 Windows git-bash 里 cargo/pi 不在 PATH

- **症状**：`cargo: command not found`、`pi: command not found`（脚本首行即断链）。
- **根因**：cargo 在 `~/.cargo/bin`、pi 在 `/d/nodejs`，未进 git-bash 默认 PATH；定时任务（schtasks）更无交互 shell 环境。
- **解决**：脚本统一 `export PATH="$HOME/.cargo/bin:/d/nodejs:$PATH"`；cmd 包装器里显式 export。
- **预防**：仓库内一切 `.sh` 开头统一加这行；已写入 scripts/ 模板。

## P-4 `serde_json::from_str(...).unwrap_or_default()` 需要目标类型 impl Default

- **症状**：`E0277 the trait bound … Default is not satisfied`（元组/自定义枚举无 Default）。
- **根因**：`unwrap_or_default()` 作用在**输出类型** T 上，不是 Result。
- **解决**：显式兜底 `unwrap_or((String::new(), QuestionKind::SingleChoice(…), 1, Vec::new()))`；或解析失败走 `.ok()` + match。
- **预防**：持久化反序列化永远写显式兜底（也符合“禁 unwrap”规范）。

## P-5 cargo fmt 会重排 import/折行 → 编辑锚点失配

- **症状**：批量 edit 报 `Could not find the exact text`（我手写的 oldText 与 fmt 后不一致：import 顺序、单行折行）。
- **根因**：rustfmt 会 `reorder_imports`、按宽度折行。
- **解决**：先 `cargo fmt` 再取**实际文本**（sed/grep 看原文）后编辑；或编辑后立即 fmt 复核。
- **预防**：多文件批量修改后统一跑 fmt；对“刚格式化过的文件”不要凭记忆写锚点。

## P-6 循环参数含空格会把值劈开（shell for + 空格值）

- **症状**：README 索引插入时出现垃圾行 `| [矩阵）.md](矩阵）.md) |`。
- **根因**：`for n in a:b c:d` 未加引号，描述值里的空格被 word splitting 拆成独立迭代项。
- **解决**：删除垃圾行重插；用逐锚点 `sed '/pattern/a\…'` 而不是带值循环。
- **预防**：shell 循环变量一律 `while IFS=` 读取或加引号；插入含空格文本用文件/awk，不用 for-in。

## P-7 Windows CRLF 会毁掉 shebang 钩子脚本（镜像坑：LF 毁 .cmd）

- **症状**：git 提示 `LF will be replaced by CRLF`；一旦 checkout 成 CRLF，`.githooks/*.sh` 头行变 `^M`，钩子静默不执行。
- **症状（镜像，2026-09-23）**：`3863dc8` 全仓统一 LF 后，`.cmd` 变 LF-only + 中文注释行 → cmd.exe 拆错行，跑 `continue_mvp.cmd` 报 `'�（Windows' is not recognized as an internal or external command`（实测：CRLF 版本无此错，LF 版本必现）。
- **解决**：`.gitattributes`：`* text=auto` + `.githooks/** text eol=lf` + `*.sh text eol=lf`；**补 `*.cmd text eol=crlf`** 并把两个 .cmd 转回 CRLF。
- **预防**：行尾规则成对写：shebang 脚本→lf、`.cmd`→crlf；做「全仓行尾统一」前先按扩展名分类；新入口文件先 `file` 看行尾。

## P-8 静默降级会吞掉真实错误

- **症状**：TUI 判题失败被降级成 `verdict=Pending` 展示，用户看不出是错误还是待裁定。
- **根因**：错误分支复用了 Pending 的展示路径，没有独立错误态。
- **解决**：（待修，见 PROGRESS.md 改进建议 #6）——需要独立错误行/状态。
- **预防**：错误与业务中间态（Pending）永远不同路；降级必须有可见信号。

## P-9 编辑工具锚点必须“当前文件实际文本”

- **症状**：edit `oldText` 匹配失败（README 首行、DESIGN 数据模型行等）。
- **根因**：sed 改写/手动记忆与真实文件有 1 字符差异（空格、句号、破折号前后空格）。
- **解决**：失败后先 `grep -n` 取真实行，再以它为锚。
- **预防**：多步 sed 后的文件，一律重新取证；别复用改名前的锚点。

## P-10 调度脚本与 agent 双重抢锁 → 死锁，且 rc=0 空烧阶段

- **症状**：定时唤起的施工会话只读了 SKILL/PITFALLS/PROGRESS 就输出「施工锁已被占用（pid=625）→ 等待中」退出（日志 2026-09-23 03:57 会话）；更严重：这种“等待中”退出 rc=0，会被 `continue_mvp.sh` 当成阶段完成 → 推进 MVP_STAGE，6 个阶段会被空烧完而零施工。
- **根因**：`continue_mvp.sh` 先自抢锁再唤起 pi，而 skill 红线要求 agent 自己再抢同一把锁 → agent 永远抢到的是外层脚本的锁 → 按红线只能退出；同时“没干活但正常退出”与“完成”都= rc0，无法区分。
- **解决**：①`task_lock.sh` 识别 `AMBI_LOCK_HELD=1`：acquire 返回“子进程继承”、release 跳过（锁归外层 trap）；脚本以 `AMBI_LOCK_HELD=1 pi …` 注入并在 prompt 里明示；②空烧防护：pi rc=0 但 `git rev-parse HEAD` 无变化 → 不推进、按失败下轮重试。
- **预防**：互斥锁必须有唯一 owner + 显式继承/传递机制；凡“以退出码判定完成”的地方必须同时校验副作用（提交/产物），正常退出 ≠ 干了活。

## P-11 心跳循环绑错 pid → 永不刷新（锁要么空转 1h、要么被误接管）

- **症状**：锁目录 heartbeat 时间戳永远停在 acquire 那一刻（日志里“心跳 117s→1017s 前”持续增长）；孤儿锁要空转满 3600s 才能被接管，反过来超 1h 的正常施工会被定时任务误接管（双施工）。
- **根因**：心跳子循环写的是 `kill -0 "$$"`，而 `$$` 是 `task_lock.sh` 自己——acquire 一返回进程就退出，循环第一次探测即失败退出，心跳再也没人刷。
- **解决**：owner 改为 `$PPID`（调用方外层脚本）：`echo $PPID > pid` + 循环按 owner 存活每 30s 刷新；实测 delta=30s ✓。
- **预防**：后台守护循环的存活探针必须绑“长生命周期的主人”；锁类代码必须实测三态：刷新（活着）、过期接管（主人死）、拒锁（他人持有）。
