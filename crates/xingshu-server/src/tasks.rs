use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::sync::broadcast;
use uuid::Uuid;
use xingshu_core::types::{TaskEvent, TaskRecord, TaskUpdate, VcsError};
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
    ) -> Result<TaskRecord, VcsError> {
        if !matches!(task_type, "scan" | "pull") {
            return Err(VcsError::Database(format!(
                "unsupported task type: {task_type}"
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
        let database = Database::open(&self.db_path);
        if let Err(error) = database.and_then(|database| database.create_task(&task)) {
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
                last_result: task.last_result,
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
        let _ = self.update(
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
        );
    }

    fn update(&self, task_id: &str, event_name: &str, change: TaskUpdate) -> Result<(), VcsError> {
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
            current: change.current,
            total: change.total,
            last_repo: change.last_repo,
            last_result: change.last_result,
            error: change.error,
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

    fn item_finished(&self, item: &str, result: &str) {
        let current = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.total.lock().ok().and_then(|value| *value);
        self.manager.report_progress(
            &self.task_id,
            Some(current),
            total,
            Some(item),
            Some(result),
        );
    }

    fn finished(&self, result: &str) {
        let current = self.current.load(Ordering::Relaxed);
        let total = self.total.lock().ok().and_then(|value| *value);
        self.manager
            .report_progress(&self.task_id, Some(current), total, None, Some(result));
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
        let task = manager.create("scan", None).expect("create task");
        let (_, mut receiver) = manager.subscribe(&task.id).expect("subscribe");

        let error = manager.create("scan", None).expect_err("duplicate task");
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

        let next = manager.create("scan", None).expect("lock released");
        assert_eq!(next.task_type, "scan");
    }
}
