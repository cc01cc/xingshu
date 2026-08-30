PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_meta (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS roots (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    disk_label TEXT,
    mount_point TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repos (
    id INTEGER PRIMARY KEY,
    root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
    org TEXT NOT NULL,
    name TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    remote_url TEXT,
    default_branch TEXT,
    head_commit TEXT,
    last_fetch_at TEXT,
    last_pull_status TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    is_bare INTEGER NOT NULL DEFAULT 0,
    clone_status TEXT NOT NULL DEFAULT 'ok',
    repo_kind TEXT NOT NULL DEFAULT 'third-party',
    modify_lock INTEGER NOT NULL DEFAULT 1,
    lock_violation INTEGER NOT NULL DEFAULT 0,
    push_protect_remote TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(root_id, rel_path)
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    color TEXT,
    kind TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repo_tags (
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (repo_id, tag_id)
);

CREATE TABLE IF NOT EXISTS policies (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE,
    tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    pull_strategy TEXT NOT NULL DEFAULT 'fetch-only',
    fetch_schedule TEXT,
    depth TEXT NOT NULL DEFAULT 'full',
    auto_tag INTEGER NOT NULL DEFAULT 0,
    pull_conflict_policy TEXT NOT NULL DEFAULT 'stop',
    unattended_conflict_policy TEXT NOT NULL DEFAULT 'abort',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((repo_id IS NOT NULL) OR (tag_id IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS fetch_log (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    strategy TEXT NOT NULL,
    result TEXT NOT NULL,
    objects INTEGER,
    bytes INTEGER,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    conflict_mode TEXT NOT NULL DEFAULT 'policy',
    request_id TEXT,
    operation_id TEXT NOT NULL,
    progress_current INTEGER,
    progress_total INTEGER,
    last_repo TEXT,
    last_result TEXT,
    error TEXT,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_repos (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    conflict_reason TEXT,
    requested_action TEXT,
    result TEXT,
    duration_ms INTEGER,
    backup_path TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(task_id, repo_id)
);

CREATE INDEX IF NOT EXISTS idx_repos_root ON repos(root_id);
CREATE INDEX IF NOT EXISTS idx_repos_kind ON repos(repo_kind);
CREATE INDEX IF NOT EXISTS idx_repos_status ON repos(clone_status, lock_violation);
CREATE INDEX IF NOT EXISTS idx_fetch_log_repo ON fetch_log(repo_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_task_repos_task_status ON task_repos(task_id, status);
