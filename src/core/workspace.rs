//! Workspace-level hierarchy: Workspace > Tab > Pane.
//!
//! A workspace groups related tabs, panes, and a working directory, mirroring
//! Herdr's workspace/tab/pane topology and Luvus's persistent workspaces.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Agents,
    Git,
    Diff,
    Files,
    Orchestration,
    Settings,
    Dashboard,
}

impl TabKind {
    pub const ALL: [TabKind; 7] = [
        TabKind::Agents,
        TabKind::Git,
        TabKind::Diff,
        TabKind::Files,
        TabKind::Orchestration,
        TabKind::Settings,
        TabKind::Dashboard,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            TabKind::Agents => "Agents",
            TabKind::Git => "Git",
            TabKind::Diff => "Diff",
            TabKind::Files => "Files",
            TabKind::Orchestration => "Orch",
            TabKind::Settings => "Settings",
            TabKind::Dashboard => "Dash",
        }
    }
}

/// A single tab inside a workspace.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub kind: TabKind,
    pub scroll: usize,
}

impl Tab {
    pub fn new(kind: TabKind) -> Self {
        Self { kind, scroll: 0 }
    }
}

/// A workspace: a named project context with its own tabs, panes, and cwd.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub id: usize,
    pub name: String,
    pub cwd: PathBuf,
    /// Ordered tabs within this workspace.
    pub tabs: Vec<Tab>,
    /// Active tab index within this workspace.
    pub active_tab: usize,
    /// Pane ids belonging to this workspace (stable ids, not vec indices).
    pub pane_ids: Vec<usize>,
    /// Whether this workspace is pinned (never auto-closed).
    pub pinned: bool,
}

impl Workspace {
    pub fn new(id: usize, name: String, cwd: PathBuf) -> Self {
        Self {
            id,
            name,
            cwd,
            tabs: vec![Tab::new(TabKind::Agents)],
            active_tab: 0,
            pane_ids: Vec::new(),
            pinned: false,
        }
    }

    pub fn active_tab_kind(&self) -> TabKind {
        self.tabs
            .get(self.active_tab)
            .map(|t| t.kind)
            .unwrap_or(TabKind::Agents)
    }

    pub fn add_tab(&mut self, kind: TabKind) {
        self.tabs.push(Tab::new(kind));
        self.active_tab = self.tabs.len() - 1;
    }

    pub fn remove_tab(&mut self, idx: usize) -> Option<TabKind> {
        if self.tabs.len() <= 1 {
            return None;
        }
        let removed = self.tabs.remove(idx).kind;
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        Some(removed)
    }

    pub fn add_pane(&mut self, pane_id: usize) {
        if !self.pane_ids.contains(&pane_id) {
            self.pane_ids.push(pane_id);
        }
    }

    pub fn remove_pane(&mut self, pane_id: usize) {
        self.pane_ids.retain(|&id| id != pane_id);
    }
}

/// Manages workspaces for the application.
#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub next_workspace_id: usize,
    pub next_tab_id: usize,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let ws = Workspace::new(1, "Default".to_string(), cwd);
        Self {
            workspaces: vec![ws],
            active_workspace: 0,
            next_workspace_id: 2,
            next_tab_id: 1,
        }
    }

    pub fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active_workspace)
    }

    pub fn active_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_workspace)
    }

    pub fn create_workspace(&mut self, name: String, cwd: PathBuf) -> usize {
        let id = self.next_workspace_id;
        self.next_workspace_id += 1;
        let ws = Workspace::new(id, name, cwd);
        self.workspaces.push(ws);
        self.active_workspace = self.workspaces.len() - 1;
        id
    }

    pub fn remove_workspace(&mut self, idx: usize) -> Option<Workspace> {
        if self.workspaces.len() <= 1 {
            return None;
        }
        let ws = self.workspaces.remove(idx);
        if !ws.pinned {
            if self.active_workspace >= self.workspaces.len() {
                self.active_workspace = self.workspaces.len() - 1;
            }
            Some(ws)
        } else {
            // Re-insert pinned workspace; cannot remove.
            self.workspaces.insert(idx, ws);
            None
        }
    }

    pub fn switch_workspace(&mut self, idx: usize) -> bool {
        if idx < self.workspaces.len() {
            self.active_workspace = idx;
            true
        } else {
            false
        }
    }

    pub fn workspace_by_id(&self, id: usize) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn workspace_by_id_mut(&mut self, id: usize) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    /// Map a stable pane id to the workspace that owns it.
    pub fn workspace_for_pane(&self, pane_id: usize) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|w| w.pane_ids.contains(&pane_id))
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable identifiers for the workspace/tab/pane topology (Herdr-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneRef {
    pub workspace_id: usize,
    pub tab_index: usize,
    pub pane_id: usize,
}

impl std::fmt::Display for PaneRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "w{}:p{}", self.workspace_id, self.pane_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_create_and_switch() {
        let mut mgr = WorkspaceManager::new();
        assert_eq!(mgr.workspaces.len(), 1);
        let id = mgr.create_workspace("Project A".to_string(), PathBuf::from("/tmp/a"));
        assert_eq!(id, 2);
        assert!(mgr.switch_workspace(1));
        assert_eq!(mgr.active_workspace(), mgr.workspace_by_id(2));
    }

    #[test]
    fn workspace_add_remove_pane() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.active_workspace_mut().unwrap();
        ws.add_pane(10);
        ws.add_pane(11);
        assert_eq!(ws.pane_ids, vec![10, 11]);
        ws.remove_pane(10);
        assert_eq!(ws.pane_ids, vec![11]);
    }

    #[test]
    fn tab_add_remove() {
        let mut ws = Workspace::new(1, "test".into(), PathBuf::from("/tmp"));
        assert_eq!(ws.tabs.len(), 1);
        ws.add_tab(TabKind::Git);
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active_tab_kind(), TabKind::Git);
        ws.remove_tab(0);
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active_tab_kind(), TabKind::Git);
    }
}
