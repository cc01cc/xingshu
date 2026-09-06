# CLI 与配置

## 安装与端口

首选 `mise run setup`（`mise.toml`）安装依赖；从 `webui/` 单独安装仍必须使用 `pnpm --ignore-workspace install`，因为项目位于 workspace 根目录下（`A08-xingshu/AGENTS.md:16`）。WebUI dev 端口为 `12680`，Axum API 和构建后 WebUI 端口为 `12681`，一键启动见 `mise run dev`。

## 常用流程

```powershell
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db tag network/third-party-demo network
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db list --tag network
```

## 数据库选择

数据库文件名统一规范 `xingshu-<env>.db`（`xingshu-dev.db` / `xingshu-prod.db` / `xingshu-test.db`）。优先级：显式 `--db` > `XINGSHU_DB` > 默认值；`XINGSHU_ENV=prod` 时默认 `./xingshu-prod.db`，否则默认 `.staging/dev-001/xingshu-dev.db`。prod 下指向 `.staging/` 内路径会拒绝启动（残留变量保护）；server 启动首行日志打印实际库路径。

`TAGS` 是星枢自己的领域标签，不是 Git tag。`repo_kind` 区分第三方、冻结、fork 和自有仓库；`policy set` 可设置 repo 或 tag 的更新和冲突策略，repo policy 优先。

## 安全参数

- 交互 pull 冲突策略默认 `stop`。
- unattended pull 冲突策略默认 `abort`。
- `scan_nested_repos` 默认关闭，`scan_max_depth=0` 表示不限制层级，`scan_skip_dirs` 用于剪枝。
