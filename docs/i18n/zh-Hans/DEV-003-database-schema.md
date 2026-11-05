# 数据库 Schema

数据库使用 SQLite WAL；当前 schema ledger 版本为 `5`。当前源码 CLI/server 的 dev 默认库是 `.staging/dev-001/xingshu-dev.db`；server 在 `XINGSHU_ENV=prod` 时默认使用 CWD 下的 `xingshu-prod.db`。所有表都包含 `created_at` 与 `updated_at`，由核心写操作维护。

**v0.1.0 官方 CLI 路径行为（已在源代码中实现）**：官方发行 CLI 的 prod 默认库位于 OS 用户数据目录的 `xingshu-prod.db`；dev 库位于同一数据根的 `dev/xingshu-dev.db`。Windows 数据根为 `%LOCALAPPDATA%/Xingshu`，Linux 为 `${XDG_DATA_HOME:-~/.local/share}/xingshu`。macOS 的 `~/Library/Application Support/Xingshu` 仅作实验性构建的数据路径设计，不属于 v0.1.0 用户支持矩阵；当前正式支持平台和二进制可用性以 XS GitHub Releases 页面为准。源码 checkout CLI 的 dev 模式继续使用 `.staging`。CLI 优先级为显式 `--db` > `XINGSHU_DB` > 环境/构建默认路径；server 继续使用现有 CWD/server resolver。

首发版本号 `0.1.0` 使用当前 schema v5 创建新用户库，不导入 `.staging` 数据或旧库；该版本不修改 schema。FTS 搜索索引是可重建 sidecar；正式 DB 路径变化后 sidecar 可按新身份重建，不作为首发迁移数据。

## 表

| 表 | 作用 |
|----|------|
| `roots` | 注册的物理目录和磁盘元数据 |
| `repos` | 仓库路径、remote、HEAD、kind、锁告警和大小；PLAN-297 起新增远端 host/namespace/org 来源、`dirty`/`ahead`/`behind` 工作区状态与增量扫描指纹 |
| `tags` | 星枢自有领域/主题标签，非 Git tag |
| `repo_tags` | repo 与领域标签的多对多关系 |
| `policies` | repo 或 tag 级 pull/conflict policy |
| `fetch_log` | 每次 pull 的业务审计记录 |
| `tasks` | 异步 scan/pull 的持久化状态和最新进度 |
| `task_repos` | 一个异步任务内每个仓库的状态、冲突决策和耗时 |

## 约束

- `repos(root_id, rel_path)` 唯一。
- 当前 Core 会在被选中的旧 schema 数据库打开时，通过 `ensure_column` 前向添加 `repos` 新列（`remote_host`/`remote_namespace`/`org_source`/`dirty`/`ahead`/`behind`/`status_checked_at`/`scan_fingerprint`）。这只描述同一数据库文件内的 schema 升级，不代表 v0.1.0 会发现、搬迁或导入旧路径数据库。
- `repo_tags(repo_id, tag_id)` 联合主键。
- `POLICIES` 的 repo 级策略优先于 tag 级策略。
- `repo_kind=third-party`/`third-party-frozen` 默认 `modify_lock=1`。
- remote URL 入库前必须剥离 userinfo，避免 token 进入数据库。
- `tasks` 的 `pending`/`running` 任务在 server 启动时标记为 `interrupted`，终态任务保留供 GET 查询。
- `task_repos` 的 `waiting_decision` 在重启后保留；`resolving` 会标记为 `interrupted`，不得自动重放 backup/overwrite。

## 迁移

未来 schema 变更必须使用显式 migration ledger 和针对实际 SQLite 文件的前向迁移测试，不能通过删除数据库规避迁移失败。本次 v0.1.0 发布不修改 schema，也不承诺旧库导入或向下迁移；新增 schema/data 变更时需由对应功能计划定义备份、恢复及失败处理。
