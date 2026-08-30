use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;

use tokio::sync::Mutex;

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, Request, StatusCode, header::CONTENT_TYPE},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::services::ServeDir;
use xingshu_core::{
    Database,
    policy::{PullConflictAction, default_for_kind},
    puller::{PullMode, make_fetch_log, pull_repo},
};

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
    scan_lock: Arc<Mutex<()>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let db_path = std::env::var_os("XINGSHU_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("xingshu.db"));
    let host = std::env::var("XINGSHU_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("XINGSHU_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(12681);
    let app = build_router(db_path);
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "xingshu server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(db_path: PathBuf) -> Router {
    let state = AppState {
        db_path,
        scan_lock: Arc::new(Mutex::new(())),
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
        .route("/api/v1/tags", get(tags).post(create_tag))
        .route("/api/v1/tags/{tag_id}", delete(delete_tag))
        .route("/api/v1/scan", post(trigger_scan))
        .route("/api/v1/stats", get(stats))
        .fallback_service(ServeDir::new("webui/dist"))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

fn init_logging() {
    let filter = std::env::var("XINGSHU_LOG_LEVEL")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_owned());
    if let Some(path) = std::env::var_os("XINGSHU_LOG_FILE") {
        let path = std::path::PathBuf::from(path);
        let max_bytes = std::env::var("XINGSHU_LOG_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10 * 1024 * 1024);
        let max_files = std::env::var("XINGSHU_LOG_MAX_FILES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(7);
        match xingshu_core::logging::SizeRollingFile::new(&path, max_bytes, max_files) {
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
            .with_target(true)
            .with_env_filter(filter)
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
                "action must be backup, overwrite, or abort",
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

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tempfile::TempDir;
    use tower::ServiceExt;
    use xingshu_core::Database;

    use super::{build_router, camelize_value};

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
}
