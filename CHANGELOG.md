# Changelog

All notable changes to Xingshu are recorded here.

## Unreleased

### WebUI redesign (shadcn-vue)

- WebUI now uses Tailwind CSS v4 with vendored shadcn-vue components (Reka UI primitives, lucide icons, neutral theme mapped to the dark gold palette).
- Typography floor: body text ≥12px with CJK-friendly line heights; Inter is bundled locally (offline-safe, OFL-1.1).
- Unicode glyph icons replaced with lucide SVG icons; copy-path buttons use a 16px copy icon with brighter contrast.
- Detail drawer is now a Sheet with formatted fetch history (`YYYY-MM-DD HH:mm:ss`, strategy, result badge).
- The kind picker confirms before switching and explains the read-only convention flip; the "modify lock" label is now "read-only convention" with a hover explanation.
- Added visual regression baselines for the repository list, drawer, and settings views (`e2e/tests/visual.spec.ts`).

### Changed (BREAKING file layout)

- Database files now follow `xingshu-<env>.db`: dev defaults to `.staging/dev-001/xingshu-dev.db`, prod to `./xingshu-prod.db` (via `XINGSHU_ENV=prod` / `mise run prod:server`).
- Database precedence is now explicit `--db` > `XINGSHU_DB` > default; `xingshu-cli --db` also honors `XINGSHU_DB`.
- Prod refuses database paths under `.staging/`; the server logs the resolved database path at startup.
- Migration (one step): rename a local legacy root `xingshu.db` to `xingshu-prod.db`.

- Added the Rust workspace, embeddable core, SQLite index, CLI, Axum API, and Vue WebUI.
- Added synthetic staging generation and repository-kind safety rules.
- Added bilingual documentation, structured JSONL logging, request IDs, Problem Details, real-process tests, and Playwright MCP staging verification.
- Added root CRUD API and WebUI settings page.
- Added repo kind picker and tag deletion in WebUI.
- Added scan trigger API with concurrency lock.
- Enhanced stats with per-kind aggregation.
- Added persisted asynchronous scan/batch-pull tasks, SSE progress streams, reconnect fallback, and WebUI task progress panels.
- Added task API contract coverage, restart interruption checks, and a compiled-server HTTP/SSE integration test.
- Added task-level conflict decisions with persisted per-repository state, conflict listing, explicit backup/overwrite/abort actions, and restart-safe resolution handling.
- Added structured pull-conflict taxonomy (dirty / diverged·non-ff / ahead-clean), WebUI conflict panels with file lists, dual-column commit views, `git cherry` patch-equivalence markers, and action-consequence hints.
- Added "local ahead" passive badge for clean-ahead repositories (possible upstream rollback) and overwrite double-confirmation.
- Added `POST /api/v1/open` directory-opening API with registered-root allowlist, favicon fix, and staged diverged fixture for E2E.
