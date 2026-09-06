# WebUI

Prefer `mise run dev` (`mise.toml`, mirrors A03-xihe): runs Axum on `12681` and Vite on `12680` in parallel (`/api`/`/health` proxied to `12681`), `Ctrl+C` stops both; raw fallback is `cargo run -p xingshu-server` plus `cd webui && pnpm --ignore-workspace run dev`. The built bundle is served by Axum on `12681`.

## Pages

- Repositories: search, tag filter, kind/lock status, detail drawer; empty states link to Settings for the scan CTA (`No index yet — scan in Settings`). Switching the kind dropdown in the drawer asks for confirmation and explains the read-only convention flip; a hover tooltip next to the read-only marker explains it (third-party means read-only by convention: local changes become decision items, pull needs a human decision).
- Fetch history: inside the detail drawer, one row per entry with `YYYY-MM-DD HH:mm:ss`, strategy, and a result badge.
- Tags: create domain labels and jump to filtered lists.
- Disk dashboard: indexed count, bytes, and decision backlog.
- Settings: root management (absolute path → Add, multiple roots across disks), hint `Scanning is required after adding roots`, primary CTA `Scan (N roots)` with `Indexed N / Not yet scanned` copy; scanning an empty dir such as `Z:\TEST` yielding 0 is expected.
- Header: `REPOSITORY INDEX` titles are `Repositories/Tags/Disk/Settings`, matching sidebar `repos/tags/disks/settings`.
- Conflicts: per dirty/conflict repo choose backup-and-pull / overwrite-local / abort.
- Async tasks: `Scan` and `Batch pull` in the sidebar (also in Settings) create background tasks; the panel uses Chinese labels (Success/Aborted/Skipped/Failed/Needs decision), capsule badges, a table (repo/status/result/duration/backup/error) and scan stats, maps `Failed to fetch` to a local-service hint for `12681`, and keeps raw JSON collapsed for debugging.

Batch pull starts with `conflictMode=ask`, so a conflicting repository is shown in a decision list while other repositories continue. The three conflict actions are backup-and-pull, overwrite-local-state, and abort. Xingshu tags never create or modify Git tags.

## Asynchronous tasks

The WebUI uses the task API for operations that may take a long time:

- `POST /api/v1/tasks` with `{ "type": "scan" }` or `{ "type": "pull", "conflictMode": "ask" }` returns `202` and a `taskId`.
- `GET /api/v1/tasks/{taskId}` returns the latest SQLite-backed snapshot. Statuses are `pending`, `running`, `waiting_for_decision`, `completed`, `failed`, and `interrupted`.
- `GET /api/v1/tasks/{taskId}/conflicts` returns task repositories that require or are applying a conflict decision.
- `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision` accepts `backup`, `overwrite`, or `abort` and returns `202`.
- `GET /api/v1/tasks/{taskId}/stream` sends an SSE `snapshot`, followed by `started`, `progress`, `conflict`, `waiting_for_decision`, `decision_applied`, `completed`, or `failed` events.

When SSE disconnects, the UI first reads the GET snapshot and conflicts, then reconnects. A disconnect is not treated as task failure. The stream closes after completion or failure, while the final state remains available through GET. After a server restart, `waiting_for_decision` remains recoverable and an in-flight destructive action is not replayed automatically.
