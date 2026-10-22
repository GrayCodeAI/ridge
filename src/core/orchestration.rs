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
    /// Free-form notes (e.g. captured inbox reply that promoted this task).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Actionable checklist (bullets captured from a promoted reply).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist: Option<Vec<CheckItem>>,
}

/// One checkbox row on a task's checklist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckItem {
    pub label: String,
    #[serde(default)]
    pub done: bool,
}

/// Extract bullets (lines starting with `-` or `*`) as a checklist.
pub fn checklist_from_reply(reply: &str) -> Option<Vec<CheckItem>> {
    let items: Vec<CheckItem> = reply
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let body = l.strip_prefix("- ").or(l.strip_prefix("* "))?;
            let body = body.trim();
            (!body.is_empty()).then(|| CheckItem {
                label: body.to_string(),
                done: false,
            })
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBoard {
    pub tasks: Vec<Task>,
    next_id: u64,
    /// Optional active filter: status label (pending/running/...) or title substring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl TaskBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn set_next_id(&mut self, id: u64) {
        self.next_id = id;
    }

    /// Persist the board to `<repo>/.term-tasks.json`.
    pub fn save(&self, repo: &Path) -> Result<(), String> {
        let path = repo.join(".term-tasks.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load a board from `<repo>/.term-tasks.json`, or a fresh one.
    pub fn load(repo: &Path) -> TaskBoard {
        let path = repo.join(".term-tasks.json");
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(board) = serde_json::from_str::<TaskBoard>(&s) {
                return board;
            }
        }
        TaskBoard::new()
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
            notes: None,
            checklist: None,
        });
        id
    }

    /// Add a task seeded from a captured inbox reply; notes record the reply
    /// and any `-`/`*` bullet lines become a checklist.
    pub fn add_from_reply(&mut self, pane_title: &str, prompt: &str, reply: &str) -> u64 {
        let title = if prompt.chars().count() > 80 {
            let t: String = prompt.chars().take(77).collect();
            format!("{t}…")
        } else {
            prompt.to_string()
        };
        let notes = format!("from: {pane_title}\nprompt: {prompt}\nreply: {reply}");
        let checklist = checklist_from_reply(reply);
        let id = self.add(title, vec![], None);
        if let Some(t) = self.tasks.last_mut() {
            t.notes = Some(notes);
            t.checklist = checklist;
        }
        id
    }

    /// Toggle checklist item `n` (1-based) on task `id`.
    /// Returns the new state of the item.
    pub fn check_toggle(&mut self, id: u64, n: usize) -> Result<bool, String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("no such task")?;
        let list = task.checklist.as_mut().ok_or("task has no checklist")?;
        if n == 0 || n > list.len() {
            return Err(format!(
                "checklist item {n} out of range (1–{})",
                list.len()
            ));
        }
        let item = &mut list[n - 1];
        item.done = !item.done;
        Ok(item.done)
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

    pub fn render_kanban(&self) -> String {
        let mut cols: [Vec<String>; 4] = [vec![], vec![], vec![], vec![]];
        for t in &self.tasks {
            let label = format!("#{} {}", t.id, t.title);
            match t.status {
                TaskStatus::Pending => cols[0].push(label),
                TaskStatus::Running => cols[1].push(label),
                TaskStatus::Blocked => cols[2].push(label),
                TaskStatus::Done | TaskStatus::Failed => cols[3].push(label),
            }
        }
        let mut out = String::new();
        out.push_str("Pending        | Running        | Blocked        | Done\n");
        out.push_str("---------------+----------------+----------------+---------------\n");
        let max = cols.iter().map(|c| c.len()).max().unwrap_or(0);
        for i in 0..max {
            let mut row = String::new();
            for c in &cols {
                let cell = c.get(i).cloned().unwrap_or_default();
                row.push_str(&format!("{:<15}|", cell));
            }
            out.push_str(&row);
            out.push('\n');
        }
        out
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for t in &self.tasks {
            if let Some(f) = &self.filter {
                if t.status.label() != f.as_str() && !t.title.contains(f.as_str()) {
                    continue;
                }
            }
            let leases = if t.leased_paths.is_empty() {
                String::new()
            } else {
                format!(" leases: {:?}", t.leased_paths)
            };
            let gate = t.quality_gate.as_deref().unwrap_or("-");
            let note = t.notes.as_deref().and_then(|n| {
                n.lines().find(|l| l.starts_with("reply:")).map(|l| {
                    let r: String = l.get(6..).unwrap_or("").trim().chars().take(57).collect();
                    format!(" [reply: {r}]")
                })
            });
            out.push_str(&format!(
                "[{}] #{} {} (deps: {:?}, gate: {}){}{}\n",
                t.status.label(),
                t.id,
                t.title,
                t.deps,
                gate,
                leases,
                note.unwrap_or_default()
            ));
            if let Some(list) = &t.checklist {
                for (i, c) in list.iter().enumerate() {
                    let mark = if c.done { "[x]" } else { "[ ]" };
                    out.push_str(&format!("       {} {}. {}\n", mark, i + 1, c.label));
                }
            }
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

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn lease_stores_paths() {
        let mut board = TaskBoard::new();
        let id = board.add("t1".into(), vec![], None);
        board
            .lease_paths(id, vec![std::path::PathBuf::from("src/main.rs")])
            .unwrap();
        assert_eq!(
            board.tasks[0].leased_paths,
            vec![std::path::PathBuf::from("src/main.rs")]
        );
    }

    #[test]
    fn kanban_render() {
        let mut board = TaskBoard::new();
        board.add("a".into(), vec![], None);
        let k = board.render_kanban();
        assert!(k.contains("Pending"));
        assert!(k.contains("a"));
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn filter_by_status_and_text() {
        let mut b = TaskBoard::new();
        b.add("alpha work".into(), vec![], None);
        let id2 = b.add("beta setup".into(), vec![], None);
        b.complete_task(id2, true);

        b.filter = Some("done".into());
        let r = b.render();
        assert!(r.contains("beta"), "done task shown: {r}");
        assert!(!r.contains("alpha"), "pending hidden: {r}");

        b.filter = Some("alpha".into());
        let r = b.render();
        assert!(r.contains("alpha"));
        assert!(!r.contains("beta"));

        b.filter = None;
        let r = b.render();
        assert!(r.contains("alpha") && r.contains("beta"));
    }
}

#[cfg(test)]
mod notes_tests {
    use super::*;

    #[test]
    fn add_from_reply_captures_notes_and_trims_title() {
        let mut b = TaskBoard::default();
        let long_prompt = "x".repeat(120);
        let id = b.add_from_reply("Claude #7", &long_prompt, "done, tests added");
        let t = b.tasks.iter().find(|t| t.id == id).unwrap();
        assert!(t.title.ends_with("…") && t.title.chars().count() == 78);
        let notes = t.notes.as_deref().unwrap();
        assert!(notes.starts_with("from: Claude #7"));
        assert!(notes.contains("reply: done, tests added"));
    }

    #[test]
    fn notes_survive_save_load() {
        let dir = std::env::temp_dir().join("term-notes-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut b = TaskBoard::default();
        b.add_from_reply("Codex #2", "write tests", "ok");
        b.save(&dir).unwrap();
        let b2 = TaskBoard::load(&dir);
        assert_eq!(b2.tasks.len(), 1);
        assert!(b2.tasks[0].notes.as_deref().unwrap().contains("reply: ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod checklist_tests {
    use super::*;

    #[test]
    fn bullets_become_checklist() {
        let items = checklist_from_reply("- fix the flaky test\n* add docs\nfinal prose line");
        let list = items.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "fix the flaky test");
        assert_eq!(list[1].label, "add docs");
        assert!(!list[0].done);

        // no bullets → None; empty body skipped
        assert!(checklist_from_reply("plain reply").is_none());
        assert!(checklist_from_reply("-\n* \n-   ").is_none());
    }

    #[test]
    fn add_from_reply_seeds_checklist() {
        let mut b = TaskBoard::default();
        let id = b.add_from_reply(
            "Claude #3",
            "refactor auth",
            "- split module\n- update tests",
        );
        let t = b.tasks.iter().find(|t| t.id == id).unwrap();
        assert_eq!(t.checklist.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn check_toggle_flips_and_validates() {
        let mut b = TaskBoard::default();
        let id = b.add_from_reply("agent", "p", "- one\n- two");
        assert!(b.check_toggle(id, 1).unwrap());
        assert!(!b.check_toggle(id, 1).unwrap());
        assert!(b.check_toggle(id, 2).unwrap());
        assert!(b.check_toggle(999, 1).is_err());
        assert!(b.check_toggle(id, 3).is_err());
        // render shows the marks
        let s = b.render();
        assert!(s.contains("[ ] 1. one"));
        assert!(s.contains("[x] 2. two"));
    }

    #[test]
    fn checklist_survives_persistence() {
        let dir = std::env::temp_dir().join("term-checklist-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut b = TaskBoard::default();
        let id = b.add_from_reply("agent", "p", "- alpha\n- beta");
        let _ = b.check_toggle(id, 2);
        b.save(&dir).unwrap();
        let b2 = TaskBoard::load(&dir);
        let list = b2.tasks[0].checklist.as_ref().unwrap();
        assert!(list[0].label == "alpha" && !list[0].done);
        assert!(list[1].done);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
