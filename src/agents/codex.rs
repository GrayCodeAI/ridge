use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct CodexAgent;

impl Agent for CodexAgent {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn name(&self) -> &'static str {
        "Codex"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("codex") || c.contains("codex")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("codex");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = CommandBuilder::new("codex");
        cmd.arg("resume");
        cmd.arg(session_id);
        cmd.cwd(cwd);
        Some(cmd)
    }
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(home).join(".codex").join("sessions");
            if base.exists() {
                walk_jsonl(&base, &mut out);
            }
        }
        out
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let t = output_tail.to_lowercase();
        if t.contains("approval") || t.contains("(y/n)") || t.contains("confirm") {
            return AgentStatus::Blocked;
        }
        if t.trim_end().ends_with(">") || t.trim_end().ends_with("$") {
            return AgentStatus::Idle;
        }
        if t.contains("working") || t.contains("thinking") {
            return AgentStatus::Working;
        }
        AgentStatus::Unknown
    }
    fn parse_cost(&self, _session_file: &Path) -> Option<CostInfo> {
        None
    }
}

fn walk_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_jsonl(&p, out);
            } else if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
                out.push(p);
            }
        }
    }
}
EOF