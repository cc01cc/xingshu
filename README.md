# 星枢 Xingshu

星枢是项目与资源的本地索引和管理中枢。当前首先支持 Git 仓库，面向分布在多个目录、磁盘、组织和远程上的大量项目，提供 SQLite 元数据、领域标签、差异化更新策略、冲突保护和本地 WebUI。

“星”取群星意象：每个项目或资源单元像一颗独立的星，彼此独立却共同构成项目星群；“枢”取中枢、枢纽与索引枢轴之意，表达将这些群星汇聚、发现、索引、分类和管理。星枢是项目自拟的原创组合，受“天枢”等天文意象启发，但不是古籍固定专名。英文释义为 *the stellar hub*。

## Current Status

首版核心、CLI、API、基础 WebUI、异步任务实时进度、结构化日志、分层测试和正式 Playwright 验证已完成；后续功能演进规划中。

## Quick Start

```powershell
cargo build --workspace --release
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\run-001
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-b
.\target\release\xingshu.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
```

启动本地 API 与构建后的 WebUI：

```powershell
$env:XINGSHU_DB = ".staging\run-001\xingshu-dev.db"
$env:XINGSHU_PORT = "12681"
.\target\release\xingshu-server.exe
```

- API/WebUI：<http://127.0.0.1:12681>
- Health：<http://127.0.0.1:12681/health>
- Vite dev：在 `webui/` 执行 `pnpm --ignore-workspace install` 后运行 `pnpm run dev`，端口 `12680`

异步 scan/pull API：

```powershell
$task = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:12681/api/v1/tasks -ContentType "application/json" -Body '{"type":"scan"}'
Invoke-RestMethod "http://127.0.0.1:12681/api/v1/tasks/$($task.taskId)"
```

批量 pull 可请求人工冲突决策：`{"type":"pull","conflictMode":"ask"}`。通过 `GET /api/v1/tasks/{taskId}/conflicts` 查询冲突，再向 `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision` 提交 `backup`、`overwrite` 或 `abort`。实时进度通过 `GET /api/v1/tasks/{taskId}/stream` 的 SSE 返回；同步 `POST /api/v1/scan` 和单仓 pull 仍保留给脚本兼容。

## CLI

```text
xingshu roots add <path> [--disk LABEL] [--priority N]
xingshu roots list | rm <id>
xingshu scan [--root PATH] [--include-nested] [--max-depth N]
xingshu list [query] [--tag TAG] [--root ID] [--status STATUS] [--org ORG]
xingshu tag <repo> <tag...> | untag <repo> <tag...>
xingshu kind set <repo> <third-party|third-party-frozen|fork|own> [--upstream REMOTE]
xingshu policy set <repo> [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xingshu policy set --tag TAG [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xingshu pull [--jobs N] [--unattended]
xingshu move <repo> <root-id>
xingshu stats
```

## Safety

- `third-party` 和 `third-party-frozen` 只在星枢自身操作中锁定修改；用户直接使用 Git 不由星枢拦截。
- pull 冲突默认停止并交给用户决定；无人值守默认 `abort`。
- 测试和开发写操作只允许使用 tempdir 或独立 staging；不得对真实参考仓库执行 pull、move、backup 或 overwrite。
- remote URL 入库前剥离 userinfo；日志不得包含 token、PAT、Bearer 或完整环境变量。

## Logging

运行日志默认输出 JSONL 到 stderr。设置 `XINGSHU_LOG_LEVEL`（或回退到 `RUST_LOG`）控制级别；设置 `XINGSHU_LOG_FILE` 可启用按日/大小轮转的 JSONL 文件日志，默认单文件 10 MiB、保留 7 个轮转文件，可用 `XINGSHU_LOG_MAX_BYTES`/`XINGSHU_LOG_MAX_FILES` 覆盖。`fetch_log` 是 SQLite 中的 pull 审计，不是运行日志替代品。

## Project Documentation

- 文档索引：`docs/AGENTS.md`
- 中文文档：`docs/i18n/zh-Hans/README.md`
- English docs: `docs/i18n/en/README.md`
- API contract: `docs/api/openapi.yaml`
- Ports: WebUI `12680`，API/server `12681`

## Development

### Development Mode (mise, recommended)

```powershell
# 1. Install deps (Rust + WebUI)
mise run setup

# 2. Create a fresh staging run (synthetic, never writes real S:\zeogit-ref)
mise run staging:dev --run-id dev-001
# (default DB is now .staging/dev-001/xingshu-dev.db; set $env:XINGSHU_DB only to switch runs)
# Prod DB: mise run prod:server (XINGSHU_ENV=prod, ./xingshu-prod.db)

# 3. Start server (12681) + Vite (12680, proxies /api to 12681) in parallel
mise run dev          # == mise run dev:host, Ctrl+C cleans up both
```

- Vite: http://127.0.0.1:12680  ·  API/built WebUI: http://127.0.0.1:12681  ·  Health: http://127.0.0.1:12681/health
- Add roots in Settings → click **扫描索引** (or `POST /api/v1/tasks {type:scan}`); repos appear in the repository table.
- Raw cargo/pnpm equivalent (two terminals):

```powershell
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server
# terminal 2
cd webui
pnpm --ignore-workspace run dev -- --host 127.0.0.1
```

### Validation

```powershell
mise run validate      # lint + typecheck + build + test
# or bare commands
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

Other mise entries: `mise run dev:server/dev:ui/setup/build/test/lint/typecheck/clean/staging` — see `mise.toml`.
