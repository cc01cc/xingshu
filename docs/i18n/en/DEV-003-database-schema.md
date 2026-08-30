# Database Schema

The default SQLite database uses WAL. Every table has `created_at` and `updated_at`, maintained by core write operations.

The tables are `roots`, `repos`, `tags`, `repo_tags`, `policies`, and `fetch_log`. Tags are Xingshu-owned domain labels. Repository policy takes precedence over tag policy. Third-party and frozen repositories default to `modify_lock=1`. Remote URL userinfo must be removed before persistence.

Schema changes require an explicit migration ledger and forward migration tests against the production schema. Tests must not delete the database to hide migration failures.
