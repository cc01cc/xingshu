# PLAN-208 Verification Evidence

Run: `20260830-1525`

## Scope

- Data source: synthetic staging only, created by `scripts/create-test-staging.ps1`.
- Staging path: `.staging/plan-208-20260830-1525`.
- Repositories: local bare remote, third-party, dirty third-party, own, fork, and bare repository.
- The real `S:\zeogit-ref` reference repository was not accessed for write operations.

## Backend and Tests

```text
cargo fmt --all -- --check                           PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                               PASS
cargo build --workspace --release                    PASS
```

The workspace test run passed the CLI real-process flow, core scanner/puller/mover/quality tests, server API tests, the OpenAPI route contract test, and the compiled-server HTTP/SSE integration test. The task-specific checks covered `202/taskId`, persisted completion, terminal SSE closure, invalid task type, missing task, same-type task locking, restart interruption, and bare-repository skipping.

## WebUI and API

```text
pnpm --ignore-workspace run typecheck  PASS
pnpm --ignore-workspace run build      PASS
```

Playwright MCP used real processes:

- Rust server `12681` used the synthetic staging database.
- Vite `12680` served the development UI and proxied task API requests to `12681`.
- Desktop scan showed an active `running` task with an indeterminate progress state, then a `completed` task with `5` processed repositories and the persisted result summary.
- Desktop batch pull showed `1 / 5` progress, completed at `5 / 5`, and exposed the persisted summary `{"aborted":1,"failed":2,"ok":1,"skipped":1}`. The failed entries are expected synthetic fixtures pointing at unavailable example remotes.
- An intentionally aborted SSE route produced `3` reconnect attempts. The UI stayed in `正在重连`, used GET fallback, did not show a global error notice, and reached the real task terminal state.
- Mobile viewport `390x844` displayed the task panel and controls; measured document `scrollWidth=375` with no outer horizontal overflow. The repository table remains horizontally scrollable inside its table container.
- No console errors occurred during the normal scan/pull flows. Console errors from the reconnect case were the deliberate browser-side SSE aborts.

## Known Gaps

- `Last-Event-ID` replay suppression is not implemented; reconnect correctness currently relies on the persisted GET snapshot.
- Task-level conflict decisions are not yet resumed from the progress panel; existing single-repository backup/overwrite/abort endpoints remain synchronous.
- CLI progress output and a task listing endpoint are outside this implementation pass.

## Update: Concurrent Pull + Duration Tracking

Added per-repo concurrent pull with `std::thread::scope` and `available_parallelism()` (capped at 8). Each SSE progress event now carries `duration_ms`. The task result JSON includes per-repo entries with `repo`, `result`, and `durationMs`. All 27 tests pass, clippy clean.
