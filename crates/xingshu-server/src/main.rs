use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;

use tokio::sync::Mutex;

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header::CONTENT_TYPE},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::services::ServeDir;
use xingshu_core::types::{
    RepoRecord, TaskEvent, TaskRecord, TaskRepoRecord, TaskRepoUpdate, VcsError,
};
use xingshu_core::{
    Database, ProgressReporter,
    policy::{PullConflictAction, default_for_kind},
    puller::{PullMode, make_fetch_log, pull_repo, pull_repo_with_reporter},
};

mod tasks;
use tasks::TaskManager;

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
    scan_lock: Arc<Mutex<()>>,
    task_manager: TaskManager,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let db_path =
        xingshu_core::config::resolve_db_path(None).map_err(|error| anyhow::anyhow!(error))?;
    xingshu_core::config::check_db_allowed(&db_path).map_err(|error| anyhow::anyhow!(error))?;
    let fresh = !db_path.exists();
    let host = std::env::var("XINGSHU_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("XINGSHU_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(12681);
    let database = Database::open(&db_path).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    database
        .mark_active_tasks_interrupted()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    database
        .mark_active_task_repos_interrupted()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let app = build_router(db_path.clone());
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(
        %address,
        db = %db_path.display(),
        env = xingshu_core::config::current_env(),
        fresh,
        "xingshu server listening"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(db_path: PathBuf) -> Router {
    let state = AppState {
        db_path: db_path.clone(),
        scan_lock: Arc::new(Mutex::new(())),
        task_manager: TaskManager::new(db_path),
    };
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/repos", get(repos))
        .route("/api/v1/repos/{repo_id}", put(update_repo_kind))
        .route("/api/v1/repos/{repo_id}/pull", post(pull))
        .route("/api/v1/repos/{repo_id}/pull/{action}", post(pull_decision))
        .route(
            "/api/v1/repos/{repo_id}/tags",
            post(attach_tag).delete(detach_tag),
        )
        .route("/api/v1/roots", get(roots).post(create_root))
        .route("/api/v1/roots/{root_id}", delete(delete_root))
        .route("/api/v1/roots/{root_id}/space", get(root_space))
        .route("/api/v1/tags", get(tags).post(create_tag))
        .route("/api/v1/tags/{tag_id}", delete(delete_tag))
        .route("/api/v1/scan", post(trigger_scan))
        .route("/api/v1/tasks", post(create_task))
        .route("/api/v1/tasks/{task_id}/conflicts", get(task_conflicts))
        .route(
            "/api/v1/tasks/{task_id}/repos/{repo_id}/decision",
            post(task_repo_decision),
        )
        .route("/api/v1/tasks/{task_id}", get(get_task))
        .route("/api/v1/tasks/{task_id}/stream", get(task_stream))
        .route("/api/v1/stats", get(stats))
        .route("/api/v1/repos/{repo_id}/fetch_log", get(fetch_logs))
        .route("/api/v1/repos/{repo_id}/move", post(move_repo_handler))
        .route("/api/v1/repos/{repo_id}/status", get(repo_status))
        .route("/api/v1/repos/{repo_id}/diff", get(repo_diff))
        .route(
            "/api/v1/repos/{repo_id}/backups",
            get(backup_list).post(backup_create),
        )
        .route(
            "/api/v1/repos/{repo_id}/backups/restore",
            post(backup_restore),
        )
        .route("/api/v1/repos/{repo_id}/backups/prune", post(backup_prune))
        .route("/api/v1/open", post(open_path))
        .route(
            "/api/v1/repos/{repo_id}/policy",
            get(get_repo_policy).put(set_repo_policy),
        )
        .route(
            "/api/v1/tags/{tag_id}/policy",
            get(get_tag_policy).put(set_tag_policy),
        )
        .fallback_service(ServeDir::new("webui/dist"))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

fn init_logging() {
    let filter = xingshu_core::logging::resolve_log_level();
    if let Some(rotation) = xingshu_core::logging::resolve_rotation() {
        match xingshu_core::logging::SizeRollingFile::new(
            &rotation.path,
            rotation.max_bytes,
            rotation.max_files,
        ) {
            Ok(writer) => {
                let _ = tracing_subscriber::fmt()
                    .json()
                    .with_ansi(false)
                    .with_target(true)
                    .with_env_filter(filter)
                    .with_writer(std::sync::Mutex::new(writer))
                    .try_init();
            }
            Err(error) => eprintln!("xingshu log file disabled: {error}"),
        }
    } else {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_target(true)
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let started = Instant::now();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| xingshu_core::logging::valid_request_id(value))
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    request.extensions_mut().insert(request_id.clone());
    let mut response = next.run(request).await;
    if !response.headers().contains_key("x-request-id")
        && let Ok(value) = HeaderValue::from_str(&request_id)
    {
        response.headers_mut().insert("x-request-id", value);
    }
    tracing::info!(
        request_id = %request_id,
        duration_ms = started.elapsed().as_millis() as u64,
        "http request completed"
    );
    response
}

async fn repos(State(state): State<AppState>) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        let repos = db.list_repos()?;
        repos
            .into_iter()
            .map(|repo| {
                let tags = db.tags_for_repo(repo.id.unwrap_or_default())?;
                let mut value = serde_json::to_value(repo)
                    .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
                value["tags"] = serde_json::json!(tags);
                Ok(camelize_value(value))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
    }) {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn roots(State(state): State<AppState>) -> impl IntoResponse {
    read_json(&state.db_path, |db| db.list_roots())
}

#[derive(Debug, Deserialize)]
struct CreateRootRequest {
    path: String,
    #[serde(default)]
    disk_label: Option<String>,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
}

async fn create_root(
    State(state): State<AppState>,
    Json(request): Json<CreateRootRequest>,
) -> impl IntoResponse {
    let root_path = std::path::PathBuf::from(&request.path);
    if !root_path.is_dir() {
        return problem_response(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            &format!("path is not a directory: {}", request.path),
        );
    }
    let name = root_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let root = xingshu_core::types::Root {
        id: None,
        name,
        path: root_path,
        disk_label: request.disk_label,
        mount_point: request.mount_point,
        priority: request.priority.unwrap_or(0),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    match Database::open(&state.db_path).and_then(|db| db.upsert_root(&root)) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(json!({ "id": id, "path": root.path.to_string_lossy() })),
        )
            .into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn delete_root(State(state): State<AppState>, Path(root_id): Path<i64>) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        let found = db.find_root_by_id(root_id)?;
        if found.is_none() {
            return Err(xingshu_core::types::VcsError::Database(
                "root not found".to_owned(),
            ));
        }
        db.remove_root(root_id)?;
        Ok(())
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn root_space(State(state): State<AppState>, Path(root_id): Path<i64>) -> impl IntoResponse {
    let result = (|| -> anyhow::Result<serde_json::Value> {
        let db = Database::open(&state.db_path)?;
        let root = db
            .find_root_by_id(root_id)?
            .ok_or_else(|| anyhow::anyhow!("root not found"))?;
        let available = xingshu_core::mover::available_space(&root.path)
            .map_err(|error| anyhow::anyhow!("space_probe_failed: {error}"))?;
        Ok(json!({ "rootId": root_id, "availableBytes": available }))
    })();
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => {
            let detail = error.to_string();
            if detail.contains("space_probe_failed") {
                return problem_response(StatusCode::BAD_GATEWAY, "space_probe_failed", &detail);
            }
            response_for_anyhow(&error)
        }
    }
}

async fn tags(State(state): State<AppState>) -> impl IntoResponse {
    read_json(&state.db_path, |db| db.list_tags())
}

#[derive(Debug, Deserialize)]
struct CreateTagRequest {
    slug: String,
    label: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AttachTagRequest {
    slug: String,
}

async fn create_tag(
    State(state): State<AppState>,
    Json(request): Json<CreateTagRequest>,
) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        db.create_tag(
            &request.slug,
            request.label.as_deref().unwrap_or(&request.slug),
            request.color.as_deref(),
        )
    }) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(json!({ "id": id, "slug": request.slug })),
        )
            .into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn attach_tag(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<AttachTagRequest>,
) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        let tag_id = db
            .tag_id(&request.slug)?
            .ok_or_else(|| xingshu_core::types::VcsError::Database("tag not found".to_owned()))?;
        db.attach_tag(repo_id, tag_id)
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn detach_tag(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<AttachTagRequest>,
) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        let tag_id = db
            .tag_id(&request.slug)?
            .ok_or_else(|| xingshu_core::types::VcsError::Database("tag not found".to_owned()))?;
        db.detach_tag(repo_id, tag_id)
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn pull(State(state): State<AppState>, Path(repo_id): Path<i64>) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let repo = db
            .find_repo_by_id(repo_id)?
            .ok_or_else(|| anyhow::anyhow!("repository not found"))?;
        let roots = db.list_roots()?;
        let root = roots
            .iter()
            .find(|root| root.id == Some(repo.root_id))
            .ok_or_else(|| anyhow::anyhow!("root not found"))?;
        let policy = default_for_kind(&repo.repo_kind);
        let outcome = pull_repo(
            &root.path.join(&repo.rel_path),
            &repo,
            &policy,
            PullMode::Interactive,
        )
        .map_err(anyhow::Error::new)?;
        let log = make_fetch_log(repo_id, &policy.pull_strategy, &outcome);
        db.record_fetch_log(&log)?;
        db.update_pull_status(repo_id, &outcome.result)?;
        if outcome.result == "ok" {
            let _ = db.set_lock_violation(repo_id, false);
        }
        Ok::<_, anyhow::Error>(outcome)
    })();
    match result {
        Ok(outcome) => (
            StatusCode::OK,
            Json(json!({ "result": outcome.result, "backupPath": outcome.backup_path })),
        )
            .into_response(),
        Err(error) if error.to_string().contains("requires a user decision") => problem_response(
            StatusCode::CONFLICT,
            "conflict_needs_decision",
            &error.to_string(),
        ),
        Err(error) => response_for_anyhow(&error),
    }
}

async fn pull_decision(
    State(state): State<AppState>,
    Path((repo_id, action)): Path<(i64, String)>,
) -> impl IntoResponse {
    let action = match PullConflictAction::try_from(action.as_str()) {
        Ok(PullConflictAction::Stop) | Err(_) => {
            return problem_response(
                StatusCode::BAD_REQUEST,
                "invalid_conflict_action",
                "action must be backup, overwrite, or skip",
            );
        }
        Ok(action) => action,
    };
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let repo = db
            .find_repo_by_id(repo_id)?
            .ok_or_else(|| anyhow::anyhow!("repository not found"))?;
        let roots = db.list_roots()?;
        let root = roots
            .iter()
            .find(|root| root.id == Some(repo.root_id))
            .ok_or_else(|| anyhow::anyhow!("root not found"))?;
        let mut policy = default_for_kind(&repo.repo_kind);
        policy.conflict_action = action;
        policy.unattended_action = action;
        let outcome = pull_repo(
            &root.path.join(&repo.rel_path),
            &repo,
            &policy,
            PullMode::Unattended,
        )
        .map_err(anyhow::Error::new)?;
        let log = make_fetch_log(repo_id, &policy.pull_strategy, &outcome);
        db.record_fetch_log(&log)?;
        db.update_pull_status(repo_id, &outcome.result)?;
        if outcome.result == "ok" {
            let _ = db.set_lock_violation(repo_id, false);
        }
        Ok::<_, anyhow::Error>(outcome)
    })();
    match result {
        Ok(outcome) => (
            StatusCode::OK,
            Json(json!({ "result": outcome.result, "backupPath": outcome.backup_path })),
        )
            .into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

/// Resolve the on-disk directory of a repository (PLAN-0314).
fn repo_dir(db: &Database, repo_id: i64) -> anyhow::Result<PathBuf> {
    let repo = db
        .find_repo_by_id(repo_id)?
        .ok_or_else(|| anyhow::anyhow!("repository not found"))?;
    let roots = db.list_roots()?;
    let root = roots
        .iter()
        .find(|root| root.id == Some(repo.root_id))
        .ok_or_else(|| anyhow::anyhow!("root not found"))?;
    Ok(root.path.join(&repo.rel_path))
}

/// Remote drift state for one repository, via read-only `git ls-remote`.
fn remote_drift(path: &std::path::Path, default_branch: Option<&str>) -> serde_json::Value {
    match xingshu_core::git::remote_refs(path, "origin") {
        Ok(refs) => {
            let Some(branch) = default_branch else {
                return json!({
                    "state": "unknown",
                    "detail": format!("{} remote branches (unknown default branch)", refs.len())
                });
            };
            let Some((_, remote_sha)) = refs
                .iter()
                .find(|(name, _)| name == &format!("refs/heads/{branch}"))
            else {
                return json!({
                    "state": "unknown",
                    "detail": format!("{} remote branches (default branch not advertised)", refs.len())
                });
            };
            let local = xingshu_core::git::run_git(
                path,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/remotes/origin/{branch}"),
                ],
            )
            .ok()
            .map(|output| output.stdout.trim().to_owned());
            match local {
                Some(local_sha) if &local_sha == remote_sha => {
                    json!({ "state": "up-to-date", "branch": branch })
                }
                Some(_) => json!({
                    "state": "changed",
                    "branch": branch,
                    "detail": "remote moved; fetch/pull needed"
                }),
                None => json!({
                    "state": "unknown",
                    "detail": "no local tracking ref; needs fetch"
                }),
            }
        }
        Err(error) => json!({ "state": "unknown", "detail": error.to_string() }),
    }
}

async fn repo_status(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Query(query): Query<StatusQuery>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let repo = db
            .find_repo_by_id(repo_id)?
            .ok_or_else(|| anyhow::anyhow!("repository not found"))?;
        let remote = if query.remote.unwrap_or(false) {
            let path = repo_dir(&db, repo_id)?;
            Some(remote_drift(&path, repo.default_branch.as_deref()))
        } else {
            None
        };
        Ok(json!({
            "dirty": repo.dirty,
            "ahead": repo.ahead,
            "behind": repo.behind,
            "statusCheckedAt": repo.status_checked_at.map(|time| time.to_rfc3339()),
            "remote": remote,
        }))
    })();
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

#[derive(Debug, Deserialize)]
struct StatusQuery {
    #[serde(rename = "remote")]
    remote: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DiffQuery {
    path: String,
}

async fn repo_diff(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Query(query): Query<DiffQuery>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        db.find_repo_by_id(repo_id)?
            .ok_or_else(|| anyhow::anyhow!("repository not found"))?;
        let dir = repo_dir(&db, repo_id)?;
        if xingshu_core::git::is_bare_layout(&dir) {
            anyhow::bail!("invalid repository: bare layout has no worktree diff");
        }
        let relative = sanitize_repo_relative(&dir, &query.path)?;
        let diff = xingshu_core::git::diff_text(&dir, &relative)?;
        let kind = match diff.kind {
            xingshu_core::git::DiffKind::Diff => "diff",
            xingshu_core::git::DiffKind::Binary => "binary",
            xingshu_core::git::DiffKind::Untracked => "untracked",
            xingshu_core::git::DiffKind::NoHead => "no_head",
        };
        Ok(json!({
            "path": relative,
            "kind": kind,
            "patch": diff.patch,
            "truncated": diff.truncated,
        }))
    })();
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

fn sanitize_repo_relative(dir: &std::path::Path, raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("invalid path: empty");
    }
    let candidate = std::path::Path::new(trimmed);
    if candidate.is_absolute() {
        anyhow::bail!("invalid path: absolute paths are rejected");
    }
    let mut relative = String::new();
    for component in candidate.components() {
        match component {
            std::path::Component::Normal(part) => {
                if !relative.is_empty() {
                    relative.push('/');
                }
                relative.push_str(&part.to_string_lossy());
            }
            std::path::Component::CurDir => {}
            _ => anyhow::bail!("invalid path: {trimmed}"),
        }
    }
    if relative.is_empty() {
        anyhow::bail!("invalid path: {trimmed}");
    }
    let base = dir
        .canonicalize()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let target = dir.join(&relative);
    let anchor = match target.canonicalize() {
        Ok(path) => path,
        Err(_) => target
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| dir.to_path_buf())
            .canonicalize()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    };
    if !path_inside(&base, &anchor) {
        anyhow::bail!("invalid path: outside repository");
    }
    Ok(relative)
}

fn path_inside(base: &std::path::Path, candidate: &std::path::Path) -> bool {
    let normalize = |path: &std::path::Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let base = normalize(base);
    let candidate = normalize(candidate);
    candidate == base || candidate.starts_with(&format!("{base}/"))
}

async fn backup_list(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Query(query): Query<BackupDestQuery>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let path = repo_dir(&db, repo_id)?;
        let dest = query.dest.as_deref().map(std::path::Path::new);
        xingshu_core::backup::list_backups(&path, dest).map_err(anyhow::Error::new)
    })()
    .and_then(|entries| {
        serde_json::to_value(entries)
            .map_err(|error| anyhow::anyhow!("serialization failed: {error}"))
    });
    match result {
        Ok(value) => (StatusCode::OK, Json(camelize_value(value))).into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

#[derive(Debug, Deserialize)]
struct BackupDestQuery {
    dest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BackupCreateRequest {
    dest: Option<String>,
    archive: Option<String>,
    keep: Option<u32>,
}

async fn backup_create(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<BackupCreateRequest>,
) -> impl IntoResponse {
    if let Some(archive) = request.archive.as_deref()
        && archive != "tar.gz"
        && archive != "tar.zst"
    {
        return problem_response(
            StatusCode::BAD_REQUEST,
            "invalid_archive_format",
            "archive must be tar.gz or tar.zst",
        );
    }
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let path = repo_dir(&db, repo_id)?;
        let dest = request.dest.as_ref().map(std::path::PathBuf::from);
        xingshu_core::backup::create_backup(
            &path,
            dest.as_deref(),
            request.archive.as_deref(),
            request.keep.unwrap_or(5) as usize,
        )
        .map_err(anyhow::Error::new)
    })();
    match result {
        Ok(created) => (
            StatusCode::CREATED,
            Json(json!({ "path": created.to_string_lossy() })),
        )
            .into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

#[derive(Debug, Deserialize)]
struct BackupRestoreRequest {
    path: String,
    #[serde(rename = "confirm")]
    confirm: Option<bool>,
}

async fn backup_restore(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<BackupRestoreRequest>,
) -> impl IntoResponse {
    if request.confirm != Some(true) {
        return problem_response(
            StatusCode::BAD_REQUEST,
            "confirm_required",
            "restore overwrites the repository; send confirm=true to proceed",
        );
    }
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let path = repo_dir(&db, repo_id)?;
        xingshu_core::backup::restore_backup(&path, std::path::Path::new(&request.path))
            .map_err(anyhow::Error::new)
    })();
    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "restored": true }))).into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

#[derive(Debug, Deserialize)]
struct BackupPruneRequest {
    keep: u32,
    #[serde(rename = "dryRun")]
    dry_run: Option<bool>,
    dest: Option<String>,
}

async fn backup_prune(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<BackupPruneRequest>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let path = repo_dir(&db, repo_id)?;
        let dest = request.dest.as_ref().map(std::path::PathBuf::from);
        let execute = !request.dry_run.unwrap_or(true);
        xingshu_core::backup::prune_backups(&path, dest.as_deref(), request.keep as usize, execute)
            .map_err(anyhow::Error::new)
    })();
    let executed = !request.dry_run.unwrap_or(true);
    match result {
        Ok(affected) => (
            StatusCode::OK,
            Json(json!({
            "executed": executed,
                "affected": affected.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        Err(error) => response_for_anyhow(&error),
    }
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    #[serde(rename = "type")]
    task_type: String,
    #[serde(rename = "conflictMode", default)]
    conflict_mode: Option<String>,
}

async fn create_task(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Json(request): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let conflict_mode = request
        .conflict_mode
        .as_deref()
        .unwrap_or("policy")
        .to_owned();
    let task = match state
        .task_manager
        .create(&request.task_type, Some(request_id), &conflict_mode)
    {
        Ok(task) => task,
        Err(error) if error.to_string().contains("already running") => {
            return problem_response(StatusCode::CONFLICT, "task_busy", &error.to_string());
        }
        Err(error) if error.to_string().contains("unsupported task type") => {
            return problem_response(
                StatusCode::BAD_REQUEST,
                "invalid_task_type",
                &error.to_string(),
            );
        }
        Err(error) if error.to_string().contains("unsupported conflict mode") => {
            return problem_response(
                StatusCode::BAD_REQUEST,
                "invalid_conflict_mode",
                &error.to_string(),
            );
        }
        Err(error) => return error_response(error.to_string()),
    };
    let task_id = task.id.clone();
    let task_type = task.task_type.clone();
    let task_conflict_mode = task.conflict_mode.clone();
    let manager = state.task_manager.clone();
    let db_path = state.db_path.clone();
    tokio::spawn(async move {
        if let Err(error) = manager.start(&task_id) {
            let _ = manager.finish(&task_id, "failed", None, Some(error.to_string()));
            return;
        }
        let worker_manager = manager.clone();
        let worker_id = task_id.clone();
        let worker_conflict_mode = task_conflict_mode.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_task(
                &worker_manager,
                &worker_id,
                &task_type,
                &worker_conflict_mode,
                &db_path,
            )
        })
        .await;
        match result {
            Ok(Ok(TaskRunResult::Completed(value))) => {
                let _ = manager.finish(&task_id, "completed", Some(value.to_string()), None);
            }
            Ok(Ok(TaskRunResult::WaitingForDecision)) => {
                tracing::info!(task_id = %task_id, "task is waiting for repository decisions");
            }
            Ok(Err(error)) => {
                let _ = manager.finish(&task_id, "failed", None, Some(error.to_string()));
            }
            Err(error) => {
                let _ = manager.finish(
                    &task_id,
                    "failed",
                    None,
                    Some(format!("task worker join failed: {error}")),
                );
            }
        }
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "taskId": task.id,
            "operationId": task.operation_id,
            "status": task.status,
            "conflictMode": task.conflict_mode,
        })),
    )
        .into_response()
}

#[derive(Debug)]
enum TaskRunResult {
    Completed(serde_json::Value),
    WaitingForDecision,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskConflictResponse {
    task_id: String,
    repo_id: i64,
    repo_name: String,
    repo_path: String,
    status: String,
    conflict_reason: Option<String>,
    conflict: Option<xingshu_core::types::ConflictInfo>,
    allowed_actions: Vec<&'static str>,
    requested_action: Option<String>,
    result: Option<String>,
    duration_ms: Option<u64>,
    backup_path: Option<String>,
    error: Option<String>,
}

async fn task_conflicts(State(state): State<AppState>, Path(task_id): Path<String>) -> Response {
    match state.task_manager.get(&task_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return problem_response(StatusCode::NOT_FOUND, "task_not_found", "task not found");
        }
        Err(error) => return error_response(error.to_string()),
    }
    let result = (|| {
        let database = Database::open(&state.db_path)?;
        state
            .task_manager
            .list_task_repos(&task_id)?
            .into_iter()
            .filter(|record| {
                matches!(
                    record.status.as_str(),
                    "waiting_decision" | "resolving" | "failed" | "interrupted"
                ) && record.conflict_reason.is_some()
            })
            .map(|record| {
                let repo = database
                    .find_repo_by_id(record.repo_id)?
                    .ok_or_else(|| VcsError::Database("repository not found".to_owned()))?;
                Ok(TaskConflictResponse {
                    task_id: record.task_id,
                    repo_id: record.repo_id,
                    repo_name: format!("{}/{}", repo.org, repo.name),
                    repo_path: repo.rel_path.to_string_lossy().to_string(),
                    status: record.status,
                    conflict_reason: record.conflict_reason,
                    conflict: record
                        .conflict_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok()),
                    allowed_actions: vec!["backup", "overwrite", "skip"],
                    requested_action: record.requested_action,
                    result: record.result,
                    duration_ms: record.duration_ms,
                    backup_path: record.backup_path,
                    error: record.error,
                })
            })
            .collect::<Result<Vec<_>, VcsError>>()
    })();
    match result {
        Ok(conflicts) => (StatusCode::OK, Json(conflicts)).into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct TaskRepoDecisionRequest {
    action: String,
}

async fn task_repo_decision(
    State(state): State<AppState>,
    Path((task_id, repo_id)): Path<(String, i64)>,
    Json(request): Json<TaskRepoDecisionRequest>,
) -> Response {
    let action = match PullConflictAction::try_from(request.action.as_str()) {
        Ok(PullConflictAction::Stop) | Err(_) => {
            return problem_response(
                StatusCode::BAD_REQUEST,
                "invalid_conflict_action",
                "action must be backup, overwrite, or skip",
            );
        }
        Ok(action) => action,
    };
    let action_name = request.action;
    let record = match state
        .task_manager
        .claim_task_repo_decision(&task_id, repo_id, &action_name)
    {
        Ok(record) => record,
        Err(error) if error.to_string() == "task repo not found" => {
            return problem_response(
                StatusCode::NOT_FOUND,
                "task_repo_not_found",
                &error.to_string(),
            );
        }
        Err(error)
            if error.to_string().contains("not waiting for a decision")
                || error.to_string().contains("already resolving") =>
        {
            return problem_response(
                StatusCode::CONFLICT,
                "task_decision_conflict",
                &error.to_string(),
            );
        }
        Err(error) => return error_response(error.to_string()),
    };

    let manager = state.task_manager.clone();
    let db_path = state.db_path.clone();
    let worker_task_id = task_id.clone();
    let worker_action_name = action_name.clone();
    tokio::spawn(async move {
        let worker_manager = manager.clone();
        let task_id_for_worker = worker_task_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            resolve_task_repo_decision(
                &worker_manager,
                &task_id_for_worker,
                repo_id,
                action,
                &db_path,
            )
        })
        .await;
        if let Err(error) = match result {
            Ok(result) => result,
            Err(error) => Err(VcsError::Database(format!(
                "task worker join failed: {error}"
            ))),
        } {
            let update = TaskRepoUpdate {
                status: "failed".to_owned(),
                conflict_reason: record.conflict_reason.clone(),
                conflict: None,
                requested_action: Some(worker_action_name.clone()),
                result: Some("failed".to_owned()),
                duration_ms: None,
                backup_path: record.backup_path.clone(),
                error: Some(error.to_string()),
            };
            let _ = manager.save_task_repo(&worker_task_id, repo_id, &update, "decision_failed");
            let _ = manager.reconcile_task(&worker_task_id, None);
            tracing::error!(
                task_id = %worker_task_id,
                repo_id,
                error = %error,
                "task conflict decision failed"
            );
        }
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "taskId": task_id,
            "repoId": repo_id,
            "status": "resolving",
            "action": action_name,
        })),
    )
        .into_response()
}

fn resolve_task_repo_decision(
    manager: &TaskManager,
    task_id: &str,
    repo_id: i64,
    action: PullConflictAction,
    db_path: &std::path::Path,
) -> Result<(), VcsError> {
    let database = Database::open(db_path)?;
    let task_repo = database
        .find_task_repo(task_id, repo_id)?
        .ok_or_else(|| VcsError::Database("task repo not found".to_owned()))?;
    let repo = database
        .find_repo_by_id(repo_id)?
        .ok_or_else(|| VcsError::Database("repository not found".to_owned()))?;
    let root = database
        .list_roots()?
        .into_iter()
        .find(|root| root.id == Some(repo.root_id))
        .ok_or_else(|| VcsError::Database("root not found".to_owned()))?;
    let policy = resolve_repo_policy(&database, &repo)?;
    let path = root.path.join(&repo.rel_path);
    let started = Instant::now();
    let action_result = xingshu_core::puller::resolve_pull_conflict(&path, &repo, action);
    let duration_ms = started.elapsed().as_millis() as u64;
    match action_result {
        Ok(outcome) => {
            let status = if outcome.result == "skipped" {
                "skipped"
            } else {
                "completed"
            };
            let backup_path = outcome
                .backup_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string());
            let log = make_fetch_log(repo_id, &policy.pull_strategy, &outcome);
            database.record_fetch_log(&log)?;
            database.update_pull_status(repo_id, &outcome.result)?;
            if outcome.result == "ok" {
                let _ = database.set_lock_violation(repo_id, false);
            }
            let update = TaskRepoUpdate {
                status: status.to_owned(),
                conflict_reason: task_repo.conflict_reason,
                conflict: task_repo
                    .conflict_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
                requested_action: task_repo.requested_action,
                result: Some(outcome.result),
                duration_ms: Some(duration_ms),
                backup_path,
                error: None,
            };
            manager.save_task_repo(task_id, repo_id, &update, "decision_applied")?;
        }
        Err(error) => {
            let conflict = match &error {
                VcsError::ConflictNeedsDecision(info) => info.as_deref().cloned(),
                _ => None,
            };
            if matches!(action, PullConflictAction::Backup) {
                let log = xingshu_core::puller::make_failed_fetch_log(
                    repo_id,
                    &policy.pull_strategy,
                    "backup_failed",
                    &error.to_string(),
                );
                let _ = database.record_fetch_log(&log);
                let _ = database.update_pull_status(repo_id, "backup_failed");
            }
            let update = TaskRepoUpdate {
                status: "failed".to_owned(),
                conflict_reason: task_repo.conflict_reason,
                conflict,
                requested_action: task_repo.requested_action,
                result: Some("failed".to_owned()),
                duration_ms: Some(duration_ms),
                backup_path: task_repo.backup_path,
                error: Some(error.to_string()),
            };
            manager.save_task_repo(task_id, repo_id, &update, "decision_failed")?;
        }
    }
    let summary = task_summary(&manager.list_task_repos(task_id)?);
    manager.reconcile_task(task_id, Some(summary.to_string()))
}

fn task_summary(records: &[TaskRepoRecord]) -> serde_json::Value {
    let mut counts = std::collections::HashMap::<String, u64>::new();
    let repos = records
        .iter()
        .map(|record| {
            let result = record
                .result
                .clone()
                .unwrap_or_else(|| record.status.clone());
            *counts.entry(result.clone()).or_default() += 1;
            json!({
                "repoId": record.repo_id,
                "status": record.status,
                "result": result,
                "durationMs": record.duration_ms,
                "backupPath": record.backup_path,
                "error": record.error,
            })
        })
        .collect::<Vec<_>>();
    json!({ "counts": counts, "repos": repos })
}

async fn get_task(State(state): State<AppState>, Path(task_id): Path<String>) -> Response {
    match state.task_manager.get(&task_id) {
        Ok(Some(task)) => match task_to_api_value(task) {
            Ok(value) => (StatusCode::OK, Json(value)).into_response(),
            Err(error) => error_response(format!("serialization failed: {error}")),
        },
        Ok(None) => problem_response(StatusCode::NOT_FOUND, "task_not_found", "task not found"),
        Err(error) => error_response(error.to_string()),
    }
}

async fn task_stream(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let last_event_id = parse_last_event_id(&headers);
    let (task, receiver) = match state.task_manager.subscribe(&task_id) {
        Ok(value) => value,
        Err(error) if error.to_string().contains("task not found") => {
            return problem_response(StatusCode::NOT_FOUND, "task_not_found", &error.to_string());
        }
        Err(error) => return error_response(error.to_string()),
    };
    let snapshot = match task_to_api_value(task) {
        Ok(value) => value,
        Err(error) => return error_response(format!("serialization failed: {error}")),
    };
    let initial = match Event::default().event("snapshot").json_data(snapshot) {
        Ok(event) => event,
        Err(error) => return error_response(format!("SSE serialization failed: {error}")),
    };
    let initial_stream = tokio_stream::once(Ok::<Event, Infallible>(initial));
    let event_stream = BroadcastStream::new(receiver).filter_map(move |event| match event {
        Ok(event) if event_visible_after(last_event_id, event.sequence) => {
            Some(Ok::<Event, Infallible>(task_event_to_sse(event)))
        }
        _ => None,
    });
    let stream = initial_stream.chain(event_stream);
    let mut response = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response();
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-cache"));
    response
}

/// Parse the SSE `Last-Event-ID` resume header.
///
/// Returns the last sequence the client has already seen, or `0` when the
/// header is absent or unparsable (full replay: snapshot + all live events).
/// The snapshot is always sent first; this id only suppresses already-seen
/// in-memory broadcast events and never replaces the persisted snapshot.
fn parse_last_event_id(headers: &HeaderMap) -> u64 {
    headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// True when a broadcast event should be replayed for a resuming client.
fn event_visible_after(last_event_id: u64, sequence: u64) -> bool {
    sequence > last_event_id
}

fn task_to_api_value(task: TaskRecord) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = camelize_value(serde_json::to_value(task)?);
    if let serde_json::Value::Object(fields) = &mut value {
        if let Some(id) = fields.remove("id") {
            fields.insert("taskId".to_owned(), id);
        }
        if let Some(task_type) = fields.remove("taskType") {
            fields.insert("type".to_owned(), task_type);
        }
    }
    Ok(value)
}

fn task_event_to_sse(event: TaskEvent) -> Event {
    let data = match serde_json::to_value(&event) {
        Ok(value) => camelize_value(value),
        Err(_) => json!({ "taskId": event.task_id, "event": "failed" }),
    };
    match Event::default()
        .id(event.sequence.to_string())
        .event(event.event)
        .json_data(data)
    {
        Ok(value) => value,
        Err(_) => Event::default().event("failed").data("{}"),
    }
}

struct RepoPullResult {
    repo_id: i64,
    repo_name: String,
    status: String,
    result: String,
    conflict_reason: Option<String>,
    conflict: Option<xingshu_core::types::ConflictInfo>,
    duration_ms: Option<u64>,
    backup_path: Option<String>,
    error: Option<String>,
}

fn run_task(
    manager: &TaskManager,
    task_id: &str,
    task_type: &str,
    conflict_mode: &str,
    db_path: &std::path::Path,
) -> Result<TaskRunResult, VcsError> {
    let database = Database::open(db_path)?;
    let reporter = manager.progress_reporter(task_id.to_owned());
    match task_type {
        "scan" => {
            let roots = database.list_roots()?;
            let report = xingshu_core::scanner::scan_roots_with_reporter(
                &database,
                &roots,
                &xingshu_core::types::ScanOptions::default(),
                Some(&reporter),
            )?;
            serde_json::to_value(report)
                .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))
                .map(TaskRunResult::Completed)
        }
        "pull" => {
            let roots = database.list_roots()?;
            let repos = database.list_repos()?;
            let repo_ids = repos.iter().filter_map(|repo| repo.id).collect::<Vec<_>>();
            manager.prepare_task_repos(task_id, &repo_ids)?;
            let total = repos.len() as u64;
            reporter.started(Some(total));
            let mut counts = std::collections::HashMap::<String, u64>::new();
            let mut repo_results: Vec<serde_json::Value> = Vec::new();
            let concurrency = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8);
            for chunk in repos.chunks(concurrency) {
                for repo in chunk {
                    let repo_id = repo
                        .id
                        .ok_or_else(|| VcsError::Database("repository has no id".to_owned()))?;
                    manager.claim_task_repo_running(task_id, repo_id)?;
                }
                let chunk_results = std::thread::scope(|scope| {
                    let handles = chunk.iter().map(|repo| {
                        let roots = roots.clone();
                        let database = database.clone();
                        scope.spawn(move || {
                            let repo_id = repo.id.unwrap_or_default();
                            let started = Instant::now();
                            let result = (|| -> Result<RepoPullResult, VcsError> {
                                let root = roots
                                    .iter()
                                    .find(|r| r.id == Some(repo.root_id))
                                    .ok_or_else(|| {
                                        VcsError::Database("root not found".to_owned())
                                    })?;
                                let mut policy = resolve_repo_policy(&database, repo)?;
                                if conflict_mode == "ask" {
                                    policy.unattended_action = PullConflictAction::Stop;
                                }
                                let path = root.path.join(&repo.rel_path);
                                match pull_repo_with_reporter(
                                    &path,
                                    repo,
                                    &policy,
                                    PullMode::Unattended,
                                    None,
                                ) {
                                    Ok(outcome) => {
                                        let log = make_fetch_log(
                                            repo_id,
                                            &policy.pull_strategy,
                                            &outcome,
                                        );
                                        database.record_fetch_log(&log)?;
                                        database.update_pull_status(repo_id, &outcome.result)?;
                                        Ok(RepoPullResult {
                                            repo_id,
                                            repo_name: format!("{}/{}", repo.org, repo.name),
                                            status: if outcome.result == "skipped" {
                                                "skipped".to_owned()
                                            } else {
                                                "completed".to_owned()
                                            },
                                            result: outcome.result,
                                            conflict_reason: None,
                                            conflict: outcome.conflict,
                                            duration_ms: Some(started.elapsed().as_millis() as u64),
                                            backup_path: outcome
                                                .backup_path
                                                .map(|path| path.to_string_lossy().to_string()),
                                            error: None,
                                        })
                                    }
                                    Err(error)
                                        if matches!(error, VcsError::ConflictNeedsDecision(_)) =>
                                    {
                                        let conflict = match &error {
                                            VcsError::ConflictNeedsDecision(info) => {
                                                info.as_deref().cloned()
                                            }
                                            _ => None,
                                        };
                                        Ok(RepoPullResult {
                                            repo_id,
                                            repo_name: format!("{}/{}", repo.org, repo.name),
                                            status: "waiting_decision".to_owned(),
                                            result: "conflict".to_owned(),
                                            conflict_reason: Some(error.to_string()),
                                            conflict,
                                            duration_ms: Some(started.elapsed().as_millis() as u64),
                                            backup_path: None,
                                            error: None,
                                        })
                                    }
                                    Err(error) => Ok(RepoPullResult {
                                        repo_id,
                                        repo_name: format!("{}/{}", repo.org, repo.name),
                                        status: "failed".to_owned(),
                                        result: "failed".to_owned(),
                                        conflict_reason: None,
                                        conflict: None,
                                        duration_ms: Some(started.elapsed().as_millis() as u64),
                                        backup_path: None,
                                        error: Some(error.to_string()),
                                    }),
                                }
                            })();
                            result.unwrap_or_else(|error| RepoPullResult {
                                repo_id,
                                repo_name: format!("{}/{}", repo.org, repo.name),
                                status: "failed".to_owned(),
                                result: "failed".to_owned(),
                                conflict_reason: None,
                                conflict: None,
                                duration_ms: Some(started.elapsed().as_millis() as u64),
                                backup_path: None,
                                error: Some(error.to_string()),
                            })
                        })
                    });
                    handles
                        .map(|handle| {
                            handle
                                .join()
                                .map_err(|_| VcsError::Database("pull worker panicked".to_owned()))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })?;
                for result in chunk_results {
                    let event_name = if result.status == "waiting_decision" {
                        "conflict"
                    } else {
                        "repo_completed"
                    };
                    manager.save_task_repo(
                        task_id,
                        result.repo_id,
                        &TaskRepoUpdate {
                            status: result.status.clone(),
                            conflict_reason: result.conflict_reason.clone(),
                            conflict: result.conflict.clone(),
                            requested_action: None,
                            result: Some(result.result.clone()),
                            duration_ms: result.duration_ms,
                            backup_path: result.backup_path.clone(),
                            error: result.error.clone(),
                        },
                        event_name,
                    )?;
                    reporter.item_finished_for_repo(
                        result.repo_id,
                        &result.status,
                        &result.repo_name,
                        &result.result,
                        result.duration_ms,
                    );
                    *counts.entry(result.result.clone()).or_default() += 1;
                    repo_results.push(json!({
                        "repoId": result.repo_id,
                        "repo": result.repo_name,
                        "status": result.status,
                        "result": result.result,
                        "conflictReason": result.conflict_reason,
                        "durationMs": result.duration_ms,
                        "backupPath": result.backup_path,
                        "error": result.error,
                    }));
                }
            }
            let summary = serde_json::json!({
                "counts": counts,
                "repos": repo_results,
            });
            if manager
                .list_task_repos(task_id)?
                .iter()
                .any(|repo| repo.status == "waiting_decision")
            {
                reporter.finished("waiting_for_decision");
                manager.wait_for_decision(task_id, Some(summary.to_string()))?;
                Ok(TaskRunResult::WaitingForDecision)
            } else {
                reporter.finished("completed");
                Ok(TaskRunResult::Completed(summary))
            }
        }
        other => Err(xingshu_core::types::VcsError::Database(format!(
            "unsupported task type: {other}"
        ))),
    }
}

fn resolve_repo_policy(
    database: &Database,
    repo: &RepoRecord,
) -> Result<xingshu_core::policy::ResolvedPolicy, xingshu_core::types::VcsError> {
    let stored = match database.repo_policy(repo.id.unwrap_or_default())? {
        Some(policy) => Some(policy),
        None => {
            let mut tag_policy = None;
            for tag_id in database.tag_ids_for_repo(repo.id.unwrap_or_default())? {
                if let Some(policy) = database.tag_policy(tag_id)? {
                    tag_policy = Some(policy);
                    break;
                }
            }
            tag_policy
        }
    };
    match stored {
        Some(policy) => Ok(xingshu_core::policy::ResolvedPolicy {
            pull_strategy: policy.pull_strategy,
            conflict_action: PullConflictAction::try_from(policy.pull_conflict_policy.as_str())?,
            unattended_action: PullConflictAction::try_from(
                policy.unattended_conflict_policy.as_str(),
            )?,
        }),
        None => Ok(default_for_kind(&repo.repo_kind)),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateRepoKindRequest {
    kind: String,
    #[serde(default)]
    upstream_remote: Option<String>,
}

async fn update_repo_kind(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<UpdateRepoKindRequest>,
) -> impl IntoResponse {
    let kind = match xingshu_core::types::RepoKind::try_from(request.kind.as_str()) {
        Ok(kind) => kind,
        Err(error) => {
            return problem_response(StatusCode::BAD_REQUEST, "invalid_kind", &error.to_string());
        }
    };
    match Database::open(&state.db_path).and_then(|db| {
        if db.find_repo_by_id(repo_id)?.is_none() {
            return Err(xingshu_core::types::VcsError::Database(
                "repository not found".to_owned(),
            ));
        }
        db.set_repo_kind(repo_id, &kind, request.upstream_remote.as_deref())
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn delete_tag(State(state): State<AppState>, Path(tag_id): Path<i64>) -> impl IntoResponse {
    match Database::open(&state.db_path).and_then(|db| {
        if db.find_tag_by_id(tag_id)?.is_none() {
            return Err(xingshu_core::types::VcsError::Database(
                "tag not found".to_owned(),
            ));
        }
        db.delete_tag(tag_id)
    }) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn trigger_scan(State(state): State<AppState>) -> impl IntoResponse {
    let lock = state.scan_lock.clone();
    let guard = lock.lock().await;
    let result = Database::open(&state.db_path).and_then(|db| {
        let roots = db.list_roots()?;
        let roots = roots
            .into_iter()
            .map(|root| {
                let mut r = root.clone();
                r.id = db.root_id_for_path(&root.path)?;
                Ok(r)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let options = xingshu_core::types::ScanOptions::default();
        xingshu_core::scanner::scan_roots(&db, &roots, &options)
    });
    drop(guard);
    match result {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    match Database::open(&state.db_path) {
        Ok(db) => {
            let repos = db.list_repos().unwrap_or_default();
            let bytes: u64 = repos.iter().map(|repo| repo.size_bytes).sum();
            let by_kind = db.count_by_kind().unwrap_or_default();
            let by_kind_map: std::collections::HashMap<String, i64> = by_kind.into_iter().collect();
            (
                StatusCode::OK,
                Json(json!({
                    "repositories": repos.len(),
                    "bytes": bytes,
                    "byKind": by_kind_map,
                })),
            )
                .into_response()
        }
        Err(error) => error_response(error.to_string()),
    }
}

async fn fetch_logs(State(state): State<AppState>, Path(repo_id): Path<i64>) -> impl IntoResponse {
    read_json(&state.db_path, |db| db.list_fetch_logs(repo_id, 20))
}

#[derive(Debug, Deserialize)]
struct MoveRequest {
    #[serde(rename = "targetRootId")]
    target_root_id: i64,
}

async fn move_repo_handler(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<MoveRequest>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        let repo = db.find_repo_by_id(repo_id)?.ok_or_else(|| {
            xingshu_core::types::VcsError::Database("repository not found".to_owned())
        })?;
        let roots = db.list_roots()?;
        let source_root = roots
            .iter()
            .find(|r| r.id == Some(repo.root_id))
            .ok_or_else(|| {
                xingshu_core::types::VcsError::Database("source root not found".to_owned())
            })?;
        let target_root = roots
            .iter()
            .find(|r| r.id == Some(request.target_root_id))
            .ok_or_else(|| {
                xingshu_core::types::VcsError::Database("target root not found".to_owned())
            })?;
        let source = source_root.path.join(&repo.rel_path);
        let dest = xingshu_core::mover::move_repo(
            &source_root.path,
            &source,
            &target_root.path,
            &repo.rel_path,
        )?;
        db.update_repo_location(repo_id, request.target_root_id, &repo.rel_path)?;
        Ok::<_, xingshu_core::types::VcsError>(json!({ "path": dest.to_string_lossy() }))
    })();
    match result {
        Ok(value) => (StatusCode::OK, Json(camelize_value(value))).into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct PolicyRequest {
    #[serde(alias = "pullStrategy", alias = "pull_strategy", alias = "strategy")]
    pull_strategy: Option<String>,
    #[serde(alias = "conflict")]
    conflict: Option<String>,
    #[serde(alias = "unattended")]
    unattended: Option<String>,
    #[serde(alias = "pullConflictPolicy")]
    pull_conflict_policy: Option<String>,
    #[serde(alias = "unattendedConflictPolicy")]
    unattended_conflict_policy: Option<String>,
}

async fn get_repo_policy(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
) -> impl IntoResponse {
    read_json(&state.db_path, |db| {
        let repo = db.find_repo_by_id(repo_id)?.ok_or_else(|| {
            xingshu_core::types::VcsError::Database("repository not found".to_owned())
        })?;
        let stored = db.repo_policy(repo_id)?;
        let mut source = "kind-default";
        let mut source_tag_id: Option<i64> = None;
        if stored.is_some() {
            source = "repo";
        } else {
            for tag_id in db.tag_ids_for_repo(repo_id)? {
                if db.tag_policy(tag_id)?.is_some() {
                    source = "tag";
                    source_tag_id = Some(tag_id);
                    break;
                }
            }
        }
        let effective = resolve_repo_policy(db, &repo)?;
        let base = stored.unwrap_or_else(|| {
            let def = xingshu_core::policy::default_for_kind(&repo.repo_kind);
            xingshu_core::types::Policy {
                id: None,
                repo_id: Some(repo_id),
                tag_id: None,
                pull_strategy: def.pull_strategy,
                fetch_schedule: None,
                depth: "full".to_owned(),
                auto_tag: false,
                pull_conflict_policy: xingshu_core::policy::action_value(def.conflict_action)
                    .to_owned(),
                unattended_conflict_policy: xingshu_core::policy::action_value(
                    def.unattended_action,
                )
                .to_owned(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }
        });
        let mut value = serde_json::to_value(&base).unwrap_or_default();
        value["source"] = json!(source);
        value["sourceTagId"] = json!(source_tag_id);
        value["effective"] = json!({
            "pullStrategy": effective.pull_strategy,
            "conflict": xingshu_core::policy::action_value(effective.conflict_action),
            "unattended": xingshu_core::policy::action_value(effective.unattended_action),
        });
        Ok(value)
    })
}

async fn set_repo_policy(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
    Json(request): Json<PolicyRequest>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        if db.find_repo_by_id(repo_id)?.is_none() {
            return Err(xingshu_core::types::VcsError::Database(
                "repository not found".to_owned(),
            ));
        }
        let existing = db.repo_policy(repo_id)?;
        let pull_strategy = request
            .pull_strategy
            .or(request.pull_conflict_policy.clone())
            .unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|p| p.pull_strategy.clone())
                    .unwrap_or_else(|| "fetch-only".to_owned())
            });
        let conflict = request
            .conflict
            .or(request.pull_conflict_policy.clone())
            .unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|p| p.pull_conflict_policy.clone())
                    .unwrap_or_else(|| "stop".to_owned())
            });
        let unattended = request
            .unattended
            .or(request.unattended_conflict_policy.clone())
            .unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|p| p.unattended_conflict_policy.clone())
                    .unwrap_or_else(|| "skip".to_owned())
            });
        // validation
        if !matches!(
            pull_strategy.as_str(),
            "fetch-only" | "mirror" | "no-update" | "archive"
        ) {
            return Err(xingshu_core::types::VcsError::GitCommand(format!(
                "invalid pull strategy: {pull_strategy}"
            )));
        }
        xingshu_core::policy::PullConflictAction::try_from(conflict.as_str())?;
        xingshu_core::policy::PullConflictAction::try_from(unattended.as_str())?;
        let now = chrono::Utc::now();
        let policy = xingshu_core::types::Policy {
            id: None,
            repo_id: Some(repo_id),
            tag_id: None,
            pull_strategy,
            fetch_schedule: None,
            depth: "full".to_owned(),
            auto_tag: false,
            pull_conflict_policy: conflict,
            unattended_conflict_policy: unattended,
            created_at: now,
            updated_at: now,
        };
        db.set_repo_policy(&policy)?;
        Ok::<_, xingshu_core::types::VcsError>(())
    })();
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

async fn get_tag_policy(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
) -> impl IntoResponse {
    read_json(&state.db_path, |db| db.tag_policy(tag_id))
}

async fn set_tag_policy(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    Json(request): Json<PolicyRequest>,
) -> impl IntoResponse {
    let result = (|| {
        let db = Database::open(&state.db_path)?;
        if db.find_tag_by_id(tag_id)?.is_none() {
            return Err(xingshu_core::types::VcsError::Database(
                "tag not found".to_owned(),
            ));
        }
        let existing = db.tag_policy(tag_id)?;
        let pull_strategy = request.pull_strategy.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|p| p.pull_strategy.clone())
                .unwrap_or_else(|| "fetch-only".to_owned())
        });
        let conflict = request.conflict.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|p| p.pull_conflict_policy.clone())
                .unwrap_or_else(|| "stop".to_owned())
        });
        let unattended = request.unattended.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|p| p.unattended_conflict_policy.clone())
                .unwrap_or_else(|| "skip".to_owned())
        });
        if !matches!(
            pull_strategy.as_str(),
            "fetch-only" | "mirror" | "no-update" | "archive"
        ) {
            return Err(xingshu_core::types::VcsError::GitCommand(format!(
                "invalid pull strategy: {pull_strategy}"
            )));
        }
        xingshu_core::policy::PullConflictAction::try_from(conflict.as_str())?;
        xingshu_core::policy::PullConflictAction::try_from(unattended.as_str())?;
        let now = chrono::Utc::now();
        let policy = xingshu_core::types::Policy {
            id: None,
            repo_id: None,
            tag_id: Some(tag_id),
            pull_strategy,
            fetch_schedule: None,
            depth: "full".to_owned(),
            auto_tag: false,
            pull_conflict_policy: conflict,
            unattended_conflict_policy: unattended,
            created_at: now,
            updated_at: now,
        };
        db.set_tag_policy(&policy)?;
        Ok::<_, xingshu_core::types::VcsError>(())
    })();
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct Problem {
    #[serde(rename = "type")]
    problem_type: String,
    title: String,
    status: u16,
    code: String,
    #[serde(rename = "requestId")]
    request_id: String,
    detail: String,
}

fn read_json<T: serde::Serialize>(
    path: &std::path::Path,
    operation: impl FnOnce(&Database) -> std::result::Result<T, xingshu_core::types::VcsError>,
) -> axum::response::Response {
    match Database::open(path).and_then(|db| operation(&db)) {
        Ok(value) => match serde_json::to_value(value) {
            Ok(value) => (StatusCode::OK, Json(camelize_value(value))).into_response(),
            Err(error) => error_response(format!("serialization failed: {error}")),
        },
        Err(error) => error_response(error.to_string()),
    }
}

fn camelize_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(camelize_value).collect())
        }
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (snake_to_camel(&key), camelize_value(value)))
                .collect(),
        ),
        other => other,
    }
}

fn snake_to_camel(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            result.extend(character.to_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn error_response(message: String) -> Response {
    let (status, code) = if message.contains("not found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    };
    problem_response(status, code, &message)
}

fn response_for_anyhow(error: &anyhow::Error) -> Response {
    let detail = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" <- ");
    let lower = detail.to_ascii_lowercase();
    let (status, code) = if lower.contains("not found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else if lower.contains("invalid") {
        (StatusCode::BAD_REQUEST, "invalid_request")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    };
    problem_response(status, code, &detail)
}

fn problem_response(status: StatusCode, code: &str, message: &str) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let body = Problem {
        problem_type: "about:blank".to_owned(),
        title: "Xingshu request failed".to_owned(),
        status: status.as_u16(),
        code: code.to_owned(),
        request_id: request_id.clone(),
        detail: xingshu_core::logging::redact(message),
    };
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

#[derive(Debug, Deserialize)]
struct OpenPathRequest {
    path: String,
}

async fn open_path(
    State(state): State<AppState>,
    Json(request): Json<OpenPathRequest>,
) -> Response {
    let requested = std::path::PathBuf::from(&request.path);
    let Ok(canonical) = requested.canonicalize() else {
        return problem_response(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "path does not exist",
        );
    };
    // 白名单：目标必须位于至少一个已注册 root 之下（canonical prefix match）
    let database = match Database::open(&state.db_path) {
        Ok(database) => database,
        Err(error) => return error_response(error.to_string()),
    };
    let roots = match database.list_roots() {
        Ok(roots) => roots,
        Err(error) => return error_response(error.to_string()),
    };
    let allowed = roots.iter().any(|root| {
        root.path
            .canonicalize()
            .map(|root_path| canonical.starts_with(&root_path))
            .unwrap_or(false)
    });
    if !allowed {
        return problem_response(
            StatusCode::BAD_REQUEST,
            "path_outside_root",
            "path is outside the configured roots",
        );
    }
    // `Command::arg` 原始传参（不经 shell），Windows 用 explorer 打开文件夹
    #[cfg(target_os = "windows")]
    let spawn_result = std::process::Command::new("explorer.exe")
        .arg(&canonical)
        .spawn();
    #[cfg(not(target_os = "windows"))]
    let spawn_result = std::process::Command::new("xdg-open")
        .arg(&canonical)
        .spawn();
    match spawn_result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(format!("failed to open path: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use tempfile::TempDir;
    use tower::ServiceExt;
    use xingshu_core::Database;
    use xingshu_core::scanner::scan_roots;
    use xingshu_core::types::{Root, ScanOptions};

    use super::{build_router, camelize_value, event_visible_after, parse_last_event_id};
    use xingshu_core::types::{RepoKind, RepoRecord};

    fn git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn setup_dirty_repo(temp: &TempDir, database: &Database) -> (i64, std::path::PathBuf) {
        let remote = temp.path().join("remote.git");
        Command::new("git")
            .args(["init", "--bare", "-q", remote.to_string_lossy().as_ref()])
            .output()
            .expect("remote");
        let seed = temp.path().join("seed");
        fs::create_dir_all(&seed).expect("seed");
        git(&seed, &["init", "-q", "-b", "main"]);
        git(&seed, &["config", "user.email", "test@example.invalid"]);
        git(&seed, &["config", "user.name", "Xingshu Test"]);
        fs::write(seed.join("README.md"), "seed\n").expect("seed file");
        git(&seed, &["add", "README.md"]);
        git(&seed, &["commit", "-q", "-m", "seed"]);
        git(
            &seed,
            &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
        );
        git(&seed, &["push", "-q", "-u", "origin", "main"]);
        git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);

        let root_path = temp.path().join("root");
        let repo_path = root_path.join("network/demo");
        fs::create_dir_all(repo_path.parent().expect("repo parent")).expect("repo parent");
        let output = Command::new("git")
            .args([
                "clone",
                "-q",
                remote.to_string_lossy().as_ref(),
                repo_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("clone");
        assert!(output.status.success());
        fs::write(repo_path.join("local.txt"), "dirty\n").expect("dirty file");

        let now = Utc::now();
        let root = Root {
            id: None,
            name: "root".to_owned(),
            path: root_path.clone(),
            disk_label: None,
            mount_point: None,
            priority: 0,
            created_at: now,
            updated_at: now,
        };
        let root_id = database.upsert_root(&root).expect("root");
        let mut registered = root;
        registered.id = Some(root_id);
        scan_roots(database, &[registered], &ScanOptions::default()).expect("scan");
        let repo_id = database
            .find_repo("network/demo")
            .expect("repo lookup")
            .expect("repo")
            .id
            .expect("repo id");
        (repo_id, repo_path)
    }

    async fn exercise_task_conflict_decision(action: &str) {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        let database = Database::open(&database_path).expect("database");
        let (repo_id, repo_path) = setup_dirty_repo(&temp, &database);
        let app = build_router(database_path.clone());

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"type":"pull","conflictMode":"ask"}"#))
            .expect("create task request");
        let response = app.clone().oneshot(request).await.expect("create task");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("create task body");
        let created: serde_json::Value = serde_json::from_slice(&body).expect("create task json");
        assert_eq!(created["conflictMode"], "ask");
        let task_id = created["taskId"].as_str().expect("task id").to_owned();

        let waiting = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let task = Database::open(&database_path)
                    .expect("open database")
                    .find_task(&task_id)
                    .expect("find task")
                    .expect("task");
                if task.status == "waiting_for_decision" {
                    break task;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("task did not wait for decision");
        assert_eq!(waiting.conflict_mode, "ask");

        let request = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}/conflicts"))
            .body(Body::empty())
            .expect("conflicts request");
        let response = app.clone().oneshot(request).await.expect("conflicts");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("conflicts body");
        let conflicts: serde_json::Value = serde_json::from_slice(&body).expect("conflicts json");
        assert_eq!(conflicts.as_array().map(Vec::len), Some(1));
        assert_eq!(conflicts[0]["repoId"], repo_id);
        assert_eq!(conflicts[0]["status"], "waiting_decision");

        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/repos/{repo_id}/decision"))
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"action":"{action}"}}"#)))
            .expect("decision request");
        let response = app.clone().oneshot(request).await.expect("decision");
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let duplicate = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/repos/{repo_id}/decision"))
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"action":"{action}"}}"#)))
            .expect("duplicate decision request");
        let duplicate_response = app
            .clone()
            .oneshot(duplicate)
            .await
            .expect("duplicate decision");
        assert_eq!(duplicate_response.status(), StatusCode::CONFLICT);

        let final_task = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let task = Database::open(&database_path)
                    .expect("open database")
                    .find_task(&task_id)
                    .expect("find task")
                    .expect("task");
                if matches!(task.status.as_str(), "completed" | "failed") {
                    break task;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("task did not finish");
        assert_eq!(final_task.status, "completed");
        let final_repo = Database::open(&database_path)
            .expect("open database")
            .find_task_repo(&task_id, repo_id)
            .expect("find task repo")
            .expect("task repo");
        match action {
            "skip" => {
                assert_eq!(final_repo.status, "skipped");
                assert_eq!(final_repo.result.as_deref(), Some("skipped"));
                assert!(repo_path.join("local.txt").is_file());
            }
            "overwrite" => {
                assert_eq!(final_repo.status, "completed");
                assert_eq!(final_repo.result.as_deref(), Some("ok"));
                assert!(!repo_path.join("local.txt").exists());
            }
            "backup" => {
                assert_eq!(final_repo.status, "completed");
                assert_eq!(final_repo.result.as_deref(), Some("ok"));
                assert!(
                    final_repo
                        .backup_path
                        .as_deref()
                        .is_some_and(|path| Path::new(path).join("local.txt").is_file())
                );
            }
            other => panic!("unsupported test action: {other}"),
        }

        let request = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}/conflicts"))
            .body(Body::empty())
            .expect("final conflicts request");
        let response = app.oneshot(request).await.expect("final conflicts");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("final conflicts body");
        let conflicts: serde_json::Value =
            serde_json::from_slice(&body).expect("final conflicts json");
        assert_eq!(conflicts, serde_json::json!([]));
    }

    #[tokio::test]
    async fn api_returns_camel_case_and_request_id_for_real_router() {
        let temp = TempDir::new().expect("temp dir");
        Database::open(temp.path().join("index.db")).expect("database");
        let app = build_router(temp.path().join("index.db"));
        let request = Request::builder()
            .uri("/api/v1/repos")
            .header("x-request-id", "plan-204-api")
            .body(Body::empty())
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["x-request-id"], "plan-204-api");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body, "[]");
    }

    #[tokio::test]
    async fn api_returns_problem_details_for_invalid_action() {
        let temp = TempDir::new().expect("temp dir");
        Database::open(temp.path().join("index.db")).expect("database");
        let app = build_router(temp.path().join("index.db"));
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/repos/1/pull/invalid")
            .body(Body::empty())
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), 400);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("problem json");
        assert_eq!(value["code"], "invalid_conflict_action");
        assert!(value["requestId"].as_str().is_some_and(|id| !id.is_empty()));
    }

    #[tokio::test]
    async fn status_backup_and_prune_api_contract() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("root-a");
        let repo_path = root.join("local").join("s");
        std::fs::create_dir_all(&repo_path).expect("repo dir");
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo_path)
                .args(args)
                .output()
                .expect("git");
            assert!(output.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.invalid"]);
        git(&["config", "user.name", "Xingshu Test"]);
        std::fs::write(repo_path.join("README.md"), "fixture\n").expect("fixture");
        git(&["add", "README.md"]);
        git(&["commit", "-q", "-m", "fixture"]);

        let db_path = temp.path().join("index.db");
        let database = Database::open(&db_path).expect("database");
        let mut root_record = Root {
            id: None,
            name: "fixture".to_owned(),
            path: root.clone(),
            disk_label: None,
            mount_point: None,
            priority: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let root_id = database.upsert_root(&root_record).expect("root");
        root_record.id = Some(root_id);
        let repo_id = database
            .upsert_repo(&RepoRecord {
                id: None,
                root_id,
                org: "local".to_owned(),
                name: "s".to_owned(),
                rel_path: std::path::PathBuf::from("local/s"),
                remote_url: None,
                remote_host: None,
                remote_namespace: None,
                org_source: Some("path".to_owned()),
                default_branch: Some("main".to_owned()),
                head_commit: None,
                last_fetch_at: None,
                last_pull_status: None,
                dirty: true,
                ahead: 2,
                behind: 1,
                status_checked_at: Some(Utc::now()),
                size_bytes: 0,
                is_bare: false,
                clone_status: "ok".to_owned(),
                repo_kind: RepoKind::Own,
                modify_lock: false,
                lock_violation: false,
                push_protect_remote: None,
                scan_fingerprint: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .expect("repo");
        drop(database);

        let app = build_router(db_path);

        // V1 local snapshot fields
        let request = Request::builder()
            .uri(format!("/api/v1/repos/{repo_id}/status"))
            .body(Body::empty())
            .expect("status request");
        let response = app.clone().oneshot(request).await.expect("status response");
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("status json");
        assert_eq!(value["dirty"], serde_json::json!(true));
        assert_eq!(value["ahead"], serde_json::json!(2));
        assert_eq!(value["behind"], serde_json::json!(1));
        assert!(value["statusCheckedAt"].is_string());
        assert!(value["remote"].is_null());

        // V1 no-origin repo reports unknown remote state when asked
        let request = Request::builder()
            .uri(format!("/api/v1/repos/{repo_id}/status?remote=true"))
            .body(Body::empty())
            .expect("status remote request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["remote"]["state"], "unknown");

        // V3 restore without confirm is rejected
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/repos/{repo_id}/backups/restore"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "path": "whatever" }).to_string(),
            ))
            .expect("restore request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 400);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["code"], "confirm_required");

        // V4 prune dry-run lists, execute deletes, keep preserves newest
        std::fs::create_dir_all(root.join("local").join("s.bak.202609101800")).expect("bak dir 1");
        std::fs::create_dir_all(root.join("local").join("s.bak.202609110900")).expect("bak dir 2");
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/repos/{repo_id}/backups/prune"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({ "keep": 1 }).to_string()))
            .expect("prune preview request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["executed"], serde_json::json!(false));
        assert_eq!(value["affected"].as_array().expect("affected").len(), 1);
        assert!(root.join("local").join("s.bak.202609101800").is_dir());

        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/repos/{repo_id}/backups/prune"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "keep": 1, "dryRun": false }).to_string(),
            ))
            .expect("prune execute request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["executed"], serde_json::json!(true));
        assert!(root.join("local").join("s.bak.202609110900").is_dir());
        assert!(!root.join("local").join("s.bak.202609101800").exists());

        // backups list reports the remaining newest backup
        let request = Request::builder()
            .uri(format!("/api/v1/repos/{repo_id}/backups"))
            .body(Body::empty())
            .expect("backups list request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value.as_array().expect("list").len(), 1);
        assert_eq!(
            value[0]["path"],
            serde_json::json!(
                root.join("local")
                    .join("s.bak.202609110900")
                    .to_string_lossy()
            )
        );

        // repository not found → 404
        let request = Request::builder()
            .uri("/api/v1/repos/999/status")
            .body(Body::empty())
            .expect("404 request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn api_boundary_converts_domain_keys_to_camel_case() {
        let value = camelize_value(serde_json::json!({
            "repo_kind": "own",
            "created_at": "now",
            "nested_value": { "root_id": 1 }
        }));
        assert_eq!(value["repoKind"], "own");
        assert_eq!(value["createdAt"], "now");
        assert_eq!(value["nestedValue"]["rootId"], 1);
    }

    #[tokio::test]
    async fn problem_details_preserve_redacted_error_chain() {
        let error = anyhow::anyhow!("token=secret").context("operation failed");
        let response = super::response_for_anyhow(&error);
        assert_eq!(response.status(), 500);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("problem json");
        assert_eq!(value["code"], "internal_error");
        assert!(
            value["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("operation failed"))
        );
        assert!(
            !body
                .windows(b"secret".len())
                .any(|window| window == b"secret")
        );
    }

    #[tokio::test]
    async fn task_api_persists_completion_and_closes_terminal_sse() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        Database::open(&database_path).expect("database");
        let app = build_router(database_path.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"type":"scan"}"#))
            .expect("create task request");
        let response = app.clone().oneshot(request).await.expect("create task");
        assert_eq!(response.status(), 202);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("task body");
        let created: serde_json::Value = serde_json::from_slice(&body).expect("task json");
        assert_eq!(created["conflictMode"], "policy");
        let task_id = created["taskId"].as_str().expect("task id").to_owned();

        let task = loop {
            let task = Database::open(&database_path)
                .expect("open database")
                .find_task(&task_id)
                .expect("find task")
                .expect("task");
            if task.status == "completed" || task.status == "failed" {
                break task;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(task.status, "completed");

        let request = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}"))
            .body(Body::empty())
            .expect("get task request");
        let response = app.clone().oneshot(request).await.expect("get task");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("get task body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("get task json");
        assert_eq!(value["taskId"], task_id);
        assert_eq!(value["type"], "scan");
        assert_eq!(value["conflictMode"], "policy");
        assert_eq!(value["status"], "completed");

        let request = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}/stream"))
            .body(Body::empty())
            .expect("stream request");
        let response = app.oneshot(request).await.expect("stream");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .expect("content type")
                .starts_with("text/event-stream")
        );
        assert_eq!(response.headers()["cache-control"], "no-cache");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("stream body");
        let body = String::from_utf8(body.to_vec()).expect("SSE text");
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("\"taskId\""));
        assert!(body.contains("\"status\":\"completed\""));
    }

    #[tokio::test]
    async fn task_api_returns_stable_problem_details_for_invalid_requests() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        Database::open(&database_path).expect("database");
        let app = build_router(database_path);

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"type":"unknown"}"#))
            .expect("invalid task request");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("invalid task response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("invalid task body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("invalid task json");
        assert_eq!(value["code"], "invalid_task_type");

        for path in ["/api/v1/tasks/missing", "/api/v1/tasks/missing/stream"] {
            let request = Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("missing task request");
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("missing task response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers()["content-type"],
                "application/problem+json"
            );
        }
    }

    #[tokio::test]
    async fn task_api_applies_all_conflict_decisions_to_the_selected_repo() {
        for action in ["backup", "overwrite", "skip"] {
            exercise_task_conflict_decision(action).await;
        }
    }

    #[test]
    fn last_event_id_parses_resume_header() {
        use axum::http::HeaderMap;
        let empty = HeaderMap::new();
        assert_eq!(parse_last_event_id(&empty), 0);
        let mut headers = HeaderMap::new();
        headers.insert("last-event-id", "42".parse().expect("header value"));
        assert_eq!(parse_last_event_id(&headers), 42);
        let mut bad = HeaderMap::new();
        bad.insert(
            "last-event-id",
            "not-a-number".parse().expect("header value"),
        );
        assert_eq!(parse_last_event_id(&bad), 0);
    }

    #[test]
    fn last_event_id_suppresses_already_seen_sequences() {
        // Stream filter contract: snapshot is always sent; only events with
        // sequence > last_event_id pass the shared `event_visible_after` gate.
        assert!(!event_visible_after(7, 5));
        assert!(!event_visible_after(7, 7));
        assert!(event_visible_after(7, 8));
        assert!(event_visible_after(7, 42));
        assert!(event_visible_after(0, 1));
    }

    #[tokio::test]
    async fn diff_api_contract_reads_worktree_and_rejects_unsafe_paths() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        let database = Database::open(&database_path).expect("database");
        let (repo_id, repo_path) = setup_dirty_repo(&temp, &database);
        fs::write(repo_path.join("README.md"), "seed\ntracked change\n").expect("modify tracked");
        drop(database);

        let status_before = Command::new("git")
            .arg("-C")
            .arg(&repo_path)
            .args(["status", "--porcelain"])
            .output()
            .expect("status before");

        let app = build_router(database_path);
        let get = |uri: String| {
            let app = app.clone();
            async move {
                let request = Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("diff request");
                app.oneshot(request).await.expect("diff response")
            }
        };

        let response = get(format!("/api/v1/repos/{repo_id}/diff?path=README.md")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("diff json");
        assert_eq!(value["path"], "README.md");
        assert_eq!(value["kind"], "diff");
        assert_eq!(value["truncated"], false);
        assert!(
            value["patch"]
                .as_str()
                .expect("patch")
                .contains("tracked change"),
            "patch should contain the worktree hunk: {value}"
        );

        let response = get(format!("/api/v1/repos/{repo_id}/diff?path=local.txt")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("untracked json");
        assert_eq!(value["kind"], "untracked");
        assert_eq!(value["patch"], "");

        let response = get(format!("/api/v1/repos/{repo_id}/diff?path=missing.txt")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get(format!(
            "/api/v1/repos/{repo_id}/diff?path=..%2Foutside.txt"
        ))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );

        let response = get(format!(
            "/api/v1/repos/{repo_id}/diff?path=C%3A%2FWindows%2Fwin.ini"
        ))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let status_after = Command::new("git")
            .arg("-C")
            .arg(&repo_path)
            .args(["status", "--porcelain"])
            .output()
            .expect("status after");
        assert_eq!(
            status_before.stdout, status_after.stdout,
            "diff endpoint must stay read-only"
        );
    }

    #[tokio::test]
    async fn diff_api_contract_rejects_bare_repository() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        let database = Database::open(&database_path).expect("database");
        let root_path = temp.path().join("root");
        let bare_path = root_path.join("archives/bare.git");
        fs::create_dir_all(&bare_path).expect("bare dir");
        git(&bare_path, &["init", "--bare", "-q"]);

        let now = Utc::now();
        let root = Root {
            id: None,
            name: "root".to_owned(),
            path: root_path,
            disk_label: None,
            mount_point: None,
            priority: 0,
            created_at: now,
            updated_at: now,
        };
        let root_id = database.upsert_root(&root).expect("root");
        let repo_id = database
            .upsert_repo(&RepoRecord {
                id: None,
                root_id,
                org: "archives".to_owned(),
                name: "bare".to_owned(),
                rel_path: std::path::PathBuf::from("archives/bare.git"),
                remote_url: None,
                remote_host: None,
                remote_namespace: None,
                org_source: Some("path".to_owned()),
                default_branch: None,
                head_commit: None,
                last_fetch_at: None,
                last_pull_status: None,
                dirty: false,
                ahead: 0,
                behind: 0,
                status_checked_at: None,
                size_bytes: 0,
                is_bare: true,
                clone_status: "ok".to_owned(),
                repo_kind: RepoKind::ThirdParty,
                modify_lock: true,
                lock_violation: false,
                push_protect_remote: None,
                scan_fingerprint: None,
                created_at: now,
                updated_at: now,
            })
            .expect("bare repo");
        drop(database);

        let app = build_router(database_path);
        let request = Request::builder()
            .uri(format!("/api/v1/repos/{repo_id}/diff?path=README.md"))
            .body(Body::empty())
            .expect("diff request");
        let response = app.oneshot(request).await.expect("diff response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }

    #[tokio::test]
    async fn root_space_api_contract_reports_bytes_and_missing_roots() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        let database = Database::open(&database_path).expect("database");
        let root_path = temp.path().join("root");
        fs::create_dir_all(&root_path).expect("root dir");
        let now = Utc::now();
        let root = Root {
            id: None,
            name: "root".to_owned(),
            path: root_path,
            disk_label: None,
            mount_point: None,
            priority: 0,
            created_at: now,
            updated_at: now,
        };
        let root_id = database.upsert_root(&root).expect("root");
        drop(database);

        let app = build_router(database_path);
        let request = Request::builder()
            .uri(format!("/api/v1/roots/{root_id}/space"))
            .body(Body::empty())
            .expect("space request");
        let response = app.clone().oneshot(request).await.expect("space response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("space json");
        assert_eq!(value["rootId"], root_id);
        assert!(
            value["availableBytes"].as_u64().expect("bytes") > 0,
            "free space should be positive: {value}"
        );

        let request = Request::builder()
            .uri("/api/v1/roots/99999/space")
            .body(Body::empty())
            .expect("missing request");
        let response = app.oneshot(request).await.expect("missing response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn repo_policy_view_reports_source_and_effective() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        let database = Database::open(&database_path).expect("database");
        let (repo_id, _repo_path) = setup_dirty_repo(&temp, &database);
        drop(database);
        let app = build_router(database_path);

        async fn get_policy(app: axum::Router, repo_id: i64) -> serde_json::Value {
            let request = Request::builder()
                .uri(format!("/api/v1/repos/{repo_id}/policy"))
                .body(Body::empty())
                .expect("policy request");
            let response = app.oneshot(request).await.expect("policy response");
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body");
            serde_json::from_slice(&body).expect("policy json")
        }

        let initial = get_policy(app.clone(), repo_id).await;
        assert_eq!(initial["source"], "kind-default");

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tags")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"slug":"e2e-source","label":"e2e-source"}"#))
            .expect("tag request");
        let response = app.clone().oneshot(request).await.expect("tag response");
        assert_eq!(response.status(), StatusCode::CREATED);

        let request = Request::builder()
            .uri("/api/v1/tags")
            .body(Body::empty())
            .expect("tags request");
        let response = app.clone().oneshot(request).await.expect("tags response");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("tags body");
        let tags: serde_json::Value = serde_json::from_slice(&body).expect("tags json");
        let tag_id = tags
            .as_array()
            .expect("tags array")
            .iter()
            .find(|tag| tag["slug"] == "e2e-source")
            .expect("created tag")["id"]
            .as_i64()
            .expect("tag id");

        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/repos/{repo_id}/tags"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"slug":"e2e-source"}"#))
            .expect("attach request");
        let response = app.clone().oneshot(request).await.expect("attach response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let request = Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/tags/{tag_id}/policy"))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"pullStrategy":"mirror","conflict":"stop","unattended":"skip"}"#,
            ))
            .expect("tag policy request");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("tag policy response");
        assert!(response.status().is_success(), "{}", response.status());

        let tagged = get_policy(app.clone(), repo_id).await;
        assert_eq!(tagged["source"], "tag");
        assert_eq!(tagged["sourceTagId"], tag_id);
        assert_eq!(tagged["effective"]["pullStrategy"], "mirror");

        let request = Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/repos/{repo_id}/policy"))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"pullStrategy":"archive","conflict":"skip","unattended":"skip"}"#,
            ))
            .expect("repo policy request");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("repo policy response");
        assert!(response.status().is_success(), "{}", response.status());

        let overridden = get_policy(app, repo_id).await;
        assert_eq!(overridden["source"], "repo");
        assert_eq!(overridden["effective"]["pullStrategy"], "archive");
        assert_eq!(overridden["effective"]["conflict"], "skip");
    }
}
