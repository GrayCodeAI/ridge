//! Local software-factory bridge. Reflect owns the workflow and status contract;
//! Ridge starts it in a pane and displays its one-line status.

use std::path::Path;
use std::process::Command;

pub fn summary(repo: &Path) -> Result<String, String> {
    let output = Command::new("reflect")
        .args(["factory", "summary", "--repo"])
        .arg(repo)
        .output()
        .map_err(|e| format!("start reflect factory: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
