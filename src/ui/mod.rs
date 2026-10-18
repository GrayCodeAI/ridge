use crate::app::{App, Tab};
use crate::core::agent::AgentStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
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

    if app.show_help {
        render_help(f, size);
    }
    if app.show_inbox {
        render_inbox(f, app, size);
    }
}

/// Inbox overlay: captured replies from delegated prompts, newest first.
fn render_inbox(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(66, 80, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Inbox reply capture — o/Esc to close ");
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let unread = app.inbox.unread_count();
    let mut lines: Vec<String> = vec![format!(
        "{} unread / {} total — o or Esc closes; :inbox clear empties",
        unread,
        app.inbox.items.len()
    )];
    for (i, item) in app.inbox.items.iter().enumerate().rev() {
        let dot = if item.arrived && !item.read {
            "●"
        } else if item.arrived {
            "○"
        } else {
            "·"
        };
        let reply = if item.arrived {
            if item.reply.is_empty() {
                "(empty)".to_string()
            } else {
                item.reply.chars().take(86).collect()
            }
        } else {
            "waiting…".to_string()
        };
        lines.push(format!(
            "[{}] {} {} → {}",
            i + 1,
            dot,
            item.pane_title,
            reply
        ));
        lines.push(format!(
            "    prompt: {}",
            item.prompt.chars().take(90).collect::<String>()
        ));
    }
    if app.inbox.items.is_empty() {
        lines.push(
            "Nothing here yet. Delegate with =<pane> <msg> or :delegate <pane> <msg> —\
            replies are captured when the agent blocks or finishes."
                .to_string(),
        );
    }
    let text = lines.join("\n");
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
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
    let shown: Vec<String> = if app.search_results.is_empty() {
        vec![format!("/{}  (no matches)", app.search_buffer)]
    } else {
        app.search_results
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, r)| {
                let marker = if i == app.search_selected.clamp(0, app.search_results.len() - 1) {
                    "▸ "
                } else {
                    "  "
                };
                format!("{}{}", marker, r)
            })
            .collect()
    };
    let para = Paragraph::new(shown.join("  ")).style(Style::default().fg(Color::Magenta));
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
        Tab::Dashboard => render_dashboard(f, app, area),
    }
}

fn render_agents(f: &mut Frame, app: &App, area: Rect) {
    // Zoomed ('z'): active terminal takes the full width (sidebar hidden).
    if app.zoomed {
        render_terminal_for(f, app, area, app.active_pane);
        return;
    }

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
                let sid_str = p
                    .session_id
                    .as_deref()
                    .map(|s| format!(" {s}"))
                    .unwrap_or_default();
                let line = Line::from(vec![
                    Span::raw(marker),
                    Span::raw(indent),
                    Span::styled(p.title.clone(), Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(p.status.label(), Style::default().fg(status_color)),
                    Span::styled(cost_str, Style::default().fg(Color::Magenta)),
                    Span::styled(
                        if p.attention { "  ⚠" } else { "" },
                        Style::default().fg(Color::Red),
                    ),
                    Span::styled(sid_str, Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items);
        f.render_widget(list, inner);
    }

    // Main terminal area (plus optional zellij-style bottom split)
    let split_valid = app
        .split_bottom
        .is_some_and(|b| b != app.active_pane && b < app.panes.len());
    if split_valid {
        let cols = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(chunks[1]);
        render_terminal_for(f, app, cols[0], app.active_pane);
        render_terminal_for(f, app, cols[1], app.split_bottom.unwrap());
    } else {
        render_terminal(f, app, chunks[1]);
    }
}

fn render_terminal(f: &mut Frame, app: &App, area: Rect) {
    render_terminal_for(f, app, area, app.active_pane);
}

fn render_terminal_for(f: &mut Frame, app: &App, area: Rect, pane_idx: usize) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(if app.panes.is_empty() {
            " Terminal ".to_string()
        } else {
            format!(" Terminal — pane {} ", app.panes[pane_idx].id)
        });
    let main_inner = main_block.inner(area);
    f.render_widget(main_block, area);

    if let Some(pane) = app.panes.get(pane_idx) {
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
    let log = crate::core::git::log_graph(&app.cwd, 20);
    let prs = crate::core::git::gh_prs(&app.cwd);
    let issues = crate::core::git::gh_issues(&app.cwd);

    let text = format!(
        "Branch: {}\n\nStatus:\n{}\n\nHistory (graph):\n{}\n\nPull requests:\n{}\n\nIssues:\n{}",
        branch,
        if status.is_empty() {
            "(clean)".to_string()
        } else {
            status
        },
        if log.is_empty() {
            "(no commits)".to_string()
        } else {
            log
        },
        prs,
        issues
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

fn render_diff(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if app.side_by_side {
            " Diff — side-by-side (j/k scroll, n/p hunk, s unified) "
        } else {
            " Diff — unified (j/k scroll, n/p hunk, s side-by-side) "
        });
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.side_by_side {
        render_split_diff(f, app, inner);
    } else {
        render_unified_diff(f, app, inner);
    }
}

/// One aligned row of the side-by-side view: left = old side, right = new side.
#[derive(Debug, Clone)]
struct SideRow {
    left: String,
    right: String,
    left_style: Style,
    right_style: Style,
}

/// Build aligned (old | new) rows from cached diff sections.
fn build_side_rows(sections: &[(&'static str, String)]) -> Vec<SideRow> {
    let mut rows: Vec<SideRow> = Vec::new();
    let mut old: Vec<(String, Style)> = Vec::new();
    let mut new: Vec<(String, Style)> = Vec::new();

    let push_pair = |l: String, ls: Style, r: String, rs: Style, rows: &mut Vec<SideRow>| {
        rows.push(SideRow {
            left: l,
            right: r,
            left_style: ls,
            right_style: rs,
        });
    };

    // Flush pending -/+ block, zipping and padding to align.
    let flush = |old: &mut Vec<(String, Style)>,
                 new: &mut Vec<(String, Style)>,
                 rows: &mut Vec<SideRow>| {
        let n = old.len().max(new.len());
        for i in 0..n {
            let (ls, _rs) = if i < old.len() {
                old[i].clone()
            } else {
                (String::new(), Style::default())
            };
            let (rst, rn) = if i < new.len() {
                new[i].clone()
            } else {
                (String::new(), Style::default())
            };
            push_pair(ls, Color::Red.into(), rst, rn, rows);
        }
        old.clear();
        new.clear();
    };

    for (title, body) in sections {
        push_pair(
            format!("=== {title} ==="),
            Style::default().fg(Color::Cyan),
            format!("=== {title} ==="),
            Style::default().fg(Color::Cyan),
            &mut rows,
        );
        if body.is_empty() {
            push_pair(
                "(none)".into(),
                Style::default().fg(Color::DarkGray),
                "(none)".into(),
                Style::default().fg(Color::DarkGray),
                &mut rows,
            );
            continue;
        }
        for raw in body.lines().take(10_000) {
            if raw.starts_with("diff ") || raw.starts_with("index ") {
                flush(&mut old, &mut new, &mut rows);
                push_pair(
                    raw.to_string(),
                    Style::default().fg(Color::Magenta),
                    raw.to_string(),
                    Style::default().fg(Color::Magenta),
                    &mut rows,
                );
            } else if raw.starts_with("@@") {
                flush(&mut old, &mut new, &mut rows);
                push_pair(
                    raw.to_string(),
                    Style::default().fg(Color::Cyan),
                    raw.to_string(),
                    Style::default().fg(Color::Cyan),
                    &mut rows,
                );
            } else if raw.starts_with("---") || raw.starts_with("+++") {
                flush(&mut old, &mut new, &mut rows);
                push_pair(
                    raw.to_string(),
                    Style::default().fg(Color::Blue),
                    raw.to_string(),
                    Style::default().fg(Color::Blue),
                    &mut rows,
                );
            } else if raw.starts_with('-') {
                old.push((raw.to_string(), Style::default()));
            } else if raw.starts_with('+') {
                new.push((raw.to_string(), Style::default()));
            } else {
                flush(&mut old, &mut new, &mut rows);
                push_pair(
                    raw.to_string(),
                    Style::default().fg(Color::White),
                    raw.to_string(),
                    Style::default().fg(Color::White),
                    &mut rows,
                );
            }
        }
        flush(&mut old, &mut new, &mut rows);
    }
    rows
}

/// Unified view (single column).
fn render_unified_diff(f: &mut Frame, app: &App, inner: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let push_section = |title: &str, body: &String, lines: &mut Vec<Line>| {
        lines.push(Line::from(Span::styled(
            format!("=== {} ===", title),
            Style::default().fg(Color::Cyan),
        )));
        if body.is_empty() {
            lines.push(Line::from(Span::styled(
                "(none)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for raw in body.lines().take(10_000) {
                let style = match raw {
                    r if r.starts_with("+++") || r.starts_with("---") => {
                        Style::default().fg(Color::Blue)
                    }
                    r if r.starts_with("@@") => Style::default().fg(Color::Cyan),
                    r if r.starts_with('+') => Style::default().fg(Color::Green),
                    r if r.starts_with('-') => Style::default().fg(Color::Red),
                    r if r.starts_with("diff ") || r.starts_with("index ") => {
                        Style::default().fg(Color::Magenta)
                    }
                    _ => Style::default().fg(Color::White),
                };
                lines.push(Line::from(Span::styled(raw.to_string(), style)));
            }
        }
        lines.push(Line::from(""));
    };
    for (title, body) in &app.diff_sections {
        push_section(title, body, &mut lines);
    }
    let total = lines.len();
    let max_scroll = total.saturating_sub(inner.height as usize);
    let para = Paragraph::new(lines).scroll((app.diff_scroll.min(max_scroll) as u16, 0));
    f.render_widget(para, inner);
}

/// Side-by-side view (old | new aligned columns).
fn render_split_diff(f: &mut Frame, app: &App, inner: Rect) {
    let rows = build_side_rows(&app.diff_sections);

    let half = (inner.width / 2).max(1) as usize;
    let left: Vec<Line> = rows
        .iter()
        .map(|r| Line::from(Span::styled(trunc(&r.left, half), r.left_style)))
        .collect();
    let right: Vec<Line> = rows
        .iter()
        .map(|r| Line::from(Span::styled(trunc(&r.right, half), r.right_style)))
        .collect();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let total = rows.len();
    let max_scroll = total.saturating_sub(cols[0].height as usize);
    let sc = app.diff_scroll.min(max_scroll) as u16;
    f.render_widget(Paragraph::new(left).scroll((sc, 0)), cols[0]);
    f.render_widget(Paragraph::new(right).scroll((sc, 0)), cols[1]);
}

/// Clamp a string to `max` chars for column rendering.
fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn render_files(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Files (j/k scroll) ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = if app.files_cache.is_empty() {
        vec![Line::from(Span::styled(
            "(no tracked files or not a git repo)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.files_cache
            .iter()
            .take(10_000)
            .map(|name| {
                let tint = match app.files_status.get(name).copied() {
                    Some('A') => Style::default().fg(Color::Green),
                    Some('M') | Some('T') => Style::default().fg(Color::Yellow),
                    Some('D') => Style::default().fg(Color::Red),
                    Some('R') => Style::default().fg(Color::Magenta),
                    Some('C') | Some('U') => Style::default().fg(Color::Cyan),
                    _ => Style::default(),
                };
                Line::from(Span::styled(name.clone(), tint))
            })
            .collect()
    };

    let total = lines.len();
    let max_scroll = total.saturating_sub(inner.height as usize);
    let para = Paragraph::new(lines).scroll((app.files_scroll.min(max_scroll) as u16, 0));
    f.render_widget(para, inner);
}

fn render_orchestration(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Orchestration ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = format!(
        "Task board:\n\n{}\n\nKanban:\n{}\n\nReady: {:?}\n\nRace:\n{}\n\nQueue:\n{}\nCommands:\n  :task add <title>\n  :task start <id> [agent]\n  :task done <id>\n  :task lease <id> <paths...>\n  :task gate <id>\n  :task merge <id>\n  :task kanban\n  :task fanout <prompt>\n  :task from-inbox <n> — promote a captured reply into a task (bullets become a checklist)\n  :task check <id> <n>\n  :race <prompt> — fan a prompt to claude/codex/opencode in worktrees\n  :race gate <idx> · :race winner <idx> [force] · :race cancel
  :race leaderboard\n  :queue <pane> <msg> — deliver next time that pane blocks\n  :queue list|drop <id>|clear\n  :delegate <pane> <msg>",
        app.task_board.render(),
        app.task_board.render_kanban(),
        app.task_board.ready_tasks(),
        app.race
            .as_ref()
            .map(|r| r.render())
            .unwrap_or_else(|| "  (none — start with :race <prompt>)\n".to_string()),
        app.queue
            .render()
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let para = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(para, inner);
}

/// Single-glance mission-control panel: panes, race, queue, inbox, tasks.
fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Dashboard (7) — o inbox · a next blocked · :race · :queue ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let text = app.dashboard_lines().join("\n");
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
    let mut accounts = String::new();
    for a in &app.config.accounts {
        accounts.push_str(&format!("  {} / {}\n", a.agent, a.name));
    }
    if accounts.is_empty() {
        accounts = "  (none)\n".to_string();
    }
    let text = format!(
        "Theme: {}\nPrefix: {}\nQuit: {}\n\nConfig: ~/.config/term/term.toml\nData:   ~/.local/share/term\nCWD:    {}\nPanes:  {}\n\nAccounts:\n{}\nModules:\n{}",
        app.config.theme,
        app.config.keybindings.prefix,
        app.config.keybindings.quit,
        app.cwd.display(),
        app.panes.len(),
        accounts,
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
    let total_cost: f64 = app.panes.iter().map(|p| p.cost_usd).sum();
    let cost_str = if total_cost > 0.0 {
        format!("  ${:.2}  ", total_cost)
    } else {
        String::new()
    };
    let left = Span::styled(
        format!(" {}{}{} ", app.status_message, blocked_str, cost_str),
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

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}

const HELP_TEXT: &str = "Keybindings\n\
  q / Ctrl+C  quit        Tab / ← →   switch tab\n\
  1-7  jump to tab        n  spawn Claude     x  close pane\n\
  r  resume session       f  fork session     a  next blocked\n\
  [ ]  switch pane        o  reply inbox      j  k  scroll\n\
  n / p  next/prev hunk (Diff)\n\
  :  command mode         =  delegate         /  search\n\
  ?  toggle this help     Esc  close prompt/overlay\n\n\
Commands\n\
  spawn <agent> · close · delegate <pane> <msg> · broadcast <msg>\n\
  race <prompt> · race winner <idx> [force] · race gate <idx>
  race leaderboard · race cancel\n\
  inbox · inbox unread · inbox clear\n  queue <pane> <msg> · queue list|drop|clear\n\
  task add|start|done|lease|gate|merge|kanban|fanout|from-inbox|check <id> <n>\n\
  session save|clear · export costs|session · account list|add|env|show\n\
  config show|theme · branch · help";

fn render_help(f: &mut Frame, area: Rect) {
    let popup = centered_rect(66, 80, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (Esc to close) ");
    let para = Paragraph::new(HELP_TEXT)
        .style(Style::default().fg(Color::White))
        .block(block);
    f.render_widget(para, popup);
}
