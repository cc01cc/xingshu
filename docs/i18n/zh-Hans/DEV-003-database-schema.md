# 数据库 Schema

默认数据库为 `xingshu.db`，使用 SQLite WAL。所有表都包含 `created_at` 与 `updated_at`，由核心写操作维护。

## 表

| 表 | 作用 |
|----|------|
| `roots` | 注册的物理目录和磁盘元数据 |
| `repos` | 仓库路径、remote、HEAD、kind、锁告警和大小 |
| `tags` | 星枢自有领域/主题标签，非 Git tag |
| `repo_tags` | repo 与领域标签的多对多关系 |
| `policies` | repo 或 tag 级 pull/conflict policy |
| `fetch_log` | 每次 pull 的业务审计记录 |
| `tasks` | 异步 scan/pull 的持久化状态和最新进度 |

## 约束

- `repos(root_id, rel_path)` 唯一。
- `repo_tags(repo_id, tag_id)` 联合主键。
- `POLICIES` 的 repo 级策略优先于 tag 级策略。
- `repo_kind=third-party`/`third-party-frozen` 默认 `modify_lock=1`。
- remote URL 入库前必须剥离 userinfo，避免 token 进入数据库。
- `tasks` 的 `pending`/`running` 任务在 server 启动时标记为 `interrupted`，终态任务保留供 GET 查询。

## 迁移

Schema 演进必须使用显式 migration ledger，不能通过删除数据库规避迁移失败。迁移测试使用真实 SQLite 文件和项目生产 schema。
