# 日志与审计

## 两类记录

运行日志用于诊断进程，`fetch_log` 用于记录 pull 业务审计，二者不能互相替代。

| 类型 | 载体 | 关键字段 |
|------|------|----------|
| 运行日志 | `tracing`，stderr 或 JSONL | timestamp、level、operation、request_id、repo_id、root_id、duration_ms、result、error |
| Pull 审计 | SQLite `fetch_log` | repo、strategy、started_at、finished_at、result、objects、bytes、error |

`xingshu-core` 只发 tracing event，不初始化全局 subscriber。CLI/server 在入口初始化 subscriber。`XINGSHU_LOG_LEVEL` 优先于 `RUST_LOG`；设置 `XINGSHU_LOG_FILE` 显式开启按日/大小轮转文件日志，默认单文件 10 MiB、保留 7 个轮转文件，可用 `XINGSHU_LOG_MAX_BYTES`/`XINGSHU_LOG_MAX_FILES` 覆盖。server 日志包含 `request_id` 与 `duration_ms`。

任何 remote URL、Authorization、Bearer、PAT、userinfo 和完整环境变量都必须脱敏。HTTP 使用 `X-Request-Id`，缺失时由 server 生成并回传。
