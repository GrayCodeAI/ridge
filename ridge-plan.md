# Ridge — Product Plan

> Mission control for your AI agents. Terminal-native TUI. Part of Gryacodeai.com.

## 1. Vision & Positioning

**Ridge** is a Rust TUI that gives you one place to run, watch, resume, and orchestrate every coding agent — without leaving your terminal or breaking SSH/mobile workflows.

**Positioning vs. the field:**
- vs. **tmux/Zellij**: adds agent-aware state, cost tracking, git/diff/files, orchestration
- vs. **Luvus**: similar scope, but Ridge is the Gryacodeai-branded, open-source take with a cleaner module system
- vs. **Herdr**: broader (not just runtime state — full mission control)
- vs. **Orca/cmux**: terminal-native, no GUI lock-in, SSH-first

**Tagline:** "Ridge — mission control for your AI agents."

## 2. Target Users

- Developers running 2+ coding agents in parallel (Claude Code, Codex, OpenCode, Gemini, Copilot, Aider, …)
- SSH-heavy / remote-dev workflows (agents on a beefy box, driven from laptop/phone)
- Want agent state visibility (working/blocked/done/idle) without polling
- Want git/diff review + file viewing without leaving the terminal

## 3. Tech Stack

- **Language:** Rust (edition 2024)
- **TUI:** ratatui + crossterm
- **Async runtime:** tokio
- **PTY:** portable-pty
- **Git:** git2 (libgit2 bindings) + shelling out to `git`/`gh` where needed
- **Config:** serde + toml (`~/.config/ridge/ridge.toml`)
- **State persistence:** SQLite (rusqlite) for sessions/workspaces, or JSON files for v0.1
- **CLI args:** clap
- **Logging:** tracing
- **Distribution:** single static binary via `cargo build --release`, plus `cargo install ridge`

## 4. Architecture

```
ridge/
├── src/
│   ├── main.rs              # entrypoint, CLI (clap)
│   ├── app.rs               # top-level App state + event loop
│   ├── event.rs             # crossterm event polling + custom events
│   ├── ui/                  # ratatui rendering
│   │   ├── layout.rs        # pane/tab layout
│   │   ├── theme.rs
│   │   ├── statusbar.rs
│   │   └── views/           # one module per tab
│   │       ├── agents.rs
│   │       ├── git.rs
│   │       ├── diff.rs
│   │       ├── files.rs
│   │       ├── orchestration.rs
│   │       └── settings.rs
│   ├── core/
│   │   ├── agent.rs         # Agent trait + registry + detection
│   │   ├── pty.rs           # PTY spawn/attach/read
│   │   ├── session.rs       # session resume/fork logic
│   │   ├── workspace.rs     # worktree management
│   │   ├── git.rs           # git operations
│   │   └── cost.rs          # token/cost parsing from session files
│   ├── agents/              # per-agent adapters
│   │   ├── claude.rs
│   │   ├── codex.rs
│   │   ├── opencode.rs
│   │   ├── gemini.rs
│   │   ├── copilot.rs
│   │   ├── aider.rs
│   │   ├── kimi.rs
│   │   ├── pi.rs
│   │   └── generic.rs       # fallback for any CLI
│   ├── modules/             # extension system (any language, via stdio/JSON)
│   └── remote/              # SSH attach, mobile-width mode
├── Cargo.toml
└── README.md
```

### Core abstractions

- **Agent** trait: `name()`, `detect()`, `spawn()`, `resume_cmd()`, `session_files()`, `status_from_output()`, `cost_from_session()`
- **Pane**: owns a PTY + scrollback + agent binding + status
- **Workspace**: a git worktree + its panes + metadata
- **Orchestration board**: tasks with deps, each task → worktree + pane + quality gate

## 5. Feature Set (v0.1 — "Full Mission Control")

### 5.1 Agent Status & Sessions
- Detect running agents (Claude, Codex, Gemini, Copilot, Aider, OpenCode, Kimi, Pi, Grok, Cursor, + generic)
- Status: working / blocked (needs you) / done / idle — based on positive evidence only
- Per-project tab: list agents, context-window fullness, session cost, project total
- Inline answer to blocked agents (type into pane without switching)
- Session resume: find each agent's session store on disk, run resume command (zero config)
- Session fork: branch a session into a new pane (right-click / `Ctrl+Space f`)

### 5.2 Agent Messaging
- `=` + pane name delegates the rest of the line: `=codex add tests to src/parse.rs`
- Sender doesn't wait; picks up reply when it arrives

### 5.3 Git Tab
- Commits, branch flow, branches, PRs, issues, repo overview
- Drives `git` + `gh` CLI (never disagrees with shell)

### 5.4 Native Diff Review
- Staged / worktree / untracked / conflicted kept separate
- Auto / Split / Stack views
- Local notes on a line or range → send focused feedback to a live agent
- Read-only: never stages, discards, commits, or pushes

### 5.5 Files
- File tree tinted by git status
- Viewer auto-refreshes as file changes on disk
- Read in place or send to `$EDITOR`
- Modules can add a panel below

### 5.6 Orchestration
- Task board (◇ orch) with dependencies
- Each started task → isolated git worktree + its own pane
- Path leases prevent overlapping edits
- Quality gate (test command) must pass before task counts as done
- Finished branches merge through a dedicated worktree; your checkout is never touched

### 5.7 Worktrees
- A worktree per branch
- Switch between worktrees without losing state

### 5.8 Layout & Theming
- Movable panels, themes, remappable keys
- Luvus-style "Luvus Bar" equivalent: modules publish widgets to top/bottom status row
- Mobile mode: at ≤64 columns, switch to one focused pane + 2-row header + full-screen navigator

### 5.9 Remote
- Attach to a session on another box over plain SSH (no port forwarding, no daemon)
- Only changed cells cross the wire (~22 bytes/keystroke)

### 5.10 Modules / Extensibility
- Modules in any language (stdio + JSON protocol)
- Publish widgets to the status bar
- Add panels below the file viewer

### 5.11 Global Fuzzy Search
- Search across files, agents, tasks, commands

## 6. Phased Delivery (realistic)

Even though v0.1 target is "full mission control", ship in phases to stay sane:

### Phase 0 — Skeleton (Week 1)
- Cargo project, CLI (clap), config loading, logging
- Event loop (tokio + crossterm), basic ratatui layout with tabs
- CI: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`

### Phase 1 — PTY + Agent Detection (Weeks 2–3)
- PTY spawn/attach, scrollback buffer
- Agent trait + registry
- Claude Code + Codex + OpenCode adapters (status detection)
- Working/blocked/done/idle status
- Single-pane + multi-pane layout

### Phase 2 — Sessions + Workspaces (Weeks 4–5)
- Session resume (find session files, run resume cmd)
- Session fork
- Git worktree creation per agent
- Workspace switching

### Phase 3 — Git Tab + Diff Review (Weeks 6–7)
- Git tab (commits, branches, PRs via `gh`)
- Native diff review (staged/worktree/untracked/conflicted)
- Line notes → send to agent

### Phase 4 — Files + Orchestration (Weeks 8–9)
- File tree + auto-refreshing viewer
- Orchestration board with deps, worktrees, path leases, quality gates

### Phase 5 — Polish (Weeks 10–12)
- Agent messaging (`=` delegation)
- Cost/context tracking
- Mobile-width mode
- Remote SSH attach
- Modules system
- Themes + keybindings
- Global fuzzy search

### Phase 6 — Release
- README, docs, install script (`curl … | sh`)
- crates.io publish (`cargo install ridge`)
- GitHub releases with prebuilt binaries (Linux/macOS/Windows)
- Homebrew tap

## 7. Key Technical Decisions

| Decision | Choice | Rationale |
|---|---|---|
| TUI lib | ratatui | Ecosystem standard, matches Zellij/Luvus |
| Async | tokio | Industry standard |
| PTY | portable-pty | Cross-platform, used by WezTerm |
| Git | git2 + shelling git/gh | git2 for status/diff, gh for PRs/issues |
| State store | JSON files (v0.1) → SQLite (v0.2) | Keep v0.1 simple |
| Agent detection | Output parsing + optional hooks | Hooks give precise events; parsing is fallback |
| Session files | Per-agent adapters read native stores | Zero config for users |
| Modules | stdio JSON protocol | Language-agnostic, simple |

## 8. Agent Support Matrix (v0.1)

| Agent | Status detection | Session resume | Cost tracking |
|---|---|---|---|
| Claude Code | ✅ hooks + parse | ✅ | ✅ (session files) |
| Codex | ✅ parse | ✅ | 🟡 |
| OpenCode | ✅ parse | ✅ | 🟡 |
| Gemini CLI | ✅ parse | ✅ | 🟡 |
| GitHub Copilot | ✅ parse | ✅ | 🟡 |
| Aider | ✅ parse | ✅ | 🟡 |
| Kimi | ✅ parse | ✅ | 🟡 |
| Pi | ✅ parse | ✅ | 🟡 |
| Grok Build | ✅ parse | 🟡 | 🟡 |
| Cursor Agent | ✅ parse | 🟡 | 🟡 |
| Generic (any CLI) | 🟡 process-name | ❌ | ❌ |

## 9. Risks

1. **Scope creep** — "full mission control" as v0.1 is large. Mitigate: ship phases, cut features if needed.
2. **Agent detection fragility** — CLIs change output formats. Mitigate: hooks preferred, parsing as fallback, generic adapter.
3. **PTY/terminal edge cases** — resize, unicode, scrollback. Mitigate: lean on portable-pty + ratatui's tested rendering.
4. **Git operations safety** — must never corrupt user's repo. Mitigate: read-only diff review, worktree isolation, tests.
5. **Cross-platform** — Linux first, macOS second, Windows third. Test on Linux only for v0.1.

## 10. Success Criteria for v0.1

- [ ] Run ≥3 agents in parallel with live status
- [ ] Resume a Claude Code session with one keypress
- [ ] Review a diff and send line comments to an agent
- [ ] Create a task → worktree → pane → quality gate → merge flow end-to-end
- [ ] Works over SSH from a phone (mobile-width mode)
- [ ] Single binary, `cargo install ridge` works
- [ ] README + install script + docs

## 11. Next Steps

1. Create repo `ridge` under your GitHub account
2. Scaffold Cargo project (Phase 0)
3. Implement PTY + agent detection (Phase 1)
4. Iterate

---

**Decision needed:** Confirm stack (Rust+ratatui) and that we start with Phase 0 scaffolding. If yes, I'll create the project in your working directory.
