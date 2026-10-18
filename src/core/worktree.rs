use std::path::{Path, PathBuf};
use std::process::Command;

pub fn create_worktree(repo: &Path, branch: &str, path: &Path) -> Result<(), String> {
    // Reject path traversal / suspicious branch names.
    if branch.contains("..")
        || branch.contains('/') && branch.split('/').any(|s| s == ".." || s.is_empty())
    {
        return Err("invalid branch name".into());
    }
    let path_str = path.to_string_lossy();
    if path_str.contains("..") {
        return Err("invalid worktree path".into());
    }
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

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_branch() {
        let res = create_worktree(Path::new("/tmp"), "../evil", Path::new("/tmp/wt"));
        assert_eq!(res.unwrap_err(), "invalid branch name");
    }

    #[test]
    fn rejects_empty_branch_segment() {
        let res = create_worktree(Path::new("/tmp"), "foo//bar", Path::new("/tmp/wt"));
        assert_eq!(res.unwrap_err(), "invalid branch name");
    }

    #[test]
    fn rejects_traversal_path() {
        let res = create_worktree(Path::new("/tmp"), "feature", Path::new("/tmp/../evil"));
        assert_eq!(res.unwrap_err(), "invalid worktree path");
    }
}
