use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct CopilotAgent;

impl Agent for CopilotAgent {
    /// Executable name for doctor / PATH checks (None = not a real binary).
    fn exe(&self) -> Option<&'static str> {
        Some("copilot")
    }
    fn id(&self) -> &'static str {
        "copilot"
    }
    fn name(&self) -> &'static str {
        "GitHub Copilot"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("copilot") || c.contains("copilot") || c.contains("gh copilot")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("gh");
        cmd.arg("copilot");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, _session_id: &str, _cwd: &Path) -> Option<CommandBuilder> {
        None
    }
    fn session_files(&self, _cwd: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let t = output_tail.to_lowercase();
        if t.contains("permission") || t.contains("(y/n)") {
            return AgentStatus::Blocked;
        }
        if t.trim_end().ends_with(">") || t.trim_end().ends_with("$") {
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
