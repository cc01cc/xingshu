# WebUI 使用

## 启动

构建后的 WebUI 由 Axum server `12681` 托管。开发时运行 Vite `12680`，其 `/api` 与 `/health` 代理到 `12681`。

## 页面

- 仓库目录：搜索、标签过滤、类型/锁状态、详情抽屉。
- 主题标签：创建领域标签并跳转到过滤后的仓库列表。
- 磁盘看板：显示索引数量、空间和待决策数量。
- 冲突决策：对 dirty 或冲突仓选择“备份后拉取”“覆盖本地”或“保持现状”。
- 异步任务：侧栏的“扫描索引”和“批量 pull”创建后台任务，面板显示状态、进度、最后处理的仓库和连接状态。
- 任务级冲突：批量 pull 默认以 `conflictMode=ask` 启动；冲突仓进入待决策列表，其他仓库继续处理，可单独选择备份后拉取、覆盖本地或保持现状。

## 异步任务

WebUI 使用任务 API，不等待长时间的同步请求：

- `POST /api/v1/tasks`，请求体为 `{ "type": "scan" }` 或 `{ "type": "pull", "conflictMode": "ask" }`，返回 `202` 和 `taskId`。
- `GET /api/v1/tasks/{taskId}`，读取 SQLite 中保存的最新状态；状态包括 `pending`、`running`、`waiting_for_decision`、`completed`、`failed` 和 `interrupted`。
- `GET /api/v1/tasks/{taskId}/conflicts`，读取任务内仍需处理或正在处理的仓库冲突。
- `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision`，提交 `backup`、`overwrite` 或 `abort` 决策，返回 `202`。
- `GET /api/v1/tasks/{taskId}/stream`，通过 SSE 先发送 `snapshot`，再发送 `started`、`progress`、`conflict`、`waiting_for_decision`、`decision_applied`、`completed` 或 `failed` 事件。

SSE 断开时页面先读取 GET 快照和 conflicts，再自动重连；断线不会直接被显示为任务失败。任务完成或失败后 SSE 关闭，但最终状态仍可通过 GET 查询。服务重启后 `waiting_for_decision` 会保留，正在执行 destructive action 的项不会自动重放。

WebUI 的标签是星枢自有分类，不会创建或修改 Git tag。冲突决策只会作用于星枢当前操作指定的 staging/目标仓库。
