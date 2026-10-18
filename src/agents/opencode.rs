use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct OpenCodeAgent;

impl Agent for OpenCodeAgent {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn name(&self) -> &'static str {
        "OpenCode"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("opencode") || c.contains("opencode")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("opencode");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = CommandBuilder::new("opencode");
        cmd.arg("--session");
        cmd.arg(session_id);
        cmd.cwd(cwd);
        Some(cmd)
    }
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(home).join(".local").join("share").join("opencode");
            if base.exists() {
                collect(&base, &mut out);
            }
        }
        let _ = cwd;
        out
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let t = output_tail.to_lowercase();
        if t.contains("permission") || t.contains("(y/n)") || t.contains("approve") {
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

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}
EOF