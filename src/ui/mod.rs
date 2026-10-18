use crate::app::{App, Tab};
use crate::core::agent::AgentStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
};

pub fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Mobile-width mode: narrow terminal -> single focused pane.
    if size.width <= 64 {
        render_mobile(f, app, size);
        return;
    }

    let extra = if app.command_mode || app.delegate_mode || app.search_mode {
        1
    } else {
        0
    };
    let mut constraints = vec![Constraint::Length(3), Constraint::Min(0)];
    if extra == 1 {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    render_tabs(f, app, chunks[0]);
    render_body(f, app, chunks[1]);
    let mut idx = 2;
    if app.command_mode {
        render_command_bar(f, app, chunks[idx]);
        idx += 1;
    } else if app.delegate_mode {
        render_delegate_bar(f, app, chunks[idx]);
        idx += 1;
    } else if app.search_mode {
        render_search_bar(f, app, chunks[idx]);
        idx += 1;
    }
    render_status(f, app, chunks[idx]);
}

fn render_mobile(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(format!(
        " Term mobile | pane {} | {} ",
        app.active_pane, app.status_message
    ))
    .style(Style::default().fg(Color::Cyan));
    f.render_widget(header, chunks[0]);

    // Reuse agents body for the single pane.
    render_agents(f, app, chunks[1]);

    let footer = Paragraph::new(" n:spawn  x:close  r:resume  f:fork  :cmd  q:quit ")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);
}

fn render_command_bar(f: &mut Frame, app: &App, area: Rect) {
    let para = Paragraph::new(format!(":{}", app.command_buffer))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(para, area);
}

fn render_delegate_bar(f: &mut Frame, app: &App, area: Rect) {
    let para = Paragraph::new(format!("={}", app.delegate_buffer))
        .style(Style::default().fg(Color::Green));
    f.render_widget(para, area);
}

fn render_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.search_results.is_empty() {
        format!("/{}  (no matches)", app.search_buffer)
    } else {
        format!("/{}  -> {}", app.search_buffer, app.search_results[0])
    };
    let para = Paragraph::new(text).style(Style::default().fg(Color::Magenta));
    f.render_widget(para, area);
}

fn theme_accent(theme: &str) -> Color {
    match theme {
        "light" => Color::Blue,
        "amber" => Color::Yellow,
        "green" => Color::Green,
        "red" => Color::Red,
        "magenta" => Color::Magenta,
        _ => Color::Cyan,
    }
}

fn theme_bg(theme: &str) -> Color {
    match theme {
        "light" => Color::White,
        _ => Color::Black,
    }
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let accent = theme_accent(&app.config.theme);
    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let selected = Tab::ALL
        .iter()
        .position(|t| *t == app.current_tab)
        .unwrap_or(0);
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Term "))
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn render_body(f: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Agents => render_agents(f, app, area),
        Tab::Git => render_git(f, app, area),
        Tab::Diff => render_diff(f, app, area),
        Tab::Files => render_files(f, app, area),
        Tab::Orchestration => render_orchestration(f, app, area),
        Tab::Settings => render_settings(f, app, area),
    }
}

fn render_agents(f: &mut Frame, app: &App, area: Rect) {
    // Sidebar (agent/pane list) + main terminal.
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(area);

    // Sidebar
    let sidebar_block = Block::default().borders(Borders::ALL).title(" Panes ");
    let inner = sidebar_block.inner(chunks[0]);
    f.render_widget(sidebar_block, chunks[0]);

    if app.panes.is_empty() {
        let p = Paragraph::new("No panes.\n\nPress 'n' to spawn\nClaude Code.")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, inner);
    } else {
        let items: Vec<ListItem> = app
            .panes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let marker = if i == app.active_pane { "▶ " } else { "  " };
                let indent = if p.parent_pane.is_some() {
                    "  └ "
                } else {
                    ""
                };
                let status_color = p.status.color();
                let cost_str = if p.cost_usd > 0.0 {
                    format!(" ${:.2}", p.cost_usd)
                } else {
                    String::new()
                };
                let line = Line::from(vec![
                    Span::raw(marker),
                    Span::raw(indent),
                    Span::styled(p.title.clone(), Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(p.status.label(), Style::default().fg(status_color)),
                    Span::styled(cost_str, Style::default().fg(Color::Magenta)),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items);
        f.render_widget(list, inner);
    }

    // Main terminal area
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(if app.panes.is_empty() {
            " Terminal ".to_string()
        } else {
            format!(" Terminal — pane {} ", app.panes[app.active_pane].id)
        });
    let main_inner = main_block.inner(chunks[1]);
    f.render_widget(main_block, chunks[1]);

    if let Some(pane) = app.panes.get(app.active_pane) {
        if let Some(pty) = &pane.pty {
            let text = pty.screen_text();
            let para = Paragraph::new(text).style(Style::default().fg(Color::White));
            f.render_widget(para, main_inner);
        } else {
            let p = Paragraph::new("No PTY attached.").style(Style::default().fg(Color::Gray));
            f.render_widget(p, main_inner);
        }
    } else {
        let p = Paragraph::new("Spawn an agent with 'n' (Claude Code).")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, main_inner);
    }
}

fn render_git(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Git ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let branch = crate::core::git::current_branch(&app.cwd);
    let status = crate::core::git::status_porcelain(&app.cwd);
    let log = crate::core::git::log_oneline(&app.cwd, 10);
    let prs = crate::core::git::gh_prs(&app.cwd);

    let text = format!(
        "Branch: {}\n\nStatus:\n{}\n\nRecent commits:\n{}\n\nPull requests:\n{}",
        branch,
        if status.is_empty() {
            "(clean)".to_string()
        } else {
            status
        },
        log,
        prs
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_diff(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Diff ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let staged = crate::core::git::diff_staged(&app.cwd);
    let worktree = crate::core::git::diff_worktree(&app.cwd);
    let untracked = crate::core::git::untracked(&app.cwd);
    let conflicted = crate::core::git::conflicted(&app.cwd);

    let text = format!(
        "=== Staged ===\n{}\n\n=== Worktree ===\n{}\n\n=== Untracked ===\n{}\n\n=== Conflicted ===\n{}",
        if staged.is_empty() {
            "(none)".to_string()
        } else {
            staged
        },
        if worktree.is_empty() {
            "(none)".to_string()
        } else {
            worktree
        },
        if untracked.is_empty() {
            "(none)".to_string()
        } else {
            untracked
        },
        if conflicted.is_empty() {
            "(none)".to_string()
        } else {
            conflicted
        }
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_files(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Files ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let out = crate::core::git::run_git(&app.cwd, &["ls-files"]);
    let text = if out.is_empty() {
        "(no tracked files or not a git repo)".to_string()
    } else {
        out
    };
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_orchestration(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Orchestration ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = format!(
        "Task board:\n\n{}\n\nKanban:\n{}\n\nReady: {:?}\n\nCommands:\n  :task add <title>\n  :task start <id> [agent]\n  :task done <id>\n  :task lease <id> <paths...>\n  :task gate <id>\n  :task merge <id>\n  :task kanban\n  :task fanout <prompt>\n  :delegate <pane> <msg>",
        app.task_board.render(),
        app.task_board.render_kanban(),
        app.task_board.ready_tasks()
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_settings(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Settings ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut modules = String::new();
    for (name, out) in &app.module_outputs {
        modules.push_str(&format!("  {name} -> {out}\n"));
    }
    if modules.is_empty() {
        modules = "  (none in ~/.config/term/modules)\n".to_string();
    }
    let text = format!(
        "Theme: {}\nPrefix: {}\nQuit: {}\n\nConfig: ~/.config/term/term.toml\nData:   ~/.local/share/term\nCWD:    {}\nPanes:  {}\n\nModules:\n{}",
        app.config.theme,
        app.config.keybindings.prefix,
        app.config.keybindings.quit,
        app.cwd.display(),
        app.panes.len(),
        modules
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let blocked = app
        .panes
        .iter()
        .filter(|p| p.status == AgentStatus::Blocked)
        .count();
    let blocked_str = if blocked > 0 {
        format!("  ⚠ {} blocked  ", blocked)
    } else {
        String::new()
    };
    let left = Span::styled(
        format!(" {}{} ", app.status_message, blocked_str),
        Style::default().fg(Color::White),
    );
    let right = Span::styled(
        " q:quit  n:spawn  a:next-blocked  x:close  r:resume  f:fork  [/]:pane  ::cmd  Tab:tabs  ",
        Style::default().fg(Color::DarkGray),
    );
    let line = Line::from(vec![left, right]);
    let para = Paragraph::new(line);
    f.render_widget(para, area);
}
