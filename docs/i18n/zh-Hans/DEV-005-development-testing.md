# 开发与测试

## 开发态启动（mise 首选，对齐 A03-xihe）

```powershell
mise run setup
mise run staging:dev --run-id dev-001
mise run dev          # == dev:host → 12681 server + 12680 Vite 并行，Ctrl+C 同停；缺省进 .staging/dev-001/xingshu-dev.db
# 换 run：$env:XINGSHU_DB = ".staging/dev-002/xingshu-dev.db"; mise run dev
# 生产库：mise run prod:server  # XINGSHU_ENV=prod，默认 ./xingshu-prod.db，拒 .staging/ 路径
```

数据库文件名统一规范 `xingshu-<env>.db`。本地遗留根 `xingshu.db`（2026-09-06 改名前的数据）手动改名 `xingshu-prod.db` 一次即可。

等价裸命令（双终端）：

```powershell
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\dev-<run-id>
# 缺省即进 .\.staging\dev-<run-id>\xingshu-dev.db，无需设变量；换 run 才设：
$env:XINGSHU_DB = ".staging\dev-<run-id>\xingshu-dev.db"

# 终端 1：Rust API
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server

# 终端 2：Vite WebUI（host/port 以 webui/vite.config.ts 为准：127.0.0.1:12680）
cd webui
pnpm --ignore-workspace run dev
```

Vite 使用 `12680`，并将 `/api` 与 `/health` 代理至 `12681`；server 在构建完成后也可直接托管 `webui/dist`。在设置页添加根目录后，点击 **扫描索引** 再查看仓库列表。

## 基础验证命令

```powershell
mise run validate     # lint + typecheck + build + test
# 细粒度
mise run lint        # clippy + fmt --check
mise run typecheck   # vue-tsc
mise run build
mise run test
# 裸命令
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

更多入口见根 `mise.toml`：`dev:server/dev:ui/setup/clean/staging`。

## 测试分层

- Unit：分类、脱敏、policy、路径和纯函数。
- Integration：临时 Git 仓、bare remote、SQLite schema 和 migration。
- Real-process：编译后的 CLI/server 与 staging 数据。
- Playwright MCP：真实 server/Vite、真实 API、桌面和移动 viewport。

任务队列额外覆盖：`POST /api/v1/tasks` 返回 `202/taskId`，GET 返回持久化终态，SSE 返回 snapshot 和终态事件；任务级冲突覆盖 `waiting_for_decision`、conflicts 查询、backup/overwrite/abort 和重复决策 `409`。独立 real-process 测试使用合成 tempdir 数据启动编译后的 server，验证 HTTP 与 SSE 全链路。WebUI 的 `task-store.ts` 覆盖 EventSource、GET 快照/冲突回退和指数退避重连。

所有写操作测试使用新的 `.staging/<run-id>` 或 tempdir。真实参考仓库不执行 pull、move、backup、overwrite。
