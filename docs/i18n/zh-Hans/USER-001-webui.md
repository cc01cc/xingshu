# WebUI 使用

## 启动

首选 `mise run dev`（`mise.toml`，对齐 A03-xihe）：一键并行启动 `12681` Axum server 与 `12680` Vite（`/api`、`/health` 代理至 `12681`），`Ctrl+C` 同停；裸命令仍可用 `cargo run -p xingshu-server` + `cd webui && pnpm --ignore-workspace run dev`。构建后 WebUI 由 `12681` 直接托管 `webui/dist`。

## 页面

- 仓库目录：搜索、标签过滤、类型/锁状态、详情抽屉；空状态会引导到设置页扫描（`暂无仓库索引·请到设置点击扫描索引`）。
- 主题标签：创建领域标签并跳转到过滤后的仓库列表。
- 磁盘看板：显示索引数量、空间和待决策数量。
- 设置：根目录管理（输入绝对路径 → 添加，可多个跨盘），卡片列表管理根，首行提示 `添加后需手动触发扫描才会生成索引`，下方一级 CTA `扫描索引（N 个根目录）` + 状态文案 `已索引 N 个仓库/尚未扫描`；空目录如 `Z:\TEST` 扫描后仍为 0 属正常。
- 标题：`REPOSITORY INDEX` 下 `仓库目录/主题标签/磁盘看板/设置` 四态与侧栏 `repos/tags/disks/settings` 一致。
- 冲突决策：对 dirty 或冲突仓选择“备份后拉取”“覆盖本地”或“保持现状”。
- 异步任务：侧栏的“扫描索引”和“批量 pull”创建后台任务，设置页亦有同等入口；面板以中文标签（成功/已中止/已跳过/失败/冲突需决策）、胶囊徽章、表格（仓库/状态/结果/耗时/备份/错误）与扫描统计展示，`Failed to fetch` 映射为 `无法连接本地服务，请确认 12681`，原始 JSON 折叠于调试区。
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
