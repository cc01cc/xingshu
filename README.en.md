# 星枢 Xingshu

English | [简体中文](README.md)

Xingshu is a local index and management hub for projects and resources. Git repositories are the first supported project type: Xingshu targets large numbers of projects spread across directories, disks, organizations, and remotes, and provides SQLite metadata, domain tags, differentiated update policies, conflict protection, and a local WebUI.

The name uses a stellar metaphor. 星 (star) means every project or resource unit is an independent star, and together they form a constellation of projects; 枢 (hub, pivot) means the hub that gathers, discovers, indexes, classifies, and manages that constellation. Xingshu is an original combination inspired by astronomical terms such as 天枢 (Tianshu), not a fixed name from older texts; the English reading is *the stellar hub*.

## Current Status

The first-version core, CLI, API, base WebUI, async-task live progress, structured logging, layered tests, and formal Playwright validation are complete. Recent increments (PLAN-0297/0314): the three-state pull-conflict panel and the CLI decision prompt, read-only status inspection (local drift plus remote drift via `ls-remote`), standalone backups (snapshot/archive/prune), incremental scanning, and the WebUI/API closeout for those capabilities. Management-surface work (PLAN-0315/0316): WebUI structural split (App.vue reduced to a shell), read-only diff overlay for conflict details, policy-source visualization with tag-policy editing, and free-space precheck for move targets. PLAN-0617 added CLI remote clone into a registered root; the API/WebUI caller follows later.

## CLI Release Status

The first stable CLI version is `0.1.0`. The XS GitHub Releases page lists the stable assets and the currently available version; only binaries listed there apply to their platform.

The current release is `v0.1.1`: download the asset for your target from the Releases page, and see "Install from Releases (recommended)" below for the install steps.

Pushing a candidate snapshot to the public branch `release-candidate/v0.1.0` triggers the candidate build automatically; it needs no `workflow_dispatch` and no local GH CLI. Candidates are built for Windows x86_64 and Linux x86_64 only; the minimum runtime environments are Windows 10+ (Rust MSVC target baseline) and Linux GNU glibc 2.35+ (Ubuntu 22.04 baseline). Those OS floors count as verified only after a candidate run provides install/run evidence. The Windows release target statically links the MSVC CRT and candidate CI runs `dumpbin /dependents`, failing if VCRUNTIME/MSVCP/UCRT imports remain. The manifest records `crtLinkage=static` and the runtime dependencies: Windows 10+ and Git on `PATH`, or Linux glibc 2.35+, Git and `df` on `PATH` (`df` comes from GNU coreutils and is used for pre-move free-space checks); the Linux manifest also records the dynamic-library SONAMEs reported by `ldd` on the runner and fails on an unresolved dependency. The tag workflow verifies the manifest fields against the target allowlist; once the candidate import check passes, Windows needs no VC++ Redistributable. Available assets and supported platforms are authoritative on the Releases page.

Each run's artifact is named `xingshu-candidate-x86_64-pc-windows-msvc-attempt-<run_attempt>` or `xingshu-candidate-x86_64-unknown-linux-gnu-attempt-<run_attempt>`, is retained for 90 days, and is immutable per attempt. Every artifact maps to the same `github.sha` and `candidateRunAttempt` and holds the archive under its stable asset name (`xingshu-v0.1.0-x86_64-pc-windows-msvc.zip` or `xingshu-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`), the matching `.sha256` file, and `xingshu-v0.1.0-<target>.manifest.json`, which records `minimumOsAbi`, `buildRunner`, `crtLinkage`, and `runtimeDependencies` (`Windows 10+` / `windows-2025` for Windows, `glibc 2.35+` / `ubuntu-22.04` for Linux GNU). Candidate artifacts carry per-archive checksum files only; a stable GitHub Release additionally publishes the aggregate `SHA256SUMS`. Direct verification of a single archive still uses its adjacent `.sha256` file. `signatureStatus` is `unsigned` in both candidate and stable manifests and release assets are not code-signed: Windows may show a SmartScreen or unknown-publisher warning and the Linux archives are unsigned too. `.sha256` and `SHA256SUMS` verify file integrity only and never authenticate a publisher. Candidates report `0.1.0`, create no `-rc` version or tag, and their artifacts are validation-only, **not a public Release**. macOS and Linux arm64 are outside the `v0.1.0` user-support targets; macOS has an experimental build only, no macOS asset is published, and macOS is outside the user-support matrix.

After candidate validation and final user approval, the release owner fast-forwards that same candidate commit to public `main` and pushes the `v0.1.0` tag. The stable publish workflow accepts only stable SemVer tags without a prerelease suffix: it finds the successful candidate run with the same source SHA and `run_attempt` on `release-candidate/v0.1.0`, downloads that attempt's immutable artifact, re-checks the manifest, the hashes, and `minimumOsAbi`/`buildRunner`/`crtLinkage`/`runtimeDependencies` against the target allowlist, and then creates the GitHub Release from the original assets. It never rebuilds assets and needs no manual run ID or push to the private `origin`.

The current source already contains `mise run release:prepare`: dry-run plus minor by default, patch only when the maintainer passes `--bump patch` explicitly, and a version change only with an explicit `--execute`. A first release with `--first-release` verifies that the workspace is `0.1.0` and then no-ops without needing `cargo-release`. A non-first bump needs the maintainer to provide `cargo-release` 1.1.7 on `PATH` by hand; the flow never installs it. While the workspace is 0.x, `--bump major` is rejected; at 1.0+ a maintainer may explicitly choose patch or major per standard SemVer, with minor still the default. The 0.x-to-1.0 move is a separate stability decision/PLAN that requires a one-time manual workspace version change; it is not a routine automated bump and there is no `--version` option today. That task only runs the Cargo workspace version step: it publishes no crate, creates no tag, and pushes nothing. An Agent never lowers the default minor bump to patch on its own. Candidate Actions artifacts are validation-only; the official CLI binaries and the currently available version are authoritative on the GitHub Releases page.

Install the pinned `cargo-release`: `cargo install cargo-release --version 1.1.7 --locked` (it lands in `~/.cargo/bin`).

The CLI needs system Git with `git` resolvable from the command line. The current source contains the CLI-specific environment resolution and `xs db path`. In the target behavior, the official CLI uses the current user's data directory while the source-build development CLI uses `.staging`; the server keeps its existing database-path contract. The official CLI binaries and current platform availability are authoritative on the GitHub Releases page; see the CLI configuration document for details.

## Install from Releases (recommended)

Installed binaries and source builds keep their databases in different places, so start from Releases unless you intend to work on the source. This section is for users; [Quick Start](#quick-start) is the source-build path.

1. Download the archive for your target from the XS GitHub Releases page. Stable archives are named `xingshu-v<version>-<target>`, so the current release publishes `xingshu-v0.1.1-x86_64-pc-windows-msvc.zip` (Windows 10+) and `xingshu-v0.1.1-x86_64-unknown-linux-gnu.tar.gz` (Linux GNU, glibc 2.35+), and the same release carries `SHA256SUMS`, each archive's adjacent `.sha256` file, and a per-target manifest.
2. Verify the archive before extracting. Linux: check the archive line in `SHA256SUMS` with `sha256sum -c`. Windows: compare `Get-FileHash .\xingshu-v0.1.1-x86_64-pc-windows-msvc.zip -Algorithm SHA256` with the matching `SHA256SUMS` line. The assets are unsigned, so `SHA256SUMS` and `.sha256` prove integrity only, never publisher identity; Windows may show a SmartScreen or unknown-publisher prompt.
3. Put `xs` on `PATH`. Windows: create a directory such as `%LOCALAPPDATA%\Programs\xingshu`, move `xs.exe` there, and add that directory to your user `PATH`. Linux: `mkdir -p ~/.local/bin && mv xs ~/.local/bin/ && chmod +x ~/.local/bin/xs`, then check that a new shell resolves `xs`.
4. Check the binary and its Git prerequisite in a new terminal: `xs --version` must print `xs 0.1.1`, and `git --version` must work because scanning, indexing, and `grep` call the system `git`.
5. Register a root, scan it, and build the text index:

```console
$ xs roots add ./repos
registered root 1
$ xs roots list
$ xs scan --root ./repos
$ xs list
$ xs index update --root 1
```

- `xs roots list` prints JSON. `xs scan --root` takes the **path** of a registered root, copied **verbatim** from the `path` field of that output (literal comparison, no normalization: same separator, same case, no added or removed trailing separator). On `list`, `search`, `files`, `grep`, `read`, `index`, `pull`, `status`, and `clone`, `--root` takes the integer `id` instead. An unregistered path exits `1` with `root is not registered` and points at `xs roots list` / `xs roots add <PATH>`.
- A released binary keeps its database in the OS user-data directory (Windows `%LOCALAPPDATA%\Xingshu\xingshu-prod.db`, Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu/xingshu-prod.db`), with the dev database under `dev/` in the same data root; a source build in development uses the checkout `.staging`. Run `xs db path` to print the resolved path.
- macOS has an experimental build only: this release publishes no macOS assets and macOS is outside the user-support matrix. Linux arm64 is likewise outside the support matrix.
- Uninstalling removes the program only (`xs.exe`, or `~/.local/bin/xs`); deleting the user data directory is a separate, explicit step. `xs` writes no registry keys and no files outside those two locations.

## Quick Start

Source-build path (developers, not the recommended install for users):

```powershell
cargo build --workspace --release
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\run-001
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-b
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
```

Start the local API and the built WebUI:

```powershell
$env:XINGSHU_DB = ".staging\run-001\xingshu-dev.db"
$env:XINGSHU_PORT = "12681"
.\target\release\xingshu-server.exe
```

- API/WebUI: <http://127.0.0.1:12681>
- Health: <http://127.0.0.1:12681/health>
- Vite dev: run `pnpm --ignore-workspace install` in `webui/`, then `pnpm run dev` on port `12680`

Async scan/pull API:

```powershell
$task = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:12681/api/v1/tasks -ContentType "application/json" -Body '{"type":"scan"}'
Invoke-RestMethod "http://127.0.0.1:12681/api/v1/tasks/$($task.taskId)"
```

Batch pull can request a human conflict decision with `{"type":"pull","conflictMode":"ask"}`. Read conflicts from `GET /api/v1/tasks/{taskId}/conflicts`, then post `backup`, `overwrite`, or `skip` (the legacy `abort` value is still accepted) to `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision`. Live progress is streamed over SSE from `GET /api/v1/tasks/{taskId}/stream`; synchronous `POST /api/v1/scan` and single-repo pull remain for script compatibility.

## CLI

```text
xs roots add <path> [--disk LABEL] [--priority N]
xs roots list | rm <id>
xs db path
xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>
xs scan [--root PATH]... [--include-nested] [--max-depth N] [--my-org ORG]... [--full]
xs list [query] [--tag TAG] [--root ID] [--status STATUS] [--org ORG]
xs tag <repo> <tag...> | untag <repo> <tag...>
xs kind set <repo> <third-party|third-party-frozen|fork|own> [--upstream REMOTE]
xs policy set <repo> [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xs policy set --tag TAG [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xs pull [--jobs N] [--unattended] [--root ID] [--dry-run] [--fetch]
xs move <repo> <root-id>
xs stats [--by root|tag|org]
xs status [--root ID] [--repo NAME] [--remote]
xs backup create <repo> [--dest DIR] [--archive tar.gz|tar.zst] [--keep N]
xs backup list <repo> [--dest DIR]
xs backup restore <repo> <backup-path>
xs backup prune <repo> --keep N [--execute]
```

`--root` has two meanings; do not mix them. `scan --root <PATH>` takes the **path of a registered root, copied verbatim from the `path` field of `xs roots list`** (literal comparison, no normalization), while `--root <ID>` on `list` / `clone` / `pull` / `status` / `search` / `files` / `grep` / `read` / `index` takes the integer `id` from that table. An unregistered path exits with code 1, reports `root is not registered`, and points at `xs roots list` / `xs roots add`.

## CLI Database Backup and Restore

The `xs backup` commands above snapshot Git repositories; they do not back up the CLI database. Run `xs db path` to confirm the database the CLI currently selects. Official release defaults are Windows `%LOCALAPPDATA%/Xingshu/xingshu-prod.db` and Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu/xingshu-prod.db`, with the dev database under `dev/xingshu-dev.db` in the same data root. A source CLI in development still uses `.staging`. macOS has an experimental build only and is outside the v0.1.0 user-support matrix.

Before a cold backup, stop every CLI command and server process that could write to that database. Copy the `.db` file together with the same-name `-wal` and `-shm` sidecars (when present) as one group, keeping the file stem identical. The FTS search sidecar is a rebuildable derived cache and need not be backed up; rebuild it on demand with `xs --db <RESTORED_DB> index rebuild`.

To restore, first copy the whole file group to an isolated profile or an alternate database path. Verify with `xs --db <RESTORED_DB> db path`, `xs --db <RESTORED_DB> roots list`, and `xs --db <RESTORED_DB> stats`, and rebuild the FTS index if needed. Only then replace the live database, and stop every writing process before that swap as well. Uninstalling the CLI removes the program only; deleting user data is a separate, explicit operation.

Full command reference with arguments, outputs, and exit codes: [`docs/i18n/en/DEV-002-cli-config.md`](docs/i18n/en/DEV-002-cli-config.md) / [`docs/i18n/zh-Hans/DEV-002-cli-config.md`](docs/i18n/zh-Hans/DEV-002-cli-config.md). `scan` prints its JSON report to stdout and progress to stderr; `pull` writes per-repo results to stdout and may exit `0` even when individual repositories fail (check the output or `fetch_log`).

The `clone` command uses `xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>`. The global `--db` may also appear after `clone`. It writes into a new directory under an existing registered root and registers only that repository. The destination parent must already exist and contain no symlink/junction; existing destinations, stale registrations, and paths inside another registered root are rejected. Private remotes use the system Git credential helper or SSH agent; credential-bearing URLs and raw Git error output are rejected or suppressed.

**PLAN-0587 search/index commands are implemented in this build.** The v1 command contract uses `xs` as the only executable name; there is no `xingshu` executable alias. `index status|update|rebuild` manages a derived SQLite FTS5 trigram index, while `search` and `files` query indexed HEAD content and paths. `grep` scans selected worktree text directly, including tracked modifications and non-ignored untracked files; the optional `--ref` reads one local Git revision without a checkout. `read` reads a bounded text file from HEAD or a local ref. Search and index status can aggregate same-key repositories across selected roots and report `rootId`; single-target operations such as `read` and index update/rebuild require `--root` when a repo selector is ambiguous. Search/grep default to 1,000 match lines and files to 10,000 paths (maximum configurable cap 100,000); truncation is a successful but incomplete result. Searches are read-only: no fetch, pull, checkout, or repository writes; they do not follow symlinks, do not search `.git` objects or binary contents, and do not implicitly traverse unregistered roots. `search` never scans a worktree and reports missing/stale indexes instead. See the full contract in the CLI reference.

## Safety

- `third-party` and `third-party-frozen` only lock modification inside Xingshu's own operations; Git use outside Xingshu is not intercepted.
- Pull conflicts stop by default and are handed to the user; unattended mode defaults to `skip` (the legacy `abort` value is still accepted).
- Test and development writes may only use a tempdir or a dedicated staging run; never run pull, move, backup, or overwrite against real reference repositories.
- Clone accepts only standard HTTP(S), SSH/SCP-like, and file URLs; URLs with embedded credentials are rejected and private repositories use the system Git credential helper or SSH agent. Logs and errors never contain tokens, PATs, Bearer values, raw Git stderr, or full environment variables.

## Logging

Runtime logs are JSONL on stderr by default. Set `XINGSHU_LOG_LEVEL` (falling back to `RUST_LOG`) to control the level; set `XINGSHU_LOG_FILE` to enable a JSONL file log rotated daily or by size, defaulting to 10 MiB per file with 7 rotated files, overridable through `XINGSHU_LOG_MAX_BYTES`/`XINGSHU_LOG_MAX_FILES`. `fetch_log` is the pull audit trail in SQLite and is not a replacement for runtime logs.

## Project Documentation

- Product contract index: [spec/README.md](spec/README.md) (accepted target contracts; implementation status is tracked separately)
- Documentation index: `docs/AGENTS.md`
- Chinese documents: `docs/i18n/zh-Hans/README.md`
- English documents: `docs/i18n/en/README.md`
- API contract: `docs/api/openapi.yaml`
- Ports: WebUI `12680`, API/server `12681`

## Development

### Development Mode (mise, recommended)

```powershell
# 1. Install deps (Rust + WebUI)
mise run setup

# 2. Create a fresh staging run (synthetic; never writes to reference repositories)
mise run staging:dev --run-id dev-001
# (default DB is now .staging/dev-001/xingshu-dev.db; set $env:XINGSHU_DB only to switch runs)
# Prod DB: mise run prod:server (XINGSHU_ENV=prod, ./xingshu-prod.db)

# 3. Start server (12681) + Vite (12680, proxies /api to 12681) in parallel
mise run dev          # == mise run dev:host, Ctrl+C cleans up both
```

- Vite: http://127.0.0.1:12680  ·  API/built WebUI: http://127.0.0.1:12681  ·  Health: http://127.0.0.1:12681/health
- Add roots in Settings → click **Scan index** (or `POST /api/v1/tasks {type:scan}`); repos appear in the repository table.
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
