# Changelog

All notable changes to Xingshu are recorded here.

## Unreleased

## [0.1.1] - 2026-11-08

### Release engineering

- Patch release with no breaking CLI, API, or WebUI change: the command set, flags, exit codes, and data/path semantics are unchanged since `v0.1.0`, so `0.1.1` is a safe in-place upgrade. The only CLI-visible deltas are the help-text and error-message clarifications listed under `### CLI`.
- Rebuilt the stable publish workflow (`.github/workflows/release-publish.yml`) as one state machine: release state is resolved from the tag-filtered release list (`absent` / `one` / `multiple`) instead of `GET /releases/tags/{tag}`, because a draft release makes that endpoint return 404 and the previous probe read it as "no release". Assets are inventoried by release id while uploads and edits stay tag-addressed (gh tag addressing with a draft GraphQL fallback), and two releases sharing one tag are a hard failure.
- The publish step now runs under `set -Eeuo pipefail` with an `ERR` trap and an `assert()` helper that emits `::error::`, and each run logs one `::notice::state=<state> assets=<visible>/<expected> notes=<ok|stale>` line; `CANDIDATE_RUN_ID`/`CANDIDATE_RUN_ATTEMPT` are passed through the referencing step's `env:` so `set -u` cannot trip on them.
- Moved the step logic to `.github/scripts/publish-release-step.sh` with missing-file self-checks, so the job's single `release-source/` checkout is the only source of truth and the same script can be drilled locally; added `scripts/tests/publish-step-drill.sh` plus a `gh` shim covering the absent / draft / partial / complete / published states.
- Candidate runs are located through the workflow-run endpoint instead of an artifact path filter, so the stable run reliably finds the approved candidate attempt.
- Pinned `cargo-release` to 1.1.7 (`cargo install cargo-release --version 1.1.7 --locked`); the previous 0.25.20 pin could not perform the documented bump flow.
- `mise run release:prepare -- --execute` now passes `--no-confirm` to cargo-release and re-reads the workspace version afterwards: if the tool exits 0 without changing `Cargo.toml`/`Cargo.lock`, the wrapper fails with an explicit error instead of reporting success, and it keeps the run record (`run.json`, `stdout.log`, `stderr.log`) for audit.

### CLI

- `--root` now says which of its two meanings it expects: `xs scan --root <PATH>` takes the **path** of a registered root, copied verbatim from the `path` field of `xs roots list` (literal comparison, no normalization), while the id-type commands (`list`, `search`, `files`, `grep`, `read`, `index`, `pull`, `status`, `clone`) keep taking the integer `id` from that table. `xs scan --help` documents both meanings, and an unregistered path now fails with `root is not registered` plus three-part guidance — what `scan --root` expects, how to see or add roots (`xs roots list` / `xs roots add <PATH>`), and which commands take `--root <ID>` — instead of an unexplained mismatch.
- With `XINGSHU_ENV=prod`, a `--db` path is now classified three ways — `Inside` (under the current working directory's `.staging/`), `Outside`, or `Undetermined` — instead of a single inside/outside test. `Inside` is refused as before, `Outside` is accepted, and `Undetermined` still fails closed but with a layered message: `cannot classify --db path <PATH> as production: <OS reason>`, no longer mixed with the staging-violation wording. Previously an unclassifiable path was treated as outside staging and only failed later when the database was opened. Commands, flags, exit codes, and data-path semantics are otherwise unchanged.

## [0.1.0] - 2026-11-06

### CLI distribution

- Release the `xs` CLI for Windows x86_64 and Linux x86_64 GNU. Stable Release assets are built from the approved `0.1.0` candidate commit; candidate Actions artifacts are validation-only, and the stable tag workflow reuses the exact verified binaries without rebuilding.
- The official CLI stores its fresh schema v5 database under the OS user-data directory, independent of the working directory. It does not import legacy databases or development `.staging` data and requires system Git for repository operations.
- The CLI release build uses a separate release marker and supports `XINGSHU_ENV=dev|prod`, `--db` > `XINGSHU_DB` > defaults, and read-only `xs db path`. Versions before 1.0 are unstable; 0.x minor is the default bump, with patch requiring an explicit maintainer choice.

### PLAN-0587 CLI content search and index

- CLI is now delivered as the `xs` binary only; the legacy `xingshu` name is retired (no consumer needed a compatibility alias).
- Add SQLite FTS5 trigram indexing with `index status`, `index update`, and `index rebuild`.
- Add indexed HEAD `search` and `files` queries; `grep` scans the selected worktree (or one local ref with `--ref`); `read` reads bounded text from HEAD or a local ref.
- Report HEAD versus worktree provenance, resolve duplicate repo selectors with `--root`, apply result caps/truncation, and keep all repository access read-only.

### Management surface (PLAN-0316)

- New read-only endpoint `GET /api/v1/repos/{repoId}/diff?path=<repo-relative>`: returns the worktree diff versus `HEAD` for one path (512 KiB cap with `truncated`), marks `binary`/`untracked`/`no_head` instead of faking a patch, rejects escaping/absolute paths and bare layouts with 400, and never writes to the repository.
- Conflict panel `dirty` file rows are clickable and open the diff in a read-only sheet.
- Repo policy responses now report the effective layer (`source`, `sourceTagId`, `effective`); the policy page shows the source badge and gains a tag-policy editor with member counts (inheritance default: repo-level policies keep priority).
- The drawer probes target free space via `GET /api/v1/roots/{rootId}/space` and disables the move when space is insufficient (probe failures return `space_probe_failed` instead of a silent zero).

### WebUI structure refactor (PLAN-0315)

- Split the monolithic `webui/src/App.vue` (827 lines) into `views/` (tags / disks / settings / policy), `components/` (repo table, task panel, conflict panel, repo drawer, sidebar) and `lib/` (api / format / types); no behavior or visual changes.

### Issue sweep (PLAN-297)

- CLI `pull` now prompts for the conflict action (backup / overwrite / skip) on interactive terminals; non-TTY input falls back to the unattended policy instead of hanging.
- Backup conflict action is atomic: a failed re-clone removes the partial clone and restores the original repository, audited as `backup_failed`; a missing `remote_url` now fails before any move.
- BREAKING: the conflict action value `abort` is renamed to `skip` (legacy `abort` is still accepted; the result value is now `skipped`).
- New CLI surface: `pull --root <id>`, `pull --dry-run [--fetch]`, `stats --by root|tag|org`, `status [--remote]` (read-only `git ls-remote`), `backup create|list|restore` (directory or `tar.gz`/`tar.zst` snapshots with `--keep`), and `scan --full` to disable the new incremental scan.
- Repository records now persist working-tree state (`dirty`/`ahead`/`behind`), remote host/namespace plus `org_source`, and an incremental-scan fingerprint; existing databases are forward-migrated (schema ledger v5).
- WebUI list shows local-change/lead/lag badges and marks path-derived organizations as unrecognized.
- Status and backup capabilities are exposed over HTTP: `GET /repos/{id}/status[?remote=1]` (read-only `ls-remote` drift check), backup list/create/restore/prune with preview-first semantics; the WebUI drawer gains status and backup panels, and the repository list gains a status filter.

### WebUI redesign (shadcn-vue)

- WebUI now uses Tailwind CSS v4 with vendored shadcn-vue components (Reka UI primitives, lucide icons, neutral theme mapped to the dark gold palette).
- Typography floor: body text ≥12px with CJK-friendly line heights; Inter is bundled locally (offline-safe, OFL-1.1).
- Unicode glyph icons replaced with lucide SVG icons; copy-path buttons use a 16px copy icon with brighter contrast.
- Detail drawer is now a Sheet with formatted fetch history (`YYYY-MM-DD HH:mm:ss`, strategy, result badge).
- The kind picker confirms before switching and explains the read-only convention flip; the "modify lock" label is now "read-only convention" with a hover explanation.
- Added visual regression baselines for the repository list, drawer, and settings views (`e2e/tests/visual.spec.ts`).

### Changed (BREAKING file layout)

- Source-checkout CLI dev continues to use `.staging`; the official release CLI uses the current user's OS data directory, independent of the working directory. The CLI keeps `--db` > `XINGSHU_DB` > environment/build default precedence; the server retains its existing CWD-based prod path and startup path log.
- Prod refuses CLI database paths under `.staging/`. First official use creates a fresh schema v5 database and does not import a legacy database or development staging data.

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
