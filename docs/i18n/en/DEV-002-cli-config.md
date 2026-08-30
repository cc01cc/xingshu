# CLI and Configuration

Install WebUI dependencies from `webui/` with `pnpm --ignore-workspace install`; a normal install can discover the parent workspace. Vite uses port `12680`; the Axum API and built UI use `12681`.

The CLI provides `roots`, `scan`, `list`, `tag`, `untag`, `kind`, `policy`, `pull`, `move`, and `stats`. Xingshu tags are domain labels owned by Xingshu, not Git tags. Repository policy overrides tag policy.

Interactive pull conflicts default to `stop`; unattended conflicts default to `abort`. `scan_nested_repos` is disabled by default and `scan_skip_dirs` prunes large non-repository trees.
