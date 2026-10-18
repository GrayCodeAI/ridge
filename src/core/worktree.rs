use std::path::{Path, PathBuf};
use std::process::Command;

pub fn create_worktree(repo: &Path, branch: &str, path: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .args(["worktree", "add", "-b", branch])
        .arg(path)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

pub fn list_worktrees(repo: &Path) -> String {
    match Command::new("git")
        .args(["worktree", "list"])
        .current_dir(repo)
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("error: {e}"),
    }
}

pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .args(["worktree", "remove"])
        .arg(path)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

pub fn worktree_base(repo: &Path) -> PathBuf {
    repo.join(".term-worktrees")
}
