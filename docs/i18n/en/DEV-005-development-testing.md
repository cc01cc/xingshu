# Development and Testing

> Sync status: the English document summarizes some examples and test-layer details from zh-Hans. The v0.1.0 candidate/stable release boundary, supported targets, and CLI/server data-path distinction are synchronized.

## Dev startup (mise first, mirrors A03-xihe)

```powershell
mise run setup
mise run staging:dev --run-id dev-001
mise run dev          # == dev:host → 12681 server + 12680 Vite in parallel, Ctrl+C stops both; server defaults to .staging/dev-001/xingshu-dev.db
# Switch server runs: $env:XINGSHU_DB = ".staging/dev-002/xingshu-dev.db"; mise run dev
# Prod server DB: mise run prod:server  # XINGSHU_ENV=prod, defaults to CWD/xingshu-prod.db, refuses .staging/ paths
```

The current database ledger is schema v5. The user confirmed that no legacy production database existed before the first stable release. The v0.1.0 CLI creates a fresh database in the OS user data directory; it does not import a legacy-named database or a development `.staging` database. Future schema changes must define migration and recovery behavior in their feature plan.

## CLI Release Process and Runtime Paths

- `mise run dev` / `prod:server` continue to use the existing server CWD/staging database contract. The CLI-specific user-data path resolver is implemented in source; official binaries and current platform availability are listed on the public GitHub Releases page.
- The official CLI release binary uses the OS user data root (Windows `%LOCALAPPDATA%/Xingshu`, Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu`); source CLI dev remains in checkout `.staging`. The CLI resolver is separate from the server resolver. The macOS data-path implementation is experimental and outside the v0.1.0 user-support matrix; GitHub Releases lists supported platforms and available binaries.
- Pushing a candidate snapshot to public `release-candidate/v0.1.0` triggers the push-only candidate workflow. Candidate artifacts are Windows/Linux x86_64 only, include `-attempt-<run_attempt>` in their names, and are immutable per run; a candidate is not a public Release. After candidate acceptance and final user approval, the release owner fast-forwards the same commit to public `main` and pushes the stable SemVer tag `v0.1.0` (no prerelease suffix). The tag workflow finds the successful candidate run on the candidate branch with the same SHA/attempt, verifies hashes, and reuses the original assets without rebuilding.
- Git is a CLI runtime dependency; users need `git` available on PATH for repository operations. `xs db path` shows the effective environment and database path without creating the database.
- The current source includes `mise run release:prepare`: dry-run minor by default; for the first release, `--first-release` verifies workspace version `0.1.0` and no-ops without requiring `cargo-release`. Non-first bumps require the maintainer to install `cargo-release` 0.25.20 manually and put it on PATH; the task does not install it. While the workspace is 0.x, explicit patch is supported and major is rejected; at 1.0+, standard SemVer permits explicit patch or major while the default remains minor. Only `--execute` updates the workspace version/lockfile. The task will not publish crates, create tags, or push. Transitioning from 0.x to 1.0 requires a separate approved stability decision/PLAN and one-time manual workspace version change; it is not routine automation, and there is no `--version` option. The maintainer curates the CHANGELOG; an Agent must not downgrade the default bump to patch.
- Before 1.0, all versions are unstable. Breaking CLI/config/default-path/schema changes use minor and carry a `BREAKING` changelog note. Standard SemVer applies after 1.0.

Raw equivalent (two terminals):

```powershell
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\dev-<run-id>
# The default already points at .\.staging\dev-<run-id>\xingshu-dev.db; set the variable only to switch runs:
$env:XINGSHU_DB = ".staging\dev-<run-id>\xingshu-dev.db"

# Terminal 1
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server

# Terminal 2 (host/port come from webui/vite.config.ts: 127.0.0.1:12680)
cd webui
pnpm --ignore-workspace run dev
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
