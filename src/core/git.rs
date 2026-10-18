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
