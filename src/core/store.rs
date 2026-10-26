//! Sled-based state store (pure Rust, no C compiler needed).
//! Replaces JSON file persistence for robust, concurrent-safe state.

use serde_json::Value;
use sled::{Db, Tree};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

type StoreResult<T> = Result<T, String>;

pub fn db_path() -> PathBuf {
    let dir = crate::config::data_dir();
    let current = dir.join("ridge.db");
    let legacy = dir.join("term.db");
    if !current.exists() && legacy.exists() {
        legacy
    } else {
        current
    }
}

fn open_db_at(path: PathBuf) -> StoreResult<Db> {
    let db = sled::open(&path).map_err(|e| e.to_string())?;
    let _ = db.open_tree("tasks").map_err(|e| e.to_string())?;
    let _ = db.open_tree("race_history").map_err(|e| e.to_string())?;
    let _ = db.open_tree("meta").map_err(|e| e.to_string())?;
    let meta = db.open_tree("meta").map_err(|e| e.to_string())?;
    let _ = meta.insert(b"schema_version", b"2");
    drop(meta);
    Ok(db)
}

#[cfg(not(test))]
pub fn open_db() -> StoreResult<Db> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = match sled::open(&path) {
        Ok(db) => db,
        Err(_) => {
            // Fall back to temp dir with unique name per call
            // (avoids sled lock conflicts when running multiple instances)
            let tmp_path = std::env::temp_dir().join(format!(
                "ridge-{}-{}.db",
                std::process::id(),
                DB_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            sled::open(&tmp_path).map_err(|e| e.to_string())?
        }
    };
    let _ = db.open_tree("tasks").map_err(|e| e.to_string())?;
    let _ = db.open_tree("race_history").map_err(|e| e.to_string())?;
    let _ = db.open_tree("meta").map_err(|e| e.to_string())?;
    let meta = db.open_tree("meta").map_err(|e| e.to_string())?;
    let _ = meta.insert(b"schema_version", b"2");
    drop(meta);
    Ok(db)
}

// Tests must never touch the user's real state store: a shared database makes
// results depend on leftover rows from earlier runs and on which test happens
// to open the file first. A unique temp path per call keeps every test
// isolated and parallel-safe.
#[cfg(test)]
pub fn open_db() -> StoreResult<Db> {
    let path = std::env::temp_dir().join(format!(
        "ridge-test-{}-{}.db",
        std::process::id(),
        DB_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    open_db_at(path)
}

pub struct TaskStore {
    db: Db,
    tasks: Tree,
}

impl TaskStore {
    pub fn new(db: Db) -> Self {
        let tasks = db.open_tree("tasks").expect("open tasks tree");
        Self { db, tasks }
    }

    pub fn insert_task(
        &self,
        id: u64,
        title: &str,
        status: &str,
        deps: &[u64],
        pane_id: Option<usize>,
    ) -> StoreResult<()> {
        let task = serde_json::json!({
            "id": id,
            "title": title,
            "status": status,
            "deps": deps,
            "pane_id": pane_id.map(|v| v as u64),
        });
        self.tasks
            .insert(
                id.to_string().as_bytes(),
                serde_json::to_vec(&task).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_all_tasks(&self) -> StoreResult<Vec<TaskRow>> {
        let mut tasks = Vec::new();
        for result in self.tasks.iter() {
            let (_, value) = result.map_err(|e| e.to_string())?;
            let task: Value = serde_json::from_slice(&value).map_err(|e| e.to_string())?;
            tasks.push(TaskRow {
                id: task.get("id").and_then(|v| v.as_u64()).unwrap_or(0),
                title: task
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: task
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pending")
                    .to_string(),
                deps: task
                    .get("deps")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default(),
                pane_id: task
                    .get("pane_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize),
            });
        }
        Ok(tasks)
    }

    pub fn update_task_status(
        &self,
        id: u64,
        status: &str,
        pane_id: Option<usize>,
    ) -> StoreResult<()> {
        let key = id.to_string().into_bytes();
        if let Some(existing) = self.tasks.get(&key).map_err(|e| e.to_string())? {
            let mut task: Value = serde_json::from_slice(&existing).map_err(|e| e.to_string())?;
            if let Some(obj) = task.as_object_mut() {
                obj.insert("status".to_string(), Value::String(status.to_string()));
                obj.insert(
                    "pane_id".to_string(),
                    match pane_id {
                        Some(v) => Value::Number((v as u64).into()),
                        None => Value::Null,
                    },
                );
            }
            self.tasks
                .insert(&key, serde_json::to_vec(&task).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn delete_task(&self, id: u64) -> StoreResult<Option<Vec<u8>>> {
        Ok(self
            .tasks
            .remove(id.to_string().as_bytes())
            .map_err(|e| e.to_string())?
            .map(|ivec| ivec.to_vec()))
    }
}

pub struct RaceStore {
    db: Db,
    races: Tree,
}

impl RaceStore {
    pub fn new(db: Db) -> Self {
        let races = db
            .open_tree("race_history")
            .expect("open race_history tree");
        Self { db, races }
    }

    pub fn insert_race(
        &self,
        prompt: &str,
        winner_agent: Option<&str>,
        gate_results: &[Value],
        forced: bool,
    ) -> StoreResult<u64> {
        let id = self.next_id()?;
        let record = serde_json::json!({
            "id": id,
            "prompt": prompt,
            "winner_agent": winner_agent,
            "finished_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "gate_results": gate_results,
            "forced": forced,
        });
        self.races
            .insert(
                id.to_string().as_bytes(),
                serde_json::to_vec(&record).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn next_id(&self) -> StoreResult<u64> {
        let mut max_id: u64 = 0;
        for result in self.races.iter() {
            let (key, _) = result.map_err(|e| e.to_string())?;
            if let Ok(id_str) = std::str::from_utf8(&key) {
                if let Ok(id) = id_str.parse::<u64>() {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }
        Ok(max_id + 1)
    }

    pub fn load_all_races(&self) -> StoreResult<Vec<RaceRecord>> {
        let mut races: Vec<RaceRecord> = Vec::new();
        for result in self.races.iter() {
            let (_, value) = result.map_err(|e| e.to_string())?;
            let record: Value = serde_json::from_slice(&value).map_err(|e| e.to_string())?;
            races.push(RaceRecord {
                id: record.get("id").and_then(|v| v.as_u64()).unwrap_or(0),
                prompt: record
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                winner_agent: record
                    .get("winner_agent")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                finished_at: record
                    .get("finished_at")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as f64,
                gate_results: record
                    .get("gate_results")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default(),
                forced: record
                    .get("forced")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            });
        }
        races.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(races)
    }

    pub fn cap_races(&self, max: usize) -> StoreResult<usize> {
        let mut all: Vec<(Vec<u8>, Vec<u8>)> = self
            .races
            .iter()
            .filter_map(|r| r.map_err(|e| e.to_string()).ok())
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect();
        all.sort_by(|a, b| {
            let id_a: u64 = std::str::from_utf8(&a.0)
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            let id_b: u64 = std::str::from_utf8(&b.0)
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            id_a.cmp(&id_b)
        });
        if all.len() <= max {
            return Ok(0);
        }
        let to_delete = all.len() - max;
        for (key, _) in all.iter().take(to_delete) {
            self.races.remove(key).map_err(|e| e.to_string())?;
        }
        Ok(to_delete)
    }
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub deps: Vec<u64>,
    pub pane_id: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RaceRecord {
    pub id: u64,
    pub prompt: String,
    pub winner_agent: Option<String>,
    pub finished_at: f64,
    pub gate_results: Vec<Value>,
    pub forced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_task_crud() {
        let db = open_db().expect("open db");
        let store = TaskStore::new(db);

        store
            .insert_task(1, "test task", "pending", &[], None)
            .expect("insert");
        let tasks = store.load_all_tasks().expect("load");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "test task");

        store
            .update_task_status(1, "done", Some(3))
            .expect("update");
        let tasks = store.load_all_tasks().expect("load");
        assert_eq!(tasks[0].status, "done");
        assert_eq!(tasks[0].pane_id, Some(3));

        store.delete_task(1).expect("delete");
        let tasks = store.load_all_tasks().expect("load");
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn store_race_cap() {
        let db = open_db().expect("open db");
        let store = RaceStore::new(db);

        for i in 1..=10 {
            store
                .insert_race(&format!("prompt {i}"), Some("claude"), &[], false)
                .expect("insert");
        }
        assert_eq!(store.cap_races(5).expect("cap"), 5);
        let races = store.load_all_races().expect("load");
        assert_eq!(races.len(), 5);
    }
}
