//! Wait-for-human queue: prompts parked for an agent, delivered automatically
//! the next time that agent blocks on the user (permission prompt, question).
//! Delivered messages are recorded in the reply inbox so the follow-up answer
//! is captured too.

#[derive(Debug, Clone)]
pub struct QueuedPrompt {
    pub id: u64,
    pub pane_id: usize,
    pub pane_title: String,
    pub msg: String,
}

#[derive(Debug, Clone, Default)]
pub struct Queue {
    pub next_id: u64,
    pub items: Vec<QueuedPrompt>,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            items: Vec::new(),
        }
    }

    pub fn push(&mut self, pane_id: usize, pane_title: &str, msg: &str) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push(QueuedPrompt {
            id,
            pane_id,
            pane_title: pane_title.to_string(),
            msg: msg.to_string(),
        });
        id
    }

    pub fn drop(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// A queued prompt fires when its pane is blocked on the user.
    pub fn ready(status: crate::core::agent::AgentStatus) -> bool {
        matches!(status, crate::core::agent::AgentStatus::Blocked)
    }

    /// Plain-text listing for `:queue list` and the Orchestration tab.
    pub fn render(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        let mut out = format!("queue ({}):\n", self.items.len());
        for i in &self.items {
            out.push_str(&format!("  #{} pane {} \"{}\"\n", i.id, i.pane_id, i.msg));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_render_drop_lifecycle() {
        let mut q = Queue::new();
        let a = q.push(5, "Claude #5", "approve the plan");
        let b = q.push(5, "Claude #5", "pick option 2");
        assert_eq!(b, a + 1);
        assert_eq!(q.items.len(), 2);
        let s = q.render();
        assert!(s.contains("approve the plan"));
        assert!(s.contains("#1"));
        q.drop(a);
        assert_eq!(q.items.len(), 1);
        q.clear();
        assert!(q.render().is_empty());
    }

    #[test]
    fn drop_unknown_id_is_a_noop() {
        let mut q = Queue::new();
        q.push(1, "p", "m");
        assert!(!q.drop(99));
        assert_eq!(q.items.len(), 1);
    }

    #[test]
    fn ready_only_when_blocked() {
        use crate::core::agent::AgentStatus;
        assert!(Queue::ready(AgentStatus::Blocked));
        assert!(!Queue::ready(AgentStatus::Working));
        assert!(!Queue::ready(AgentStatus::Done));
        assert!(!Queue::ready(AgentStatus::Idle));
    }
}
