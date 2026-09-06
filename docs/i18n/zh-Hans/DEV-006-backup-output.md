# 备份与输出可读性

> 对应根 skill：`readable-output`

## 冲突分类（PLAN-250）

| 态 | 判定 | UI |
|---|------|----|
| `dirty` | `git status --porcelain` 有条目（inspect 保留明细） | 冲突面板：概览 + 文件清单 + 后果对照 |
| `diverged`/`non-ff` | fetch 后 `rev-list --count` ahead>0 且 behind>0，或 ff-only 失败报文匹配 | 冲突面板：双栏提交清单 + 分叉点 + `git cherry` ≡ 等价对 |
| `ahead_clean` | 干净且仅 ahead（含上游 force-push 回退导致的假领先） | 不进面板；`pull` 结果记 `ahead`（no-op），`last_pull_status=ahead` 驱动仓库列表"本地领先"徽标 |

fetch 失败（离线）回退旧判定：ahead 视为冲突（信息可能过期）。同步仓（ahead=behind=0）正常 ff pull。结构化明细经 `ConflictInfo`（serde camelCase）贯通 `task_repos.conflict_json`（DB 迁移 `ensure_column`）、SSE `TaskEvent.conflict` 与 `GET /tasks/{id}/conflicts` 的 `conflict` 字段；`conflictReason` 保留人读摘要，向前兼容。

## 备份后拉取

| 项 | 约定 |
|---|------|
| 触发 | `crates/xingshu-core/src/puller.rs:resolve_pull_conflict`，工作区脏 `dirty` 或 `pull --ff-only` 冲突文案（`cannot fast-forward`/`divergent branches`/`would be overwritten`） |
| 位置 | 同父目录 `<repo>.bak.<YYYYMMDDHHmmSS.mmm>`（毫秒，碰撞则 `_{n}`），`fs::rename` 原子移动 |
| 忽略 | `scanner.rs:is_generated_backup` 认为含 `.bak.` 或 `.broken.` 的目录非仓库；`.gitignore` 亦忽略 |
| 模式 | 备份后**全量** `git clone <remote_url> <repoName>` 于父目录；失败保留备份并报错 `re-clone failed after backup` |
| 对比 | `Overwrite` 为增量 `reset --hard` + `clean -fd` + `pull --ff-only`；`Abort` 跳过；UI 对 `Overwrite` 强制二次确认 |
| 打开 | `POST /api/v1/open`（canonicalize + 已注册 root 前缀白名单，`Command::arg` 原始传参不经 shell）打开仓库/备份目录 |
| 持久化 | `task_repos.backup_path` + `tasks.result_json: counts/repos[].backupPath`，SSE `decision_applied` 广播；重启后 `waiting_decision` 保留，`resolving` 置 `interrupted` 不重放 |

## 输出可读性

- 服务端 `tasks.result_json` 存紧凑串（便于持久化），客户端解析：`parsedTaskResult / prettyTaskJson / taskCounts / taskRepos / taskScanStats`
- 映射：`ok→成功` `aborted→已中止` `skipped→已跳过` `failed→失败` `conflict→冲突需决策`，`pending→等待中` `running→运行中` `completed→已完成`；耗时 `ms/s/min` 自动档
- 任务面板顺序：摘要 `共 N 个仓库：成功 X、已中止 Y` → 徽章 → 表格（仓库/状态/结果/耗时/备份/错误）/ 扫描统计 → 折叠原始 JSON（调试）
- 通用错误：`Failed to fetch` → `无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681`

## 引用

- `crates/xingshu-core/src/puller.rs:204 backup_path`, `crates/xingshu-server/src/main.rs:681 resolve_task_repo_decision`, `crates/xingshu-server/src/tasks.rs:317 result_json`, `webui/src/App.vue:56 parsedTaskResult`
