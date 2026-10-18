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

pub fn run_module(path: &PathBuf) -> (String, String) {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match Command::new(path).arg("status").output() {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (name, text)
        }
        Err(e) => (name, format!("err: {e}")),
    }
}

pub fn run_all() -> Vec<(String, String)> {
    discover().into_iter().map(|p| run_module(&p)).collect()
}
