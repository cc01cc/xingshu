# CLI Reference and Configuration

> Sync status: the CLI release workflow and version-preparation contract are synchronized with zh-Hans; platform-specific command examples are localized.

## Install and Ports

Prefer `mise run setup` (`mise.toml`) for development dependencies; installing WebUI deps separately from `webui/` still requires `pnpm --ignore-workspace install`. Vite uses port `12680`; the Axum API and built UI use `12681`; one-shot start is `mise run dev`.

## CLI v0.1.0 First-install and Release Flow

The first stable release version is `v0.1.0`. A push of a sanitized candidate snapshot to public branch `release-candidate/v0.1.0` triggers a push-only candidate workflow; there is no `workflow_dispatch` trigger. Candidate versions remain `0.1.0` with no RC tag. Candidate Actions artifact names are `xingshu-candidate-x86_64-pc-windows-msvc-attempt-<run_attempt>` and `xingshu-candidate-x86_64-unknown-linux-gnu-attempt-<run_attempt>`, with 90-day retention; each immutable artifact is associated with the same `github.sha` and `candidateRunAttempt` and contains its platform archive, a matching `<archive>.sha256` file, and `xingshu-v0.1.0-<target>.manifest.json` recording `candidateRunAttempt`, `minimumOsAbi`, `buildRunner`, `crtLinkage`, and `runtimeDependencies`. Candidate artifacts are validation-only, not stable builds; they contain per-archive `.sha256` files but no aggregate checksum. Stable GitHub Releases also include `SHA256SUMS`; the public Releases page lists official stable binaries and current availability.

The v0.1.0 candidate artifacts and stable Release target Windows x86_64 and Linux x86_64 only. The Windows release target statically links the MSVC CRT; candidate CI runs `dumpbin /dependents` and fails if VCRUNTIME/MSVCP/UCRT imports remain. Manifest `minimumOsAbi` / `buildRunner` values are Windows `Windows 10+` / `windows-2025` and Linux x86_64 GNU `glibc 2.35+` / `ubuntu-22.04`; `crtLinkage` records `static`, while `runtimeDependencies` records Windows 10+ and Git on PATH or Linux glibc 2.35+, Git and `df` on PATH (`df` is supplied by GNU coreutils for move free-space checks); the Linux manifest also records the runner's `ldd` dynamic-library SONAMEs and the build fails if a dependency is unresolved. The tag workflow verifies all these fields against the target allowlist. The candidate run provides installation/runtime evidence for the OS floors; after the Windows import gate passes, Windows 10+ and system Git are the only runtime prerequisites, with no VC++ Redistributable required. Linux arm64 and macOS are outside the v0.1.0 user-support matrix; macOS has an experimental build only and no macOS asset is published. The public Releases page is authoritative for available assets and supported platforms. Stable archive names are `xingshu-v0.1.0-x86_64-pc-windows-msvc.zip` (Windows) and `xingshu-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` (Linux).

Signature and integrity: candidate and stable manifests set `signatureStatus=unsigned`; the platform archives are not code-signed. Windows may show SmartScreen or unknown-publisher warnings; Linux archives are also unsigned. The adjacent `.sha256` file and Release `SHA256SUMS` check file integrity only and do not authenticate a publisher.

Installation steps (choose the archive matching your target):

1. Download the candidate artifact from its Actions run, or a stable archive from Releases; also obtain its adjacent `<archive>.sha256` file and matching manifest (for example, `xingshu-v0.1.0-x86_64-pc-windows-msvc.zip.sha256`). Stable Releases also include aggregate `SHA256SUMS`; verify an individual archive against its adjacent `.sha256` file. Candidate and stable archive/per-file checksum names match.
2. Verify the archive hash: Windows `Get-FileHash <archive> -Algorithm SHA256`; Linux `sha256sum <archive>`. The result must match that archive's `.sha256` file. macOS has no official user archive in the first release and is not covered by these install steps.
3. Extract the archive and place `xs` (`xs.exe` on Windows) in a user-writable directory on PATH. Open a new terminal and run `xs --version`, `xs db path`, and `git --version`.
4. Git is a runtime dependency for repository operations. The current CLI checks Git before opening the database; if `git` is not on PATH it reports ``git command failed: Git executable not found on PATH; install Git and ensure `git` is available, then restart the terminal``. Install Git, open a new terminal, and retry. `xs db path` is exempt and does not require Git. Test candidate builds with an isolated temporary user profile, not daily data.

Stable install/upgrade assets become available only after candidate validation and final user approval. The release owner fast-forwards that same candidate commit to public `main` and pushes the `v0.1.0` tag. The user-level path from Releases to a first scan is the "Install from Releases" section of the repository root `README.en.md`; this document keeps the command-level details. Uninstalling removes the CLI program but does not automatically delete the user data directory.

## Versioning and Releases

- The first stable version is `0.1.0`. A candidate artifact reports version `0.1.0` but is validation-only; the stable `v0.1.0` tag follows candidate validation and final user approval.
- All versions before `1.0.0` are unstable. The default 0.x bump is minor; a maintainer may explicitly choose patch only for fixes that do not change public behavior, configuration, or data semantics. An Agent must not automatically lower the default minor bump to patch.
- In 0.x, minor is used for new features and breaking CLI/config/default-path/schema/data changes; such changes must be marked `BREAKING` in the changelog and describe their impact. `release:prepare` rejects major while the workspace is 0.x. At 1.0+, standard SemVer applies: minor remains the default, and a maintainer may explicitly choose patch or major. The 0.x-to-1.0 transition requires a separate approved stability decision/PLAN and a one-time manual workspace version change; it is not a routine automated bump, and there is no `--version` option.
- The stable publish workflow is triggered only by a stable SemVer `v*` tag push with no prerelease suffix. After candidate validation and final user approval, the release owner fast-forwards the same candidate commit to public `main` and pushes `v0.1.0`. The workflow finds a successful candidate run on `release-candidate/v0.1.0` with the same source SHA and `run_attempt`, downloads that exact immutable artifact, verifies its manifest/checksum and `minimumOsAbi`/`buildRunner`/`crtLinkage`/`runtimeDependencies` values against the target allowlist, then creates the Release from the tag without rebuilding or pushing to the private repository. There is no stable publish `workflow_dispatch` or candidate-run-ID input.
- The current source includes `mise run release:prepare`: dry-run minor by default; for the first release, `--first-release` verifies workspace version `0.1.0` and no-ops without requiring `cargo-release`. Non-first bumps require the maintainer to install `cargo-release` 1.1.7 manually and put it on PATH; the task does not install it. Install it with `cargo install cargo-release --version 1.1.7 --locked` (into `~/.cargo/bin`). While the workspace is 0.x, explicit patch is supported and major is rejected; at 1.0+, standard SemVer permits explicit patch or major, while the default remains minor. Only `--execute` changes the version. The task runs only the Cargo workspace version step, without publishing crates, creating tags, or pushing; the maintainer curates the CHANGELOG. The transition from 0.x to 1.0 requires a separate approved stability decision/PLAN and one-time manual workspace version change; it is not automated, and there is no `--version` option. An Agent must not downgrade the default bump to patch automatically.
- Candidate Actions artifacts are validation-only; official stable binaries and current availability are listed on the public GitHub Releases page.

## Database Selection

**Current source-build and server behavior:** files follow `xingshu-<env>.db`. Dev defaults to `.staging/dev-001/xingshu-dev.db`; the server defaults to `./xingshu-prod.db` when `XINGSHU_ENV=prod`. The current source CLI precedence is explicit `--db` > `XINGSHU_DB` > default; prod refuses `.staging/` paths (`refusing to open staging database`). A `--db` path whose location cannot be determined (illegal characters, a dangling symlink/junction, a non-directory component) is also refused in prod with `cannot classify --db path <PATH> as production: <OS reason>`; dev runs skip that classification and let the open layer report the original error.

**Official v0.1.0 CLI path behavior (implemented in source):** the official release CLI defaults to the current user's OS data directory; a source-checkout CLI in dev continues to use `.staging/dev-001/xingshu-dev.db`. `XINGSHU_ENV=dev|prod` explicitly selects the CLI runtime environment; both modes in an official release use the OS data root, and invalid values fail. Final precedence is explicit `--db` > `XINGSHU_DB` > build-marker/environment default. The first official run creates schema v5 and does not import a development `.staging` database. The FTS search sidecar is rebuildable derived data. The public GitHub Releases page lists official stable binaries and current platform availability.

The server retains its existing path contract: dev defaults to `.staging/dev-001/xingshu-dev.db`; with `XINGSHU_ENV=prod`, it defaults to `./xingshu-prod.db` and logs the resolved path at startup. The official CLI's user-data paths do not change `mise run dev:server` or `mise run prod:server`.

`--db` is a global CLI option (Clap global) accepted before or after the subcommand. Absolute paths are used as given; relative paths resolve from the current working directory.

## CLI Database Backup and Restore

The `xs backup` commands above snapshot Git repositories; they do not back up the CLI database. Run `xs db path` to locate the database actually selected by the CLI. Official release defaults are Windows `%LOCALAPPDATA%/Xingshu/xingshu-prod.db` (dev: `%LOCALAPPDATA%/Xingshu/dev/xingshu-dev.db`) and Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu/xingshu-prod.db` (dev: `<data-root>/dev/xingshu-dev.db`). A source CLI in dev still uses checkout `.staging`. macOS has an experimental build only: no macOS asset is published and macOS is outside the v0.1.0 user-support matrix.

For a cold backup, stop all CLI commands and server processes that may write that database. Copy the `.db` file and any same-basename `-wal` and `-shm` sidecars together as one set. The FTS search sidecar is derived/rebuildable data and need not be backed up; if it is missing or stale, rebuild it on the restored copy with `xs --db <RESTORED_DB> index rebuild`.

Restore the full file set to an isolated profile or alternate database path first. Confirm the target with `xs --db <RESTORED_DB> db path`, then verify records with `xs --db <RESTORED_DB> roots list` and `xs --db <RESTORED_DB> stats`. Switch to the active path only after verification, with all writers stopped. Uninstalling the CLI removes only the program; deleting user data is a separate deliberate action.

Tags are Xingshu-owned domain labels, not Git tags. `repo_kind` distinguishes `third-party`, `third-party-frozen`, `fork`, and `own`; `policy set` configures update and conflict behavior per repository or per tag, and repository policy wins.

## Logging and Output Contract

Logging (full variables in DEV-004):

- Level: `XINGSHU_LOG_LEVEL` (falls back to `RUST_LOG`), default `info`; `off` disables logging entirely.
- Sink: JSONL to **stderr** by default; setting `XINGSHU_LOG_FILE` switches to size-rotated JSONL file logging.
- stdout carries business output only; the `scan` report is pure JSON and can be piped to `jq`/scripts.

Exit codes:

- `0`: command succeeded.
- `1`: argument or domain error; stderr prints `Error: <reason>` (missing path, unknown repo/tag, invalid strategy, ...).

Exception: per-repository `pull` failures (network errors, no upstream, conflict with `stop`, ...) only write to stderr and `fetch_log`; the **process still exits 0**. Scripts must inspect the output or `fetch_log` instead of relying on the exit code alone.

Output contract per command:

| Command | stdout | stderr |
|---------|--------|--------|
| `roots add` | `registered root <id>` | logs |
| `roots list` | root JSON array (pretty) | logs |
| `roots rm` | none | logs |
| `db path` | JSON containing effective environment and absolute `dbPath` | error details (for example, an invalid path environment variable) |
| `clone` | `cloned <org>/<name> into root <id> at <rel_path> (HEAD <sha>)` | fixed safe error summary + logs; Git stderr is suppressed |
| `scan` | ScanReport JSON (pretty) | per-repo progress + logs |
| `list` | RepoRecord JSON array (pretty), `[]` when empty | logs |
| `tag` / `untag` | none | logs |
| `kind set` | none | logs |
| `policy set` | none | logs |
| `pull` | one line per repo `[i/total] org/name: result (ms)` | `pull started/finished` summary + failure details + logs |
| `move` | `moved to <path>` | logs |
| `stats` | two lines `repositories: N` / `bytes: N` | logs |

## Command Reference

### db path

```text
xs db path
```

- Read-only JSON output: `environment` is the effective mode and `dbPath` is the resolved absolute path.
- Runs before opening SQLite or creating the database parent directory; it does not require Git and does not create the database file.

### roots

```text
xs roots add <PATH> [--disk LABEL] [--priority N]
xs roots list
xs roots rm <ID>
```

- `add`: registers a root directory; `PATH` must exist and be a directory, otherwise exit 1. `--disk` records the disk label, `--priority` records priority (default 0).
- `list`: prints all root fields (`id/name/path/disk_label/mount_point/priority/created_at/updated_at`).
- `rm`: removes a registration by `ID`; an unknown `ID` succeeds silently (idempotent). Only the index changes; repository data on disk is untouched.

### clone

```text
xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>
```

- `REMOTE` accepts Git HTTP(S), `ssh://`, SCP-like, and `file://` URLs; bare relative paths and Windows drive paths are unsupported. HTTP(S)/file URLs must not contain userinfo; all forms reject query strings and fragments. SSH/SCP-like forms allow no username or the literal `git` username only. Private remotes authenticate through the system Git credential helper or SSH agent; never put a password/token in the URL.
- `--root` must identify a registered root in the database selected by `--db`/`XINGSHU_DB`; normal database open/migrations still run before command dispatch. `--dest` is relative to that root and its parent must already exist. Absolute/drive/UNC paths, `..`, any symlink/junction component in the destination parent, existing files/directories, stale repo rows, and destinations inside another registered root are rejected. Core exclusively creates the final directory.
- Repeatable `--my-org` uses the same classification input as `scan --my-org`. When omitted, classification uses an empty owner list: an `upstream` remote means `fork`; otherwise the default is `third-party`.
- Success registers only this repository's metadata; it does not scan the whole root or build a text index. Run `xs index update --repo <ORG/REPO> --root <ROOT_ID>` when indexing is wanted.
- stdout reports repo key, root id, relative destination, and HEAD. Failures return nonzero; raw Git stderr is never printed. After Git starts, failures and no-HEAD clones are not automatically cleaned; the error reports whether the destination remains. A clone is retained if DB registration fails.

### scan

```text
xs scan [--root PATH]... [--max-depth N] [--include-nested] [--my-org ORG]...
```

- `--root` has two meanings: on this command it takes a **registered root path**, copied verbatim from the `path` field of `xs roots list` (literal comparison, no normalization); on `list`, `clone`, `pull`, `status`, `search`, `files`, `grep`, `read`, and `index` it takes the integer `id` from that table. `--root` is repeatable; when omitted, every registered root is scanned. An unregistered path exits 1, and the error points at `xs roots list` / `xs roots add`.
- `--max-depth`: `0` (default) means unlimited depth.
- `--include-nested`: also index nested repositories (by default only the outer repository is indexed).
- `--my-org` is repeatable: a remote owner match classifies the repository as `own` on first scan; a repository with an upstream remote pointing elsewhere is classified as `fork` first.
- stdout is the scan report: `roots_scanned / repos_found / broken_repos / skipped_directories / nested_repos / errors`.
- stderr reports per-repo progress: `[N] <rel_path>: <clone_status>`.

### list

```text
xs list [QUERY] [--tag TAG] [--root ID] [--status STATUS] [--org ORG]
```

- `QUERY` is case-insensitive and matches `org/name/rel_path`.
- `--tag` is a Xingshu domain tag (not a Git tag); multiple filters are AND-combined.
- Prints a RepoRecord array (pretty JSON); an empty result prints `[]` and exits 0.

### tag / untag

```text
xs tag <repo> <tag...>
xs untag <repo> <tag...>
```

- `<repo>` accepts `name`, `org/name`, or the rel_path (for example `local/demo`).
- `tag`: creates missing tags automatically; `untag`: a missing tag exits 1 (`Error: tag not found: ...`).
- Success is silent (no output, exit 0).

### kind set

```text
xs kind set <repo> <third-party|third-party-frozen|fork|own> [--upstream REMOTE]
```

- Explicitly sets `repo_kind` (rescans never overwrite it); `--upstream` records the fork upstream remote.
- An invalid kind exits 1 (`Error: invalid repository kind: ...`).

### policy set

```text
xs policy set [REPO] [--tag TAG] [--strategy fetch-only|mirror|archive|no-update] [--conflict stop|backup|overwrite|skip] [--unattended stop|backup|overwrite|skip]
```

- `REPO` and `--tag` are mutually exclusive; supplying both or neither exits 1.
- Defaults: `--strategy fetch-only`, `--conflict stop`, `--unattended skip`.
- Repository policy wins over tag policy.

### pull

```text
xs pull [--jobs N] [--unattended]
```

- Interactive mode (default) pulls repositories sequentially; `--unattended` processes them in chunks of `--jobs` (default 8).
- Conflict handling: interactive mode reads the repo/tag `pull_conflict_policy` (default `stop` -> conflicting repositories are reported as error lines on stderr and skipped, never rewritten); unattended mode reads `unattended_conflict_policy` (default `skip`).
- Successful, skipped, and `skip` results are written to `fetch_log` and update the repository pull status; conflicts that `stop` do not write `fetch_log`.
- Failure details and start/finish summaries go to stderr; result lines go to stdout.

Example: `org/demo` has local changes (dirty) and triggers `stop`; `org/clean` updates normally.

```text
# stdout
[1/2] org/clean: ok (1178ms)

# stderr (JSONL log lines omitted)
pull started: 0/2
[2/2] <repo-root>/org/demo: repository conflict requires a user decision
pull finished: 2 repositories processed

# exit code 0: demo is skipped and left untouched (no fetch_log); clean was updated and recorded.
```

The same scenario with `--unattended` (default `skip`):

```text
# stdout (chunked concurrency; completion order varies, [i/total] is the task index)
[2/2] org/demo: skipped (355ms)
[1/2] org/clean: ok (1224ms)

# stderr (JSONL log lines omitted)
pull started: 0/2
pull finished: 2 repositories processed

# exit code 0: skip is recorded in fetch_log (result=skipped); demo is left untouched. Legacy `abort` remains accepted as an alias.
```

### move

```text
xs move <repo> <ROOT_ID>
```

- Moves a repository across roots and updates its index location; a same-root move or unknown target exits 1.
- On success prints `moved to <path>` to stdout.

### stats

```text
xs stats
```

- Prints `repositories: N` and `bytes: <total>`.
- There is no per-root/tag/org aggregation yet (the HTTP API `/api/v1/stats` provides `byKind`).

## Common Flows

```powershell
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db tag network/third-party-demo network
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db list --tag network
```

## Safety Parameters

- Interactive pull conflicts default to `stop`.
- Unattended pull conflicts default to `skip`.
- `scan_nested_repos` is disabled by default; `scan_max_depth=0` means unlimited depth and `scan_skip_dirs` prunes large non-repository trees.
- `third-party`/`third-party-frozen` repositories are constrained by `modify_lock` inside Xingshu only; real reference repositories are read-only for `scan`/`list`/`stats`, never pull/move/backup/overwrite.

## PLAN-0587: Content Index and Search

**Implemented in this build.** The v1 executable is `xs` only, with no `xingshu` alias. Existing Cargo package invocations such as `cargo run -p xingshu-cli` and server names remain unchanged.

```text
xs index status [--repo <ORG/REPO>]... [--root <ID>] [--json]
xs index update [--repo <ORG/REPO>] [--root <ID>] [--all-roots] [--json]
xs index rebuild [--repo <ORG/REPO>] [--root <ID>] [--all-roots] [--json]
xs search <PATTERN> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--glob <GLOB>]... [--fixed-strings] [--ignore-case] [--word-regexp] [--context <N>] [--files-with-matches] [--count] [--max-count <N>] [--max-results <N>] [--json]
xs grep <PATTERN> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--glob <GLOB>]... [--fixed-strings] [--ignore-case] [--word-regexp] [--context <N>] [--files-with-matches] [--count] [--max-count <N>] [--max-results <N>] [--ref <REV>] [--all-roots] [--json]
xs files <GLOB> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--max-results <N>] [--json]
xs read <ORG/REPO> <PATH> [--root <ID>] [--ref <REV>] [--start-line <N>] [--max-lines <N>] [--json]
```

- `index status` reports HEAD, indexed commit, coverage and freshness without scanning. `index update` incrementally indexes the selected repo(s); `index rebuild` recreates their derived index. Update/rebuild require an explicit scope (`--repo`, `--root`, or `--all-roots`); `--all-roots` cannot be combined with repo/root selectors.
- SQLite FTS5 trigram with `detail=none` is the selected v1 content index. Safe literal patterns of at least three characters can use FTS5 `LIKE` as a candidate filter, followed by exact matching. Short literals, wildcard fixed strings, and complex regex scan only cached indexed text, never the worktree. `search` reads indexed HEAD only; it does not auto-update missing or stale indexes. It reports `index_missing` or `index_stale` and suggests `index update`.
- `grep` bypasses the persistent index and reads eligible text from the selected live worktree, including tracked edits and non-ignored untracked files. Every result identifies `source=worktree`, the scan-start `baseHead`, and `fileState`; changed/untracked files have no commit revision. With `--ref`, grep reads one locally available branch, tag, or commit tree without fetch or checkout and reports `source=git-ref`. `--ref` requires exactly one `--repo` and cannot be used with `--all-roots`.
- `files` searches repo-relative paths in the indexed HEAD path catalog (including paths for binary files); it does not read bodies or scan the live worktree. `read` returns bounded text from HEAD or one local ref: default maximum 200 lines, maximum 5,000 lines, file-size ceiling 5 MiB.
- Repo keys use `org/name`. Multi-result queries can return same-key repositories from multiple selected roots, with `rootId` on every result. A single-target operation such as `read` or index update/rebuild must be disambiguated with `--root`; otherwise it returns `ambiguous_repo`. `--all-roots` is an explicit scope and cannot combine with `--repo` or `--root`; it is available only for grep and index maintenance as specified above. Unregistered paths are never implicit search scope.
- Search and grep default to 1,000 match lines; files defaults to 10,000 paths. `--max-results` accepts 1–100,000. Reaching the cap returns a stable, bounded prefix with `truncated=true`, `complete=false`, no errors, and exit code 0. Output lines are capped at 4,096 Unicode characters. No matches is a successful empty result. Repository-level read/scan errors preserve partial results, set `complete=false`, populate `errors`, and exit 1.
- Safety: all these operations are read-only with respect to source repositories. They do not fetch, pull, checkout, or modify repository files. Index writes are confined to the database-identity and environment-specific derived cache; rebuild failure preserves the prior published generation. Symlinks are not followed; `.git` objects, binary bodies, generated directories, and unregistered directories are excluded. Text indexing/search applies a 5 MiB per-file limit and requires valid UTF-8 without NUL. The index covers current HEAD only, not history; use single-repo `grep --ref` for a local historical revision.


- `scan --full`: disables the incremental scan and re-reads metadata for every repository (the default skips unchanged repositories via a fingerprint; the report gains `unchanged_repos`).
- `pull --root <id>`: limit the batch to one registered root; `--dry-run`: zero-write preview (remote divergence marked "needs fetch"); `--dry-run --fetch`: refresh local refs first for an exact prediction (no audit writes).
- `stats --by root|tag|org`: aggregate by dimension; multi-tag repositories are counted in every matching tag group (noted in the output).
- `status [--root <id>] [--repo <name>] [--remote]`: read-only status check; `--remote` uses `git ls-remote` to detect remote movement (zero local writes; failures are reported as unknown).
- `backup create <repo> [--dest DIR] [--archive tar.gz|tar.zst] [--keep N]`: independent backup (source untouched; a directory snapshot next to the repository by default); `backup list <repo>`; `backup restore <repo> <backup-path>` (the current state is kept as `*.bak.pre-restore.*`); `backup prune <repo> --keep N [--execute]`: prune (dry-run preview by default, `--execute` deletes; mirrors the API `dryRun` semantics). Compressed archives require system `tar`; creating/restoring `.tar.zst` also requires system `zstd`.
- Conflict action value is now `skip` (legacy `abort` accepted; result value `skipped`).
