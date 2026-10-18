//! Inter-session reply inbox: every delegation records a pending item, and the
//! agent's reply (tail of its scrollback at/status transition) is captured on
//! the next tick after it finishes or blocks on a question.

use crate::core::agent::AgentStatus;

#[derive(Debug, Clone)]
pub struct InboxItem {
    pub id: u64,
    /// Stable pane id the prompt was sent to.
    pub pane_id: usize,
    /// Pane/agent display label captured at send time.
    pub pane_title: String,
    /// The prompt we sent.
    pub prompt: String,
    /// Captured reply (last meaningful scrollback line).
    pub reply: String,
    pub arrived: bool,
    pub read: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Inbox {
    pub next_id: u64,
    pub items: Vec<InboxItem>,
    pub max: usize,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            items: Vec::new(),
            max: 50,
        }
    }

    pub fn send(&mut self, pane_id: usize, pane_title: &str, prompt: &str) {
        let item = InboxItem {
            id: self.next_id,
            pane_id,
            pane_title: pane_title.to_string(),
            prompt: prompt.to_string(),
            reply: String::new(),
            arrived: false,
            read: false,
        };
        self.next_id += 1;
        self.items.push(item);
        // Keep the newest `max` items.
        while self.items.len() > self.max {
            self.items.remove(0);
        }
    }

    pub fn reply_ready(status: AgentStatus, arrived: bool) -> bool {
        !arrived
            && matches!(
                status,
                AgentStatus::Blocked | AgentStatus::Done | AgentStatus::Error
            )
    }

    pub fn unread_count(&self) -> usize {
        self.items.iter().filter(|i| i.arrived && !i.read).count()
    }

    pub fn mark_all_read(&mut self) {
        for i in &mut self.items {
            i.read = true;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_and_unread_lifecycle() {
        let mut b = Inbox::new();
        b.send(7, "Claude #7", "write tests");
        assert_eq!(b.items.len(), 1);
        assert!(!b.items[0].arrived);
        assert!(b.unread_count() == 0);
        b.items[0].arrived = true;
        assert_eq!(b.unread_count(), 1);
        b.mark_all_read();
        assert_eq!(b.unread_count(), 0);
        assert_eq!(b.items[0].reply, "");
        b.clear();
        assert!(b.items.is_empty());
    }

    #[test]
    fn reply_ready_predicate() {
        assert!(Inbox::reply_ready(AgentStatus::Blocked, false));
        assert!(Inbox::reply_ready(AgentStatus::Done, false));
        assert!(Inbox::reply_ready(AgentStatus::Error, false));
        assert!(!Inbox::reply_ready(AgentStatus::Working, false));
        assert!(!Inbox::reply_ready(AgentStatus::Blocked, true));
    }

    #[test]
    fn send_trims_to_capacity() {
        let mut b = Inbox::new();
        b.max = 3;
        for i in 0..5 {
            b.send(i, "p", "m");
        }
        assert_eq!(b.items.len(), 3);
        assert_eq!(b.items[0].pane_id, 2); // oldest trimmed
    }
}
