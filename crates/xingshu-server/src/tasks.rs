use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::sync::broadcast;
use uuid::Uuid;
use xingshu_core::types::{
    TaskEvent, TaskRecord, TaskRepoRecord, TaskRepoUpdate, TaskUpdate, VcsError,
};
use xingshu_core::{Database, ProgressReporter};

const EVENT_BUFFER: usize = 256;

#[derive(Clone)]
pub struct TaskManager {
    db_path: PathBuf,
    runtimes: Arc<Mutex<HashMap<String, RuntimeTask>>>,
    active_types: Arc<Mutex<HashSet<String>>>,
}

struct RuntimeTask {
    sequence: u64,
    sender: broadcast::Sender<TaskEvent>,
}

#[derive(Default)]
struct TaskEventDetails {
    duration_ms: Option<u64>,
    repo_id: Option<i64>,
    repo_status: Option<String>,
    conflict_reason: Option<String>,
    requested_action: Option<String>,
    backup_path: Option<String>,
}

impl TaskManager {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            active_types: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn create(
        &self,
        task_type: &str,
        request_id: Option<String>,
        conflict_mode: &str,
    ) -> Result<TaskRecord, VcsError> {
        if !matches!(task_type, "scan" | "pull") {
            return Err(VcsError::Database(format!(
                "unsupported task type: {task_type}"
            )));
        }
        if !matches!(conflict_mode, "policy" | "ask") {
            return Err(VcsError::Database(format!(
                "unsupported conflict mode: {conflict_mode}"
            )));
        }
        let database = Database::open(&self.db_path)?;
        if database.has_active_task_type(task_type)? {
            return Err(VcsError::Database(format!(
                "{task_type} task already running"
            )));
        }
        {
            let mut active = self
                .active_types
                .lock()
                .map_err(|_| VcsError::Database("task lock poisoned".to_owned()))?;
            if !active.insert(task_type.to_owned()) {
                return Err(VcsError::Database(format!(
                    "{task_type} task already running"
                )));
            }
        }

        let now = Utc::now();
        let task = TaskRecord {
            id: Uuid::new_v4().to_string(),
            task_type: task_type.to_owned(),
            status: "pending".to_owned(),
            conflict_mode: conflict_mode.to_owned(),
            request_id,
            operation_id: Uuid::new_v4().to_string(),
            progress_current: Some(0),
            progress_total: None,
            last_repo: None,
            last_result: None,
            error: None,
            result_json: None,
            created_at: now,
            updated_at: now,
        };
        if let Err(error) = database.create_task(&task) {
            if let Ok(mut active) = self.active_types.lock() {
                active.remove(task_type);
            }
            return Err(error);
        }
        let (sender, _) = broadcast::channel(EVENT_BUFFER);
        self.runtimes
            .lock()
            .map_err(|_| VcsError::Database("task lock poisoned".to_owned()))?
            .insert(
                task.id.clone(),
                RuntimeTask {
                    sequence: 0,
                    sender,
                },
            );
        Ok(task)
    }

    pub fn get(&self, task_id: &str) -> Result<Option<TaskRecord>, VcsError> {
        Database::open(&self.db_path)?.find_task(task_id)
    }

    pub fn prepare_task_repos(&self, task_id: &str, repo_ids: &[i64]) -> Result<(), VcsError> {
        Database::open(&self.db_path)?.create_task_repos(task_id, repo_ids)
    }

    pub fn list_task_repos(&self, task_id: &str) -> Result<Vec<TaskRepoRecord>, VcsError> {
        Database::open(&self.db_path)?.list_task_repos(task_id)
    }

    pub fn claim_task_repo_running(&self, task_id: &str, repo_id: i64) -> Result<(), VcsError> {
        let claimed = Database::open(&self.db_path)?
            .claim_task_repo(task_id, repo_id, "pending", "running", None)?;
        if claimed {
            Ok(())
        } else {
            Err(VcsError::Database(format!(
                "task repo {repo_id} is not pending"
            )))
        }
    }

    pub fn save_task_repo(
        &self,
        task_id: &str,
        repo_id: i64,
        update: &TaskRepoUpdate,
        event_name: &str,
    ) -> Result<TaskRepoRecord, VcsError> {
        let database = Database::open(&self.db_path)?;
        database.update_task_repo(task_id, repo_id, update)?;
        let record = database
            .find_task_repo(task_id, repo_id)?
            .ok_or_else(|| VcsError::Database("task repo not found after update".to_owned()))?;
        self.publish_task_repo_event(task_id, event_name, &record)?;
        Ok(record)
    }

    pub fn claim_task_repo_decision(
        &self,
        task_id: &str,
        repo_id: i64,
        action: &str,
    ) -> Result<TaskRepoRecord, VcsError> {
        let database = Database::open(&self.db_path)?;
        let current = database
            .find_task_repo(task_id, repo_id)?
            .ok_or_else(|| VcsError::Database("task repo not found".to_owned()))?;
        if current.status != "waiting_decision" {
            return Err(VcsError::Database(
                "task repo is not waiting for a decision".to_owned(),
            ));
        }
        if !database.claim_task_repo(
            task_id,
            repo_id,
            "waiting_decision",
            "resolving",
            Some(action),
        )? {
            return Err(VcsError::Database(
                "task repo decision is already resolving".to_owned(),
            ));
        }
        if let Some(task) = self.get(task_id)?
            && task.status == "waiting_for_decision"
        {
            self.update(
                task_id,
                "decision_started",
                TaskUpdate {
                    status: "running".to_owned(),
                    current: task.progress_current,
                    total: task.progress_total,
                    last_repo: task.last_repo,
                    last_result: Some("decision_started".to_owned()),
                    error: None,
                    result_json: None,
                },
            )?;
        }
        let record = database
            .find_task_repo(task_id, repo_id)?
            .ok_or_else(|| VcsError::Database("task repo not found after claim".to_owned()))?;
        self.publish_task_repo_event(task_id, "decision_started", &record)?;
        Ok(record)
    }

    pub fn wait_for_decision(
        &self,
        task_id: &str,
        result_json: Option<String>,
    ) -> Result<(), VcsError> {
        let task = self
            .get(task_id)?
            .ok_or_else(|| VcsError::Database("task not found".to_owned()))?;
        self.update(
            task_id,
            "waiting_for_decision",
            TaskUpdate {
                status: "waiting_for_decision".to_owned(),
                current: task.progress_current,
                total: task.progress_total,
                last_repo: task.last_repo,
                last_result: Some("waiting_for_decision".to_owned()),
                error: None,
                result_json,
            },
        )
    }

    pub fn reconcile_task(
        &self,
        task_id: &str,
        result_json: Option<String>,
    ) -> Result<(), VcsError> {
        let repos = self.list_task_repos(task_id)?;
        if repos
            .iter()
            .any(|repo| matches!(repo.status.as_str(), "pending" | "running" | "resolving"))
        {
            return Ok(());
        }
        if repos.iter().any(|repo| repo.status == "waiting_decision") {
            return self.wait_for_decision(task_id, result_json);
        }
        let status = if repos.iter().any(|repo| repo.status == "failed") {
            "failed"
        } else {
            "completed"
        };
        let error = (status == "failed").then(|| "one or more task repositories failed".to_owned());
        self.finish(task_id, status, result_json, error)
    }

    pub fn start(&self, task_id: &str) -> Result<(), VcsError> {
        let task = self
            .get(task_id)?
            .ok_or_else(|| VcsError::Database("task not found".to_owned()))?;
        self.update(
            task_id,
            "started",
            TaskUpdate {
                status: "running".to_owned(),
                current: task.progress_current,
                total: task.progress_total,
                last_repo: None,
                last_result: Some("started".to_owned()),
                error: None,
                result_json: None,
            },
        )
    }

    pub fn subscribe(
        &self,
        task_id: &str,
    ) -> Result<(TaskRecord, broadcast::Receiver<TaskEvent>), VcsError> {
        let task = self
            .get(task_id)?
            .ok_or_else(|| VcsError::Database("task not found".to_owned()))?;
        let mut runtimes = self
            .runtimes
            .lock()
            .map_err(|_| VcsError::Database("task lock poisoned".to_owned()))?;
        if matches!(task.status.as_str(), "completed" | "failed" | "interrupted") {
            let (sender, receiver) = broadcast::channel(1);
            drop(sender);
            return Ok((task, receiver));
        }
        let receiver = runtimes
            .entry(task_id.to_owned())
            .or_insert_with(|| {
                let (sender, _) = broadcast::channel(EVENT_BUFFER);
                RuntimeTask {
                    sequence: 0,
                    sender,
                }
            })
            .sender
            .subscribe();
        Ok((task, receiver))
    }

    pub fn progress_reporter(&self, task_id: String) -> TaskReporter {
        TaskReporter {
            manager: self.clone(),
            task_id,
            current: AtomicU64::new(0),
            total: Mutex::new(None),
        }
    }

    pub fn finish(
        &self,
        task_id: &str,
        status: &str,
        result_json: Option<String>,
        error: Option<String>,
    ) -> Result<(), VcsError> {
        let task = self
            .get(task_id)?
            .ok_or_else(|| VcsError::Database("task not found".to_owned()))?;
        let event_name = if status == "completed" {
            "completed"
        } else {
            "failed"
        };
        self.update(
            task_id,
            event_name,
            TaskUpdate {
                status: status.to_owned(),
                current: task.progress_current,
                total: task.progress_total,
                last_repo: task.last_repo,
                last_result: Some(status.to_owned()),
                error,
                result_json,
            },
        )?;
        if let Ok(mut runtimes) = self.runtimes.lock() {
            runtimes.remove(task_id);
        }
        if let Ok(mut active) = self.active_types.lock() {
            active.remove(&task.task_type);
        }
        Ok(())
    }

    fn report_progress(
        &self,
        task_id: &str,
        current: Option<u64>,
        total: Option<u64>,
        last_repo: Option<&str>,
        last_result: Option<&str>,
    ) {
        self.report_progress_with_duration(task_id, current, total, last_repo, last_result, None);
    }

    fn report_progress_with_duration(
        &self,
        task_id: &str,
        current: Option<u64>,
        total: Option<u64>,
        last_repo: Option<&str>,
        last_result: Option<&str>,
        duration_ms: Option<u64>,
    ) {
        let _ = self.update_with_duration(
            task_id,
            "progress",
            TaskUpdate {
                status: "running".to_owned(),
                current,
                total,
                last_repo: last_repo.map(str::to_owned),
                last_result: last_result.map(str::to_owned),
                error: None,
                result_json: None,
            },
            duration_ms,
        );
    }

    fn report_progress_for_repo(
        &self,
        task_id: &str,
        update: TaskUpdate,
        details: TaskEventDetails,
    ) {
        let _ = self.update_with_details(task_id, "progress", update, details);
    }

    fn update(&self, task_id: &str, event_name: &str, change: TaskUpdate) -> Result<(), VcsError> {
        self.update_with_details(task_id, event_name, change, TaskEventDetails::default())
    }

    fn update_with_duration(
        &self,
        task_id: &str,
        event_name: &str,
        change: TaskUpdate,
        duration_ms: Option<u64>,
    ) -> Result<(), VcsError> {
        self.update_with_details(
            task_id,
            event_name,
            change,
            TaskEventDetails {
                duration_ms,
                ..TaskEventDetails::default()
            },
        )
    }

    fn update_with_details(
        &self,
        task_id: &str,
        event_name: &str,
        change: TaskUpdate,
        details: TaskEventDetails,
    ) -> Result<(), VcsError> {
        Database::open(&self.db_path)?.update_task(task_id, &change)?;
        let mut runtimes = self
            .runtimes
            .lock()
            .map_err(|_| VcsError::Database("task lock poisoned".to_owned()))?;
        let Some(runtime) = runtimes.get_mut(task_id) else {
            return Ok(());
        };
        runtime.sequence += 1;
        let event = TaskEvent {
            sequence: runtime.sequence,
            event: event_name.to_owned(),
            task_id: task_id.to_owned(),
            repo_id: details.repo_id,
            repo_status: details.repo_status,
            conflict_reason: details.conflict_reason,
            requested_action: details.requested_action,
            backup_path: details.backup_path,
            current: change.current,
            total: change.total,
            last_repo: change.last_repo,
            last_result: change.last_result,
            error: change.error,
            duration_ms: details.duration_ms,
        };
        let _ = runtime.sender.send(event);
        Ok(())
    }

    fn publish_task_repo_event(
        &self,
        task_id: &str,
        event_name: &str,
        record: &TaskRepoRecord,
    ) -> Result<(), VcsError> {
        let mut runtimes = self
            .runtimes
            .lock()
            .map_err(|_| VcsError::Database("task lock poisoned".to_owned()))?;
        let Some(runtime) = runtimes.get_mut(task_id) else {
            return Ok(());
        };
        runtime.sequence += 1;
        let event = TaskEvent {
            sequence: runtime.sequence,
            event: event_name.to_owned(),
            task_id: task_id.to_owned(),
            repo_id: Some(record.repo_id),
            repo_status: Some(record.status.clone()),
            conflict_reason: record.conflict_reason.clone(),
            requested_action: record.requested_action.clone(),
            backup_path: record.backup_path.clone(),
            current: None,
            total: None,
            last_repo: None,
            last_result: record.result.clone(),
            error: record.error.clone(),
            duration_ms: record.duration_ms,
        };
        let _ = runtime.sender.send(event);
        Ok(())
    }
}

pub struct TaskReporter {
    manager: TaskManager,
    task_id: String,
    current: AtomicU64,
    total: Mutex<Option<u64>>,
}

impl ProgressReporter for TaskReporter {
    fn started(&self, total: Option<u64>) {
        self.current.store(0, Ordering::Relaxed);
        if let Ok(mut stored_total) = self.total.lock() {
            *stored_total = total;
        }
        self.manager
            .report_progress(&self.task_id, Some(0), total, None, Some("started"));
    }

    fn item_finished(&self, item: &str, result: &str, duration_ms: Option<u64>) {
        let current = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.total.lock().ok().and_then(|value| *value);
        self.manager.report_progress_with_duration(
            &self.task_id,
            Some(current),
            total,
            Some(item),
            Some(result),
            duration_ms,
        );
    }

    fn finished(&self, result: &str) {
        let current = self.current.load(Ordering::Relaxed);
        let total = self.total.lock().ok().and_then(|value| *value);
        self.manager
            .report_progress(&self.task_id, Some(current), total, None, Some(result));
    }
}

impl TaskReporter {
    pub fn item_finished_for_repo(
        &self,
        repo_id: i64,
        repo_status: &str,
        item: &str,
        result: &str,
        duration_ms: Option<u64>,
    ) {
        let current = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.total.lock().ok().and_then(|value| *value);
        self.manager.report_progress_for_repo(
            &self.task_id,
            TaskUpdate {
                status: "running".to_owned(),
                current: Some(current),
                total,
                last_repo: Some(item.to_owned()),
                last_result: Some(result.to_owned()),
                error: None,
                result_json: None,
            },
            TaskEventDetails {
                duration_ms,
                repo_id: Some(repo_id),
                repo_status: Some(repo_status.to_owned()),
                ..TaskEventDetails::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use xingshu_core::Database;

    use super::TaskManager;

    #[test]
    fn manager_serializes_task_type_and_closes_terminal_event_stream() {
        let temp = TempDir::new().expect("temp dir");
        let database_path = temp.path().join("index.db");
        Database::open(&database_path).expect("database");
        let manager = TaskManager::new(database_path);
        let task = manager.create("scan", None, "policy").expect("create task");
        let (_, mut receiver) = manager.subscribe(&task.id).expect("subscribe");

        let error = manager
            .create("scan", None, "policy")
            .expect_err("duplicate task");
        assert!(error.to_string().contains("already running"));

        manager.start(&task.id).expect("start task");
        assert_eq!(receiver.try_recv().expect("started event").event, "started");
        manager
            .finish(&task.id, "completed", Some("{}".to_owned()), None)
            .expect("finish task");
        assert_eq!(
            receiver.try_recv().expect("completed event").event,
            "completed"
        );
        assert!(receiver.try_recv().is_err());

        let next = manager
            .create("scan", None, "policy")
            .expect("lock released");
        assert_eq!(next.task_type, "scan");
    }
}
