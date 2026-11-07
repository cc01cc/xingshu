# Backup and Output Readability

> Sync status: the conflict three-state table, the backup-then-pull contract, and the output-readability rules mirror zh-Hans. The UI label examples are the Chinese strings the panel actually renders. The reference list names the same symbols as zh-Hans but omits line numbers, because the numbers in the zh-Hans copy are stale (tracked as documentation debt).

## Conflict classification (PLAN-250)

| State | Decision | UI |
|---|---|---|
| `dirty` | `git status --porcelain` has entries (inspect keeps the details) | Conflict panel: overview counts + file list + consequences comparison |
| `diverged`/`non-ff` | after fetch `rev-list --count` reports ahead>0 and behind>0, or the ff-only failure output matches | Conflict panel: two-column commit lists + merge base + `git cherry` ≡ equivalent pairs |
| `ahead_clean` | clean and ahead only (including a false lead caused by an upstream force-push rollback) | Not in the panel; `pull` records `ahead` (a no-op) and `last_pull_status=ahead` drives the "locally ahead" badge in the repository list |

When fetch fails (offline), the earlier decision is used as a fallback: ahead counts as a conflict because the information may be stale. A synchronized repository (ahead=behind=0) pulls with a normal fast-forward. The structured details travel through `ConflictInfo` (serde camelCase) into `task_repos.conflict_json` (DB migration `ensure_column`), the SSE `TaskEvent.conflict`, and the `conflict` field of `GET /tasks/{id}/conflicts`; `conflictReason` keeps the human-readable summary and stays backward compatible.

## Backup-then-pull

| Item | Contract |
|---|---|
| Trigger | `crates/xingshu-core/src/puller.rs:resolve_pull_conflict` — a dirty worktree `dirty`, or a `pull --ff-only` conflict message (`cannot fast-forward`/`divergent branches`/`would be overwritten`) |
| Location | `<repo>.bak.<YYYYMMDDHHmmSS.mmm>` in the same parent directory (milliseconds; `_{n}` on collision), moved atomically with `fs::rename` |
| Ignore | `scanner.rs:is_generated_backup` treats directories whose name contains `.bak.` or `.broken.` as non-repositories; `.gitignore` ignores them too |
| Mode | after the backup, a **full** `git clone <remote_url> <repoName>` into the parent directory; since PLAN-297 a failure rolls back automatically (clears the half-built copy and renames `.bak` back to the original name) and writes a `backup_failed` audit; without `remote_url` it fails **before** the rename, so the repository is untouched |
| Compare | `Overwrite` is incremental `reset --hard` + `clean -fd` + `pull --ff-only`; `Skip` (formerly `Abort`) skips; the UI requires a second confirmation for `Overwrite` |
| Open | `POST /api/v1/open` (canonicalize + registered-root prefix allowlist, raw `Command::arg` arguments with no shell) opens the repository or backup directory |
| Persistence | `task_repos.backup_path` + `tasks.result_json: counts/repos[].backupPath`, broadcast as the SSE `decision_applied` event; `waiting_decision` survives a restart while `resolving` becomes `interrupted` and is not replayed |

## Output readability

- The server stores `tasks.result_json` as a compact string (cheap to persist); the client parses it with `parsedTaskResult / prettyTaskJson / taskCounts / taskRepos / taskScanStats`
- Mapping (the panel renders Chinese labels, glossed here in parentheses): `ok→成功` (success), `aborted/skipped→已跳过` (skipped; `aborted` is a legacy value), `failed→失败` (failed), `conflict→冲突需决策` (conflict needs a decision), `pending→等待中` (pending), `running→运行中` (running), `completed→已完成` (completed); durations pick the `ms/s/min` unit automatically
- Task panel order: summary `共 N 个仓库：成功 X、已中止 Y` → badges → table (repo/status/result/duration/backup/error) or scan stats → collapsed raw JSON (debugging)
- Common error: `Failed to fetch` → `无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681`

## References

- `crates/xingshu-core/src/puller.rs` — `backup_path`, `resolve_pull_conflict`
- `crates/xingshu-server/src/main.rs` — `resolve_task_repo_decision`
- `crates/xingshu-server/src/tasks.rs` — `result_json`
- `webui/src/components/TaskPanel.vue` — `parsedTaskResult`; `webui/src/lib/format.ts` — `labelResult`/`labelStatus`/`formatDuration`
