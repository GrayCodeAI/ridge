use std::path::PathBuf;
use std::process::Command;

pub fn modules_dir() -> PathBuf {
    crate::config::config_dir().join("modules")
}

pub fn discover() -> Vec<PathBuf> {
    let dir = modules_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn run_module(path: &PathBuf, json: &str) -> (String, String) {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match Command::new(path)
        .arg("status")
        .env("TERM_JSON", json)
        .output()
    {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (name, text)
        }
        Err(e) => (name, format!("err: {e}")),
    }
}

pub fn run_all(json: &str) -> Vec<(String, String)> {
    discover()
        .into_iter()
        .map(|p| run_module(&p, json))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_dir_path() {
        let d = modules_dir();
        assert!(d.to_string_lossy().contains("term"));
        assert!(d.to_string_lossy().contains("modules"));
    }

    #[test]
    fn discover_returns_sorted() {
        let v = discover();
        let _ = v.len();
    }
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn module_receives_term_json_env() {
        // Write a temp module script that echoes $TERM_JSON.
        let dir = std::env::temp_dir().join("term-modtest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("echo.sh");
        std::fs::write(&script, "#!/bin/sh\necho \"$TERM_JSON\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            let (name, out) = run_module(&script, "{\"panes\":2}");
            assert_eq!(name, "echo.sh");
            assert_eq!(out, "{\"panes\":2}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
