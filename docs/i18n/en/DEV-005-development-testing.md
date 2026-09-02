# Development and Testing

## Dev startup (mise first, mirrors A03-xihe)

```powershell
mise run setup
mise run staging:dev --run-id dev-001
$env:XINGSHU_DB = ".staging/dev-001/xingshu.db"
mise run dev          # == dev:host → 12681 server + 12680 Vite in parallel, Ctrl+C stops both
```

Raw equivalent (two terminals):

```powershell
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\dev-<run-id>
$env:XINGSHU_DB = ".staging\dev-<run-id>\xingshu.db"

# Terminal 1
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server

# Terminal 2
cd webui
pnpm --ignore-workspace run dev -- --host 127.0.0.1
```

Vite uses `12680` and proxies `/api` and `/health` to `12681`; the built bundle is served by Axum on `12681` after `pnpm run build`. After adding roots in Settings, click **Scan** to populate the repository table.

## Validation

```powershell
mise run validate     # lint + typecheck + build + test
# granular
mise run lint        # clippy + fmt --check
mise run typecheck   # vue-tsc
mise run build
mise run test
# raw
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
# webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

More entries in `mise.toml`: `dev:server/dev:ui/setup/clean/staging`.

Tests are layered: unit tests for pure logic, integration tests with temporary Git repositories and SQLite, real-process tests for compiled CLI/server, and Playwright MCP checks against real Vite/server processes. Task coverage verifies `POST /api/v1/tasks` returning `202/taskId`, a persisted terminal GET snapshot, SSE snapshot/terminal events, task-level conflict wait/decision, and duplicate-decision rejection. `webui/src/task-store.ts` covers EventSource, GET snapshot/conflict fallback, and exponential reconnect backoff. Every write test uses a new staging run or tempdir; the real reference repository is read-only and excluded from write tests.
