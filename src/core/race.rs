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

// ---- Race history / leaderboard -------------------------------------------

/// One finished race: who raced, who won (if decided), gate outcomes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RaceRecord {
    pub prompt: String,
    /// Unix epoch seconds at recording time.
    pub finished_at: u64,
    pub winner_agent: Option<String>,
    pub gate_results: Vec<(String, Option<bool>)>,
    /// true when the winner was promoted with `force` despite a failed gate.
    pub forced: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RaceHistory {
    pub records: Vec<RaceRecord>,
}

impl RaceHistory {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn load(repo: &std::path::Path) -> Self {
        let current = repo.join(".ridge-races.json");
        let legacy = repo.join(".term-races.json");
        let path = if current.exists() { current } else { legacy };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<RaceHistory>(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, repo: &std::path::Path) -> Result<(), String> {
        let path = repo.join(".ridge-races.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Append a record, keeping at most 50 entries (newest last).
    pub fn record(&mut self, rec: RaceRecord) {
        self.records.push(rec);
        while self.records.len() > 50 {
            self.records.remove(0);
        }
    }

    /// Aggregate wins per agent, sorted by wins then name.
    pub fn leaderboard(&self) -> String {
        let mut wins: Vec<(String, usize)> = Vec::new();
        for r in &self.records {
            if let Some(a) = &r.winner_agent {
                match wins.iter_mut().find(|(n, _)| n == a) {
                    Some(e) => e.1 += 1,
                    None => wins.push((a.clone(), 1)),
                }
            }
        }
        wins.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if wins.is_empty() {
            return "(no decided races yet)".to_string();
        }
        wins.iter()
            .map(|(n, c)| {
                format!(
                    "  {n}: {c} win{} over {} race{}",
                    if *c == 1 { "" } else { "s" },
                    self.records.len(),
                    if self.records.len() == 1 { "" } else { "s" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    fn rec(winner: Option<&str>) -> RaceRecord {
        RaceRecord {
            prompt: "fix it".to_string(),
            finished_at: 1_700_000_000,
            winner_agent: winner.map(|s| s.to_string()),
            gate_results: vec![("claude".to_string(), Some(true))],
            forced: false,
        }
    }

    #[test]
    fn record_caps_at_50() {
        let mut h = RaceHistory::default();
        for _ in 0..55 {
            h.record(rec(Some("claude")));
        }
        assert_eq!(h.records.len(), 50);
    }

    #[test]
    fn leaderboard_counts_and_sorts() {
        let mut h = RaceHistory::default();
        h.record(rec(Some("claude")));
        h.record(rec(Some("claude")));
        h.record(rec(Some("codex")));
        h.record(rec(None)); // abandoned — not on the scoreboard
        let s = h.leaderboard();
        assert!(s.starts_with("  claude: 2 wins"), "{s}");
        assert!(s.contains("codex: 1 win"));
    }

    #[test]
    fn empty_history_leaderboard() {
        let h = RaceHistory::default();
        assert_eq!(h.leaderboard(), "(no decided races yet)");
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("ridge-race-history");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = RaceHistory::default();
        h.record(rec(Some("opencode")));
        h.save(&dir).unwrap();
        let h2 = RaceHistory::load(&dir);
        assert_eq!(h2.records.len(), 1);
        assert_eq!(h2.records[0].winner_agent.as_deref(), Some("opencode"));
        let path = dir.join(".ridge-races.json");
        assert_eq!(path, dir.join(".ridge-races.json"));
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
