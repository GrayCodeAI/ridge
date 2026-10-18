use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Unknown,
    Idle,
    Working,
    Blocked, // needs user attention
    Done,
    Error,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Unknown => "unknown",
            AgentStatus::Idle => "idle",
            AgentStatus::Working => "working",
            AgentStatus::Blocked => "BLOCKED",
            AgentStatus::Done => "done",
            AgentStatus::Error => "error",
        }
    }

    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            AgentStatus::Unknown => Color::Gray,
            AgentStatus::Idle => Color::DarkGray,
            AgentStatus::Working => Color::Green,
            AgentStatus::Blocked => Color::Yellow,
            AgentStatus::Done => Color::Cyan,
            AgentStatus::Error => Color::Red,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CostInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: AgentStatus,
    pub cwd: PathBuf,
    pub cost: CostInfo,
    pub session_id: Option<String>,
}

pub trait Agent: Send + Sync {
    /// Stable identifier (e.g. "claude").
    fn id(&self) -> &'static str;
    /// Human-readable name.
    fn name(&self) -> &'static str;
    /// Does this agent handle the given process/cmdline?
    fn detect(&self, process_name: &str, cmdline: &str) -> bool;
    /// Build the spawn command for a new session.
    fn spawn_cmd(&self, cwd: &Path) -> portable_pty::CommandBuilder;
    /// Build a resume command for a previous session id, if supported.
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<portable_pty::CommandBuilder>;
    /// Where this agent stores sessions on disk.
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf>;
    /// Infer status from recent terminal output.
    fn parse_status(&self, output_tail: &str) -> AgentStatus;
    /// Parse cost/token info from a session file.
    fn parse_cost(&self, session_file: &Path) -> Option<CostInfo>;
    /// Default session id to resume ("last" if supported).
    fn default_resume_arg(&self) -> Option<&'static str> {
        None
    }
}

pub struct AgentRegistry {
    agents: Vec<Box<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        let mut reg = Self { agents: Vec::new() };
        reg.register(Box::new(crate::agents::claude::ClaudeAgent));
        reg.register(Box::new(crate::agents::codex::CodexAgent));
        reg.register(Box::new(crate::agents::opencode::OpenCodeAgent));
        reg.register(Box::new(crate::agents::generic::GenericAgent));
        reg
    }

    pub fn register(&mut self, a: Box<dyn Agent>) {
        self.agents.push(a);
    }

    pub fn all(&self) -> &[Box<dyn Agent>] {
        &self.agents
    }

    pub fn find_by_id(&self, id: &str) -> Option<&dyn Agent> {
        self.agents
            .iter()
            .find(|a| a.id() == id)
            .map(|a| a.as_ref())
    }

    pub fn detect(&self, process_name: &str, cmdline: &str) -> Option<&dyn Agent> {
        self.agents
            .iter()
            .find(|a| a.detect(process_name, cmdline))
            .map(|a| a.as_ref())
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
