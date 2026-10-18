use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct AiderAgent;

impl Agent for AiderAgent {
    fn id(&self) -> &'static str {
        "aider"
    }
    fn name(&self) -> &'static str {
        "Aider"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("aider") || c.contains("aider")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("aider");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, _session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = CommandBuilder::new("aider");
        cmd.arg("--restore-chat-history");
        cmd.cwd(cwd);
        Some(cmd)
    }
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(cwd) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(".aider"))
                    .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
        out
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let t = output_tail.to_lowercase();
        if t.contains("(y/n)") || t.contains("run shell command") {
            return AgentStatus::Blocked;
        }
        if t.trim_end().ends_with(">") {
            return AgentStatus::Idle;
        }
        if t.contains("thinking") || t.contains("...") {
            return AgentStatus::Working;
        }
        AgentStatus::Unknown
    }
    fn parse_cost(&self, _session_file: &Path) -> Option<CostInfo> {
        None
    }
}
