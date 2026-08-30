# 开发与测试

## 开发态启动

必须使用新的 staging run：

```powershell
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\dev-<run-id>
$env:XINGSHU_DB = ".staging\dev-<run-id>\xingshu.db"
```

在两个终端分别启动：

```powershell
# 终端 1：Rust API
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server

# 终端 2：Vite WebUI
cd webui
pnpm --ignore-workspace run dev -- --host 127.0.0.1
```

Vite 使用 `12680`，并将 `/api` 与 `/health` 代理至 `12681`；server 在构建完成后也可直接托管 `webui/dist`。

## 基础验证命令

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

## 测试分层

- Unit：分类、脱敏、policy、路径和纯函数。
- Integration：临时 Git 仓、bare remote、SQLite schema 和 migration。
- Real-process：编译后的 CLI/server 与 staging 数据。
- Playwright MCP：真实 server/Vite、真实 API、桌面和移动 viewport。

所有写操作测试使用新的 `.staging/<run-id>` 或 tempdir。真实参考仓库不执行 pull、move、backup、overwrite。
