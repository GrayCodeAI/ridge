//! Remote machine profiles (from Herdr/Luvus).
//!
//! Save SSH machine profiles and attach to remote workspaces over plain SSH.
//! Profiles are persisted to data_dir/machines.json.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A saved SSH machine profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub ssh_key: Option<PathBuf>,
    /// Named session on the remote machine to attach to.
    pub session: Option<String>,
}

/// Path to the machine profiles file.
pub fn profiles_path() -> PathBuf {
    crate::config::data_dir().join("machines.json")
}

/// Managed machine profiles with persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MachineManager {
    pub machines: BTreeMap<String, MachineProfile>,
    pub active_machine: Option<String>,
}

impl MachineManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load machines from data_dir/machines.json.
    pub fn load() -> Self {
        let path = profiles_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save machines to data_dir/machines.json.
    pub fn save(&self) -> Result<(), String> {
        let path = profiles_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn add_machine(&mut self, profile: MachineProfile) {
        self.machines.insert(profile.id.clone(), profile);
    }

    pub fn remove_machine(&mut self, id: &str) -> bool {
        self.machines.remove(id).is_some()
    }

    pub fn list_machines(&self) -> Vec<&MachineProfile> {
        self.machines.values().collect()
    }

    pub fn get_machine(&self, id: &str) -> Option<&MachineProfile> {
        self.machines.get(id)
    }

    pub fn set_active(&mut self, id: &str) -> bool {
        if self.machines.contains_key(id) {
            self.active_machine = Some(id.to_string());
            true
        } else {
            false
        }
    }

    /// Build an SSH command string for this machine.
    pub fn ssh_command(&self, id: &str) -> Option<String> {
        self.machines.get(id).map(|m| {
            let mut cmd = format!("ssh {}@{} -p {}", m.user, m.host, m.port);
            if let Some(key) = &m.ssh_key {
                cmd.push_str(&format!(" -i {}", key.display()));
            }
            if let Some(session) = &m.session {
                cmd.push_str(&format!(" -t 'term --attach-session {}'", session));
            }
            cmd
        })
    }

    /// Attach to a machine via SSH. Spawns an SSH process and waits for it.
    pub fn attach(&self, id: &str) -> Result<(), String> {
        let profile = self
            .machines
            .get(id)
            .ok_or_else(|| format!("machine not found: {id}"))?;

        let mut cmd = std::process::Command::new("ssh");
        cmd.arg(format!("{}@{}", profile.user, profile.host))
            .arg(format!("-p{}", profile.port));
        if let Some(key) = &profile.ssh_key {
            cmd.arg("-i").arg(key);
        }
        if let Some(session) = &profile.session {
            cmd.arg("-t")
                .arg(format!("term --attach-session {session}"));
        }

        let status = cmd.status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ssh exited with status: {status}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_add_and_list() {
        let mut mgr = MachineManager::new();
        mgr.add_machine(MachineProfile {
            id: "prod".to_string(),
            name: "Production".to_string(),
            host: "prod.example.com".to_string(),
            port: 22,
            user: "ubuntu".to_string(),
            ssh_key: None,
            session: None,
        });
        assert_eq!(mgr.list_machines().len(), 1);
        assert!(mgr.get_machine("prod").is_some());
        assert!(mgr.get_machine("nonexistent").is_none());
    }

    #[test]
    fn machine_ssh_command() {
        let mut mgr = MachineManager::new();
        mgr.add_machine(MachineProfile {
            id: "prod".to_string(),
            name: "Production".to_string(),
            host: "prod.example.com".to_string(),
            port: 2222,
            user: "root".to_string(),
            ssh_key: Some(PathBuf::from("~/.ssh/id_ed25519")),
            session: Some("my-session".to_string()),
        });
        let cmd = mgr.ssh_command("prod").unwrap();
        assert!(cmd.contains("root@prod.example.com"));
        assert!(cmd.contains("-p 2222"));
        assert!(cmd.contains("-i ~/.ssh/id_ed25519"));
    }

    #[test]
    fn machine_save_load_roundtrip() {
        let mut mgr = MachineManager::new();
        mgr.add_machine(MachineProfile {
            id: "prod".to_string(),
            name: "Production".to_string(),
            host: "prod.example.com".to_string(),
            port: 22,
            user: "ubuntu".to_string(),
            ssh_key: None,
            session: None,
        });
        mgr.set_active("prod");
        let json = serde_json::to_string(&mgr).unwrap();
        let loaded: MachineManager = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.list_machines().len(), 1);
        assert_eq!(loaded.active_machine, Some("prod".to_string()));
    }
}
