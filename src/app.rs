use crate::cli::Cli;
use crate::config::Config;
use crate::core::agent::{AgentRegistry, AgentStatus};
use crate::core::pty::PtySession;
use crate::event::{Event, EventHandler};
use crate::ui;
use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Agents,
    Git,
    Diff,
    Files,
    Orchestration,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Agents,
        Tab::Git,
        Tab::Diff,
        Tab::Files,
        Tab::Orchestration,
        Tab::Settings,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Agents => "Agents",
            Tab::Git => "Git",
            Tab::Diff => "Diff",
            Tab::Files => "Files",
            Tab::Orchestration => "Orch",
            Tab::Settings => "Settings",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

pub struct Pane {
    pub id: usize,
    pub title: String,
    pub agent_id: Option<String>,
    pub pty: Option<PtySession>,
    pub status: AgentStatus,
    pub cost_usd: f64,
    pub parent_pane: Option<usize>,
    pub children: Vec<usize>,
}

impl Pane {
    pub fn new(id: usize, title: String) -> Self {
        Self {
            id,
            title,
            agent_id: None,
            pty: None,
            status: AgentStatus::Idle,
            cost_usd: 0.0,
            parent_pane: None,
            children: Vec::new(),
        }
    }
}

pub struct App {
    pub config: Config,
    pub current_tab: Tab,
    pub should_quit: bool,
    pub status_message: String,
    pub tick_count: u64,

    pub registry: AgentRegistry,
    pub panes: Vec<Pane>,
    pub active_pane: usize,
    pub next_pane_id: usize,

    pub command_mode: bool,
    pub command_buffer: String,
    pub delegate_mode: bool,
    pub delegate_buffer: String,
    pub search_mode: bool,
    pub search_buffer: String,
    pub search_results: Vec<String>,

    pub module_outputs: Vec<(String, String)>,

    pub task_board: crate::core::orchestration::TaskBoard,
    pub cwd: std::path::PathBuf,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            current_tab: Tab::Agents,
            should_quit: false,
            status_message: "Term — mission control for your AI agents".to_string(),
            tick_count: 0,
            registry: AgentRegistry::new(),
            panes: Vec::new(),
            active_pane: 0,
            next_pane_id: 1,
            command_mode: false,
            command_buffer: String::new(),
            delegate_mode: false,
            delegate_buffer: String::new(),
            search_mode: false,
            search_buffer: String::new(),
            search_results: Vec::new(),

            module_outputs: crate::core::modules::run_all(),
            task_board: crate::core::orchestration::TaskBoard::new(),
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }

    pub fn spawn_agent(&mut self, agent_id: &str, cwd: std::path::PathBuf, parent: Option<usize>) {
        let agent = match self.registry.find_by_id(agent_id) {
            Some(a) => a,
            None => {
                self.status_message = format!("unknown agent: {agent_id}");
                return;
            }
        };
        let cmd = agent.spawn_cmd(&cwd);
        match PtySession::spawn(cmd, 24, 80) {
            Ok(pty) => {
                let id = self.next_pane_id;
                self.next_pane_id += 1;
                let mut pane = Pane::new(id, format!("{} #{}", agent.name(), id));
                pane.agent_id = Some(agent_id.to_string());
                pane.pty = Some(pty);
                pane.parent_pane = parent;
                pane.status = AgentStatus::Working;
                self.panes.push(pane);
                // link to parent
                if let Some(pid) = parent {
                    if let Some(p) = self.panes.iter_mut().find(|x| x.id == pid) {
                        p.children.push(id);
                    }
                }
                self.active_pane = self.panes.len().saturating_sub(1);
                self.status_message = format!("spawned {} (pane {})", agent.name(), id);
            }
            Err(e) => {
                self.status_message = format!("spawn failed: {e}");
            }
        }
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(self.active_pane)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.active_pane)
    }

    pub fn close_active_pane(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        let idx = self.active_pane;
        if let Some(p) = self.panes.get(idx) {
            if let Some(pty) = &p.pty {
                let _ = pty.write(b"\x03"); // Ctrl+C
            }
        }
        self.panes.remove(idx);
        if self.active_pane >= self.panes.len() && !self.panes.is_empty() {
            self.active_pane = self.panes.len() - 1;
        }
        self.status_message = "pane closed".to_string();
    }

    pub fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        match parts[0] {
            "spawn" => {
                let agent = parts.get(1).copied().unwrap_or("claude");
                let cwd = self.cwd.clone();
                self.spawn_agent(agent, cwd, None);
            }
            "close" => self.close_active_pane(),
            "delegate" => {
                // delegate <pane_index> <message...>
                if parts.len() >= 3 {
                    if let Ok(idx) = parts[1].parse::<usize>() {
                        let msg = parts[2..].join(" ");
                        if let Some(pane) = self.panes.get(idx) {
                            if let Some(pty) = &pane.pty {
                                let _ = pty.write(msg.as_bytes());
                                let _ = pty.write(b"\r");
                                self.status_message = format!("delegated to pane {idx}");
                            }
                        }
                    }
                }
            }
            "task" => {
                if parts.len() >= 3 && parts[1] == "add" {
                    let title = parts[2..].join(" ");
                    let id = self.task_board.add(title, vec![], None);
                    self.status_message = format!("task {id} added");
                } else if parts.len() >= 3 && parts[1] == "start" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        match self.task_board.start_task(id, &self.cwd) {
                            Ok(path) => {
                                // spawn an agent in that worktree
                                let agent = parts.get(3).copied().unwrap_or("claude");
                                self.spawn_agent(agent, path, None);
                                self.status_message = format!("task {id} started");
                            }
                            Err(e) => self.status_message = format!("start failed: {e}"),
                        }
                    }
                } else if parts.len() >= 3 && parts[1] == "done" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        self.task_board.complete_task(id, true);
                        self.status_message = format!("task {id} done");
                    }
                } else if parts.len() >= 2 && parts[1] == "broadcast" {
                    let msg = parts[2..].join(" ");
                    for i in 0..self.panes.len() {
                        self.delegate_to_pane(i, &msg);
                    }
                    self.status_message = format!("broadcast to {} panes", self.panes.len());
                } else if parts.len() >= 4 && parts[1] == "lease" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        let paths: Vec<std::path::PathBuf> =
                            parts[3..].iter().map(std::path::PathBuf::from).collect();
                        match self.task_board.lease_paths(id, paths) {
                            Ok(()) => self.status_message = format!("leased paths for task {id}"),
                            Err(e) => self.status_message = format!("lease failed: {e}"),
                        }
                    }
                } else if parts.len() >= 3 && parts[1] == "gate" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        match self.task_board.run_quality_gate(id) {
                            Ok(true) => self.status_message = format!("task {id} gate passed"),
                            Ok(false) => self.status_message = format!("task {id} gate failed"),
                            Err(e) => self.status_message = format!("gate error: {e}"),
                        }
                    }
                } else if parts.len() >= 3 && parts[1] == "kanban" {
                    self.status_message = self.task_board.render_kanban();
                } else if parts.len() >= 3 && parts[1] == "subagent" {
                    // subagent <parent_pane> <agent>
                    if let Ok(parent) = parts[2].parse::<usize>() {
                        let agent = parts.get(3).copied().unwrap_or("claude");
                        let cwd = self.cwd.clone();
                        self.spawn_agent(agent, cwd, Some(parent));
                        self.status_message = format!("subagent spawned under pane {}", parent);
                    }
                } else if parts.len() >= 3 && parts[1] == "fanout" {
                    let title = parts[2..].join(" ");
                    let agents = ["claude", "codex", "opencode"];
                    for a in agents {
                        let id = self
                            .task_board
                            .add(format!("{} [{}]", title, a), vec![], None);
                        if let Ok(path) = self.task_board.start_task(id, &self.cwd) {
                            self.spawn_agent(a, path, None);
                        }
                    }
                    self.status_message = format!("fanout: spawned {} agents", agents.len());
                } else if parts.len() >= 3 && parts[1] == "merge" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        match self.task_board.merge_task(id, &self.cwd) {
                            Ok(()) => self.status_message = format!("task {id} merged"),
                            Err(e) => self.status_message = format!("merge failed: {e}"),
                        }
                    }
                }
            }
            "branch" => {
                self.status_message = crate::core::git::current_branch(&self.cwd);
            }
            "help" => {
                self.status_message =
                    "commands: spawn <agent>, close, delegate <pane> <msg>, broadcast <msg>, task add/start/done/lease/gate/merge/kanban/fanout, branch".to_string();
            }
            other => {
                self.status_message = format!("unknown command: {other}");
            }
        }
    }

    pub fn run_search(&mut self) {
        let q = self.search_buffer.to_lowercase();
        let mut results = Vec::new();
        if q.is_empty() {
            self.search_results = results;
            return;
        }
        // Files
        let files = crate::core::git::run_git(&self.cwd, &["ls-files"]);
        for line in files.lines() {
            if line.to_lowercase().contains(&q) {
                results.push(format!("file: {line}"));
            }
        }
        // Agents/panes
        for p in &self.panes {
            if p.title.to_lowercase().contains(&q) {
                results.push(format!("pane: {} ({})", p.title, p.status.label()));
            }
        }
        // Tasks
        for t in &self.task_board.tasks {
            if t.title.to_lowercase().contains(&q) {
                results.push(format!("task: #{} {}", t.id, t.title));
            }
        }
        // Commands (static list)
        for c in ["spawn", "close", "delegate", "task", "branch", "help"] {
            if c.contains(&q) {
                results.push(format!("cmd: {c}"));
            }
        }
        results.truncate(50);
        self.search_results = results;
    }

    pub fn delegate_to_pane(&self, idx: usize, msg: &str) {
        if let Some(pane) = self.panes.get(idx) {
            if let Some(pty) = &pane.pty {
                let _ = pty.write(msg.as_bytes());
                let _ = pty.write(b"\r");
            }
        }
    }
}

pub async fn run(config: Config, cli: Cli) -> Result<()> {
    if cli.headless {
        tracing::info!("headless mode not yet implemented");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);
    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        // Refresh agent statuses from PTY output and cost from session files.
        for pane in app.panes.iter_mut() {
            if let Some(pty) = &pane.pty {
                let tail = pty.scrollback_tail(8);
                if let Some(id) = &pane.agent_id.clone() {
                    if let Some(agent) = app.registry.find_by_id(id) {
                        pane.status = agent.parse_status(&tail);
                        // cost from latest session file
                        let files = agent.session_files(&app.cwd);
                        if let Some(f) = files.into_iter().last() {
                            if let Some(c) = agent.parse_cost(&f) {
                                pane.cost_usd = c.cost_usd;
                            }
                        }
                    }
                }
                if !pty.is_alive() {
                    pane.status = AgentStatus::Done;
                }
            }
        }

        match events.next().await {
            Some(Event::Key(k)) => handle_key(&mut app, k),
            Some(Event::Resize(w, h)) => {
                tracing::debug!("resize {}x{}", w, h);
                // Resize active pane PTY to fit.
                if let Some(pane) = app.active_pane_mut() {
                    if let Some(pty) = &pane.pty {
                        let _ = pty.resize(h.saturating_sub(4), w);
                    }
                }
            }
            Some(Event::Tick) => {
                app.tick_count = app.tick_count.wrapping_add(1);
                if app.tick_count.is_multiple_of(20) {
                    app.module_outputs = crate::core::modules::run_all();
                }
            }
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use crossterm::event::{KeyCode, KeyModifiers};

    // Search mode.
    if app.search_mode {
        match key.code {
            KeyCode::Esc => {
                app.search_mode = false;
                app.search_buffer.clear();
                app.search_results.clear();
            }
            KeyCode::Enter => {
                // keep results shown; Esc to exit
            }
            KeyCode::Backspace => {
                app.search_buffer.pop();
                app.run_search();
            }
            KeyCode::Char(c) => {
                app.search_buffer.push(c);
                app.run_search();
            }
            _ => {}
        }
        return;
    }

    // Delegate mode: capture "=pane msg" input.
    if app.delegate_mode {
        match key.code {
            KeyCode::Esc => {
                app.delegate_mode = false;
                app.delegate_buffer.clear();
            }
            KeyCode::Enter => {
                let buf = app.delegate_buffer.clone();
                app.delegate_mode = false;
                app.delegate_buffer.clear();
                // parse: first token = pane index, rest = message
                let mut parts = buf.splitn(2, char::is_whitespace);
                let pane_tok = parts.next().unwrap_or("").trim();
                let msg = parts.next().unwrap_or("").trim();
                if let Ok(idx) = pane_tok.parse::<usize>() {
                    app.delegate_to_pane(idx, msg);
                    app.status_message = format!("delegated to pane {idx}");
                } else {
                    // try pane title match
                    app.status_message = "usage: =<pane_index> <message>".to_string();
                }
            }
            KeyCode::Backspace => {
                app.delegate_buffer.pop();
            }
            KeyCode::Char(c) => {
                app.delegate_buffer.push(c);
            }
            _ => {}
        }
        return;
    }

    // Command mode: capture input into buffer.
    if app.command_mode {
        match key.code {
            KeyCode::Esc => {
                app.command_mode = false;
                app.command_buffer.clear();
            }
            KeyCode::Enter => {
                let cmd = app.command_buffer.clone();
                app.command_mode = false;
                app.command_buffer.clear();
                app.execute_command(&cmd);
            }
            KeyCode::Backspace => {
                app.command_buffer.pop();
            }
            KeyCode::Char(c) => {
                app.command_buffer.push(c);
            }
            _ => {}
        }
        return;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => app.should_quit = true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.should_quit = true,
        (KeyCode::Tab, _) => app.current_tab = app.current_tab.next(),
        (KeyCode::BackTab, _) => app.current_tab = app.current_tab.prev(),
        (KeyCode::Right, _) => app.current_tab = app.current_tab.next(),
        (KeyCode::Left, _) => app.current_tab = app.current_tab.prev(),
        (KeyCode::Char('1'), _) => app.current_tab = Tab::Agents,
        (KeyCode::Char('2'), _) => app.current_tab = Tab::Git,
        (KeyCode::Char('3'), _) => app.current_tab = Tab::Diff,
        (KeyCode::Char('4'), _) => app.current_tab = Tab::Files,
        (KeyCode::Char('5'), _) => app.current_tab = Tab::Orchestration,
        (KeyCode::Char('6'), _) => app.current_tab = Tab::Settings,
        // Pane management
        (KeyCode::Char('n'), _) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            app.spawn_agent("claude", cwd, None);
        }
        (KeyCode::Char('x'), _) => app.close_active_pane(),
        (KeyCode::Char(']'), _) => {
            if !app.panes.is_empty() {
                app.active_pane = (app.active_pane + 1) % app.panes.len();
            }
        }
        (KeyCode::Char('r'), _) => {
            let info = app
                .active_pane()
                .and_then(|p| p.agent_id.clone())
                .map(|id| (id, app.cwd.clone()));
            if let Some((id, cwd)) = info {
                let agent = app.registry.find_by_id(&id);
                if let Some(agent) = agent {
                    if let Some(cmd) = crate::core::session::resume_command(agent, &cwd) {
                        match PtySession::spawn(cmd, 24, 80) {
                            Ok(pty) => {
                                let new_id = app.next_pane_id;
                                app.next_pane_id += 1;
                                let mut new_pane = Pane::new(new_id, format!("{id} (resumed)"));
                                new_pane.agent_id = Some(id);
                                new_pane.pty = Some(pty);
                                new_pane.status = AgentStatus::Working;
                                app.panes.push(new_pane);
                                app.active_pane = app.panes.len() - 1;
                                app.status_message = "session resumed".to_string();
                            }
                            Err(e) => app.status_message = format!("resume failed: {e}"),
                        }
                    } else {
                        app.status_message = "resume not supported".to_string();
                    }
                }
            }
        }
        (KeyCode::Char('a'), _) => {
            // Jump to next blocked pane.
            if !app.panes.is_empty() {
                let start = app.active_pane;
                for offset in 1..=app.panes.len() {
                    let idx = (start + offset) % app.panes.len();
                    if app.panes[idx].status == AgentStatus::Blocked {
                        app.active_pane = idx;
                        app.status_message =
                            format!("jumped to blocked pane {}", app.panes[idx].id);
                        break;
                    }
                }
            }
        }
        (KeyCode::Char('f'), _) => {
            let info = app
                .active_pane()
                .and_then(|p| p.agent_id.clone())
                .map(|id| (id, app.cwd.clone()));
            if let Some((id, cwd)) = info {
                app.spawn_agent(&id, cwd, None);
                app.status_message = "session forked".to_string();
            }
        }
        (KeyCode::Char('['), _) => {
            if !app.panes.is_empty() {
                app.active_pane = (app.active_pane + app.panes.len() - 1) % app.panes.len();
            }
        }
        (KeyCode::Char(':'), _) => {
            app.command_mode = true;
            app.command_buffer.clear();
        }
        (KeyCode::Char('='), _) => {
            app.delegate_mode = true;
            app.delegate_buffer.clear();
        }
        (KeyCode::Char('/'), _) => {
            app.search_mode = true;
            app.search_buffer.clear();
            app.search_results.clear();
        }
        // Forward typed input to active pane PTY.
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            if let Some(pane) = app.active_pane_mut() {
                if let Some(pty) = &pane.pty {
                    let mut b = [0u8; 4];
                    let n = c.encode_utf8(&mut b).len();
                    let _ = pty.write(&b[..n]);
                }
            }
        }
        (KeyCode::Enter, _) => {
            if let Some(pane) = app.active_pane_mut() {
                if let Some(pty) = &pane.pty {
                    let _ = pty.write(b"\r");
                }
            }
        }
        (KeyCode::Backspace, _) => {
            if let Some(pane) = app.active_pane_mut() {
                if let Some(pty) = &pane.pty {
                    let _ = pty.write(b"\x7f");
                }
            }
        }
        _ => {}
    }
}
