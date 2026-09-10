# CLI and Configuration

Prefer `mise run setup` (`mise.toml`); installing WebUI deps separately from `webui/` still requires `pnpm --ignore-workspace install` because the project sits inside a parent workspace. Vite uses port `12680`; the Axum API and built UI use `12681`; one-shot start is `mise run dev`.

The CLI provides `roots`, `scan`, `list`, `tag`, `untag`, `kind`, `policy`, `pull`, `move`, and `stats`. Xingshu tags are domain labels owned by Xingshu, not Git tags. Repository policy overrides tag policy.

`scan`/`pull` print per-item progress to stderr (`[N] <repo>: <result> (<ms>ms)`); the final `scan` JSON report still goes to stdout for piping. `pull` prints `[i/total] <org>/<name>: <result> (<ms>ms)` with failures on stderr under the same counter prefix, plus a stderr total at the end.

Interactive pull conflicts default to `stop`; unattended conflicts default to `abort`. `scan_nested_repos` is disabled by default and `scan_skip_dirs` prunes large non-repository trees.

Database files follow `xingshu-<env>.db` (`xingshu-dev.db` / `xingshu-prod.db` / `xingshu-test.db`). Precedence: explicit `--db` > `XINGSHU_DB` > default (`.staging/dev-001/xingshu-dev.db`, or `./xingshu-prod.db` with `XINGSHU_ENV=prod`). Prod refuses paths under `.staging/`; the server logs the resolved database path at startup.
