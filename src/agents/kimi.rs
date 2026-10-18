use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct KimiAgent;

impl Agent for KimiAgent {
    fn id(&self) -> &'static str {
        "kimi"
    }
    fn name(&self) -> &'static str {
        "Kimi"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("kimi") || c.contains("kimi")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("kimi");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = CommandBuilder::new("kimi");
        cmd.arg("--session");
        cmd.arg(session_id);
        cmd.cwd(cwd);
        Some(cmd)
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
