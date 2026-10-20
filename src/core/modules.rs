//! Module system — Luvus-inspired extensibility for Term.
//!
//! Modules are executables or scripts dropped into
//! `~/.config/term/modules/`. Each module can declare a
//! `term-module.toml` manifest with actions, startup hooks, events,
//! settings, and bar widgets. Modules receive `$TERM_JSON` (live app state)
//! on every tick and can register actions invokable from the commander or
//! command mode.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn modules_dir() -> PathBuf {
    crate::config::config_dir().join("modules")
}

/// Discover all module entries in the modules directory.
pub fn discover() -> Vec<PathBuf> {
    let dir = modules_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() || p.is_dir() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Run a module script with the `status` command and `$TERM_JSON` env.
/// Returns (module_name, output_text).
pub fn run_module(path: &PathBuf, json: &str) -> (String, String) {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match std::process::Command::new(path)
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

/// Run all discovered modules.
pub fn run_all(json: &str) -> Vec<(String, String)> {
    discover()
        .into_iter()
        .map(|p| run_module(&p, json))
        .collect()
}

/// `term-module.toml` manifest — declarative module definition.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
    /// Startup hooks: commands run once when the module is loaded.
    #[serde(default)]
    pub startup: Vec<StartupHook>,
    /// Actions: named commands the module registers for UI invocation.
    #[serde(default)]
    pub actions: Vec<ModuleAction>,
    /// Event hooks: commands run when app events fire.
    #[serde(default)]
    pub events: Vec<EventHook>,
    /// Sidebar dock entries contributed by this module.
    #[serde(default)]
    pub docks: Vec<DockEntry>,
    /// Bar widget entries contributed by this module.
    #[serde(default)]
    pub bars: Vec<BarWidgetEntry>,
    /// User-editable settings rendered in Settings → Modules.
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupHook {
    pub command: Vec<String>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleAction {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHook {
    pub event: String, // e.g. "pane_spawn", "agent_blocked", "task_done"
    pub command: Vec<String>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub command: Vec<String>, // command that produces dock content
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarWidgetEntry {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub command: Vec<String>, // command that produces bar widget text
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub r#type: String, // "string", "bool", "number"
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// Load a module manifest from a directory (looks for term-module.toml).
pub fn load_manifest(module_dir: &Path) -> Option<ModuleManifest> {
    let manifest_path = module_dir.join("term-module.toml");
    if !manifest_path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&manifest_path).ok()?;
    toml::from_str(&raw).ok()
}

/// Run a module startup hook command.
pub fn run_startup_hook(hook: &StartupHook) -> Result<(), String> {
    if !hook_platform_ok(&hook.platforms) {
        return Ok(());
    }
    let output = std::process::Command::new(&hook.command[0])
        .args(&hook.command[1..])
        .output()
        .map_err(|e| format!("startup hook failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "startup hook exited {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Run a module action command.
pub fn run_action(action: &ModuleAction) -> Result<(), String> {
    if !hook_platform_ok(&action.platforms) {
        return Ok(());
    }
    let output = std::process::Command::new(&action.command[0])
        .args(&action.command[1..])
        .output()
        .map_err(|e| format!("action '{}' failed: {}", action.id, e))?;
    if !output.status.success() {
        return Err(format!(
            "action '{}' exited {}",
            action.id,
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Fire an event hook: run all hooks registered for the given event name.
pub fn fire_event(event: &str, hooks: &[EventHook]) -> Vec<Result<(), String>> {
    hooks
        .iter()
        .filter(|h| h.event == event && hook_platform_ok(&h.platforms))
        .map(|h| {
            std::process::Command::new(&h.command[0])
                .args(&h.command[1..])
                .output()
                .map(|_| ())
                .map_err(|e| format!("event '{}' hook failed: {}", event, e))
        })
        .collect()
}

/// Check if the current platform matches the module's platform filter.
fn hook_platform_ok(platforms: &Option<Vec<String>>) -> bool {
    match platforms {
        None => true,
        Some(p) => p.iter().any(|plat| {
            let p = plat.to_lowercase();
            p == "linux" && cfg!(target_os = "linux")
                || p == "macos" && cfg!(target_os = "macos")
                || p == "windows" && cfg!(target_os = "windows")
                || p == "freebsd" && cfg!(target_os = "freebsd")
        }),
    }
}

/// Registered module actions available for UI invocation.
#[derive(Debug, Clone, Default)]
pub struct ModuleRegistry {
    pub actions: BTreeMap<String, ModuleAction>,
    pub hooks: Vec<EventHook>,
    pub docks: Vec<DockEntry>,
    pub bars: Vec<BarWidgetEntry>,
    pub settings: Vec<SettingSpec>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register all manifests from discovered module directories.
    pub fn register_all(&mut self) {
        for path in discover() {
            if path.is_dir() {
                if let Some(manifest) = load_manifest(&path) {
                    for action in &manifest.actions {
                        self.actions.insert(action.id.clone(), action.clone());
                    }
                    self.hooks.extend(manifest.events.clone());
                    self.docks.extend(manifest.docks.clone());
                    self.bars.extend(manifest.bars.clone());
                    self.settings.extend(manifest.settings.clone());
                }
            }
        }
    }

    /// Run a registered action by id.
    pub fn run_action(&self, id: &str) -> Result<(), String> {
        self.actions
            .get(id)
            .ok_or_else(|| format!("unknown module action: {id}"))
            .and_then(run_action)
    }
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

    #[test]
    fn manifest_default_roundtrip() {
        let m = ModuleManifest::default();
        let s = toml::to_string(&m).unwrap();
        let parsed: ModuleManifest = toml::from_str(&s).unwrap();
        assert_eq!(parsed.id, m.id);
    }

    #[test]
    fn registry_register_and_run() {
        let mut reg = ModuleRegistry::new();
        reg.register_all();
        // Should not panic even with no modules installed.
        let _ = reg.run_action("nonexistent");
    }

    #[test]
    fn platform_filter_linux() {
        let hook = StartupHook {
            command: vec!["true".to_string()],
            platforms: Some(vec!["linux".to_string()]),
        };
        assert!(hook_platform_ok(&Some(vec!["linux".to_string()])));
        assert!(!hook_platform_ok(&Some(vec!["windows".to_string()])));
    }
}
