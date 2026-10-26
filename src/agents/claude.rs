use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct ClaudeAgent;

impl Agent for ClaudeAgent {
    /// Executable name for doctor / PATH checks (None = not a real binary).
    fn exe(&self) -> Option<&'static str> {
        Some("claude")
    }
    fn id(&self) -> &'static str {
        "claude"
    }
    fn name(&self) -> &'static str {
        "Claude Code"
    }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool {
        let p = process_name.to_lowercase();
        let c = cmdline.to_lowercase();
        p.contains("claude") || c.contains("claude") || c.contains("claude-code")
    }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("claude");
        cmd.cwd(cwd);
        cmd
    }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> {
        let mut cmd = if session_id == "last" {
            let mut c = CommandBuilder::new("claude");
            c.arg("-c");
            c
        } else {
            let mut c = CommandBuilder::new("claude");
            c.arg("--resume");
            c.arg(session_id);
            c
        };
        cmd.cwd(cwd);
        Some(cmd)
    }
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(home).join(".claude").join("projects");
            // Claude stores per-project dirs keyed by cwd with slashes -> dashes.
            let slug = cwd.to_string_lossy().replace('/', "-");
            let dir = base.join(slug.trim_start_matches('-'));
            if dir.exists() {
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
                            out.push(p);
                        }
                    }
                }
            }
        }
        out
    }
    fn parse_status(&self, output_tail: &str) -> AgentStatus {
        let t = output_tail.to_lowercase();
        if t.contains("do you want") || t.contains("permission") || t.contains("(y/n)") {
            return AgentStatus::Blocked;
        }
        if t.contains("error") || t.contains("failed") {
            // only if near the end; heuristic
        }
        if t.trim_end().ends_with(">") || t.trim_end().ends_with("$") {
            return AgentStatus::Idle;
        }
        if t.contains("thinking") || t.contains("…") || t.contains("...") {
            return AgentStatus::Working;
        }
        AgentStatus::Unknown
    }
    fn parse_cost(&self, session_file: &Path) -> Option<CostInfo> {
        // Claude Code session files are JSONL; sum usage from assistant messages.
        let raw = std::fs::read_to_string(session_file).ok()?;
        let mut cost = CostInfo::default();
        for line in raw.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(usage) = v.get("message").and_then(|m| m.get("usage")) {
                    cost.input_tokens += usage
                        .get("input_tokens")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(0);
                    cost.output_tokens += usage
                        .get("output_tokens")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(0);
                }
            }
        }
        // Rough cost estimate: $3/M input, $15/M output (Opus-ish). Refined later.
        cost.cost_usd = (cost.input_tokens as f64 / 1_000_000.0) * 3.0
            + (cost.output_tokens as f64 / 1_000_000.0) * 15.0;
        Some(cost)
    }
    fn default_resume_arg(&self) -> Option<&'static str> {
        Some("last")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::agent::Agent;

    #[test]
    fn status_blocked_on_permission() {
        let a = ClaudeAgent;
        assert_eq!(
            a.parse_status("Do you want to proceed? (y/n)"),
            AgentStatus::Blocked
        );
    }

    #[test]
    fn status_idle_on_prompt() {
        let a = ClaudeAgent;
        assert_eq!(a.parse_status("done\n> "), AgentStatus::Idle);
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    use crate::core::agent::Agent;
    use std::io::Write;

    #[test]
    fn parse_cost_from_jsonl() {
        let dir = std::env::temp_dir().join("ridge-claude-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"message":{{"usage":{{"input_tokens":1000,"output_tokens":500}}}}}}"#
        )
        .unwrap();
        let a = ClaudeAgent;
        let c = a.parse_cost(&path).unwrap();
        assert_eq!(c.input_tokens, 1000);
        assert_eq!(c.output_tokens, 500);
        assert!(c.cost_usd > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
