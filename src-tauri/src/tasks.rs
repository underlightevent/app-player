//! Tâches de fond : progression, annulation, affichage dans le panneau Tâches.
//! L'interface reçoit l'événement « tasks:update » à chaque changement.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    Done,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub id: u64,
    /// Clé de traduction du libellé (« tasks.scan »…).
    pub label: String,
    /// Détail libre (nom du dossier, du fichier…).
    pub detail: Option<String>,
    pub done: u64,
    pub total: u64,
    pub state: TaskState,
    pub error: Option<String>,
    pub started_ms: u64,
}

struct Entry {
    info: TaskInfo,
    cancel: Arc<AtomicBool>,
    last_emit: Instant,
}

pub struct Tasks {
    next: AtomicU64,
    entries: Mutex<BTreeMap<u64, Entry>>,
    app: Option<AppHandle>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Tasks {
    pub fn new(app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Tasks { next: AtomicU64::new(1), entries: Mutex::new(BTreeMap::new()), app })
    }

    pub fn start(self: &Arc<Self>, label: &str, detail: Option<String>) -> TaskHandle {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let info = TaskInfo {
            id,
            label: label.to_string(),
            detail,
            done: 0,
            total: 0,
            state: TaskState::Running,
            error: None,
            started_ms: now_ms(),
        };
        if let Ok(mut map) = self.entries.lock() {
            // On ne garde que les 20 dernières tâches terminées.
            let finished: Vec<u64> =
                map.iter().filter(|(_, e)| e.info.state != TaskState::Running).map(|(k, _)| *k).collect();
            for k in finished.iter().take(finished.len().saturating_sub(20)) {
                map.remove(k);
            }
            map.insert(id, Entry { info: info.clone(), cancel: cancel.clone(), last_emit: Instant::now() });
        }
        self.emit(&info);
        TaskHandle { id, cancel, tasks: self.clone() }
    }

    pub fn list(&self) -> Vec<TaskInfo> {
        self.entries.lock().map(|m| m.values().map(|e| e.info.clone()).collect()).unwrap_or_default()
    }

    pub fn cancel(&self, id: u64) {
        if let Ok(map) = self.entries.lock() {
            if let Some(e) = map.get(&id) {
                e.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    fn update(&self, id: u64, force: bool, f: impl FnOnce(&mut TaskInfo)) {
        let to_emit = {
            let Ok(mut map) = self.entries.lock() else { return };
            let Some(e) = map.get_mut(&id) else { return };
            f(&mut e.info);
            if force || e.last_emit.elapsed() >= Duration::from_millis(150) {
                e.last_emit = Instant::now();
                Some(e.info.clone())
            } else {
                None
            }
        };
        if let Some(info) = to_emit {
            self.emit(&info);
        }
    }

    fn emit(&self, info: &TaskInfo) {
        if let Some(app) = &self.app {
            let _ = app.emit("tasks:update", info);
        }
    }
}

pub struct TaskHandle {
    pub id: u64,
    cancel: Arc<AtomicBool>,
    tasks: Arc<Tasks>,
}

impl TaskHandle {
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn set_total(&self, total: u64) {
        self.tasks.update(self.id, true, |i| i.total = total);
    }
    pub fn progress(&self, done: u64) {
        self.tasks.update(self.id, false, |i| i.done = done);
    }
    pub fn detail(&self, detail: String) {
        self.tasks.update(self.id, false, |i| i.detail = Some(detail));
    }
    pub fn finish(self) {
        let cancelled = self.is_cancelled();
        self.tasks.update(self.id, true, |i| {
            i.state = if cancelled { TaskState::Cancelled } else { TaskState::Done };
            if !cancelled {
                i.done = i.total.max(i.done);
            }
        });
    }
    pub fn fail(self, error: String) {
        log::error!("Tâche {} en échec : {error}", self.id);
        self.tasks.update(self.id, true, |i| {
            i.state = TaskState::Error;
            i.error = Some(error);
        });
    }
}
