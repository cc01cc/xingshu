# WebUI

The Axum server on `12681` serves the built UI. During development, Vite runs on `12680` and proxies `/api` and `/health` to `12681`.

The UI provides a searchable repository table, Xingshu domain-tag management, a repository detail drawer, a disk summary, conflict decisions, and asynchronous scan/batch-pull progress. The three conflict actions are backup-and-pull, overwrite-local-state, and abort. Xingshu tags never create or modify Git tags.

## Asynchronous tasks

The WebUI uses the task API for operations that may take a long time:

- `POST /api/v1/tasks` with `{ "type": "scan" }` or `{ "type": "pull" }` returns `202` and a `taskId`.
- `GET /api/v1/tasks/{taskId}` returns the latest SQLite-backed snapshot. Statuses are `pending`, `running`, `completed`, `failed`, and `interrupted`.
- `GET /api/v1/tasks/{taskId}/stream` sends an SSE `snapshot`, followed by `started`, `progress`, `completed`, or `failed` events.

When SSE disconnects, the UI first reads the GET snapshot and then reconnects. A disconnect is not treated as task failure. The stream closes after completion or failure, while the final state remains available through GET.
