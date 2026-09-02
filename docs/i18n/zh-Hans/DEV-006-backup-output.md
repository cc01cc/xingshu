# 备份与输出可读性

> 对应根 skill：`readable-output`

## 备份后拉取

| 项 | 约定 |
|---|------|
| 触发 | `crates/xingshu-core/src/puller.rs:resolve_pull_conflict`，工作区脏 `dirty||ahead_of_upstream` 或 `pull --ff-only` 冲突文案（`cannot fast-forward`/`divergent branches`/`would be overwritten`） |
| 位置 | 同父目录 `<repo>.bak.<YYYYMMDDHHmmSS.mmm>`（毫秒，碰撞则 `_{n}`），`fs::rename` 原子移动 |
| 忽略 | `scanner.rs:is_generated_backup` 认为含 `.bak.` 或 `.broken.` 的目录非仓库；`.gitignore` 亦忽略 |
| 模式 | 备份后**全量** `git clone <remote_url> <repoName>` 于父目录；失败保留备份并报错 `re-clone failed after backup` |
| 对比 | `Overwrite` 为增量 `reset --hard` + `clean -fd` + `pull --ff-only`；`Abort` 跳过 |
| 持久化 | `task_repos.backup_path` + `tasks.result_json: counts/repos[].backupPath`，SSE `decision_applied` 广播；重启后 `waiting_decision` 保留，`resolving` 置 `interrupted` 不重放 |

## 输出可读性

- 服务端 `tasks.result_json` 存紧凑串（便于持久化），客户端解析：`parsedTaskResult / prettyTaskJson / taskCounts / taskRepos / taskScanStats`
- 映射：`ok→成功` `aborted→已中止` `skipped→已跳过` `failed→失败` `conflict→冲突需决策`，`pending→等待中` `running→运行中` `completed→已完成`；耗时 `ms/s/min` 自动档
- 任务面板顺序：摘要 `共 N 个仓库：成功 X、已中止 Y` → 徽章 → 表格（仓库/状态/结果/耗时/备份/错误）/ 扫描统计 → 折叠原始 JSON（调试）
- 通用错误：`Failed to fetch` → `无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681`

## 引用

- `crates/xingshu-core/src/puller.rs:204 backup_path`, `crates/xingshu-server/src/main.rs:681 resolve_task_repo_decision`, `crates/xingshu-server/src/tasks.rs:317 result_json`, `webui/src/App.vue:56 parsedTaskResult`
