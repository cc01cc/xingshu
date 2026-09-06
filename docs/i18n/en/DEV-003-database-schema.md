# Database Schema

Database files follow `xingshu-<env>.db` and use WAL: `.staging/dev-001/xingshu-dev.db` by default, `./xingshu-prod.db` with `XINGSHU_ENV=prod` (explicit `--db` / `XINGSHU_DB` win). The current schema ledger version is `4`. Every table has `created_at` and `updated_at`, maintained by core write operations.

The tables are `roots`, `repos`, `tags`, `repo_tags`, `policies`, `fetch_log`, `tasks`, and `task_repos`. Tags are Xingshu-owned domain labels. Repository policy takes precedence over tag policy. Third-party and frozen repositories default to `modify_lock=1`. Remote URL userinfo must be removed before persistence. The server marks `pending` and `running` tasks as `interrupted` at startup; `waiting_decision` task repositories remain recoverable, while `resolving` repositories are interrupted without automatic destructive-action replay.

Schema changes require an explicit migration ledger and forward migration tests against the production schema. Tests must not delete the database to hide migration failures.
