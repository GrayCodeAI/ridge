//! `term --doctor`: environment sanity checks (agent binaries on PATH, git,
//! gh, writable data dir) printed as a plain report, then exit.

use crate::core::agent::AgentRegistry;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Locate `exe` scanning a PATH-style variable value.
pub fn find_in_path_in(exe: &str, path_var: &OsStr) -> Option<PathBuf> {
    for dir in std::env::split_paths(path_var) {
        let candidate = dir.join(exe);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    let Ok(md) = p.symlink_metadata() else {
        return false;
    };
    if !md.is_file() {
        // on unix, follow: symlinks are resolved by the join probe anyway
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        (md.permissions().mode() & 0o111) != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    /// Concrete copy-paste fix for a failing check (None = nothing to fix).
    pub fix: Option<String>,
}

/// Copy-paste install line for a missing CLI (best-effort, popular channels).
pub fn install_hint(exe: &str) -> Option<&'static str> {
    Some(match exe {
        "claude" => "npm install -g @anthropic-ai/claude-code",
        "codex" => "npm install -g @openai/codex",
        "opencode" => "npm install -g opencode-ai",
        "gemini" => "npm install -g @google/gemini-cli",
        "copilot" => "npm install -g @github/copilot  (or: gh extension install github/gh-copilot)",
        "aider" => "python -m pip install aider-install && aider-install",
        "kimi" => "npm install -g @moonshot-ai/kimi-cli",
        "pi" => "npm install -g pi-cli-ai",
        "grok" => "npm install -g @vibe-kit/grok-cli",
        "cursor-agent" => "curl -fsSL https://cursor.com/install | sh",
        "git" => "apt install git · brew install git · winget install Git.Git",
        "gh" => "apt install gh · brew install gh · winget install GitHub.cli",
        _ => return None,
    })
}

/// Run all checks against a registry.
pub fn run_checks(registry: &AgentRegistry) -> Vec<Check> {
    let mut out = Vec::new();
    for a in registry.all() {
        let Some(exe) = a.exe() else {
            continue; // template/ssh agents have no binary
        };
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        match find_in_path_in(exe, &path_var) {
            Some(p) => out.push(Check {
                name: a.name().to_string(),
                ok: true,
                detail: format!("{exe} found at {}", p.display()),
                fix: None,
            }),
            None => out.push(Check {
                name: a.name().to_string(),
                ok: false,
                detail: format!("{exe} NOT found on PATH"),
                fix: install_hint(exe).map(|s| s.to_string()),
            }),
        }
    }

    for (name, cmd, required) in [("git", "git", true), ("gh", "gh", false)] {
        let ok = std::process::Command::new(cmd)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let detail = if ok {
            format!("{cmd} available")
        } else if required {
            "⚠ required for diff review, worktrees — install git".to_string()
        } else {
            "optional — used for PR/issue views".to_string()
        };
        out.push(Check {
            name: name.to_string(),
            ok,
            detail,
            fix: if ok {
                None
            } else {
                install_hint(cmd).map(|s| s.to_string())
            },
        });
    }

    let data = crate::config::data_dir();
    let dir_ok = std::fs::create_dir_all(&data).is_ok();
    out.push(Check {
        name: "data dir".to_string(),
        ok: dir_ok,
        detail: format!("{}", data.display()),
        fix: if dir_ok {
            None
        } else {
            Some("set XDG_DATA_HOME to a writable location, or check disk/permissions".to_string())
        },
    });
    out
}

/// CLI entry: print the report; with `fix`, add copy-paste suggestions.
pub fn run_cli_checks(with_fix: bool) {
    println!("term doctor — environment checks\n");
    let registry = AgentRegistry::new();
    let checks = run_checks(&registry);
    let mut missing = 0usize;
    for c in &checks {
        let mark = if c.ok { "✓" } else { "✗" };
        if !c.ok {
            missing += 1;
        }
        println!("  {mark} {:<12} {}", c.name, c.detail);
    }
    println!();
    if missing == 0 {
        println!("all good ✓ — agents, tools, and data dir are ready");
    } else {
        println!(
            "{missing} check{} need{} attention above; Term still runs with the rest",
            if missing == 1 { "" } else { "s" },
            if missing == 1 { "s" } else { "" }
        );
        if with_fix {
            println!("\nsuggested fixes:");
            for c in &checks {
                if let (false, Some(hint)) = (c.ok, &c.fix) {
                    println!("  → [{}] {hint}", c.name);
                }
            }
        } else {
            println!("hint: rerun with `term --doctor --fix` for copy-paste install steps,");
            println!("      or use `spawn generic` / shell panes for missing agents.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn finds_executable_in_path_in() {
        let dir = std::env::temp_dir().join("term-doctor-bin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-agent");
        std::fs::write(&fake, "#!/bin/sh\ntrue\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let path = std::ffi::OsString::from(dir.clone());
        assert_eq!(find_in_path_in("fake-agent", &path), Some(fake.clone()));

        // non-executable file must not match
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(find_in_path_in("fake-agent", &path), None);
        // arbitrary missing exe
        assert_eq!(find_in_path_in("surely-not-installed-xyz", &path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_checks_lists_tools_and_agents() {
        let registry = AgentRegistry::new();
        let checks = run_checks(&registry);
        // 10 real agents + git + gh + data dir (generic has no exe → skipped)
        assert_eq!(checks.len(), 10 + 3);
        assert!(checks.iter().any(|c| c.name == "git"));
        let git = checks.iter().find(|c| c.name == "git").unwrap();
        assert!(git.ok, "git is required in CI and dev environments");
        // generic (no exe) is skipped
        assert!(!checks.iter().any(|c| c.name == "Generic"));
    }

    #[test]
    fn install_hint_matches_known_exes() {
        assert_eq!(
            install_hint("claude"),
            Some("npm install -g @anthropic-ai/claude-code")
        );
        assert_eq!(
            install_hint("cursor-agent"),
            Some("curl -fsSL https://cursor.com/install | sh")
        );
        assert!(install_hint("generic").is_none());
        assert!(install_hint("definitely-unknown-xyz").is_none());
    }

    #[test]
    fn failed_checks_carry_fix_hints() {
        let registry = AgentRegistry::new();
        let checks = run_checks(&registry);
        for c in &checks {
            if c.ok {
                assert!(
                    c.fix.is_none(),
                    "passing check should have no fix: {}",
                    c.name
                );
                continue;
            }
            match c.name.as_str() {
                "data dir" => assert!(c.fix.is_some()),
                "git" | "gh" => assert_eq!(c.fix.as_deref(), install_hint(c.name.as_str())),
                _ => {
                    // agent check: detail starts with the exe name
                    let exe = c.detail.split_whitespace().next().unwrap_or("");
                    assert_eq!(
                        c.fix.as_deref(),
                        install_hint(exe),
                        "hint mismatch for {}",
                        c.name
                    );
                }
            }
        }
    }
}
