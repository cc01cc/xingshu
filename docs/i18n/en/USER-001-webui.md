# WebUI

The Axum server on `12681` serves the built UI. During development, Vite runs on `12680` and proxies `/api` and `/health` to `12681`.

The UI provides a searchable repository table, Xingshu domain-tag management, a repository detail drawer, a disk summary, and conflict decisions. The three conflict actions are backup-and-pull, overwrite-local-state, and abort. Xingshu tags never create or modify Git tags.
