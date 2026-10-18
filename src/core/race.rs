//! Fan-out race: spawn the same prompt into N agents, each in its own git
//! worktree/branch, then promote a winner by merging its branch and cleaning
//! up the losing panes.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RaceContestant {
    pub agent_id: String,
    pub pane_id: usize,
    pub branch: String,
    pub worktree: PathBuf,
    pub status: Option<crate::core::agent::AgentStatus>,
    /// Result of the quality gate run in this contestant's worktree.
    pub gate_ok: Option<bool>,
    pub winner: bool,
    pub promoted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Race {
    pub prompt: String,
    pub contestants: Vec<RaceContestant>,
    /// Optional shell command overriding the default quality gate.
    pub gate_cmd: Option<String>,
}

impl Race {
    /// Plain-text summary for the Orchestration tab.
    pub fn render(&self) -> String {
        let mut out = format!(
            "race: \"{}\" — :race winner <idx> | :race cancel\n",
            self.prompt
        );
        for (i, c) in self.contestants.iter().enumerate() {
            let st = c
                .status
                .map(|s| s.label().to_string())
                .unwrap_or_else(|| "(gone)".to_string());
            let gate = match c.gate_ok {
                Some(true) => " ✓",
                Some(false) => " ✗gate",
                None => "",
            };
            let star = if c.winner { " ★" } else { "" };
            let closed = if c.promoted { " (closed)" } else { "" };
            out.push_str(&format!(
                "  [{}] {} pane {} — {}{}{}{}\n",
                i, c.agent_id, c.pane_id, st, gate, star, closed
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contestant(agent: &str, winner: bool, promoted: bool) -> RaceContestant {
        RaceContestant {
            agent_id: agent.to_string(),
            pane_id: 1,
            branch: format!("race-1-{agent}"),
            worktree: PathBuf::from(format!("/tmp/wt-{agent}")),
            status: Some(crate::core::agent::AgentStatus::Working),
            gate_ok: None,
            winner,
            promoted,
        }
    }

    #[test]
    fn render_lists_contestants_and_winner_star() {
        let mut r = Race {
            prompt: "add tests".to_string(),
            contestants: vec![
                contestant("claude", false, false),
                contestant("codex", true, true),
            ],
            gate_cmd: None,
        };
        r.contestants[1].status = Some(crate::core::agent::AgentStatus::Done);
        let s = r.render();
        assert!(s.contains("add tests"));
        assert!(s.contains("claude"));
        assert!(s.contains("codex"));
        assert!(s.contains("★"));
        assert!(s.contains("(closed)"));
        assert!(s.contains(":race winner <idx>"));
    }

    #[test]
    fn render_empty_race_is_usable() {
        let r = Race::default();
        let s = r.render();
        assert!(s.contains("race:"));
    }
}
