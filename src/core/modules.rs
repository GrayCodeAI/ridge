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
    /// Optional worktree provider: modules can provide custom worktree
    /// creation and removal (Luvus worktree provider protocol).
    #[serde(default)]
    pub worktree_provider: Option<WorktreeProvider>,
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

/// Optional worktree provider declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeProvider {
    /// Fixed argv for creation requests.
    pub command: Vec<String>,
    /// Optional fixed argv for explicit removal requests.
    #[serde(default)]
    pub remove_command: Option<Vec<String>>,
    /// When true, the remove command binds confirmed UI deletion to expected_identity.
    #[serde(default)]
    pub identity_bound_remove: bool,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

/// Worktree create request sent to module provider via stdin JSON.
#[derive(Debug, Serialize)]
pub struct WorktreeCreateRequest {
    pub op: String, // "create"
    pub branch: String,
    pub path: String,
    pub base: String,
}

/// Worktree remove request sent to module provider via stdin JSON.
#[derive(Debug, Serialize)]
pub struct WorktreeRemoveRequest {
    pub op: String, // "remove"
    pub path: String,
    pub identity: Option<String>,
}

/// Worktree provider response received on stdout JSON.
#[derive(Debug, Deserialize)]
pub struct WorktreeProviderResponse {
    pub ok: bool,
    pub path: Option<String>,
    pub error: Option<String>,
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

/// Run a module worktree create provider. Sends JSON request to stdin,
/// parses JSON response from stdout.
pub fn run_worktree_create(
    provider: &WorktreeProvider,
    branch: &str,
    path: &Path,
    base: &Path,
) -> Result<bool, String> {
    if !hook_platform_ok(&provider.platforms) {
        return Ok(false);
    }
    let req = WorktreeCreateRequest {
        op: "create".to_string(),
        branch: branch.to_string(),
        path: path.to_string_lossy().to_string(),
        base: base.to_string_lossy().to_string(),
    };
    let req_json = serde_json::to_string(&req).map_err(|e| format!("serialize request: {e}"))?;
    let mut child = std::process::Command::new(&provider.command[0])
        .args(&provider.command[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn worktree provider: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        std::io::Write::write_all(&mut stdin, req_json.as_bytes())
            .map_err(|e| format!("write request: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("wait provider: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "worktree create provider failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let resp: WorktreeProviderResponse =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse response: {e}"))?;
    Ok(resp.ok)
}

/// Run a module worktree remove provider.
pub fn run_worktree_remove(
    provider: &WorktreeProvider,
    path: &Path,
    identity: Option<&str>,
) -> Result<bool, String> {
    if !hook_platform_ok(&provider.platforms) {
        return Ok(false);
    }
    let Some(remove_cmd) = &provider.remove_command else {
        return Err("module has no remove command".to_string());
    };
    let req = WorktreeRemoveRequest {
        op: "remove".to_string(),
        path: path.to_string_lossy().to_string(),
        identity: identity.map(|s| s.to_string()),
    };
    let req_json = serde_json::to_string(&req).map_err(|e| format!("serialize request: {e}"))?;
    let mut child = std::process::Command::new(&remove_cmd[0])
        .args(&remove_cmd[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn worktree remove provider: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        std::io::Write::write_all(&mut stdin, req_json.as_bytes())
            .map_err(|e| format!("write request: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("wait provider: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "worktree remove provider failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let resp: WorktreeProviderResponse =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse response: {e}"))?;
    Ok(resp.ok)
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

/// Registered worktree providers from discovered modules.
#[derive(Debug, Clone, Default)]
pub struct WorktreeProviderRegistry {
    pub providers: BTreeMap<String, WorktreeProvider>,
}

impl WorktreeProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register worktree providers from all discovered module manifests.
    pub fn register_all(&mut self) {
        for path in discover() {
            if path.is_dir() {
                if let Some(manifest) = load_manifest(&path) {
                    if let Some(provider) = &manifest.worktree_provider {
                        self.providers.insert(manifest.id.clone(), provider.clone());
                    }
                }
            }
        }
    }

    /// Run create via the named provider.
    pub fn run_create(
        &self,
        provider_id: &str,
        branch: &str,
        path: &Path,
        base: &Path,
    ) -> Result<bool, String> {
        self.providers
            .get(provider_id)
            .ok_or_else(|| format!("unknown worktree provider: {provider_id}"))
            .and_then(|p| run_worktree_create(p, branch, path, base))
    }

    /// Run remove via the named provider.
    pub fn run_remove(
        &self,
        provider_id: &str,
        path: &Path,
        identity: Option<&str>,
    ) -> Result<bool, String> {
        self.providers
            .get(provider_id)
            .ok_or_else(|| format!("unknown worktree provider: {provider_id}"))
            .and_then(|p| run_worktree_remove(p, path, identity))
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
    fn platform_filter_linux() {
        let hook = StartupHook {
            command: vec!["true".to_string()],
            platforms: Some(vec!["linux".to_string()]),
        };
        assert!(hook_platform_ok(&Some(vec!["linux".to_string()])));
        assert!(!hook_platform_ok(&Some(vec!["windows".to_string()])));
    }

    #[test]
    fn registry_register_and_run() {
        let mut reg = ModuleRegistry::new();
        reg.register_all();
        // Should not panic even with no modules installed.
        let _ = reg.run_action("nonexistent");
    }

    #[test]
    fn worktree_provider_registry_register() {
        let mut reg = WorktreeProviderRegistry::new();
        reg.register_all();
        // Should not panic even with no providers installed.
        let _ = reg.run_create("nonexistent", "branch", Path::new("/tmp"), Path::new("/tmp"));
    }
}
