# PLAN-204 Verification Evidence

Run: `20260829-1329`

## Scope

- Data source: synthetic staging only, created by `scripts/create-test-staging.ps1`.
- Repositories: local bare remote, third-party, dirty third-party, own, fork, and bare repository.
- The real reference repository was not used for write operations.

## Backend

```text
cargo clippy --workspace --all-targets -- -D warnings  PASS
cargo test --workspace                               PASS (15 core + 1 CLI + 4 server tests)
cargo build --workspace --release                    PASS
XINGSHU_LOG_FILE=<staging>/logs/xingshu.jsonl       PASS (daily/size JSONL file created)
```

The scan reported `2 roots`, `5 repos`, and one expected dirty-repository lock violation. Unattended pull produced `ok` for the clean local third-party repository and `aborted` for the dirty repository. Tag and repository policy commands returned successfully.

## WebUI and API

```text
pnpm --ignore-workspace install  PASS
pnpm run typecheck              PASS
pnpm run build                  PASS
```

Playwright MCP used real processes:

- Rust server `12681` served the built UI and staging API.
- Vite `12680` served the development UI and proxied `/api/v1/repos`, `/api/v1/tags`, and `/api/v1/stats` to `12681`; all returned `200`.
- API responses were checked for camelCase boundary fields and `X-Request-Id` propagation.
- Browser checks passed for navigation, tag filter, detail drawer, dashboard, and dirty conflict abort.
- Browser conflict actions passed against staging: backup created a `.bak.<timestamp>` copy and re-cloned; overwrite removed a staging-only file; abort returned `200` and preserved the repository.
- Desktop and mobile viewport checks were executed; no console errors were observed during the tested flow.

## Known Gaps

- File logging uses daily/size rotation with a bounded rotation count; full error-chain coverage remains follow-up work.
- Playwright MCP raw state and binary screenshots remain outside the project; this file is the curated evidence record.
