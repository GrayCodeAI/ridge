use crate::core::worktree;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Blocked,
    Done,
    Failed,
}

impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub deps: Vec<u64>,
    pub status: TaskStatus,
    pub worktree: Option<PathBuf>,
    pub branch: Option<String>,
    pub quality_gate: Option<String>,
    pub pane_id: Option<usize>,
    pub leased_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBoard {
    pub tasks: Vec<Task>,
    next_id: u64,
}

impl TaskBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, title: String, deps: Vec<u64>, gate: Option<String>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.tasks.push(Task {
            id,
            title,
            deps,
            status: TaskStatus::Pending,
            worktree: None,
            branch: None,
            quality_gate: gate,
            pane_id: None,
            leased_paths: Vec::new(),
        });
        id
    }

    pub fn ready_tasks(&self) -> Vec<u64> {
        self.tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.deps.iter().all(|d| {
                        self.tasks
                            .iter()
                            .any(|x| x.id == *d && x.status == TaskStatus::Done)
                    })
            })
            .map(|t| t.id)
            .collect()
    }

    pub fn start_task(&mut self, id: u64, repo: &Path) -> Result<PathBuf, String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("not found")?;
        if task.status != TaskStatus::Pending {
            return Err("not pending".into());
        }
        let branch = format!("term/task-{id}");
        let base = worktree::worktree_base(repo);
        let _ = std::fs::create_dir_all(&base);
        let path = base.join(format!("task-{id}"));
        worktree::create_worktree(repo, &branch, &path)?;
        task.status = TaskStatus::Running;
        task.worktree = Some(path.clone());
        task.branch = Some(branch);
        Ok(path)
    }

    pub fn complete_task(&mut self, id: u64, success: bool) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.status = if success {
                TaskStatus::Done
            } else {
                TaskStatus::Failed
            };
        }
    }

    /// Lease paths for a task. Returns Err if any path is already leased by another running task.
    pub fn lease_paths(&mut self, id: u64, paths: Vec<PathBuf>) -> Result<(), String> {
        // check conflicts
        for p in &paths {
            for t in &self.tasks {
                if t.id != id && t.status == TaskStatus::Running && t.leased_paths.contains(p) {
                    return Err(format!(
                        "path {} already leased by task {}",
                        p.display(),
                        t.id
                    ));
                }
            }
        }
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.leased_paths = paths;
            Ok(())
        } else {
            Err("task not found".into())
        }
    }

    /// Run the quality gate command in the task's worktree. Returns Ok(true) on success.
    pub fn run_quality_gate(&self, id: u64) -> Result<bool, String> {
        let task = self.tasks.iter().find(|t| t.id == id).ok_or("not found")?;
        let wt = task.worktree.as_ref().ok_or("no worktree")?;
        let gate = task.quality_gate.as_ref().ok_or("no quality gate set")?;
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(gate)
            .current_dir(wt)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(out.status.success())
    }

    /// Merge the task's branch into the repo's current branch via a dedicated merge worktree.
    pub fn merge_task(&mut self, id: u64, repo: &Path) -> Result<(), String> {
        let (branch, wt) = {
            let task = self.tasks.iter().find(|t| t.id == id).ok_or("not found")?;
            if task.status != TaskStatus::Done {
                return Err("task not done".into());
            }
            (
                task.branch.clone().ok_or("no branch")?,
                task.worktree.clone(),
            )
        };
        // merge the task branch into current HEAD
        let out = std::process::Command::new("git")
            .args(["merge", "--no-ff", &branch])
            .current_dir(repo)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        // clean up worktree
        if let Some(w) = wt {
            let _ = crate::core::worktree::remove_worktree(repo, &w);
        }
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.worktree = None;
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for t in &self.tasks {
            let leases = if t.leased_paths.is_empty() {
                String::new()
            } else {
                format!(" leases: {:?}", t.leased_paths)
            };
            let gate = t.quality_gate.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "[{}] #{} {} (deps: {:?}, gate: {}){}\n",
                t.status.label(),
                t.id,
                t.title,
                t.deps,
                gate,
                leases
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_board_add_and_ready() {
        let mut board = TaskBoard::new();
        let id1 = board.add("first".into(), vec![], None);
        let id2 = board.add("second".into(), vec![id1], None);
        assert_eq!(board.ready_tasks(), vec![id1]);
        board.complete_task(id1, true);
        assert_eq!(board.ready_tasks(), vec![id2]);
    }

    #[test]
    fn task_status_labels() {
        assert_eq!(TaskStatus::Pending.label(), "pending");
        assert_eq!(TaskStatus::Done.label(), "done");
    }

    #[test]
    fn task_board_render() {
        let mut board = TaskBoard::new();
        board.add("hello".into(), vec![], None);
        let r = board.render();
        assert!(r.contains("hello"));
        assert!(r.contains("pending"));
    }
}
