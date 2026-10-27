//! Local software-factory bridge. Reflect owns the workflow and status contract;
//! Ridge starts it in a pane and displays its one-line status.

use std::path::{Path, PathBuf};
use std::process::Command;

const REPOSITORIES: &[&str] = &[
    "reflect",
    "relay",
    "resolve",
    "record",
    "recall",
    "rally",
    "rivet",
    "rein",
    "ridge",
    "graycode-platform",
    "graycode-logo",
];

pub fn is_workspace_root(path: &Path) -> bool {
    path.join("reflect/go.mod").is_file() && path.join("ridge/Cargo.toml").is_file()
}

pub fn target(repo: &Path, input: &str) -> Result<(PathBuf, String), String> {
    if !is_workspace_root(repo) {
        return Ok((repo.to_path_buf(), input.trim().to_string()));
    }
    let mut parts = input.trim().splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("");
    let request = parts.next().unwrap_or("").trim();
    if !REPOSITORIES.contains(&name) || request.is_empty() {
        return Err(
            "from the workspace, use: factory run <repo> <request> or factory ticket <repo> <id>"
                .to_string(),
        );
    }
    Ok((repo.join(name), request.to_string()))
}

pub fn summary(repo: &Path) -> Result<String, String> {
    let mut command = Command::new("reflect");
    command.arg("factory");
    if is_workspace_root(repo) {
        command.args(["workspace", "summary", "--root"]);
    } else {
        command.args(["summary", "--repo"]);
    }
    let output = command
        .arg(repo)
        .output()
        .map_err(|e| format!("start reflect factory: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_target_requires_known_repo_and_request() {
        let root = std::env::temp_dir().join(format!("ridge-factory-{}", std::process::id()));
        std::fs::create_dir_all(root.join("reflect")).unwrap();
        std::fs::create_dir_all(root.join("ridge")).unwrap();
        std::fs::write(root.join("reflect/go.mod"), "module example.com/reflect\n").unwrap();
        std::fs::write(root.join("ridge/Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(
            target(&root, "relay fix this").unwrap(),
            (root.join("relay"), "fix this".to_string())
        );
        assert!(target(&root, "unknown fix this").is_err());
        assert!(target(&root, "relay").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
