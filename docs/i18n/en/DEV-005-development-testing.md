# Development and Testing

Use a new staging run for development data. Start the backend and Vite in separate terminals:

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

Vite uses `12680` and proxies `/api` and `/health` to `12681`. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release`. From `webui/`, use `pnpm --ignore-workspace install`, then `pnpm run typecheck` and `pnpm run build`.

Tests are layered: unit tests for pure logic, integration tests with temporary Git repositories and SQLite, real-process tests for compiled CLI/server, and Playwright MCP checks against real Vite/server processes. Task coverage verifies `POST /api/v1/tasks` returning `202/taskId`, a persisted terminal GET snapshot, SSE snapshot/terminal events, task-level conflict wait/decision, and duplicate-decision rejection. `webui/src/task-store.ts` covers EventSource, GET snapshot/conflict fallback, and exponential reconnect backoff. Every write test uses a new staging run or tempdir; the real reference repository is read-only and excluded from write tests.
