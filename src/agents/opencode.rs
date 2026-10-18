use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use serde_json;
use std::path::{Path, PathBuf};

pub struct OpenCodeAgent;

impl Agent for OpenCodeAgent {
    /// Executable name for doctor / PATH checks (None = not a real binary).
    fn exe(&self) -> Option<&'static str> {
        Some("opencode")
    }
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
            let base = PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("opencode");
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
    fn parse_cost(&self, session_file: &Path) -> Option<CostInfo> {
        let raw = std::fs::read_to_string(session_file).ok()?;
        let mut cost = CostInfo::default();
        for line in raw.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(usage) = v.get("usage") {
                    cost.input_tokens += usage
                        .get("input")
                        .or_else(|| usage.get("input_tokens"))
                        .and_then(|x| x.as_u64())
                        .unwrap_or(0);
                    cost.output_tokens += usage
                        .get("output")
                        .or_else(|| usage.get("output_tokens"))
                        .and_then(|x| x.as_u64())
                        .unwrap_or(0);
                }
            }
        }
        cost.cost_usd = (cost.input_tokens as f64 / 1_000_000.0) * 3.0
            + (cost.output_tokens as f64 / 1_000_000.0) * 15.0;
        Some(cost)
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
