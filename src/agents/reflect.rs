use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct ReflectAgent;

impl Agent for ReflectAgent {
    fn exe(&self) -> Option<&'static str> {
        Some("reflect")
    }
    fn id(&self) -> &'static str {
        "reflect"
    }
    fn name(&self) -> &'static str {
        "Reflect"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let process = Path::new(process_name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let command = cmdline.split_whitespace().next().unwrap_or("");
        process.eq_ignore_ascii_case("reflect")
            || Path::new(command)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("reflect"))
                .unwrap_or(false)
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("reflect");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = CommandBuilder::new("reflect");
        if session_id == "last" {
            cmd.arg("--continue");
        } else {
            cmd.arg("--resume");
            cmd.arg(session_id);
        }
        cmd.cwd(cwd);
        Some(cmd)
    }
    fn session_files(&self, _cwd: &Path) -> Vec<PathBuf> {
        let base = std::env::var_os("REFLECT_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_STATE_HOME").map(|p| PathBuf::from(p).join("reflect"))
            })
            .or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(|p| PathBuf::from(p).join("reflect").join("state"))
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|p| {
                    PathBuf::from(p)
                        .join(".config")
                        .join("reflect")
                        .join("state")
                })
            });
        let Some(dir) = base.map(|p| p.join("sessions")) else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|s| s == "jsonl").unwrap_or(false))
            .collect()
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let tail = output_tail.to_lowercase();
        if tail.contains("allow this") || tail.contains("permission required") {
            return AgentStatus::Blocked;
        }
        if tail.contains("ready for review") || tail.contains("to resume this session") {
            return AgentStatus::Done;
        }
        if tail.contains("verifying commit") || tail.contains("thinking") {
            return AgentStatus::Working;
        }
        AgentStatus::Unknown
    }
    fn parse_cost(&self, _session_file: &Path) -> Option<CostInfo> {
        None
    }
    fn default_resume_arg(&self) -> Option<&'static str> {
        Some("last")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_only_reflect_process() {
        let a = ReflectAgent;
        assert!(a.detect("reflect", "reflect work --request hello"));
        assert!(a.detect("/usr/local/bin/reflect", "/usr/local/bin/reflect"));
        assert!(!a.detect("bash", "echo reflect"));
    }
}
