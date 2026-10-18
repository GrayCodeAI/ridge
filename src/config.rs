use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub agent: String,
    pub name: String,
    pub env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub theme: String,
    pub keybindings: Keybindings,
    pub accounts: Vec<Account>,
    pub active_account: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    pub prefix: String,
    pub quit: String,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            prefix: "Ctrl+Space".to_string(),
            quit: "q".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_file();
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(&path)?;
        let cfg: Self = toml::from_str(&raw).unwrap_or_default();
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("term");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("term");
    }
    PathBuf::from(".config").join("term")
}

pub fn config_file() -> PathBuf {
    config_dir().join("term.toml")
}

pub fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("term");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("term");
    }
    PathBuf::from(".local").join("share").join("term")
}
