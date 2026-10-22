use std::path::Path;
use std::process::Command;

pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    match Command::new("git").args(args).current_dir(cwd).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            if s.is_empty() {
                s = String::from_utf8_lossy(&o.stderr).to_string();
            }
            s
        }
        Err(e) => format!("git error: {e}"),
    }
}

pub fn current_branch(cwd: &Path) -> String {
    run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string()
}

pub fn status_porcelain(cwd: &Path) -> String {
    run_git(cwd, &["status", "--porcelain"])
}

pub fn log_oneline(cwd: &Path, n: usize) -> String {
    run_git(cwd, &["log", "--oneline", "-n", &n.to_string()])
}

pub fn diff_worktree(cwd: &Path) -> String {
    run_git(cwd, &["diff"])
}

pub fn diff_staged(cwd: &Path) -> String {
    run_git(cwd, &["diff", "--cached"])
}

pub fn diff_stat(cwd: &Path) -> String {
    run_git(cwd, &["diff", "--stat"])
}

pub fn branches(cwd: &Path) -> String {
    run_git(cwd, &["branch", "-a"])
}

pub fn untracked(cwd: &Path) -> String {
    run_git(cwd, &["ls-files", "--others", "--exclude-standard"])
}

pub fn conflicted(cwd: &Path) -> String {
    run_git(cwd, &["diff", "--name-only", "--diff-filter=U"])
}

pub fn repo_root(cwd: &Path) -> Option<std::path::PathBuf> {
    let out = run_git(cwd, &["rev-parse", "--show-toplevel"]);
    let s = out.trim();
    if s.is_empty() || s.contains("error") {
        None
    } else {
        Some(std::path::PathBuf::from(s))
    }
}

pub fn gh_prs(cwd: &Path) -> String {
    match Command::new("gh")
        .args(["pr", "list", "--limit", "20"])
        .current_dir(cwd)
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => "gh CLI not available".to_string(),
    }
}

pub fn gh_issues(cwd: &Path) -> String {
    match Command::new("gh")
        .args(["issue", "list", "--limit", "20"])
        .current_dir(cwd)
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => "gh CLI not available".to_string(),
    }
}

/// Recent commit history with ASCII graph arrows (git log --graph --oneline).
pub fn log_graph(cwd: &Path, n: usize) -> String {
    let n = format!("-{n}");
    run_git(cwd, &["log", "--graph", "--oneline", "--all", &n])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn run_git_in_temp_repo() {
        let dir = std::env::temp_dir().join("term-test-repo");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&dir)
            .output()
            .unwrap();

        // just check it doesn't panic and returns something
        let b = current_branch(&dir);
        assert!(!b.is_empty());
        let _ = status_porcelain(&dir);
        let _ = log_oneline(&dir, 5);
        let _ = branches(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
