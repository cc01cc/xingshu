# WebUI

> Sync status: the section layout and the page behavior below are synchronized with zh-Hans; on-screen label examples are localized to the strings the UI actually renders.

## Startup

Prefer `mise run dev` (`mise.toml`, mirrors A03-xihe): runs Axum on `12681` and Vite on `12680` in parallel (`/api`/`/health` proxied to `12681`), `Ctrl+C` stops both; raw fallback is `cargo run -p xingshu-server` plus `cd webui && pnpm --ignore-workspace run dev`. The built bundle is served by Axum on `12681`.

## Pages

- Repositories: search, tag filter, kind/lock status, detail drawer; empty states link to Settings for the scan CTA (`No index yet — scan in Settings`). Switching the kind dropdown in the drawer asks for confirmation and explains the read-only convention flip; a hover tooltip next to the read-only marker explains it (third-party means read-only by convention: local changes become decision items, pull needs a human decision).
- Fetch history: inside the detail drawer, one row per entry with `YYYY-MM-DD HH:mm:ss`, strategy, and a result badge.
- Tags: create domain labels and jump to filtered lists.
- Disk dashboard: indexed count, bytes, and decision backlog.
- Settings: root management (absolute path → Add, multiple roots across disks), hint `Scanning is required after adding roots`, primary CTA `Scan (N roots)` with `Indexed N / Not yet scanned` copy; scanning an empty directory and getting 0 is expected.
- Header: `REPOSITORY INDEX` titles are `Repositories/Tags/Disk/Settings`, matching sidebar `repos/tags/disks/settings`.
- Conflicts: per dirty/conflict repo choose backup-and-pull / overwrite-local / keep-the-current-state (the UI uses `skip`; legacy `abort` remains accepted).
- Conflict classification panel: conflicts are grouped into three states — `dirty` (worktree changes: summary counts, the `status --porcelain` file list, and a consequences comparison for each action), `diverged`/`non-ff` (two-column commit lists, local-only vs upstream-only, plus the merge base and `git cherry` patch-equivalent pairs marked "≡ same content, likely redone after a revert"), and `ahead_clean` (a purely local ahead state stays out of the panel and only shows a "locally ahead" badge in the repository-list status column: third-party hints at a suspected upstream revert, own/fork hints at unpushed commits). Overwriting local state requires a second confirmation that states untracked files are discarded and the change is unrecoverable.
- Read-only diff: clicking a file row in the `dirty` detail opens a read-only worktree diff versus HEAD (capped at 512 KiB, truncated on a UTF-8 boundary); untracked and binary files are labeled instead of patched, escaping or absolute paths are rejected with 400, missing files return 404, and bare repositories are rejected.
- Policy page: pick a repository and Load to see the effective policy with a source badge (`repo` / `tag "X"` / `kind default`; the first matching tag wins); Save writes the repo-level policy.
- Tag policy: pick a tag to see its member count and load/save the tag policy; a tag policy is the inheritance default — member repos without their own repo-level policy inherit it, existing repo-level exceptions stay untouched.
- Move precheck: after picking a target root in the drawer the UI probes that disk and shows `available X · required Y`; insufficient space (including equality) disables the move, and probe failures surface as errors instead of over-reporting free space.
- Open directory: the "Open directory" button in the repository detail drawer and the folder icon next to the backup path in the conflict card call `POST /api/v1/open` (the path must be inside a registered root) to open the repository or the backup directory in the system file manager, so a backup can be packed or moved by hand or investigated on site.
- Async tasks: `Scan` and `Batch pull` in the sidebar (also in Settings) create background tasks; the panel uses Chinese labels (Success/Aborted/Skipped/Failed/Needs decision), capsule badges, a table (repo/status/result/duration/backup/error) and scan stats, maps `Failed to fetch` to a local-service hint for `12681`, and keeps raw JSON collapsed for debugging.

Batch pull starts with `conflictMode=ask`, so a conflicting repository is shown in a decision list while other repositories continue. The three conflict actions are backup-and-pull, overwrite-local-state, and abort. Xingshu tags never create or modify Git tags.

## Asynchronous tasks

The WebUI uses the task API for operations that may take a long time:

- `POST /api/v1/tasks` with `{ "type": "scan" }` or `{ "type": "pull", "conflictMode": "ask" }` returns `202` and a `taskId`.
- `GET /api/v1/tasks/{taskId}` returns the latest SQLite-backed snapshot. Statuses are `pending`, `running`, `waiting_for_decision`, `completed`, `failed`, and `interrupted`.
- `GET /api/v1/tasks/{taskId}/conflicts` returns task repositories that require or are applying a conflict decision.
- `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision` accepts `backup`, `overwrite`, or `skip` (legacy `abort` is still accepted) and returns `202`.
- `GET /api/v1/tasks/{taskId}/stream` sends an SSE `snapshot`, followed by `started`, `progress`, `conflict`, `waiting_for_decision`, `decision_applied`, `completed`, or `failed` events.

When SSE disconnects, the UI first reads the GET snapshot and conflicts, then reconnects. A disconnect is not treated as task failure. The stream closes after completion or failure, while the final state remains available through GET. After a server restart, `waiting_for_decision` remains recoverable and an in-flight destructive action is not replayed automatically.
