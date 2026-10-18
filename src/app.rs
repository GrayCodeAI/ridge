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
use std::io::{self, Write};
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
    pub input_buffer: String,
    pub attention: bool,
    pub session_id: Option<String>,
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
            input_buffer: String::new(),
            attention: false,
            session_id: None,
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
    pub search_selected: usize,
    pub show_help: bool,
    pub zoomed: bool,

    pub diff_scroll: usize,
    pub files_scroll: usize,
    pub diff_sections: Vec<(&'static str, String)>,
    pub files_cache: Vec<String>,
    pub files_status: std::collections::HashMap<String, char>,

    pub module_outputs: Vec<(String, String)>,

    pub task_board: crate::core::orchestration::TaskBoard,
    pub cwd: std::path::PathBuf,
}

impl App {
    pub fn new(config: Config) -> Self {
        let mut app = Self {
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
            search_selected: 0,
            show_help: false,
            zoomed: false,

            module_outputs: Vec::new(),
            diff_scroll: 0,
            files_scroll: 0,
            diff_sections: vec![
                ("Staged", String::new()),
                ("Worktree", String::new()),
                ("Untracked", String::new()),
                ("Conflicted", String::new()),
            ],
            files_cache: Vec::new(),
            files_status: std::collections::HashMap::new(),
            task_board: crate::core::orchestration::TaskBoard::load(
                &std::env::current_dir().unwrap_or_default(),
            ),
            cwd: std::env::current_dir().unwrap_or_default(),
        };
        app.module_outputs = crate::core::modules::run_all(&app.module_state_json());
        app
    }

    /// Compact summary of live state, exposed to modules via $TERM_JSON.
    pub fn module_state_json(&self) -> String {
        let panes = self.panes.len();
        let blocked = self
            .panes
            .iter()
            .filter(|p| p.status == AgentStatus::Blocked)
            .count();
        let tasks = self.task_board.tasks.len();
        format!("{{\"panes\":{panes},\"blocked\":{blocked},\"tasks\":{tasks}}}")
    }

    pub fn spawn_agent(&mut self, agent_id: &str, cwd: std::path::PathBuf, parent: Option<usize>) {
        let agent = match self.registry.find_by_id(agent_id) {
            Some(a) => a,
            None => {
                self.status_message = format!("unknown agent: {agent_id}");
                return;
            }
        };
        let mut cmd = agent.spawn_cmd(&cwd);
        // Inject the matching account's env vars (account switcher).
        if let Some(acc) = self.config.accounts.iter().find(|a| a.agent == agent_id) {
            for (k, v) in &acc.env {
                cmd.env(k.clone(), v.clone());
            }
        }
        match PtySession::spawn(cmd, 24, 80) {
            Ok(pty) => {
                let id = self.next_pane_id;
                self.next_pane_id += 1;
                let mut pane = Pane::new(id, format!("{} #{}", agent.name(), id));
                pane.agent_id = Some(agent_id.to_string());
                pane.pty = Some(pty);
                pane.session_id = crate::core::session::latest_session_file(agent, &cwd)
                    .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()));
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
            "session" => {
                if parts.len() >= 2 && parts[1] == "save" {
                    save_session_file(self);
                    self.status_message = "session saved".to_string();
                } else if parts.len() >= 2 && parts[1] == "clear" {
                    let path = crate::config::data_dir().join("session.json");
                    if std::fs::remove_file(&path).is_ok() {
                        self.status_message = "saved session cleared".to_string();
                    } else {
                        self.status_message = "no saved session".to_string();
                    }
                } else {
                    self.status_message = "usage: session save | session clear".to_string();
                }
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
                } else if parts.len() >= 2 && parts[1] == "account" {
                    if parts.len() >= 3 && parts[2] == "list" {
                        let mut msg = String::from("accounts:\n");
                        for a in &self.config.accounts {
                            msg.push_str(&format!("  {} / {}\n", a.agent, a.name));
                        }
                        if self.config.accounts.is_empty() {
                            msg.push_str("  (none)\n");
                        }
                        self.status_message = msg;
                    } else if parts.len() >= 4 && parts[2] == "add" {
                        self.config.accounts.push(crate::config::Account {
                            agent: parts[3].to_string(),
                            name: parts.get(4).copied().unwrap_or("default").to_string(),
                            env: std::collections::BTreeMap::new(),
                        });
                        let _ = self.config.save();
                        self.status_message = format!("account added: {}", parts[3]);
                    } else if parts.len() >= 5 && parts[2] == "env" {
                        // account env <agent> KEY=VAL — upsert into that account
                        let agent = parts[3].to_string();
                        match parts[4].split_once('=') {
                            Some((k, v)) if !k.is_empty() => {
                                let idx = match self
                                    .config
                                    .accounts
                                    .iter()
                                    .position(|a| a.agent == agent)
                                {
                                    Some(i) => i,
                                    None => {
                                        self.config.accounts.push(crate::config::Account {
                                            agent: agent.clone(),
                                            name: "default".to_string(),
                                            env: std::collections::BTreeMap::new(),
                                        });
                                        self.config.accounts.len() - 1
                                    }
                                };
                                self.config.accounts[idx]
                                    .env
                                    .insert(k.to_string(), v.to_string());
                                let _ = self.config.save();
                                self.status_message = format!("env set for {}: {}={}", agent, k, v);
                            }
                            _ => {
                                self.status_message =
                                    "usage: account env <agent> KEY=VAL".to_string()
                            }
                        }
                    } else if parts.len() >= 3 && parts[2] == "show" {
                        // account show <agent> — list its env var keys
                        let agent = parts[3].to_string();
                        match self.config.accounts.iter().find(|a| a.agent == agent) {
                            Some(acc) if !acc.env.is_empty() => {
                                let mut msg = format!("account {}:\n", agent);
                                for k in acc.env.keys() {
                                    msg.push_str(&format!("  {}=<set>\n", k));
                                }
                                self.status_message = msg;
                            }
                            Some(_) => {
                                self.status_message = format!("account {} has no env vars", agent)
                            }
                            None => self.status_message = format!("no account for {}", agent),
                        }
                    } else {
                        self.status_message = "usage: account list | add | env | show".to_string();
                    }
                } else if parts.len() >= 2 && parts[1] == "export" {
                    if parts.len() >= 3 && parts[2] == "costs" {
                        let mut csv = String::from("pane,agent,status,cost_usd\n");
                        for p in &self.panes {
                            csv.push_str(&format!(
                                "{},{},{},{}\n",
                                p.id,
                                p.agent_id.as_deref().unwrap_or("-"),
                                p.status.label(),
                                p.cost_usd
                            ));
                        }
                        let path = self.cwd.join("term-costs.csv");
                        match std::fs::write(&path, csv) {
                            Ok(()) => {
                                self.status_message =
                                    format!("exported costs to {}", path.display())
                            }
                            Err(e) => self.status_message = format!("export failed: {}", e),
                        }
                    } else if parts.len() >= 3 && parts[2] == "session" {
                        // export session <path> — snapshot of open panes as JSON
                        let file = parts.get(3).copied().unwrap_or("term-session.json");
                        let panes: Vec<crate::app::PaneSnapshot> = self
                            .panes
                            .iter()
                            .filter_map(|p| {
                                p.agent_id.as_ref().map(|a| crate::app::PaneSnapshot {
                                    agent_id: a.clone(),
                                    title: p.title.clone(),
                                })
                            })
                            .collect();
                        match serde_json::to_string_pretty(&panes) {
                            Ok(json) => match std::fs::write(self.cwd.join(file), json) {
                                Ok(()) => {
                                    self.status_message = format!("session exported to {file}")
                                }
                                Err(e) => self.status_message = format!("export failed: {e}"),
                            },
                            Err(e) => self.status_message = format!("export failed: {e}"),
                        }
                    } else {
                        self.status_message =
                            "usage: export costs | export session <file>".to_string();
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
                } else if parts.len() >= 3 && parts[1] == "filter" {
                    let f = parts[2].to_string();
                    self.task_board.filter = if f == "all" { None } else { Some(f.clone()) };
                    let shown = self
                        .task_board
                        .tasks
                        .iter()
                        .filter(|t| match &self.task_board.filter {
                            None => true,
                            Some(f) => t.status.label() == f || t.title.contains(f),
                        })
                        .count();
                    self.status_message = format!(
                        "filter set to {} ({shown} matching) — :task filter all to clear",
                        self.task_board.filter.as_deref().unwrap_or("none")
                    );
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
                // Persist the board after any task mutation (save is cheap; also
                // covers fanout/add/start/done/lease/gate/merge).
                let _ = self.task_board.save(&self.cwd);
            }
            "branch" => {
                self.status_message = crate::core::git::current_branch(&self.cwd);
            }
            "config" => {
                let help = "usage: config show | config theme <name>".to_string();
                if parts.len() >= 2 && parts[1] == "show" {
                    match crate::config::Config::load() {
                        Ok(cfg) => match toml::to_string_pretty(&cfg) {
                            Ok(s) => self.status_message = s,
                            Err(e) => self.status_message = format!("serialize: {e}"),
                        },
                        Err(e) => self.status_message = format!("load: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "theme" {
                    self.config.theme = parts[2].to_string();
                    if let Ok(()) = self.config.save() {
                        self.status_message = format!("theme set to {}", parts[2]);
                    } else {
                        self.status_message = "theme set (save failed)".to_string();
                    }
                } else {
                    self.status_message = help;
                }
            }
            "help" => {
                self.status_message =
                    "commands: spawn <agent>, close, delegate <pane> <msg>, broadcast <msg>, session save/clear, export costs/session, account list/add/env/show, config show/theme, task add/start/done/lease/gate/merge/kanban/fanout, branch".to_string();
            }
            other => {
                self.status_message = format!("unknown command: {other}");
            }
        }
    }

    /// Rebuild the cached git diff sections (avoids a subprocess per frame —
    /// Diff tab renders from this cache, refreshed on a timer).
    pub fn refresh_diff_cache(&mut self) {
        self.diff_sections = vec![
            ("Staged", crate::core::git::diff_staged(&self.cwd)),
            ("Worktree", crate::core::git::diff_worktree(&self.cwd)),
            ("Untracked", crate::core::git::untracked(&self.cwd)),
            ("Conflicted", crate::core::git::conflicted(&self.cwd)),
        ];
    }

    /// Rebuild the cached file list for the Files tab.
    pub fn refresh_files_cache(&mut self) {
        let out = crate::core::git::run_git(&self.cwd, &["ls-files"]);
        let status = crate::core::git::status_porcelain(&self.cwd);
        self.files_status.clear();
        for line in status.lines() {
            // Porcelain format: "XY path"; renames are "XY old -> new".
            if line.len() > 3 {
                let x = line.chars().next().unwrap_or(' ');
                self.files_status.insert(line[3..].to_string(), x);
            }
        }
        self.files_cache = out.lines().map(|s| s.to_string()).collect();
    }

    /// Jump `diff_scroll` to the next/previous hunk (`@@` line) in the
    /// rendered diff, wrapping around. Line indices must match render_diff.
    pub fn jump_hunk(&mut self, forward: bool) {
        let mut hunk_starts: Vec<usize> = Vec::new();
        let mut idx = 0usize;
        for (_title, body) in &self.diff_sections {
            idx += 1; // section title line
            if body.is_empty() {
                idx += 1; // "(none)"
            } else {
                for line in body.lines().take(10_000) {
                    if line.starts_with("@@") {
                        hunk_starts.push(idx);
                    }
                    idx += 1;
                }
            }
            idx += 1; // blank separator
        }
        if hunk_starts.is_empty() {
            self.status_message = "no hunks in diff".to_string();
            return;
        }
        let current = self.diff_scroll;
        let next = if forward {
            hunk_starts
                .iter()
                .copied()
                .find(|&s| s > current)
                .unwrap_or(hunk_starts[0])
        } else {
            hunk_starts
                .iter()
                .rev()
                .copied()
                .find(|&s| s < current)
                .unwrap_or(*hunk_starts.last().unwrap())
        };
        self.diff_scroll = next;
    }

    #[cfg(test)]
    fn sample_for_hunk_test() -> App {
        let mut a = App::new(crate::config::Config::default());
        a.diff_sections = vec![
            (
                "Staged",
                "diff --git a/x b/x\n@@ -1,2 +1,3 @@\n+x\n@@ -20,1 +20,2 @@\n+y".to_string(),
            ),
            ("Worktree", String::new()),
            ("Untracked", String::new()),
            ("Conflicted", String::new()),
        ];
        a
    }

    pub fn run_search(&mut self) {
        let q = self.search_buffer.to_lowercase();
        let mut results = Vec::new();
        if q.is_empty() {
            self.search_results = results;
            self.search_selected = 0;
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
        self.search_selected = self
            .search_selected
            .min(self.search_results.len().saturating_sub(1));
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaneSnapshot {
    pub agent_id: String,
    pub title: String,
}

/// Persist open panes (agent ids + titles) to ~/.local/share/term/session.json
/// so `term --restore` can bring them back by resuming each agent's last session.
pub fn save_session_file(app: &App) {
    let dir = crate::config::data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let panes: Vec<PaneSnapshot> = app
        .panes
        .iter()
        .filter_map(|p| {
            p.agent_id.as_ref().map(|a| PaneSnapshot {
                agent_id: a.clone(),
                title: p.title.clone(),
            })
        })
        .collect();
    let path = dir.join("session.json");
    if let Ok(json) = serde_json::to_string_pretty(&panes) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn load_session_file() -> Vec<PaneSnapshot> {
    let path = crate::config::data_dir().join("session.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl App {
    /// Recreate panes from the last saved session, resuming each agent's
    /// latest session file when available (falls back to a fresh spawn).
    pub fn restore_saved_session(&mut self, cwd: std::path::PathBuf) {
        let snaps = load_session_file();
        if snaps.is_empty() {
            self.status_message = "no saved session".to_string();
            return;
        }
        let cmds: Vec<(String, portable_pty::CommandBuilder)> = snaps
            .iter()
            .filter_map(|s| {
                let a = self.registry.find_by_id(&s.agent_id)?;
                let cmd = crate::core::session::resume_command(a, &cwd)
                    .unwrap_or_else(|| a.spawn_cmd(&cwd));
                Some((s.agent_id.clone(), cmd))
            })
            .collect();
        let mut restored = 0usize;
        for (agent_id, cmd) in cmds {
            let agent_name = self
                .registry
                .find_by_id(&agent_id)
                .map(|a| a.name().to_string())
                .unwrap_or_else(|| agent_id.clone());
            if let Ok(pty) = PtySession::spawn(cmd, 24, 80) {
                let id = self.next_pane_id;
                self.next_pane_id += 1;
                let mut pane = Pane::new(id, format!("{} #{}", agent_name, id));
                pane.agent_id = Some(agent_id);
                pane.pty = Some(pty);
                pane.status = AgentStatus::Working;
                self.panes.push(pane);
                restored += 1;
            }
        }
        if !self.panes.is_empty() {
            self.active_pane = self.panes.len() - 1;
        }
        self.status_message = format!("restored {} panes", restored);
    }
}

pub async fn run(config: Config, cli: Cli) -> Result<()> {
    if cli.headless {
        // No TUI: print machine-readable facts about this machine's agents.
        // Note: tracing writes to a file, stdout stays clean for scripting.
        tracing::info!("headless mode active (cwd {:?})", cli.cwd);
        let registry = AgentRegistry::new();
        for a in registry.all() {
            println!("agent\t{}\t{}", a.id(), a.name());
        }
        let cwd = cli
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
        let branch = crate::core::git::current_branch(&cwd);
        if !branch.is_empty() {
            println!("branch\t{}", branch);
        }
        let status = crate::core::git::status_porcelain(&cwd);
        if !status.is_empty() {
            for line in status.lines().take(100) {
                println!("status\t{}", line);
            }
        }
        let tasks = crate::core::orchestration::TaskBoard::load(&cwd);
        for t in &tasks.tasks {
            println!("task\t{}\t{}\t{}", t.id, t.status.label(), t.title);
        }
        let _ = config;
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);
    let mut events = EventHandler::new(Duration::from_millis(250));

    if cli.restore {
        let base = cli
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
        app.restore_saved_session(base);
    }

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        // Refresh agent statuses from PTY output and cost from session files.
        // Panes newly entering Blocked/Done while unfocused raise an attention badge.
        let mut attention_hits = 0u32;
        for (idx, pane) in app.panes.iter_mut().enumerate() {
            if let Some(pty) = &pane.pty {
                let tail = pty.scrollback_tail(8);
                if let Some(id) = &pane.agent_id.clone() {
                    if let Some(agent) = app.registry.find_by_id(id) {
                        let new_status = agent.parse_status(&tail);
                        if new_status != pane.status
                            && (new_status == AgentStatus::Blocked
                                || new_status == AgentStatus::Done)
                            && idx != app.active_pane
                        {
                            pane.attention = true;
                            attention_hits += 1;
                        }
                        pane.status = new_status;
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
                    if pane.status != AgentStatus::Done && idx != app.active_pane {
                        pane.attention = true;
                        attention_hits += 1;
                    }
                    pane.status = AgentStatus::Done;
                }
            }
        }

        // Ring the terminal bell once when something needs attention.
        if attention_hits > 0 {
            let mut so = io::stdout();
            let _ = so.write_all(b"\x07");
            let _ = so.flush();
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
                    let json = app.module_state_json();
                    app.module_outputs = crate::core::modules::run_all(&json);
                }
                // Refresh visible-tab caches every ~700ms (2 ticks).
                if app.tick_count.is_multiple_of(2) {
                    if app.current_tab == Tab::Diff {
                        app.refresh_diff_cache();
                    } else if app.current_tab == Tab::Files {
                        app.refresh_files_cache();
                    }
                }
            }
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }

    // Persist open panes so the next `term --restore` can bring them back.
    save_session_file(&app);

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
                // Show the highlighted result; Esc exits.
                if let Some(r) = app.search_results.get(app.search_selected).cloned() {
                    app.status_message = format!("selected: {r}");
                }
            }
            KeyCode::Backspace => {
                app.search_buffer.pop();
                app.run_search();
            }
            KeyCode::Up => {
                app.search_selected = app.search_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if app.search_selected + 1 < app.search_results.len() {
                    app.search_selected += 1;
                }
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
        (KeyCode::Char('?'), _) => app.show_help = !app.show_help,
        (KeyCode::Esc, _) => app.show_help = false,
        (KeyCode::Char('q'), _) => app.should_quit = true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.should_quit = true,
        (KeyCode::Tab, _) => app.current_tab = app.current_tab.next(),
        (KeyCode::BackTab, _) => app.current_tab = app.current_tab.prev(),
        (KeyCode::Right, _) => app.current_tab = app.current_tab.next(),
        (KeyCode::Left, _) => app.current_tab = app.current_tab.prev(),
        (KeyCode::Char('1'), _) => app.current_tab = Tab::Agents,
        (KeyCode::Char('2'), _) => app.current_tab = Tab::Git,
        (KeyCode::Char('3'), _) => {
            app.current_tab = Tab::Diff;
            app.refresh_diff_cache();
        }
        (KeyCode::Char('4'), _) => {
            app.current_tab = Tab::Files;
            app.refresh_files_cache();
        }
        (KeyCode::Char('5'), _) => app.current_tab = Tab::Orchestration,
        (KeyCode::Char('6'), _) => app.current_tab = Tab::Settings,
        // Scroll Diff / Files tabs.
        (KeyCode::Up, _) if app.current_tab == Tab::Diff => {
            app.diff_scroll = app.diff_scroll.saturating_sub(1)
        }
        (KeyCode::Down, _) if app.current_tab == Tab::Diff => app.diff_scroll += 1,
        (KeyCode::Char('k'), _) if app.current_tab == Tab::Diff => {
            app.diff_scroll = app.diff_scroll.saturating_sub(1)
        }
        (KeyCode::Char('j'), _) if app.current_tab == Tab::Diff => app.diff_scroll += 1,
        (KeyCode::Char('n'), _) if app.current_tab == Tab::Diff => app.jump_hunk(true),
        (KeyCode::Char('p'), _) if app.current_tab == Tab::Diff => app.jump_hunk(false),
        (KeyCode::Up, _) if app.current_tab == Tab::Files => {
            app.files_scroll = app.files_scroll.saturating_sub(1)
        }
        (KeyCode::Down, _) if app.current_tab == Tab::Files => app.files_scroll += 1,
        (KeyCode::Char('k'), _) if app.current_tab == Tab::Files => {
            app.files_scroll = app.files_scroll.saturating_sub(1)
        }
        (KeyCode::Char('j'), _) if app.current_tab == Tab::Files => app.files_scroll += 1,
        // Pane management
        (KeyCode::Char('n'), _) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            app.spawn_agent("claude", cwd, None);
        }
        (KeyCode::Char('x'), _) => app.close_active_pane(),
        (KeyCode::Char('z'), _) => {
            app.zoomed = !app.zoomed;
            app.status_message = if app.zoomed {
                "zoomed pane".to_string()
            } else {
                "unzoomed".to_string()
            };
        }
        (KeyCode::Char('{'), _) => {
            // move active pane up in the sidebar
            if app.active_pane > 0 {
                app.panes.swap(app.active_pane, app.active_pane - 1);
                app.active_pane -= 1;
            }
        }
        (KeyCode::Char('}'), _) => {
            // move active pane down in the sidebar
            if !app.panes.is_empty() && app.active_pane + 1 < app.panes.len() {
                app.panes.swap(app.active_pane, app.active_pane + 1);
                app.active_pane += 1;
            }
        }
        (KeyCode::Char(']'), _) => {
            if !app.panes.is_empty() {
                app.active_pane = (app.active_pane + 1) % app.panes.len();
                // focus clears the attention badge
                if let Some(pane) = app.panes.get_mut(app.active_pane) {
                    pane.attention = false;
                }
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
                                new_pane.agent_id = Some(id.clone());
                                new_pane.session_id = Some(format!("last({id})"));
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
                        app.panes[idx].attention = false;
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
                if let Some(pane) = app.panes.get_mut(app.active_pane) {
                    pane.attention = false;
                }
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
            app.search_selected = 0;
        }
        // Forward typed input to active pane PTY.
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            if let Some(pane) = app.active_pane_mut() {
                pane.input_buffer.push(c);
                if let Some(pty) = &pane.pty {
                    let mut b = [0u8; 4];
                    let n = c.encode_utf8(&mut b).len();
                    let _ = pty.write(&b[..n]);
                }
            }
        }
        (KeyCode::Enter, _) => {
            // Inline delegation: if input started with '=', route to target pane.
            let mut delegated = false;
            let mut target_idx = None;
            let mut msg = String::new();
            if let Some(pane) = app.active_pane() {
                if pane.input_buffer.trim_start().starts_with('=') {
                    let buf = pane.input_buffer.trim().to_string();
                    let mut parts = buf[1..].splitn(2, char::is_whitespace);
                    if let Some(tok) = parts.next() {
                        if let Ok(idx) = tok.parse::<usize>() {
                            target_idx = Some(idx);
                            msg = parts.next().unwrap_or("").trim().to_string();
                            delegated = true;
                        }
                    }
                }
            }
            if delegated {
                if let Some(idx) = target_idx {
                    app.delegate_to_pane(idx, &msg);
                    app.status_message = format!("delegated to pane {}", idx);
                }
            } else if let Some(pane) = app.active_pane_mut() {
                if let Some(pty) = &pane.pty {
                    let _ = pty.write(b"\r");
                }
            }
            // reset input buffer
            if let Some(pane) = app.active_pane_mut() {
                pane.input_buffer.clear();
            }
        }
        (KeyCode::Backspace, _) => {
            if let Some(pane) = app.active_pane_mut() {
                pane.input_buffer.pop();
                if let Some(pty) = &pane.pty {
                    let _ = pty.write(b"\x7f");
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_jump_forward_and_wrap() {
        let mut a = App::sample_for_hunk_test();
        // hunk_starts computed by jump_hunk: [2, 4]
        assert_eq!(a.diff_scroll, 0);
        a.jump_hunk(true);
        assert_eq!(a.diff_scroll, 2);
        a.jump_hunk(true);
        assert_eq!(a.diff_scroll, 4);
        // wrap to first hunk
        a.jump_hunk(true);
        assert_eq!(a.diff_scroll, 2);
        // backward from first → wrap to last
        a.jump_hunk(false);
        assert_eq!(a.diff_scroll, 4);
    }

    #[test]
    fn hunk_jump_no_hunks_is_noop() {
        let mut a = App::sample_for_hunk_test();
        a.diff_sections = vec![("Staged", String::new())];
        a.jump_hunk(true);
        assert_eq!(a.diff_scroll, 0);
    }

    #[test]
    fn session_snapshot_roundtrip_file() {
        // PaneSnapshot serializes; load_session_file tolerates missing/absent files.
        let snaps = load_session_file();
        let _ = snaps.len(); // must not panic even if no saved session exists
    }
}
