//! Self-update command (from Luvus's `luvus update`).
//!
//! Checks GitHub releases for a newer version of term and downloads/installs it.
//! Uses `curl` via std::process::Command (no HTTP crate needed).

use serde::Deserialize;

use anyhow::{Result, anyhow};

/// GitHub release info from the API.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// Result of an update check.
#[derive(Debug)]
pub enum UpdateStatus {
    /// Already on the latest version.
    UpToDate { current: String, latest: String },
    /// A newer version is available.
    UpdateAvailable {
        current: String,
        latest: String,
        asset_url: String,
    },
    /// Error checking for updates.
    Error(String),
}

/// Check for updates against GitHub releases using curl.
pub fn check_update(current_version: &str) -> UpdateStatus {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "https://api.github.com/repos/GrayCodeAI/term/releases/latest",
        ])
        .output();

    let json = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            return UpdateStatus::Error(format!(
                "curl failed (status {}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            ));
        }
        Err(e) => return UpdateStatus::Error(format!("curl not found: {e}")),
    };

    let release: GitHubRelease = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => return UpdateStatus::Error(format!("parse release: {e}")),
    };

    let latest = release.tag_name.trim_start_matches('v');
    if latest == current_version {
        return UpdateStatus::UpToDate {
            current: current_version.to_string(),
            latest: latest.to_string(),
        };
    }

    // Find the linux x86_64 tarball asset.
    let asset_url = release
        .assets
        .iter()
        .find(|a| a.name.contains("x86_64-unknown-linux-gnu") && a.name.ends_with(".tar.gz"))
        .map(|a| a.browser_download_url.clone());

    match asset_url {
        Some(url) => UpdateStatus::UpdateAvailable {
            current: current_version.to_string(),
            latest: latest.to_string(),
            asset_url: url,
        },
        None => UpdateStatus::Error("no linux x86_64 asset found".to_string()),
    }
}

/// Download and install an update.
/// Downloads the tarball with curl, extracts, and replaces the current binary.
pub fn install_update(asset_url: &str) -> Result<()> {
    let tmp_dir = std::env::temp_dir().join("term-update");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| anyhow!("create tmp dir: {e}"))?;

    let tarball = tmp_dir.join("term.tar.gz");

    // Download with curl.
    let download = std::process::Command::new("curl")
        .args(["-fsSL", asset_url, tarball.to_str().unwrap()])
        .output()
        .map_err(|e| anyhow!("curl not found: {e}"))?;
    if !download.status.success() {
        return Err(anyhow!(
            "download failed: {}",
            String::from_utf8_lossy(&download.stderr)
        ));
    }

    // Extract tarball.
    let extract_dir = tmp_dir.join("extract");
    std::fs::create_dir_all(&extract_dir).map_err(|e| anyhow!("create extract dir: {e}"))?;
    let extract = std::process::Command::new("tar")
        .args([
            "-xzf",
            tarball.to_str().unwrap(),
            "-C",
            extract_dir.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| anyhow!("tar not found: {e}"))?;
    if !extract.status.success() {
        return Err(anyhow!(
            "extract failed: {}",
            String::from_utf8_lossy(&extract.stderr)
        ));
    }

    // Find the term binary in the extracted archive.
    let mut term_bin = None;
    for entry in std::fs::read_dir(&extract_dir).map_err(|e| anyhow!("read extract dir: {e}"))? {
        let entry = entry.map_err(|e| anyhow!("read entry: {e}"))?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == "term" || name.starts_with("term-") {
                    term_bin = Some(path);
                    break;
                }
            }
        }
    }

    let new_bin = term_bin.ok_or_else(|| anyhow!("term binary not found in archive"))?;

    // Get current binary path.
    let current_bin = std::env::current_exe().map_err(|e| anyhow!("current exe: {e}"))?;

    // Backup current binary.
    let backup = current_bin.with_extension("bak");
    let _ = std::fs::copy(&current_bin, &backup);

    // Replace current binary with new one.
    std::fs::copy(&new_bin, &current_bin).map_err(|e| anyhow!("install: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&current_bin, std::fs::Permissions::from_mode(0o755));
    }

    // Clean up backup on success.
    let _ = std::fs::remove_file(&backup);

    Ok(())
}

/// Entry point for `term update`. Checks for updates and offers to install.
pub fn run_update(current_version: &str) -> Result<()> {
    match check_update(current_version) {
        UpdateStatus::UpToDate { current, latest } => {
            println!("term {current} is up to date (latest: {latest})");
        }
        UpdateStatus::UpdateAvailable {
            current,
            latest,
            asset_url,
        } => {
            println!("term update available: {current} → {latest}");
            print!("Download and install now? [y/N] ");
            use std::io::{self, Write};
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if input.trim().eq_ignore_ascii_case("y") {
                install_update(&asset_url)?;
                println!("term updated to {latest}. Please restart term.");
            } else {
                println!("Update cancelled.");
            }
        }
        UpdateStatus::Error(msg) => {
            eprintln!("update check failed: {msg}");
            std::process::exit(1);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_status_up_to_date() {
        let status = UpdateStatus::UpToDate {
            current: "0.9.0".to_string(),
            latest: "0.9.0".to_string(),
        };
        match status {
            UpdateStatus::UpToDate { current, latest } => {
                assert_eq!(current, "0.9.0");
                assert_eq!(latest, "0.9.0");
            }
            _ => panic!("expected UpToDate"),
        }
    }

    #[test]
    fn update_status_available() {
        let status = UpdateStatus::UpdateAvailable {
            current: "0.9.0".to_string(),
            latest: "1.0.0".to_string(),
            asset_url: "https://example.com/term.tar.gz".to_string(),
        };
        match status {
            UpdateStatus::UpdateAvailable {
                current, latest, ..
            } => {
                assert_eq!(current, "0.9.0");
                assert_eq!(latest, "1.0.0");
            }
            _ => panic!("expected UpdateAvailable"),
        }
    }
}
