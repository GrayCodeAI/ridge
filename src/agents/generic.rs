use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct GenericAgent;

impl Agent for GenericAgent {
    fn id(&self) -> &'static str {
        "generic"
    }
    fn name(&self) -> &'static str {
        "Generic Agent"
    }
    fn detect(&self, _process_name: &str, _cmdline: &str) -> bool {
        // Fallback: matches anything. Registered last so specific agents win.
        true
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        // Default to a shell; user overrides via config later.
        let mut cmd = CommandBuilder::new("bash");
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
        if t.contains("(y/n)") || t.contains("permission") {
            return AgentStatus::Blocked;
        }
        if t.trim_end().ends_with("$") || t.trim_end().ends_with(">") {
            return AgentStatus::Idle;
        }
        AgentStatus::Unknown
    }
    fn parse_cost(&self, _session_file: &Path) -> Option<CostInfo> {
        None
    }
}
EOF