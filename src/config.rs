use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub agent: String,
    pub name: String,
    pub env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: String,
    pub language: String,
    pub keybindings: Keybindings,
    pub accounts: Vec<Account>,
    pub active_account: std::collections::BTreeMap<String, String>,
    /// Fire OS notifications when agents block or finish (needs
    /// notify-send on Linux / osascript on macOS).
    #[serde(default = "default_notify")]
    pub notify: bool,
}

/// Old configs (missing the key) treat notifications as enabled.
fn default_notify() -> bool {
    true
}

fn default_language() -> String {
    "en".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: String::new(),
            language: default_language(),
            keybindings: Keybindings::default(),
            accounts: Vec::new(),
            active_account: std::collections::BTreeMap::new(),
            notify: true,
        }
    }
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
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from(".config")
    };
    prefer_existing_dir(base)
}

pub fn config_file() -> PathBuf {
    let dir = config_dir();
    let current = dir.join("ridge.toml");
    let legacy = dir.join("term.toml");
    if !current.exists() && legacy.exists() {
        legacy
    } else {
        current
    }
}

pub fn data_dir() -> PathBuf {
    let base = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        PathBuf::from(".local").join("share")
    };
    prefer_existing_dir(base)
}

fn prefer_existing_dir(base: PathBuf) -> PathBuf {
    let current = base.join("ridge");
    let legacy = base.join("term");
    if !current.exists() && legacy.is_dir() {
        legacy
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_roundtrip() {
        let cfg = Config::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&s).unwrap();
        assert_eq!(parsed.theme, cfg.theme);
    }

    #[test]
    fn legacy_directory_is_used_until_ridge_directory_exists() {
        let base = std::env::temp_dir().join(format!("ridge-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let legacy = base.join("term");
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(prefer_existing_dir(base.clone()), legacy);
        let current = base.join("ridge");
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(prefer_existing_dir(base.clone()), current);
        std::fs::remove_dir_all(base).unwrap();
    }
}
