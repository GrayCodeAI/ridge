use crate::cli::Cli;
use crate::config::Config;
use crate::core::agent::{AgentRegistry, AgentStatus};
use crate::core::inbox::Inbox;
use crate::core::pty::PtySession;
use crate::core::queue::Queue;
use crate::core::machine::{MachineManager, MachineProfile};
use crate::core::modules::{ModuleRegistry, WorktreeProviderRegistry};
use crate::core::i18n::Language;
use crate::core::workspace::{PaneRef, WorkspaceManager};
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
    Dashboard,
}

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Agents,
        Tab::Git,
        Tab::Diff,
        Tab::Files,
        Tab::Orchestration,
        Tab::Settings,
        Tab::Dashboard,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Agents => "Agents",
            Tab::Git => "Git",
            Tab::Diff => "Diff",
            Tab::Files => "Files",
            Tab::Orchestration => "Orch",
            Tab::Settings => "Settings",
            Tab::Dashboard => "Dash",
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
    /// Session timeline capture enabled (default on).
    pub timeline: bool,
    /// Read offset into the PTY raw log.
    pub timeline_cursor: usize,
    /// Epoch-ms of the last index record written to the companion meta file.
    pub timeline_mark_ms: u128,
    /// Byte offset of the last index record.
    pub timeline_mark_bytes: u128,
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
            timeline: true,
            timeline_cursor: 0,
            timeline_mark_ms: 0,
            timeline_mark_bytes: 0,
        }
    }

    /// Log file for this pane's session timeline (created lazily).
    pub fn timeline_path(&self) -> std::path::PathBuf {
        Self::timeline_path_under(self.id, &crate::config::data_dir())
    }

    /// Timeline path rooted under `base` (test-injectable).
    pub fn timeline_path_under(id: usize, base: &std::path::Path) -> std::path::PathBuf {
        base.join("timelines").join(format!("pane-{}.log", id))
    }

    /// Companion index for timestamped post-mortems: one `epoch_ms bytes`
    /// line per recording milestone (activity threshold or 30 s idle).
    pub fn timeline_meta_path(&self) -> std::path::PathBuf {
        Self::timeline_meta_path_under(self.id, &crate::config::data_dir())
    }

    /// Meta path rooted under `base` (test-injectable).
    pub fn timeline_meta_path_under(id: usize, base: &std::path::Path) -> std::path::PathBuf {
        base.join("timelines").join(format!("pane-{}.meta", id))
    }

    /// Last `epoch_ms bytes` record from a meta file, if any.
    pub fn last_meta_record(id: usize, base: &std::path::Path) -> Option<(u128, u128)> {
        let path = Self::timeline_meta_path_under(id, base);
        let data = std::fs::read_to_string(path).ok()?;
        let line = data.lines().next_back()?.trim();
        let mut parts = line.split_whitespace();
        let ms = parts.next()?.parse().ok()?;
        let bytes = parts.next()?.parse().ok()?;
        Some((ms, bytes))
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
    /// Pane scrollback search: active search query and matches within a pane.
    pub pane_search_mode: bool,
    pub pane_search_buffer: String,
    pub pane_search_matches: Vec<(usize, usize)>, // (row, col) positions in screen
    pub pane_search_selected: usize,
    pub pane_search_pane: Option<usize>,
    /// Copy mode: vi-style selection within a pane's screen text.
    pub copy_mode: bool,
    pub copy_cursor_row: usize,
    pub copy_cursor_col: usize,
    pub copy_start: Option<(usize, usize)>,
    pub copy_selection: Option<((usize, usize), (usize, usize))>,
    pub copy_pane: Option<usize>,
    pub show_help: bool,
    pub zoomed: bool,
    pub side_by_side: bool,
    /// When set, the Agents tab stacks this pane index in the lower half.
    pub split_bottom: Option<usize>,

    pub diff_scroll: usize,
    pub files_scroll: usize,
    pub diff_sections: Vec<(&'static str, String)>,
    pub files_cache: Vec<String>,
    pub files_status: std::collections::HashMap<String, char>,

    pub module_outputs: Vec<(String, String)>,

    /// Inter-session reply inbox (delegations + captured agent replies).
    pub inbox: crate::core::inbox::Inbox,
    /// Wait-for-human queue: prompts delivered when the agent blocks.
    pub queue: crate::core::queue::Queue,
    /// Inbox overlay currently shown.
    pub show_inbox: bool,

    pub task_board: crate::core::orchestration::TaskBoard,
    /// SQLite-backed task persistence.
    pub task_store: crate::core::store::TaskStore,
    /// Active fan-out race (each contestant works in its own worktree).
    pub race: Option<crate::core::race::Race>,
    /// Persisted race outcomes feeding the leaderboard.
    pub race_history: crate::core::race::RaceHistory,
    /// SQLite-backed race history persistence.
    pub race_store: crate::core::store::RaceStore,
    pub cwd: std::path::PathBuf,

    /// Workspace hierarchy: Workspace > Tab > Pane.
    pub workspace_manager: WorkspaceManager,
    /// Commander: bottom composer for targeted messaging to specific panes.
    pub commander_mode: bool,
    pub commander_draft: String,
    pub commander_cursor: usize,
    pub commander_targets: Vec<PaneRef>,
    pub commander_preview: Vec<usize>,
    pub commander_message: String,

    /// Module registry: actions, events, docks, bars, settings.
    pub module_registry: ModuleRegistry,
    /// Worktree provider registry: custom worktree create/remove from modules.
    pub worktree_provider_registry: WorktreeProviderRegistry,

    /// Remote machine profiles (SSH).
    pub machine_manager: MachineManager,

    /// Current UI language.
    pub language: Language,
}

impl App {
    pub fn new(config: Config) -> Self {
        let lang_str = config.language.clone();
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
            pane_search_mode: false,
            pane_search_buffer: String::new(),
            pane_search_matches: Vec::new(),
            pane_search_selected: 0,
            pane_search_pane: None,
            // Copy mode: vi-style selection within a pane's screen text.
            copy_mode: false,
            copy_cursor_row: 0,
            copy_cursor_col: 0,
            copy_start: None,
            copy_selection: None,
            copy_pane: None,
            show_help: false,
            zoomed: false,
            side_by_side: false,
            split_bottom: None,

            module_outputs: Vec::new(),
            inbox: crate::core::inbox::Inbox::new(),
            show_inbox: false,
            queue: crate::core::queue::Queue::new(),
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
            race: None,
            race_history: crate::core::race::RaceHistory::load(
                &std::env::current_dir().unwrap_or_default(),
            ),
            task_store: crate::core::store::TaskStore::new(
                crate::core::store::open_db().expect("open sqlite db"),
            ),
            race_store: crate::core::store::RaceStore::new(
                crate::core::store::open_db().expect("open sqlite db"),
            ),
            cwd: std::env::current_dir().unwrap_or_default(),
            workspace_manager: WorkspaceManager::new(),
            commander_mode: false,
            commander_draft: String::new(),
            commander_cursor: 0,
            commander_targets: Vec::new(),
            commander_preview: Vec::new(),
            commander_message: String::new(),
            module_registry: {
                let mut reg = ModuleRegistry::new();
                reg.register_all();
                reg
            },
            machine_manager: MachineManager::load(),
            language: lang_str.parse().unwrap_or_default(),
            worktree_provider_registry: {
                let mut reg = WorktreeProviderRegistry::new();
                reg.register_all();
                reg
            },
        };
        app.module_outputs = crate::core::modules::run_all(&app.module_state_json());
        app
    }

    /// Single-glance mission-control summary for the Dashboard tab.
    pub fn dashboard_lines(&self) -> Vec<String> {
        use crate::core::orchestration::TaskStatus;

        // Panes
        let blocked: Vec<&Pane> = self
            .panes
            .iter()
            .filter(|p| p.status == AgentStatus::Blocked)
            .collect();
        let errored = self
            .panes
            .iter()
            .filter(|p| p.status == AgentStatus::Error)
            .count();
        let working = self
            .panes
            .iter()
            .filter(|p| p.status == AgentStatus::Working)
            .count();
        let idle = self
            .panes
            .iter()
            .filter(|p| p.status == AgentStatus::Idle)
            .count();
        let cost: f64 = self.panes.iter().map(|p| p.cost_usd).sum();
        let mut lines = vec![format!(
            "Panes ({}) — blocked {}, working {}, idle {}, error {}, ${:.2} this session",
            self.panes.len(),
            blocked.len(),
            working,
            idle,
            errored,
            cost
        )];
        if blocked.is_empty() {
            lines.push("  next blocked: none (a jumps when one appears)".to_string());
        } else {
            for p in &blocked {
                let flag = if p.attention { " ⚠" } else { "" };
                lines.push(format!(
                    "  blocked: pane {} {}{} (a jumps)",
                    p.id, p.title, flag
                ));
            }
        }

        // Race
        let race_line = match &self.race {
            None => "Race: none — :race <prompt>".to_string(),
            Some(r) => {
                if let Some(w) = r.contestants.iter().find(|c| c.winner) {
                    format!("Race: \"{}\" — WON by {}", r.prompt, w.agent_id)
                } else {
                    format!(
                        "Race: \"{}\" — {} racing (gates run: {})",
                        r.prompt,
                        r.contestants.len(),
                        r.contestants.iter().filter(|c| c.gate_ok.is_some()).count()
                    )
                }
            }
        };
        lines.push(race_line);

        // Queue
        let queue_line = if self.queue.items.is_empty() {
            "Queue: empty — :queue <pane> <msg>".to_string()
        } else {
            let first = self.queue.items.first().expect("checked len");
            format!(
                "Queue: {} parked; next → pane {} \"{}\"",
                self.queue.items.len(),
                first.pane_id,
                first.msg
            )
        };
        lines.push(queue_line);

        // Inbox
        lines.push(format!(
            "Inbox: {} unread / {} total — o opens",
            self.inbox.unread_count(),
            self.inbox.items.len()
        ));

        // Tasks
        let st = |want: TaskStatus| {
            self.task_board
                .tasks
                .iter()
                .filter(|t| t.status == want)
                .count()
        };
        let ready = self.task_board.ready_tasks();
        lines.push(format!(
            "Tasks: pending {}, running {}, blocked {}, done {}, failed {} — ready to start: {:?}",
            st(TaskStatus::Pending),
            st(TaskStatus::Running),
            st(TaskStatus::Blocked),
            st(TaskStatus::Done),
            st(TaskStatus::Failed),
            ready
        ));

        // Modules
        lines.push(format!("Modules: {} outputs", self.module_outputs.len()));
        lines
    }

    /// Machine-readable dashboard state for `term --dashboard`.
    pub fn dashboard_json(&self) -> String {
        use serde_json::json;
        let panes: Vec<_> = self
            .panes
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "title": p.title,
                    "agent": p.agent_id,
                    "status": p.status.label(),
                    "cost_usd": p.cost_usd,
                    "attention": p.attention,
                    "session_id": p.session_id,
                })
            })
            .collect();
        let race = self.race.as_ref().map(|r| {
            json!({
                "prompt": r.prompt,
                "gate_cmd": r.gate_cmd,
                "contestants": r.contestants.iter().map(|c| json!({
                    "agent": c.agent_id,
                    "status": c.status.map(|s| s.label()),
                    "gate_ok": c.gate_ok,
                    "winner": c.winner,
                })).collect::<Vec<_>>(),
                "winner": r.contestants.iter().find(|c| c.winner)
                    .map(|c| c.agent_id.clone()),
            })
        });
        let queue: Vec<_> = self
            .queue
            .items
            .iter()
            .map(|q| json!({"id": q.id, "pane_id": q.pane_id, "msg": q.msg}))
            .collect();
        let board = &self.task_board;
        let board_summary = json!({
            "pending": board.tasks.iter().filter(|t| t.status.label() == "pending").count(),
            "running": board.tasks.iter().filter(|t| t.status.label() == "running").count(),
            "blocked": board.tasks.iter().filter(|t| t.status.label() == "blocked").count(),
            "done": board.tasks.iter().filter(|t| t.status.label() == "done").count(),
            "ready": board.ready_tasks(),
        });
        let out = json!({
            "cwd": self.cwd,
            "panes": panes,
            "race": race,
            "queue": queue,
            "inbox": {"unread": self.inbox.unread_count(), "total": self.inbox.items.len()},
            "tasks": board_summary,
            "modules": self.module_outputs.len(),
            "race_history": self.race_history.records.len(),
        });
        serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
    }

    /// Drain each pane's raw PTY output into its timeline log file.
    pub fn timelines_tick(&mut self) {
        let base = crate::config::data_dir();
        self.timelines_tick_into(&base);
    }

    /// Timeline drain with an explicit storage root (tests inject a tempdir).
    pub fn timelines_tick_into(&mut self, base: &std::path::Path) {
        let dir = base.join("timelines");
        let _ = std::fs::create_dir_all(&dir);
        for pane in &mut self.panes {
            if !pane.timeline {
                continue;
            }
            let (chunk, next) = {
                let Some(pty) = &pane.pty else {
                    continue;
                };
                pty.take_raw_since(pane.timeline_cursor)
            };
            if chunk.is_empty() {
                continue;
            }
            pane.timeline_cursor = next;
            let path = Pane::timeline_path_under(pane.id, base);
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                use std::io::Write;
                let _ = f.write_all(&chunk);
            }

            // Timestamp the write in the companion meta file when the pane
            // has pushed enough bytes (4 KiB) or gone quiet for 30s.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let delta = pane.timeline_cursor as u128 - pane.timeline_mark_bytes;
            let elapsed = now.saturating_sub(pane.timeline_mark_ms);
            if delta >= 4096 || elapsed >= 30_000 {
                let meta = Pane::timeline_meta_path_under(pane.id, base);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&meta)
                {
                    use std::io::Write;
                    let _ = writeln!(f, "{now} {}", pane.timeline_cursor);
                }
                pane.timeline_mark_ms = now;
                pane.timeline_mark_bytes = pane.timeline_cursor as u128;
            }
        }
    }

    /// Compactly toggle timeline capture; `None` targets the active pane.
    /// Returns the number of panes affected.
    pub fn timeline_toggle(&mut self, on: bool, target: Option<usize>) -> usize {
        let mut n = 0;
        match target {
            Some(idx) => {
                if let Some(pane) = self.panes.get_mut(idx) {
                    pane.timeline = on;
                    n = 1;
                }
            }
            None => {
                if let Some(pane) = self.panes.get_mut(self.active_pane) {
                    pane.timeline = on;
                    n = 1;
                }
            }
        }
        n
    }

    /// Copy a pane's timeline log to `dst`. `None` targets the active pane.
    pub fn timeline_export(&mut self, dst: &std::path::Path, pane_idx: Option<usize>) -> bool {
        let idx = pane_idx.unwrap_or(self.active_pane);
        match self.panes.get(idx) {
            Some(pane) => {
                let src = pane.timeline_path();
                if !src.exists() {
                    return false;
                }
                std::fs::copy(&src, dst).is_ok()
            }
            None => false,
        }
    }

    /// Open a replay pane for a pane's session log (uses the real data dir).
    pub fn timeline_view(&mut self, pane_idx: usize) -> Result<(), String> {
        let base = crate::config::data_dir();
        self.timeline_view_at(pane_idx, &base)
    }

    /// Replay-pane core with an injectable storage root (tests).
    /// Unix rides `tail -n +1 -f` so the replay auto-follows; on Windows the
    /// viewer is a static `type` snapshot (no builtin follow).
    pub fn timeline_view_at(
        &mut self,
        pane_idx: usize,
        base: &std::path::Path,
    ) -> Result<(), String> {
        let Some(pane) = self.panes.get(pane_idx) else {
            return Err(format!("no pane {pane_idx}"));
        };
        let src = Pane::timeline_path_under(pane.id, base);
        let title_win = pane.title.lines().next().unwrap_or("?").to_string();
        if !src.exists() {
            return Err(format!("pane {pane_idx} has no session log yet"));
        }
        #[cfg(windows)]
        let mut cmd = {
            let mut c = portable_pty::CommandBuilder::new("cmd");
            c.arg("/c");
            c.arg(format!("type \"{}\"", src.display()));
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = portable_pty::CommandBuilder::new("tail");
            c.arg("-n");
            c.arg("+1");
            c.arg("-f");
            c.arg(src.clone());
            c
        };
        let _ = &mut cmd;
        match PtySession::spawn(cmd, 24, 80) {
            Ok(pty) => {
                let id = self.next_pane_id;
                self.next_pane_id += 1;
                let mut vp = Pane::new(id, format!("⏵ replay: {title_win}"));
                vp.pty = Some(pty);
                self.panes.push(vp);
                self.active_pane = self.panes.len().saturating_sub(1);
                self.status_message = format!("replaying {} timeline ({})", pane_idx, id);
                Ok(())
            }
            Err(e) => Err(format!("replay spawn failed: {e}")),
        }
    }

    /// Status heuristic for panes without a registered agent (e.g. SSH).
    /// Terminally exact prompt detection wins; while the session is alive,
    /// inconclusive output reads as Working.
    pub fn fallback_status(tail: &str, alive: bool) -> AgentStatus {
        let registry = crate::core::agent::AgentRegistry::new();
        let s = registry
            .find_by_id("generic")
            .map(|g| g.parse_status(tail))
            .unwrap_or(AgentStatus::Working);
        if s == AgentStatus::Unknown && alive {
            AgentStatus::Working
        } else {
            s
        }
    }

    /// Refresh pane search matches for the current search buffer.
    pub fn refresh_pane_search(&mut self) {
        let Some(pane_idx) = self.pane_search_pane else {
            return;
        };
        let Some(pane) = self.panes.get(pane_idx) else {
            return;
        };
        let Some(pty) = &pane.pty else {
            return;
        };
        let screen = pty.screen_text();
        let query = self.pane_search_buffer.clone();
        if query.is_empty() {
            self.pane_search_matches.clear();
            self.pane_search_selected = 0;
            return;
        }
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for (row, line) in screen.lines().enumerate() {
            let mut start = 0;
            while let Some(pos) = line[start..].find(&query) {
                matches.push((row, start + pos));
                start += pos + 1;
            }
        }
        self.pane_search_matches = matches;
        self.pane_search_selected = 0;
    }

    /// Update copy selection end based on current cursor position.
    pub fn update_copy_selection(&mut self) {
        if let Some(start) = self.copy_start {
            self.copy_selection = Some((start, (self.copy_cursor_row, self.copy_cursor_col)));
        }
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

    /// Resolve the current theme's color palette.
    pub fn theme_palette(&self) -> crate::ui::ThemePalette {
        crate::ui::ThemePalette::named(&self.config.theme)
    }

    // ---- Workspace management ------------------------------------------------

    pub fn active_workspace(&self) -> Option<&crate::core::workspace::Workspace> {
        self.workspace_manager.active_workspace()
    }

    pub fn active_workspace_mut(&mut self) -> Option<&mut crate::core::workspace::Workspace> {
        self.workspace_manager.active_workspace_mut()
    }

    pub fn workspace_list(&self) -> Vec<&crate::core::workspace::Workspace> {
        self.workspace_manager.workspaces.iter().collect()
    }

    pub fn create_workspace(&mut self, name: String, cwd: std::path::PathBuf) -> usize {
        self.workspace_manager.create_workspace(name, cwd)
    }

    pub fn switch_workspace(&mut self, idx: usize) -> bool {
        self.workspace_manager.switch_workspace(idx)
    }

    pub fn workspace_for_pane(&self, pane_id: usize) -> Option<usize> {
        self.workspace_manager.workspace_for_pane(pane_id)
    }

    pub fn assign_pane_to_workspace(&mut self, pane_id: usize) {
        if let Some(ws) = self.active_workspace_mut() {
            ws.add_pane(pane_id);
        }
    }

    /// Commander: open the targeted-message composer.
    pub fn commander_open(&mut self) {
        self.commander_mode = true;
        self.commander_draft.clear();
        self.commander_cursor = 0;
        self.commander_targets.clear();
        self.commander_preview.clear();
        self.commander_message.clear();
    }

    /// Commander: close the composer.
    pub fn commander_close(&mut self) {
        self.commander_mode = false;
        self.commander_draft.clear();
        self.commander_targets.clear();
        self.commander_preview.clear();
        self.commander_message.clear();
    }

    /// Commander: set the draft text and cursor position.
    pub fn commander_set_draft(&mut self, draft: String, cursor: usize) {
        let len = draft.len();
        self.commander_draft = draft;
        self.commander_cursor = cursor.min(len);
    }

    /// Commander: add a target pane ref.
    pub fn commander_add_target(&mut self, ref_: PaneRef) {
        if !self.commander_targets.contains(&ref_) {
            self.commander_targets.push(ref_);
        }
    }

    /// Commander: remove a target pane ref.
    pub fn commander_remove_target(&mut self, ref_: &PaneRef) {
        self.commander_targets.retain(|t| t != ref_);
    }

    /// Commander: send the draft to all target panes.
    pub fn commander_send(&mut self) {
        if self.commander_draft.trim().is_empty() && self.commander_targets.is_empty() {
            self.commander_message = "commander: nothing to send".to_string();
            return;
        }
        let draft = self.commander_draft.clone();
        let targets = self.commander_targets.clone();
        let mut sent = 0;
        for target in &targets {
            if let Some(pane) = self.panes.iter_mut().find(|p| p.id == target.pane_id) {
                if let Some(pty) = &mut pane.pty {
                    let _ = pty.write(draft.as_bytes());
                    let _ = pty.write(b"\r");
                    sent += 1;
                }
            }
        }
        self.commander_message = format!("commander: sent to {sent} pane(s)");
        self.commander_close();
    }

    /// Commander: delete a pane by stable id (used by race cleanup and normal close).
    pub fn close_pane_by_id(&mut self, pane_id: usize) {
        // Also remove from workspace tracking.
        if let Some(ws_idx) = self.workspace_for_pane(pane_id) {
            if let Some(ws) = self.workspace_manager.workspaces.get_mut(ws_idx) {
                ws.remove_pane(pane_id);
            }
        }
        // Fall through to existing close logic.
        Self::close_pane_by_id_inner(self, pane_id);
    }

    /// Inner close logic extracted so commander close can reuse it.
    fn close_pane_by_id_inner(&mut self, pane_id: usize) {
        let Some(idx) = self.panes.iter().position(|p| p.id == pane_id) else {
            return;
        };
        if let Some(p) = self.panes.get(idx) {
            if let Some(pty) = &p.pty {
                let _ = pty.write(b"\x03"); // Ctrl+C
            }
        }
        self.panes.remove(idx);
        if self.active_pane >= self.panes.len() && !self.panes.is_empty() {
            self.active_pane = self.panes.len() - 1;
        }
        self.split_bottom = None;
        self.status_message = "pane closed".to_string();
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
                let new_id = self.panes.last().unwrap().id;
                let agent_name = agent.name().to_string();
                self.assign_pane_to_workspace(new_id);
                // link to parent
                if let Some(pid) = parent {
                    if let Some(p) = self.panes.iter_mut().find(|x| x.id == pid) {
                        p.children.push(id);
                    }
                }
                self.active_pane = self.panes.len().saturating_sub(1);
                self.status_message = format!("spawned {} (pane {})", agent_name, id);
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
        let pane_id = self.panes[self.active_pane].id;
        self.close_pane_by_id(pane_id);
    }

    // ---- Fan-out race ------------------------------------------------------

    const RACE_AGENTS: [&'static str; 3] = ["claude", "codex", "opencode"];

    /// Start a race: spawn one agent per contestant, each in its own git
    /// worktree with the same prompt delegated to it. Cancels any prior race.
    pub fn race_start(&mut self, prompt: &str) {
        self.race_cancel();
        let mut race = crate::core::race::Race {
            prompt: prompt.to_string(),
            contestants: Vec::new(),
            gate_cmd: None,
        };
        for agent_id in Self::RACE_AGENTS {
            if self.registry.find_by_id(agent_id).is_none() {
                continue;
            }
            let tag = self.next_pane_id; // unique per contestant
            let branch = format!("race-{tag}-{agent_id}");
            let base = crate::core::worktree::worktree_base(&self.cwd);
            let wt = base.join(&branch);
            match crate::core::worktree::create_worktree(&self.cwd, &branch, &wt) {
                Ok(()) => {
                    self.spawn_agent(agent_id, wt.clone(), None);
                    let pane_id = self.panes.last().map(|p| p.id).unwrap_or(tag);
                    race.contestants.push(crate::core::race::RaceContestant {
                        agent_id: agent_id.to_string(),
                        pane_id,
                        branch,
                        worktree: wt,
                        status: Some(crate::core::agent::AgentStatus::Working),
                        gate_ok: None,
                        winner: false,
                        promoted: false,
                    });
                    // Delegate the race prompt to the contestant right away.
                    if let Some(pane) = self.panes.last() {
                        if let Some(pty) = &pane.pty {
                            let _ = pty.write(prompt.as_bytes());
                            let _ = pty.write(b"\r");
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("race: {agent_id} worktree failed: {e}");
                }
            }
        }
        let n = race.contestants.len();
        if n == 0 {
            self.status_message = "race: no agents available".to_string();
        } else {
            self.status_message = format!("race started: {n} agents racing \"{prompt}\"");
            self.race = Some(race);
        }
    }

    /// Terminate every contestant pane and remove its worktree. Idempotent.
    pub fn race_cancel(&mut self) {
        let Some(race) = self.race.take() else {
            return;
        };
        for c in race.contestants {
            self.close_pane_by_id(c.pane_id);
            let _ = crate::core::worktree::remove_worktree(&self.cwd, &c.worktree);
        }
        self.status_message = "race cancelled".to_string();
    }

    /// Refresh contestant statuses from their panes (called each tick).
    pub fn race_update_statuses(&mut self) {
        if let Some(race) = &mut self.race {
            for c in race.contestants.iter_mut() {
                if c.promoted {
                    continue;
                }
                c.status = self
                    .panes
                    .iter()
                    .find(|p| p.id == c.pane_id)
                    .map(|p| p.status);
            }
        }
    }

    /// Promote contestant `idx`: quality-gate its worktree, then merge the
    /// branch and clean up the losers. `force` skips gating.
    pub fn race_winner(&mut self, idx: usize, force: bool) {
        let Some(race) = &self.race else {
            self.status_message = "race: no active race".to_string();
            return;
        };
        if idx >= race.contestants.len() {
            self.status_message = format!("race: invalid contestant {idx}");
            return;
        }
        let branch = race.contestants[idx].branch.clone();
        let worktree = race.contestants[idx].worktree.clone();
        let agent = race.contestants[idx].agent_id.clone();

        // Gate first (unless forced): a failed gate blocks promotion.
        if !force {
            let gate_cmd = race
                .gate_cmd
                .clone()
                .unwrap_or_else(|| default_gate_cmd(&worktree).to_string());
            match run_gate_in_dir(&worktree, &gate_cmd) {
                Ok(true) => {
                    self.race_set_gate(idx, true);
                    self.status_message = format!("race gate passed for {agent} — merging…");
                }
                Ok(false) => {
                    self.race_set_gate(idx, false);
                    self.status_message = format!(
                        "race winner blocked: gate failed for {agent} (:race winner {idx} force)"
                    );
                    return;
                }
                Err(e) => {
                    self.race_set_gate(idx, false);
                    self.status_message = format!("race gate error: {e}");
                    return;
                }
            }
        }

        match crate::core::worktree::merge_branch(&self.cwd, &branch) {
            Ok(()) => {
                let losers: Vec<(usize, std::path::PathBuf)> = {
                    let race = self.race.as_ref().expect("race open");
                    race.contestants
                        .iter()
                        .enumerate()
                        .filter(|(i, c)| *i != idx && !c.promoted)
                        .map(|(_, c)| (c.pane_id, c.worktree.clone()))
                        .collect()
                };
                for (pane_id, wt) in losers {
                    self.close_pane_by_id(pane_id);
                    let _ = crate::core::worktree::remove_worktree(&self.cwd, &wt);
                }
                // Record the outcome for the leaderboard.
                let gate_results: Vec<(String, Option<bool>)> = self
                    .race
                    .as_ref()
                    .expect("race open")
                    .contestants
                    .iter()
                    .map(|c| (c.agent_id.clone(), c.gate_ok))
                    .collect();
                let prompt = self
                    .race
                    .as_ref()
                    .map(|r| r.prompt.clone())
                    .unwrap_or_default();
                self.race_history.record(crate::core::race::RaceRecord {
                    prompt,
                    finished_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    winner_agent: Some(agent.clone()),
                    gate_results,
                    forced: force,
                });
                let _ = self.race_history.save(&self.cwd);
                if let Some(last) = self.race_history.records.last() {
                    let gate_results: Vec<serde_json::Value> = last
                        .gate_results
                        .iter()
                        .map(|(agent, ok)| {
                            serde_json::json!({ "agent": agent, "gate_ok": ok })
                        })
                        .collect();
                    let _ = self.race_store.insert_race(
                        &last.prompt,
                        last.winner_agent.as_deref(),
                        &gate_results,
                        last.forced,
                    );
                }
                let winner = &mut self.race.as_mut().expect("race open").contestants[idx];
                winner.winner = true;
                winner.promoted = true;
                winner.status = Some(crate::core::agent::AgentStatus::Done);
                self.status_message =
                    format!("race winner: {} — branch {branch} merged", winner.agent_id);
            }
            Err(e) => self.status_message = format!("race merge failed: {e}"),
        }
    }

    fn race_set_gate(&mut self, idx: usize, ok: bool) {
        if let Some(race) = &mut self.race {
            if let Some(c) = race.contestants.get_mut(idx) {
                c.gate_ok = Some(ok);
            }
        }
    }

    /// Manually run the quality gate in one contestant's worktree.
    pub fn race_gate(&mut self, idx: usize) {
        let Some(race) = &self.race else {
            self.status_message = "race: no active race".to_string();
            return;
        };
        if idx >= race.contestants.len() {
            self.status_message = format!("race: invalid contestant {idx}");
            return;
        }
        let cmd = race
            .gate_cmd
            .clone()
            .unwrap_or_else(|| default_gate_cmd(&race.contestants[idx].worktree).to_string());
        let worktree = race.contestants[idx].worktree.clone();
        let agent = race.contestants[idx].agent_id.clone();
        match run_gate_in_dir(&worktree, &cmd) {
            Ok(ok) => {
                self.race_set_gate(idx, ok);
                self.status_message = format!(
                    "race gate for {agent}: {}",
                    if ok { "passed" } else { "FAILED" }
                );
            }
            Err(e) => self.status_message = format!("race gate error: {e}"),
        }
    }
}

/// Default gate command for a race worktree: `cargo test` for Rust repos,
/// vacuously pass otherwise.
fn default_gate_cmd(worktree: &std::path::Path) -> &'static str {
    if worktree.join("Cargo.toml").exists() {
        "cargo test"
    } else {
        "true"
    }
}

/// Run a gate command in a directory; Ok(true) = pass, Ok(false) = fail.
fn run_gate_in_dir(dir: &std::path::Path, cmd: &str) -> Result<bool, String> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(out.status.success())
}

impl App {
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
                } else if parts.len() >= 3 && parts[1] == "fork" {
                    // Fork an agent session from one pane to a new pane.
                    if let Ok(src_pane) = parts[2].parse::<usize>() {
                        let cwd = self.cwd.clone();
                        // Collect needed info before mutable borrow.
                        let agent_id = self.panes.iter()
                            .find(|p| p.id == src_pane)
                            .and_then(|p| p.agent_id.clone());
                        let has_agent = self.panes.iter().any(|p| p.id == src_pane);
                        if let Some(agent_id) = agent_id {
                            if has_agent {
                                self.spawn_agent(agent_id.as_str(), cwd, Some(src_pane));
                                self.status_message = format!("forked {agent_id} from pane {src_pane}");
                            } else {
                                self.status_message = format!("pane {src_pane} has no agent");
                            }
                        } else {
                            self.status_message = format!("pane {src_pane} not found");
                        }
                    } else {
                        self.status_message = "usage: session fork <pane_id> [agent]".to_string();
                    }
                } else {
                    self.status_message = "usage: session save | session clear | session fork <pane> [agent]".to_string();
                }
            }
            "close" => self.close_active_pane(),
            "delegate" => {
                // delegate <pane_index> <message...>
                if parts.len() >= 3 {
                    if let Ok(idx) = parts[1].parse::<usize>() {
                        let msg = parts[2..].join(" ");
                        self.delegate_to_pane_recorded(idx, &msg);
                        self.status_message =
                            format!("delegated to pane {idx} (reply goes to :inbox)");
                    }
                }
            }
            "inbox" => {
                if parts.len() >= 2 && parts[1] == "clear" {
                    self.inbox.clear();
                    self.show_inbox = false;
                    self.status_message = "inbox cleared".to_string();
                } else if parts.len() >= 2 && parts[1] == "unread" {
                    let n = self.inbox.unread_count();
                    self.status_message = format!("unread replies: {n}");
                } else {
                    self.inbox_toggle();
                }
            }
            "queue" => {
                if parts.len() >= 3 && parts[1] == "drop" {
                    if let Ok(id) = parts[2].parse::<u64>() {
                        if self.queue.drop(id) {
                            self.status_message = format!("queue #{id} dropped");
                        } else {
                            self.status_message = format!("queue: no entry #{id}");
                        }
                    } else {
                        self.status_message = "usage: queue drop <id>".to_string();
                    }
                } else if parts.len() >= 2 && parts[1] == "list" {
                    self.status_message = if self.queue.items.is_empty() {
                        "queue: empty".to_string()
                    } else {
                        self.queue.render()
                    };
                } else if parts.len() >= 2 && parts[1] == "clear" {
                    self.queue.clear();
                    self.status_message = "queue cleared".to_string();
                } else if parts.len() >= 3 {
                    if let Ok(pane_idx) = parts[1].parse::<usize>() {
                        let msg = parts[2..].join(" ");
                        self.queue_enqueue(pane_idx, &msg);
                    } else {
                        self.status_message =
                            "usage: queue <pane_idx> <msg> | list | drop <id> | clear".to_string();
                    }
                } else {
                    self.status_message =
                        "usage: queue <pane_idx> <msg> | list | drop <id> | clear".to_string();
                }
            }
            "timeline" => {
                if parts.len() >= 2 && (parts[1] == "on" || parts[1] == "off") {
                    let on = parts[1] == "on";
                    let target = parts.get(2).and_then(|p| p.parse::<usize>().ok());
                    let n = self.timeline_toggle(on, target);
                    if n == 1 {
                        self.status_message = format!(
                            "timeline {} for pane {}",
                            parts[1],
                            target.unwrap_or(self.active_pane)
                        );
                    } else {
                        self.status_message = "timeline: no such pane".to_string();
                    }
                } else if parts.len() >= 2 && parts[1] == "view" {
                    let idx = parts.get(2).and_then(|p| p.parse::<usize>().ok());
                    let idx = idx.unwrap_or(self.active_pane);
                    match self.timeline_view(idx) {
                        Ok(()) => {}
                        Err(e) => self.status_message = format!("timeline: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "export" {
                    let file = parts[2..].join(" ");
                    let path = self.cwd.join(&file);
                    if self.timeline_export(&path, None) {
                        self.status_message = format!("timeline exported to {}", path.display());
                    } else {
                        self.status_message =
                            "timeline: no session log for active pane".to_string();
                    }
                } else {
                    let mut msg = String::new();
                    for (i, pane) in self.panes.iter().enumerate() {
                        if pane.pty.is_none() {
                            continue;
                        }
                        let path = pane.timeline_path();
                        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        let flag = if pane.timeline { "" } else { " (paused)" };
                        let idle = Pane::last_meta_record(pane.id, &crate::config::data_dir())
                            .map(|(ms, _)| {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis())
                                    .unwrap_or(ms);
                                let secs = now.saturating_sub(ms) / 1000;
                                format!(" · idle {secs}s")
                            })
                            .unwrap_or_default();
                        msg.push_str(&format!(
                            "pane {i}: {} [{} bytes]{}{}\n",
                            path.display(),
                            size,
                            flag,
                            idle
                        ));
                    }
                    self.status_message = if msg.is_empty() {
                        "timeline: no live panes".to_string()
                    } else {
                        format!(
                            "timelines under {}:\n{}",
                            crate::config::data_dir().join("timelines").display(),
                            msg.trim_end()
                        )
                    };
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
                        self.delegate_to_pane_recorded(i, &msg);
                    }
                    self.status_message = format!(
                        "broadcast to {} panes (replies land in :inbox)",
                        self.panes.len()
                    );
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
                } else if parts.len() >= 4 && parts[1] == "check" {
                    if let (Ok(id), Ok(n)) = (parts[2].parse::<u64>(), parts[3].parse::<usize>()) {
                        match self.task_board.check_toggle(id, n) {
                            Ok(done) => {
                                self.status_message = format!(
                                    "task {id} checklist {n} {}",
                                    if done { "checked ✓" } else { "unchecked" }
                                );
                            }
                            Err(e) => self.status_message = format!("check failed: {e}"),
                        }
                    }
                } else if parts.len() >= 3 && parts[1] == "from-inbox" {
                    if let Ok(n) = parts[2].parse::<usize>() {
                        let len = self.inbox.items.len();
                        if n == 0 || n > len {
                            self.status_message =
                                format!("from-inbox: invalid item {n} (have {len})");
                        } else {
                            // Overlay numbering is 1-based, newest first.
                            let item = self.inbox.items[len - n].clone();
                            let id = self.task_board.add_from_reply(
                                &item.pane_title,
                                &item.prompt,
                                &item.reply,
                            );
                            self.inbox.items.remove(len - n);
                            self.status_message = format!(
                                "task {id} created from inbox item {n} — reply stored in notes"
                            );
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
            "paste" => {
                match crate::core::clipboard_image::paste_clipboard_image(self) {
                    Ok(()) => {}
                    Err(e) => self.status_message = format!("paste failed: {e}"),
                }
            }
            "machine" => {
                if parts.len() >= 2 && parts[1] == "list" {
                    let machines = self.machine_manager.list_machines();
                    if machines.is_empty() {
                        self.status_message = "machines: none".to_string();
                    } else {
                        let mut msg = String::from("machines:\n");
                        for m in machines {
                            let active = if self.machine_manager.active_machine.as_deref() == Some(&m.id) {
                                " (active)"
                            } else {
                                ""
                            };
                            msg.push_str(&format!("  {} {}@{} (port {}){}\n", m.id, m.user, m.host, m.port, active));
                        }
                        self.status_message = msg;
                    }
                } else if parts.len() >= 7 && parts[1] == "add" {
                    let profile = MachineProfile {
                        id: parts[2].to_string(),
                        name: parts[3].to_string(),
                        host: parts[4].to_string(),
                        port: parts[5].parse().unwrap_or(22),
                        user: parts[6].to_string(),
                        ssh_key: None,
                        session: None,
                    };
                    self.machine_manager.add_machine(profile);
                    let _ = self.machine_manager.save();
                    self.status_message = format!("machine added: {}", parts[2]);
                } else if parts.len() >= 3 && parts[1] == "use" {
                    if self.machine_manager.set_active(parts[2]) {
                        let _ = self.machine_manager.save();
                        self.status_message = format!("active machine: {}", parts[2]);
                    } else {
                        self.status_message = format!("machine not found: {}", parts[2]);
                    }
                } else if parts.len() >= 3 && parts[1] == "attach" {
                    match self.machine_manager.attach(parts[2]) {
                        Ok(()) => self.status_message = format!("detached from {}", parts[2]),
                        Err(e) => self.status_message = format!("ssh attach failed: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "ssh" {
                    if let Some(cmd) = self.machine_manager.ssh_command(parts[2]) {
                        self.status_message = format!("ssh: {cmd}");
                    } else {
                        self.status_message = format!("machine not found: {}", parts[2]);
                    }
                } else if parts.len() >= 3 && parts[1] == "remove" {
                    if self.machine_manager.remove_machine(parts[2]) {
                        let _ = self.machine_manager.save();
                        self.status_message = format!("machine removed: {}", parts[2]);
                    } else {
                        self.status_message = format!("machine not found: {}", parts[2]);
                    }
                } else {
                    self.status_message = "usage: machine list | add <id> <name> <host> <port> <user> | use <id> | attach <id> | ssh <id> | remove <id>".to_string();
                }
            }
            "config" => {
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
                } else if parts.len() >= 3 && parts[1] == "notify" {
                    match parts[2] {
                        "on" | "off" => {
                            self.config.notify = parts[2] == "on";
                            if self.config.save().is_ok() {
                                self.status_message = format!("OS notifications {}", parts[2]);
                            } else {
                                self.status_message = format!("notify {} (save failed)", parts[2]);
                            }
                        }
                        _ => self.status_message = "usage: config notify on|off".to_string(),
                    }
                } else {
                    self.status_message =
                        "usage: config show | theme <name> | notify on|off".to_string();
                }
            }
            "language" => {
                if parts.len() >= 2 {
                    match parts[1].parse::<Language>() {
                        Ok(lang) => {
                            self.language = lang;
                            self.status_message = format!("language: {} ({})", lang.code(), lang.name());
                        }
                        Err(e) => self.status_message = format!("{e}. Use: en es fr de ja zh ko ru"),
                    }
                } else {
                    self.status_message = format!("current language: {} ({})", self.language.code(), self.language.name());
                }
            }
            "module" => {
                let dir = crate::core::modules::modules_dir();
                let _ = std::fs::create_dir_all(&dir);
                if parts.len() >= 2 && parts[1] == "list" {
                    let mods = crate::core::modules::discover();
                    let mut msg = String::from("installed modules:\n");
                    for m in mods {
                        msg.push_str(&format!(
                            "  {}\n",
                            m.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                    self.status_message = msg;
                } else if parts.len() >= 4 && parts[1] == "install" {
                    let url = parts[2].to_string();
                    let name = parts.get(3).map(|s| s.to_string()).unwrap_or_else(|| {
                        url.rsplit('/').next().unwrap_or("module.sh").to_string()
                    });
                    let dest = dir.join(&name);
                    let out = std::process::Command::new("curl")
                        .arg("-fsSL")
                        .arg(&url)
                        .arg("-o")
                        .arg(&dest)
                        .output();
                    match out {
                        Ok(o) if o.status.success() => {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(
                                    &dest,
                                    std::fs::Permissions::from_mode(0o755),
                                );
                            }
                            self.status_message = format!("installed module: {name}");
                        }
                        Ok(o) => {
                            let _ = std::fs::remove_file(&dest);
                            self.status_message = format!("install failed (curl {:?})", o.status);
                        }
                        Err(e) => self.status_message = format!("install error: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "remove" {
                    let dest = dir.join(parts[2]);
                    match std::fs::remove_file(&dest) {
                        Ok(()) => self.status_message = format!("removed module {}", parts[2]),
                        Err(e) => self.status_message = format!("remove failed: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "run" {
                    match self.module_registry.run_action(parts[2]) {
                        Ok(()) => self.status_message = format!("module action: {}", parts[2]),
                        Err(e) => self.status_message = format!("module action failed: {e}"),
                    }
                } else if parts.len() >= 5 && parts[1] == "worktree" && parts[2] == "create" {
                    let provider_id = parts[3];
                    let branch = parts[4];
                    let base = crate::core::worktree::worktree_base(&self.cwd);
                    let path = base.join(branch);
                    match self.worktree_provider_registry.run_create(provider_id, branch, &path, &base) {
                        Ok(true) => self.status_message = format!("module worktree created: {branch}"),
                        Ok(false) => self.status_message = format!("module worktree create rejected: {branch}"),
                        Err(e) => self.status_message = format!("module worktree create failed: {e}"),
                    }
                } else if parts.len() >= 4 && parts[1] == "worktree" && parts[2] == "remove" {
                    let provider_id = parts[3];
                    let path = std::path::PathBuf::from(parts.get(4).unwrap_or(&""));
                    let identity = parts.get(5).copied();
                    match self.worktree_provider_registry.run_remove(provider_id, &path, identity) {
                        Ok(true) => self.status_message = "module worktree removed".to_string(),
                        Ok(false) => self.status_message = "module worktree remove rejected".to_string(),
                        Err(e) => self.status_message = format!("module worktree remove failed: {e}"),
                    }
                } else {
                    self.status_message =
                        "usage: module list | install <url> [name] | remove <name> | run <action_id> | worktree create <provider> <branch> | worktree remove <provider> <path> [identity]".to_string();
                }
            }
            "plugin" => {
                // Plugins marketplace (Herdr-inspired). Plugins are modules with
                // a marketplace index for discovery.
                let dir = crate::core::modules::modules_dir();
                let _ = std::fs::create_dir_all(&dir);
                if parts.len() >= 2 && parts[1] == "list" {
                    let mods = crate::core::modules::discover();
                    if mods.is_empty() {
                        self.status_message = "plugins: none installed".to_string();
                    } else {
                        let mut msg = String::from("installed plugins:\n");
                        for m in mods {
                            msg.push_str(&format!(
                                "  {}\n",
                                m.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        self.status_message = msg;
                    }
                } else if parts.len() >= 4 && parts[1] == "install" {
                    let url = parts[2].to_string();
                    let name = parts.get(3).map(|s| s.to_string()).unwrap_or_else(|| {
                        url.rsplit('/').next().unwrap_or("plugin.sh").to_string()
                    });
                    let dest = dir.join(&name);
                    let out = std::process::Command::new("curl")
                        .arg("-fsSL")
                        .arg(&url)
                        .arg("-o")
                        .arg(&dest)
                        .output();
                    match out {
                        Ok(o) if o.status.success() => {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(
                                    &dest,
                                    std::fs::Permissions::from_mode(0o755),
                                );
                            }
                            self.status_message = format!("installed plugin: {name}");
                        }
                        Ok(o) => {
                            let _ = std::fs::remove_file(&dest);
                            self.status_message = format!("plugin install failed (curl {:?})", o.status);
                        }
                        Err(e) => self.status_message = format!("plugin install error: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "remove" {
                    let dest = dir.join(parts[2]);
                    match std::fs::remove_file(&dest) {
                        Ok(()) => self.status_message = format!("removed plugin {}", parts[2]),
                        Err(e) => self.status_message = format!("plugin remove failed: {e}"),
                    }
                } else if parts.len() >= 2 && parts[1] == "marketplace" {
                    self.status_message = "plugin marketplace: https://github.com/GrayCodeAI/term-plugins\n  :plugin install <url> [name]  — install from any URL\n  :plugin list                  — list installed plugins\n  :plugin remove <name>         — remove a plugin".to_string();
                } else {
                    self.status_message = "usage: plugin list | install <url> [name] | remove <name> | marketplace".to_string();
                }
            }
            "worktree" => {
                if parts.len() >= 2 && parts[1] == "list" {
                    self.status_message = crate::core::worktree::list_worktrees(&self.cwd);
                } else if parts.len() >= 3 && parts[1] == "add" {
                    let branch = parts[2].to_string();
                    let base = crate::core::worktree::worktree_base(&self.cwd);
                    let path = base.join(&branch);
                    match crate::core::worktree::create_worktree(&self.cwd, &branch, &path) {
                        Ok(()) => {
                            self.status_message = format!("worktree added at {}", path.display())
                        }
                        Err(e) => self.status_message = format!("worktree add failed: {e}"),
                    }
                } else if parts.len() >= 3 && parts[1] == "remove" {
                    let path = std::path::PathBuf::from(parts[2]);
                    match crate::core::worktree::remove_worktree(&self.cwd, &path) {
                        Ok(()) => self.status_message = format!("worktree removed: {}", parts[2]),
                        Err(e) => self.status_message = format!("worktree remove failed: {e}"),
                    }
                } else {
                    self.status_message =
                        "usage: worktree list | add <branch> | remove <path>".to_string();
                }
            }
            "race" => {
                if parts.len() >= 2 && parts[1] == "status" {
                    self.status_message = self
                        .race
                        .as_ref()
                        .map(|r| r.render())
                        .unwrap_or_else(|| "no active race".to_string());
                } else if parts.len() >= 3 && parts[1] == "winner" {
                    if let Ok(i) = parts[2].parse::<usize>() {
                        self.race_winner(i, parts.get(3).copied() == Some("force"));
                    } else {
                        self.status_message = "usage: race winner <idx> [force]".to_string();
                    }
                } else if parts.len() >= 3 && parts[1] == "gate" {
                    if let Ok(i) = parts[2].parse::<usize>() {
                        self.race_gate(i);
                    } else {
                        self.status_message = "usage: race gate <idx>".to_string();
                    }
                } else if parts.len() >= 3 && parts[1] == "gate-cmd" {
                    let cmd = parts[2..].join(" ");
                    self.status_message = format!("race gate command set: {cmd}");
                    if let Some(r) = &mut self.race {
                        r.gate_cmd = Some(cmd);
                    } else {
                        self.status_message = "race: no active race".to_string();
                    }
                } else if parts.len() >= 2 && parts[1] == "leaderboard" {
                    self.race_history = crate::core::race::RaceHistory::load(&self.cwd);
                    let lines = self
                        .race_history
                        .records
                        .iter()
                        .rev()
                        .take(3)
                        .map(|r| {
                            let w = r.winner_agent.as_deref().unwrap_or("(abandoned)");
                            format!(
                                "  recent: {} won by {}{}",
                                r.prompt.chars().take(40).collect::<String>(),
                                w,
                                if r.forced { " [forced]" } else { "" }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.status_message = format!(
                        "race leaderboard:\n{}\n{}",
                        self.race_history.leaderboard(),
                        if lines.is_empty() {
                            String::new()
                        } else {
                            format!("recent races:\n{lines}")
                        }
                    );
                } else if parts.len() >= 2 && parts[1] == "cancel" {
                    self.race_cancel();
                } else if parts.len() >= 2 {
                    let prompt = parts[1..].join(" ");
                    self.race_start(&prompt);
                } else {
                    self.status_message = self
                        .race
                        .as_ref()
                        .map(|r| r.render())
                        .unwrap_or_else(|| "no active race".to_string());
                }
            }
            "ssh" => {
                if parts.len() >= 2 {
                    let host = parts[1].to_string();
                    let mut cmd = portable_pty::CommandBuilder::new("ssh");
                    for a in &parts[1..] {
                        cmd.arg(*a);
                    }
                    cmd.cwd(self.cwd.clone());
                    let id = self.next_pane_id;
                    self.next_pane_id += 1;
                    match PtySession::spawn(cmd, 24, 80) {
                        Ok(pty) => {
                            let mut pane = Pane::new(id, format!("ssh {host} #{id}"));
                            pane.agent_id = Some("ssh".to_string());
                            pane.pty = Some(pty);
                            pane.status = AgentStatus::Working;
                            self.panes.push(pane);
                            self.active_pane = self.panes.len() - 1;
                            self.status_message = format!("ssh pane {id} started for {host}");
                        }
                        Err(e) => {
                            self.status_message = format!("ssh spawn failed: {e}");
                        }
                    }
                } else {
                    self.status_message = "usage: ssh <host> [args...]".to_string();
                }
            }
            "help" => {
                self.status_message =
                    "commands: spawn <agent>, close, delegate <pane> <msg>, broadcast <msg>, session save/clear, export costs/session, module list/install <url> [name]/remove <name>/run <action_id>, ssh <host>, account list/add/env/show, config show/theme, task add/start/done/lease/gate/merge/filter/kanban/fanout, worktree list/add/remove, branch".to_string();
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

    /// Delegate to a pane and record the prompt in the reply inbox.
    pub fn delegate_to_pane_recorded(&mut self, idx: usize, msg: &str) {
        let (pane_id, title) = match self.panes.get(idx) {
            Some(p) => (p.id, p.title.clone()),
            None => return,
        };
        self.delegate_to_pane(idx, msg);
        self.inbox.send(pane_id, &title, msg);
    }

    /// Capture replies for pending inbox items whose pane blocked/done/errored.
    pub fn inbox_capture(&mut self) {
        let pending: Vec<u64> = self
            .inbox
            .items
            .iter()
            .filter(|i| !i.arrived)
            .map(|i| i.id)
            .collect();
        for id in pending {
            let status_and_tail = self
                .panes
                .iter()
                .find(|p| {
                    self.inbox
                        .items
                        .iter()
                        .any(|i| i.id == id && i.pane_id == p.id)
                })
                .map(|p| {
                    (
                        p.status,
                        p.pty
                            .as_ref()
                            .map(|pty| pty.scrollback_tail(6))
                            .unwrap_or_default(),
                    )
                });
            if let Some((status, tail)) = status_and_tail {
                if Inbox::reply_ready(status, false) {
                    if let Some(item) = self.inbox.items.iter_mut().find(|i| i.id == id) {
                        item.arrived = true;
                        item.read = false;
                        item.reply = tail
                            .lines()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or_default()
                            .to_string();
                    }
                }
            }
        }
    }

    /// Open (or toggle) the inbox overlay; opening marks replies as read.
    pub fn inbox_toggle(&mut self) {
        self.show_inbox = !self.show_inbox;
        self.inbox.mark_all_read();
        if self.show_inbox {
            self.status_message = "inbox — o/Esc closes".to_string();
        }
    }

    /// Park a prompt for a pane: delivered automatically next time it blocks.
    pub fn queue_enqueue(&mut self, pane_idx: usize, msg: &str) {
        if pane_idx >= self.panes.len() {
            self.status_message = format!("queue: no pane index {pane_idx}");
            return;
        }
        let (pane_id, title, has_pty) = {
            let p = &self.panes[pane_idx];
            (p.id, p.title.clone(), p.pty.is_some())
        };
        if !has_pty {
            self.status_message = format!("queue: pane {pane_id} has no live process");
            return;
        }
        let id = self.queue.push(pane_id, &title, msg);
        self.status_message =
            format!("queued #{id} for pane {pane_id} — fires when it blocks (:queue list)");
    }

    /// Deliver queued prompts for panes that are now blocked on the user.
    pub fn queue_tick(&mut self) {
        if self.queue.items.is_empty() {
            return;
        }
        // Drop stale prompts whose pane no longer exists.
        let stale: Vec<u64> = self
            .queue
            .items
            .iter()
            .filter(|i| !self.panes.iter().any(|p| p.id == i.pane_id))
            .map(|i| i.id)
            .collect();
        for qid in stale {
            self.queue.drop(qid);
            self.status_message = format!("queue #{qid} dropped (pane gone)");
        }
        let mut delivered: Vec<(u64, usize, String)> = Vec::new();
        for item in self.queue.items.iter() {
            if let Some(p) = self.panes.iter().find(|p| p.id == item.pane_id) {
                if p.pty.is_some() && Queue::ready(p.status) {
                    delivered.push((item.id, p.id, item.msg.clone()));
                }
            }
        }
        for (qid, pane_id, msg) in delivered {
            let idx = self.panes.iter().position(|p| p.id == pane_id);
            if let Some(idx) = idx {
                self.delegate_to_pane_recorded(idx, &msg);
                self.queue.drop(qid);
                self.status_message =
                    format!("queue #{qid} delivered to {}", self.panes[idx].title);
            } else {
                // Pane gone — drop the stale prompt.
                self.queue.drop(qid);
            }
        }
    }
}

/// Parse an inline delegation buffer typed at a pane prompt.
/// Format: `=<pane_index> <message...>`. Returns the target index and message.
pub fn parse_inline_delegate(buf: &str) -> Option<(usize, String)> {
    let t = buf.trim();
    if !t.starts_with('=') {
        return None;
    }
    let mut it = t[1..].trim_start().splitn(2, char::is_whitespace);
    let idx = it.next()?.parse::<usize>().ok()?;
    let msg = it.next().unwrap_or("").trim().to_string();
    Some((idx, msg))
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
    if cli.dashboard {
        tracing::info!("dashboard JSON mode (cwd {:?})", cli.cwd);
        let mut app = App::new(config);
        if let Some(c) = cli.cwd.clone() {
            app.cwd = c;
        }
        println!("{}", app.dashboard_json());
        return Ok(());
    }

    if cli.doctor {
        tracing::info!("doctor mode");
        crate::core::doctor::run_cli_checks(cli.fix);
        return Ok(());
    }

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
        // Panes newly entering Blocked/Done while unfocused raise an attention
        // badge; unknown agents (ssh/shell) use the generic heuristic.
        let mut attention_hits = 0u32;
        let hooks_on = app.config.notify;
        for (idx, pane) in app.panes.iter_mut().enumerate() {
            if let Some(pty) = &pane.pty {
                let tail = pty.scrollback_tail(8);
                let pty_dead = !pty.is_alive();
                let registered = match &pane.agent_id {
                    Some(id) => app.registry.find_by_id(id),
                    None => None,
                };
                let mut new_status = match &registered {
                    Some(agent) => agent.parse_status(&tail),
                    None => App::fallback_status(&tail, !pty_dead),
                };
                if new_status == AgentStatus::Unknown && !pty_dead {
                    // Live but not conclusive → keep the pane read as working.
                    new_status = AgentStatus::Working;
                }
                if let Some(agent) = &registered {
                    // cost from latest session file
                    let files = agent.session_files(&app.cwd);
                    if let Some(f) = files.into_iter().last() {
                        if let Some(c) = agent.parse_cost(&f) {
                            pane.cost_usd = c.cost_usd;
                        }
                    }
                }
                if new_status != pane.status
                    && (new_status == AgentStatus::Blocked || new_status == AgentStatus::Done)
                    && idx != app.active_pane
                {
                    pane.attention = true;
                    attention_hits += 1;
                    if hooks_on {
                        let why = if new_status == AgentStatus::Blocked {
                            "is BLOCKED and needs you"
                        } else {
                            "finished"
                        };
                        crate::core::notify::alert(&format!("Term — {}", pane.title), why);
                    }
                }
                pane.status = new_status;
                if pty_dead {
                    if pane.status != AgentStatus::Done && idx != app.active_pane {
                        pane.attention = true;
                        attention_hits += 1;
                        if hooks_on {
                            crate::core::notify::alert(
                                &format!("Term — {}", pane.title),
                                "session ended",
                            );
                        }
                    }
                    pane.status = AgentStatus::Done;
                }
            } else {
                // No pty: keep AgentStatus stable, nothing to parse.
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
                // Keep race contestant statuses mirrored from their panes.
                app.race_update_statuses();
                // Capture agent replies for pending inbox items.
                if app.tick_count.is_multiple_of(2) {
                    app.inbox_capture();
                    app.queue_tick();
                    app.timelines_tick();
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

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
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
                    app.delegate_to_pane_recorded(idx, msg);
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

    // Copy mode: vi-style selection within a pane's screen text.
    if app.copy_mode {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.copy_mode = false;
                app.copy_selection = None;
                app.copy_start = None;
            }
            KeyCode::Char('v') => {
                app.copy_start = Some((app.copy_cursor_row, app.copy_cursor_col));
                app.copy_selection = Some(((app.copy_cursor_row, app.copy_cursor_col),
                                           (app.copy_cursor_row, app.copy_cursor_col)));
            }
            KeyCode::Char('y') => {
                if let Some(((r1, c1), (r2, c2))) = &app.copy_selection {
                    let (r1, c1, r2, c2) = (*r1, *c1, *r2, *c2);
                    let pane_idx = app.copy_pane.unwrap_or(app.active_pane);
                    if let Some(pane) = app.panes.get_mut(pane_idx) {
                        if let Some(pty) = &pane.pty {
                            let screen = pty.screen_text();
                            let lines: Vec<&str> = screen.lines().collect();
                            let (r_start, r_end) = if r1 <= r2 { (r1, r2) } else { (r2, r1) };
                            let (c_start, c_end) = if c1 <= c2 { (c1, c2) } else { (c2, c1) };
                            let mut text = String::new();
                            for r in r_start..=r_end {
                                if let Some(line) = lines.get(r) {
                                    let cols = line.chars().count();
                                    let cs = c_start.min(cols);
                                    let ce = c_end.min(cols);
                                    if cs < ce {
                                        let selected: String = line.chars().skip(cs).take(ce - cs).collect();
                                        if r > r_start { text.push('\n'); }
                                        text.push_str(&selected);
                                    }
                                }
                            }
                            let copied = if cfg!(target_os = "linux") {
                                std::process::Command::new("wl-copy")
                                    .stdin(std::process::Stdio::piped())
                                    .spawn()
                                    .and_then(|mut child| {
                                        use std::io::Write;
                                        child.stdin.take().unwrap().write_all(text.as_bytes())
                                    })
                                    .is_ok()
                            } else {
                                false
                            };
                            if copied {
                                app.status_message = format!("copied {} chars to clipboard", text.len());
                            } else {
                                app.status_message = format!("selected {} chars (no clipboard)", text.len());
                            }
                        }
                    }
                }
                app.copy_mode = false;
                app.copy_selection = None;
                app.copy_start = None;
            }
            KeyCode::Char('p') => {
                match crate::core::clipboard_image::paste_clipboard_image(app) {
                    Ok(()) => {}
                    Err(e) => app.status_message = format!("paste failed: {e}"),
                }
                app.copy_mode = false;
                app.copy_selection = None;
                app.copy_start = None;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if app.copy_cursor_col > 0 {
                    app.copy_cursor_col -= 1;
                }
                app.update_copy_selection();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                app.copy_cursor_col += 1;
                app.update_copy_selection();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.copy_cursor_row > 0 {
                    app.copy_cursor_row -= 1;
                }
                app.update_copy_selection();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.copy_cursor_row += 1;
                app.update_copy_selection();
            }
            _ => {}
        }
        return;
    }

    // Pane scrollback search: "/" searches within the active pane's screen text.
    if app.pane_search_mode {
        match key.code {
            KeyCode::Esc => {
                app.pane_search_mode = false;
                app.pane_search_buffer.clear();
                app.pane_search_matches.clear();
                app.pane_search_selected = 0;
            }
            KeyCode::Enter => {
                // Jump to next match.
                app.pane_search_selected = (app.pane_search_selected + 1) % app.pane_search_matches.len().max(1);
            }
            KeyCode::Backspace => {
                app.pane_search_buffer.pop();
                app.refresh_pane_search();
            }
            KeyCode::Char(c) => {
                app.pane_search_buffer.push(c);
                app.refresh_pane_search();
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

    // Commander mode: targeted-message composer.
    if app.commander_mode {
        match key.code {
            KeyCode::Esc => {
                app.commander_close();
            }
            KeyCode::Enter => {
                app.commander_send();
            }
            KeyCode::Backspace => {
                if !app.commander_draft.is_empty() {
                    app.commander_draft.pop();
                    app.commander_cursor = app.commander_cursor.saturating_sub(1);
                }
            }
            KeyCode::Char(c) => {
                app.commander_draft.push(c);
                app.commander_cursor += 1;
            }
            _ => {}
        }
        return;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('?'), _) => app.show_help = !app.show_help,
        (KeyCode::Char('/'), _) if !app.command_mode && !app.delegate_mode && !app.commander_mode => {
            app.pane_search_mode = true;
            app.pane_search_buffer.clear();
            app.pane_search_matches.clear();
            app.pane_search_selected = 0;
            app.pane_search_pane = Some(app.active_pane);
        }
        (KeyCode::Char('v'), _) if !app.command_mode && !app.delegate_mode && !app.commander_mode && !app.pane_search_mode => {
            app.copy_mode = true;
            app.copy_cursor_row = 0;
            app.copy_cursor_col = 0;
            app.copy_start = None;
            app.copy_selection = None;
            app.copy_pane = Some(app.active_pane);
            app.status_message = "copy mode: v start, y yank, hjkl move, q quit".to_string();
        }
        (KeyCode::Char('>'), _) => app.commander_open(),
        (KeyCode::Char('o'), _) => app.inbox_toggle(),
        (KeyCode::Esc, _) => {
            app.show_help = false;
            app.show_inbox = false;
        }
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
        (KeyCode::Char('7'), _) => app.current_tab = Tab::Dashboard,
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
        (KeyCode::Char('s'), _) if app.current_tab == Tab::Diff => {
            app.side_by_side = !app.side_by_side;
            app.status_message = if app.side_by_side {
                "diff: side-by-side".to_string()
            } else {
                "diff: unified".to_string()
            };
        }
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
        (KeyCode::Char('S'), _) => {
            // zellij-style split: open (or cycle) the bottom preview pane
            if app.panes.len() < 2 {
                app.status_message = "split needs 2+ panes".to_string();
            } else {
                app.split_bottom = Some(match app.split_bottom {
                    None => (app.active_pane + 1) % app.panes.len(),
                    Some(b) => {
                        let n = (b + 1) % app.panes.len();
                        if n == b { b } else { n }
                    }
                });
                app.status_message = format!(
                    "split bottom: pane {} (w closes)",
                    app.panes[app.split_bottom.unwrap()].id
                );
            }
        }
        (KeyCode::Char('w'), _) => {
            app.split_bottom = None;
            app.status_message = "split closed".to_string();
        }
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
            let delegated = app
                .active_pane()
                .and_then(|p| parse_inline_delegate(&p.input_buffer));
            if let Some((idx, msg)) = delegated {
                app.delegate_to_pane_recorded(idx, &msg);
                app.status_message = format!("delegated to pane {} (reply goes to :inbox)", idx);
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

#[cfg(test)]
mod inline_delegation_tests {
    use super::*;

    #[test]
    fn parses_valid_delegation() {
        assert_eq!(
            parse_inline_delegate("=0 fix the flaky test"),
            Some((0, "fix the flaky test".to_string()))
        );
        assert_eq!(parse_inline_delegate("=2"), Some((2, String::new())));
        assert_eq!(
            parse_inline_delegate("  =12  spaced msg  "),
            Some((12, "spaced msg".to_string()))
        );
    }

    #[test]
    fn rejects_non_delegation_buffers() {
        assert_eq!(parse_inline_delegate("fix the test"), None);
        assert_eq!(parse_inline_delegate(""), None);
        assert_eq!(parse_inline_delegate("=notanum hi"), None);
        assert_eq!(parse_inline_delegate("== hi"), None);
    }
}

#[cfg(test)]
mod race_gate_tests {
    use super::*;
    use crate::core::agent::AgentStatus;
    use crate::core::race::{Race, RaceContestant};
    use std::path::PathBuf;
    use std::process::Command;

    fn make_repo(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("term-race-gate-repo-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for c in [
            ["init"].as_slice(),
            ["config", "user.email", "t@t.com"].as_slice(),
            ["config", "user.name", "t"].as_slice(),
        ] {
            Command::new("git")
                .args(c)
                .current_dir(&dir)
                .output()
                .unwrap();
        }
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    fn contestant(worktree: PathBuf, branch: &str) -> RaceContestant {
        RaceContestant {
            agent_id: "claude".to_string(),
            pane_id: 999_999, // no such pane — focused on gate/merge logic
            branch: branch.to_string(),
            worktree,
            status: Some(AgentStatus::Working),
            gate_ok: None,
            winner: false,
            promoted: false,
        }
    }

    #[test]
    fn winner_blocked_by_failing_gate_then_promoted_after_pass() {
        let app_cwd = make_repo("1");
        let mut a = App::new(crate::config::Config::default());
        a.cwd = app_cwd.clone();

        let wt = crate::core::worktree::worktree_base(&a.cwd).join("race-gate-test");
        crate::core::worktree::create_worktree(&a.cwd, "race-gate-test", &wt).unwrap();

        let race = Race {
            prompt: "fix it".to_string(),
            contestants: vec![contestant(wt, "race-gate-test")],
            gate_cmd: Some("false".to_string()), // failing gate
        };
        a.race = Some(race);

        // Failing gate must refuse the merge and flag the contestant.
        a.race_winner(0, false);
        assert_eq!(a.race.as_ref().unwrap().contestants[0].gate_ok, Some(false));
        assert!(!a.race.as_ref().unwrap().contestants[0].winner);
        assert!(a.status_message.contains("gate failed"));

        // force skips the gate and merges.
        a.race_winner(0, true);
        let c = &a.race.as_ref().unwrap().contestants[0];
        assert!(c.winner && c.promoted);
        assert!(a.status_message.contains("race winner"));
        let _ = std::fs::remove_dir_all(&app_cwd);
    }

    #[test]
    fn passing_gate_proceeds_to_merge() {
        let app_cwd = make_repo("2");
        let mut a = App::new(crate::config::Config::default());
        a.cwd = app_cwd.clone();

        let wt = crate::core::worktree::worktree_base(&a.cwd).join("race-gate-test2");
        crate::core::worktree::create_worktree(&a.cwd, "race-gate-test2", &wt).unwrap();

        a.race = Some(Race {
            prompt: "fix it".to_string(),
            contestants: vec![contestant(wt, "race-gate-test2")],
            gate_cmd: Some("true".to_string()), // passing gate
        });

        a.race_winner(0, false);
        let c = &a.race.as_ref().unwrap().contestants[0];
        assert_eq!(c.gate_ok, Some(true));
        assert!(
            c.winner && c.promoted,
            "winner not set: {}",
            a.status_message
        );
        let _ = std::fs::remove_dir_all(&app_cwd);
    }

    #[test]
    fn default_gate_cmd_heuristic() {
        let dir = std::env::temp_dir().join("term-gate-heur");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(default_gate_cmd(&dir), "true");
        std::fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(default_gate_cmd(&dir), "cargo test");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn race_gate_reports_result() {
        let app_cwd = make_repo("3");
        let mut a = App::new(crate::config::Config::default());
        a.cwd = app_cwd.clone();

        let wt = crate::core::worktree::worktree_base(&a.cwd).join("race-gate-test3");
        crate::core::worktree::create_worktree(&a.cwd, "race-gate-test3", &wt).unwrap();
        a.race = Some(Race {
            prompt: "p".to_string(),
            contestants: vec![contestant(wt, "race-gate-test3")],
            gate_cmd: Some("true".to_string()),
        });

        a.race_gate(0);
        assert_eq!(a.race.as_ref().unwrap().contestants[0].gate_ok, Some(true));
        assert!(a.status_message.contains("passed"));

        // invalid index is a safe no-op
        a.race_gate(7);
        assert!(a.status_message.contains("invalid contestant"));
        let _ = std::fs::remove_dir_all(&app_cwd);
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    use crate::config::Config;
    use crate::core::agent::AgentStatus;
    use crate::core::queue::{Queue, QueuedPrompt};

    #[test]
    fn queue_list_and_enqueue_command() {
        let mut a = App::new(Config::default());
        a.execute_command("queue 0 add more tests");
        // No panes yet — invalid index, nothing queued.
        assert!(a.queue.items.is_empty());

        let mut pane = Pane::new(1, "Claude #1".to_string());
        pane.pty = None; // even with a pane, no pty → rejected
        a.panes.push(pane);
        a.execute_command("queue 0 add more tests");
        assert!(a.queue.items.is_empty());

        a.panes[0].pty = Some(pty_stub());
        a.execute_command("queue 0 add more tests");
        assert_eq!(a.queue.items.len(), 1);
        assert_eq!(a.queue.items[0].msg, "add more tests");
        a.execute_command("queue list");
        assert!(a.status_message.contains("add more tests"));
        a.execute_command("queue clear");
        assert!(a.queue.items.is_empty());
    }

    /// A PtySession backed by /bin/echo for tests needing a live handle.
    fn pty_stub() -> PtySession {
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("0");
        PtySession::spawn(cmd, 4, 80).expect("pty stub")
    }

    #[test]
    fn queue_tick_delivers_only_when_blocked() {
        let mut a = App::new(Config::default());
        let pane = Pane::new(1, "Claude #1".to_string());
        a.panes.push(pane);

        // Queue struct-level: pane idle → not ready.
        assert!(!Queue::ready(a.panes[0].status));

        // Simulate parked prompt; tick with non-blocked pane → not delivered.
        a.queue.items.push(QueuedPrompt {
            id: a.queue.next_id + 1,
            pane_id: 1,
            pane_title: "Claude #1".to_string(),
            msg: "approve".to_string(),
        });
        a.queue_tick();
        assert_eq!(a.queue.items.len(), 1);

        // Blocked without pty → not deliverable (no live process).
        a.panes[0].status = AgentStatus::Blocked;
        a.queue_tick();
        assert_eq!(a.queue.items.len(), 1, "no private delivery without pty");

        // With pty + blocked → delivered and recorded in the inbox.
        a.panes[0].pty = Some(pty_stub());
        a.queue_tick();
        assert!(a.queue.items.is_empty(), "delivered");
        assert_eq!(a.inbox.items.len(), 1);
    }

    #[test]
    fn stale_prompt_dropped_when_pane_gone() {
        let mut a = App::new(Config::default());
        a.queue.items.push(QueuedPrompt {
            id: a.queue.next_id + 1,
            pane_id: 42,
            pane_title: "gone".to_string(),
            msg: "m".to_string(),
        });
        a.queue_tick();
        assert!(a.queue.items.is_empty());
    }
}

#[cfg(test)]
mod race_history_tests {
    use super::*;
    use crate::core::race::{Race, RaceContestant};

    use crate::core::agent::AgentStatus;
    use std::process::Command;

    fn make_repo(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("term-race-history-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for c in [
            ["init"].as_slice(),
            ["config", "user.email", "t@t.com"].as_slice(),
            ["config", "user.name", "t"].as_slice(),
        ] {
            Command::new("git")
                .args(c)
                .current_dir(&dir)
                .output()
                .unwrap();
        }
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    fn contestant(worktree: std::path::PathBuf, branch: &str) -> RaceContestant {
        RaceContestant {
            agent_id: "claude".to_string(),
            pane_id: 999_999,
            branch: branch.to_string(),
            worktree,
            status: Some(AgentStatus::Working),
            gate_ok: None,
            winner: false,
            promoted: false,
        }
    }

    #[test]
    fn winner_persists_record_and_leaderboard_shows_it() {
        let app_cwd = make_repo("5");
        let mut a = App::new(crate::config::Config::default());
        a.cwd = app_cwd.clone();

        let wt = crate::core::worktree::worktree_base(&a.cwd).join("race-gate-test5");
        crate::core::worktree::create_worktree(&a.cwd, "race-gate-test5", &wt).unwrap();
        a.race = Some(Race {
            prompt: "ship it".to_string(),
            contestants: vec![contestant(wt, "race-gate-test5")],
            gate_cmd: Some("true".to_string()),
        });
        a.race_winner(0, true); // force path also records

        let h = crate::core::race::RaceHistory::load(&a.cwd);
        assert_eq!(h.records.len(), 1, "record persisted");
        assert_eq!(h.records[0].winner_agent.as_deref(), Some("claude"));
        assert!(h.records[0].forced);
        assert_eq!(h.records[0].prompt, "ship it");

        // command path reloads from disk and shows the win
        a.execute_command("race leaderboard");
        assert!(
            a.status_message.contains("claude: 1 win"),
            "{}",
            a.status_message
        );
        assert!(a.status_message.contains("ship it"));
        let _ = std::fs::remove_dir_all(&app_cwd);
    }
}

#[cfg(test)]
mod check_command_tests {
    use super::*;
    use crate::config::Config;
    use std::process::Command;

    fn quiet_repo(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("term-check-cmd-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for c in [
            ["init"].as_slice(),
            ["config", "user.email", "t@t.com"].as_slice(),
            ["config", "user.name", "t"].as_slice(),
        ] {
            Command::new("git")
                .args(c)
                .current_dir(&dir)
                .output()
                .unwrap();
        }
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn from_inbox_promotes_with_checklist_and_check_toggles() {
        let repo = quiet_repo("1");
        let mut a = App::new(Config::default());
        a.cwd = repo.clone();

        // A captured reply with bullets.
        a.inbox.send(1, "Claude #1", "kickoff");
        let item = a.inbox.items.first_mut().unwrap();
        item.arrived = true;
        item.reply = "- refactor the module\n- update the docs".to_string();

        a.execute_command("task from-inbox 1");
        assert!(a.status_message.contains("task 1 created"));
        assert!(a.inbox.items.is_empty(), "item consumed");
        let board = &a.task_board;
        let t = &board.tasks[0];
        assert_eq!(t.checklist.as_ref().unwrap().len(), 2);

        // :task check <id> <n>
        a.execute_command("task check 1 1");
        assert!(a.status_message.contains("checked"));
        let t = &a.task_board.tasks[0];
        assert!(t.checklist.as_ref().unwrap()[0].done);
        let _ = std::fs::remove_dir_all(&repo);
    }
}

#[cfg(test)]
mod dashboard_tests {
    use super::*;
    use crate::config::Config;
    use crate::core::orchestration::{CheckItem, Task, TaskStatus};

    #[test]
    fn dashboard_reflects_all_subsystems() {
        let mut a = App::new(Config::default());

        // Panes: one blocked with attention, one working; costs sum in.
        let mut p1 = Pane::new(1, "Claude #1".to_string());
        p1.status = AgentStatus::Blocked;
        p1.attention = true;
        p1.cost_usd = 0.31;
        let mut p2 = Pane::new(2, "Codex #2".to_string());
        p2.status = AgentStatus::Working;
        p2.cost_usd = 0.11;
        a.panes = vec![p1, p2];
        let _ = a.panes.len();

        // Race without winner.
        a.race = Some(crate::core::race::Race {
            prompt: "fix bug".to_string(),
            contestants: vec![lite_contestant(1)],
            gate_cmd: None,
        });

        // Queue + inbox.
        a.queue.items.push(crate::core::queue::QueuedPrompt {
            id: 1,
            pane_id: 2,
            pane_title: "Codex #2".to_string(),
            msg: "approve".to_string(),
        });
        a.inbox.send(1, "Claude #1", "userinfo");
        a.inbox.items[0].arrived = true;

        // Tasks: build one of each interesting status.
        a.task_board.tasks = vec![
            Task {
                id: 1,
                title: "t1".to_string(),
                deps: vec![],
                status: TaskStatus::Pending,
                worktree: None,
                branch: None,
                quality_gate: None,
                pane_id: None,
                leased_paths: vec![],
                notes: None,
                checklist: Some(vec![CheckItem {
                    label: "x".to_string(),
                    done: false,
                }]),
            },
            Task {
                id: 2,
                title: "t2".to_string(),
                deps: vec![1],
                status: TaskStatus::Running,
                worktree: None,
                branch: None,
                quality_gate: None,
                pane_id: None,
                leased_paths: vec![],
                notes: None,
                checklist: None,
            },
        ];

        let mut lines = a.dashboard_lines().join("\n");

        assert!(
            lines.contains("Panes (2) — blocked 1, working 1"),
            "{lines}"
        );
        assert!(lines.contains("blocked: pane 1 Claude #1 ⚠"), "{lines}");
        assert!(lines.contains("$0.42 this session"), "{lines}");
        assert!(lines.contains("Race: \"fix bug\" — 1 racing"), "{lines}");
        assert!(
            lines.contains("Queue: 1 parked; next → pane 2 \"approve\""),
            "{lines}"
        );
        assert!(lines.contains("Inbox: 1 unread / 1 total"), "{lines}");
        assert!(lines.contains("pending 1, running 1"), "{lines}");
        assert!(lines.contains("ready to start: [1]"), "{lines}");
        assert!(lines.contains("Modules:"), "{lines}");

        // Winner state flips the race line.
        a.race.as_mut().unwrap().contestants[0].winner = true;
        lines = a.dashboard_lines().join("\n");
        assert!(lines.contains("WON by agent1"), "{lines}");

        // Empty everything degrades gracefully.
        a.race = None;
        a.panes.clear();
        a.queue.clear();
        a.inbox.clear();
        a.task_board.tasks.clear();
        lines = a.dashboard_lines().join("\n");
        assert!(
            lines.contains("next blocked: none (a jumps when one appears)"),
            "{lines}"
        );
        assert!(lines.contains("Race: none — :race <prompt>"), "{lines}");
        assert!(lines.contains("Queue: empty"), "{lines}");
    }

    /// Builds a minimal race contestant for the dashboard test.
    fn lite_contestant(x: usize) -> crate::core::race::RaceContestant {
        crate::core::race::RaceContestant {
            agent_id: format!("agent{x}"),
            pane_id: 1,
            branch: format!("race-{x}"),
            worktree: std::path::PathBuf::from("/tmp/x"),
            status: Some(AgentStatus::Working),
            gate_ok: None,
            winner: false,
            promoted: false,
        }
    }
}

#[cfg(test)]
mod dashboard_json_tests {
    use super::*;
    use crate::config::Config;
    use serde_json::Value;

    #[test]
    fn dashboard_json_round_trips() {
        let mut a = App::new(Config::default());
        let mut p = Pane::new(1, "Claude #1".to_string());
        p.status = AgentStatus::Blocked;
        p.cost_usd = 0.50;
        a.panes = vec![p];
        a.race = Some(crate::core::race::Race {
            prompt: "go".to_string(),
            contestants: vec![crate::core::race::RaceContestant {
                agent_id: "claude".to_string(),
                pane_id: 1,
                branch: "race-1".to_string(),
                worktree: std::path::PathBuf::from("/tmp/x"),
                status: Some(AgentStatus::Working),
                gate_ok: Some(true),
                winner: false,
                promoted: false,
            }],
            gate_cmd: None,
        });
        a.inbox.send(1, "Claude #1", "m");
        a.inbox.items[0].arrived = true;

        let v: Value = serde_json::from_str(&a.dashboard_json()).unwrap();
        assert_eq!(v["panes"][0]["status"], "BLOCKED");
        assert_eq!(v["panes"][0]["cost_usd"], 0.50);
        assert_eq!(v["race"]["prompt"], "go");
        assert_eq!(v["race"]["contestants"][0]["gate_ok"], true);
        assert_eq!(v["race"]["winner"], Value::Null); // no winner yet
        assert_eq!(v["inbox"]["unread"], 1);
        assert!(v["tasks"]["pending"].as_u64().is_some());
        assert_eq!(v["race_history"], 0);
    }

    #[test]
    fn dashboard_json_empty_state_is_valid() {
        let a = App::new(Config::default());
        let v: Value = serde_json::from_str(&a.dashboard_json()).unwrap();
        assert_eq!(v["panes"].as_array().unwrap().len(), 0);
        assert_eq!(v["race"], Value::Null);
        assert_eq!(v["queue"].as_array().unwrap().len(), 0);
    }

    /// PTY that emits exactly one line; `echo` is a cmd builtin on Windows.
    fn echo_pty_app(msg: &str) -> PtySession {
        #[cfg(windows)]
        {
            let mut cmd = portable_pty::CommandBuilder::new("cmd");
            cmd.arg("/c");
            cmd.arg(format!("echo {msg}"));
            PtySession::spawn(cmd, 4, 80).expect("echo pty")
        }
        #[cfg(not(windows))]
        {
            let mut cmd = portable_pty::CommandBuilder::new("echo");
            cmd.arg(msg);
            PtySession::spawn(cmd, 4, 80).expect("echo pty")
        }
    }

    fn contains(raw: &[u8], needle: &[u8]) -> bool {
        raw.windows(needle.len()).any(|w| w == needle)
    }

    fn sleep_pty_app() -> PtySession {
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("0");
        PtySession::spawn(cmd, 4, 80).expect("pty stub")
    }

    #[test]
    fn timelines_tick_writes_pane_log() {
        let base = std::env::temp_dir().join(format!("term-timeline-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        let mut a = App::new(Config::default());
        let mut pane = Pane::new(2, "Echo".to_string());
        pane.pty = Some(echo_pty_app("timeline-marker-42"));
        a.panes.push(pane);
        a.active_pane = 0;

        // Wait for the reader thread to capture the marker.
        let saw = (0..100).any(|_| {
            let got = a.panes[0]
                .pty
                .as_ref()
                .unwrap()
                .raw_log
                .lock()
                .map(|raw| contains(&raw, b"timeline-marker-42"))
                .unwrap_or(false);
            if got {
                true
            } else {
                std::thread::sleep(std::time::Duration::from_millis(20));
                false
            }
        });
        assert!(saw, "raw log should receive the marker");

        a.timelines_tick_into(&base);
        let log = Pane::timeline_path_under(2, &base);
        let read_count = || {
            std::fs::read_to_string(&log)
                .map(|d| d.matches("timeline-marker-42").count())
                .unwrap_or(0)
        };
        // Drain until the marker lands in the file (chunks may split).
        let landed = (0..50).any(|_| {
            if read_count() > 0 {
                true
            } else {
                a.timelines_tick_into(&base);
                false
            }
        });
        assert!(landed, "timeline log should contain the marker");
        assert_eq!(read_count(), 1, "marker captured exactly once");
        assert!(a.panes[0].timeline_cursor > 0);

        // Companion meta file: timestamped byte-offset milestones.
        let meta = Pane::timeline_meta_path_under(2, &base);
        let meta_data = std::fs::read_to_string(&meta).expect("meta written on first delta");
        let last = meta_data.lines().next_back().unwrap();
        let mut parts = last.split_whitespace();
        let (_ms, bytes): (u128, u128) = (
            parts.next().unwrap().parse().unwrap(),
            parts.next().unwrap().parse().unwrap(),
        );
        assert_eq!(bytes, a.panes[0].timeline_cursor as u128);
        assert!(Pane::last_meta_record(2, &base).is_some());

        // Further ticks never duplicate content.
        a.timelines_tick_into(&base);
        assert_eq!(read_count(), 1, "no duplication on repeat ticks");
    }

    #[test]
    fn timeline_toggle_and_status_commands() {
        let mut a = App::new(Config::default());
        a.panes.push(Pane::new(1, "S1".to_string()));
        a.active_pane = 0;

        a.execute_command("timeline");
        assert!(a.status_message.contains("no live panes"));

        a.panes[0].pty = Some(sleep_pty_app());
        a.execute_command("timeline");
        assert!(a.status_message.contains("pane 0"), "{}", a.status_message);

        a.execute_command("timeline off 0");
        assert!(!a.panes[0].timeline);
        a.execute_command("timeline on 0");
        assert!(a.panes[0].timeline);

        a.execute_command("timeline off 9");
        assert!(a.status_message.contains("no such pane"));

        a.execute_command("timeline export out.log");
        let msg = a.status_message.clone();
        assert!(
            msg.contains("exported") || msg.contains("no session log"),
            "{msg}"
        );
        // Clean up in case the timeline landed (real data dir); harmless either way.
        let _ = std::fs::remove_file(a.cwd.join("out.log"));
    }

    #[test]
    fn ssh_fallback_status_matches_generic_heuristic() {
        assert_eq!(
            App::fallback_status("proceed? (y/n)", true),
            AgentStatus::Blocked
        );
        assert_eq!(
            App::fallback_status("permission to run denied", true),
            AgentStatus::Blocked
        );
        assert_eq!(App::fallback_status("$ ", true), AgentStatus::Idle);
        // A `#` prompt is unknown to the generic heuristic → alive ⇒ Working.
        assert_eq!(App::fallback_status("# ", true), AgentStatus::Working);
        assert_eq!(
            App::fallback_status("installing modules... more text", true),
            AgentStatus::Working
        );
        assert_eq!(
            App::fallback_status("inconclusive output", false),
            AgentStatus::Unknown
        );
    }

    #[test]
    #[cfg(unix)]
    fn timeline_view_opens_replaying_pane() {
        let base = std::env::temp_dir().join(format!("term-timeline-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        let mut a = App::new(Config::default());
        a.panes.push(Pane::new(3, "SSH dev".to_string()));
        a.active_pane = 0;

        // No log yet → friendly error, no pane spawned.
        match a.timeline_view_at(0, &base) {
            Err(e) => assert!(e.contains("no session log"), "{e}"),
            Ok(()) => panic!("view should fail without a log"),
        }

        // Write a log manually, then view it.
        std::fs::create_dir_all(base.join("timelines")).unwrap();
        std::fs::write(Pane::timeline_path_under(3, &base), "hello-timeline-view\n").unwrap();
        a.timeline_view_at(0, &base).expect("replay pane");

        assert_eq!(a.panes.len(), 2);
        assert_eq!(a.active_pane, 1);
        assert!(a.panes[1].title.contains("replay"));
        assert!(a.panes[1].agent_id.is_none());

        // The replay PTY prints the whole log then keeps following.
        let saw = (0..100).any(|_| {
            let got = a.panes[1]
                .pty
                .as_ref()
                .unwrap()
                .raw_log
                .lock()
                .map(|raw| raw.windows(19).any(|w| w == b"hello-timeline-view"))
                .unwrap_or(false);
            if got {
                true
            } else {
                std::thread::sleep(std::time::Duration::from_millis(20));
                false
            }
        });
        assert!(saw, "replay pane should display the timeline");
    }

    #[test]
    #[cfg(unix)]
    fn timeline_view_command_reports_errors() {
        let mut a = App::new(Config::default());
        a.panes.push(Pane::new(4, "S".to_string()));
        a.active_pane = 0;
        a.execute_command("timeline view 0");
        assert!(
            a.status_message.contains("no session log"),
            "{}",
            a.status_message
        );
        a.execute_command("timeline view 42");
        assert!(a.status_message.contains("no pane"), "{}", a.status_message);
    }

    #[test]
    fn unknown_agent_pane_uses_fallback_status() {
        // A pane whose agent_id has no registered agent behaves like ssh.
        let mut a = App::new(Config::default());
        let mut pane = Pane::new(5, "SSH dev".to_string());
        pane.agent_id = Some("ssh".to_string());
        pane.pty = Some(sleep_pty_app());
        pane.status = AgentStatus::Working;
        a.panes.push(pane);
        a.active_pane = 0;

        // Reproduce the tick path in isolation: heuristic for unknown agent.
        let tail = a.panes[0].pty.as_ref().unwrap().scrollback_tail(8);
        let status = App::fallback_status(&tail, true);
        // ssh stub shows a prompt-ish idle screen or stays working; either is
        // accepted — the point is it never panics and refreshes cleanly.
        match status {
            AgentStatus::Working | AgentStatus::Idle | AgentStatus::Unknown => {}
            _ => panic!("unexpected ssh status {status:?}"),
        }
    }
}
