# Changelog

## v0.1.0 — full mission control (rolling release)

### Agents
- 11 adapters: Claude Code, Codex, OpenCode, Gemini CLI, GitHub Copilot, Aider, Kimi, Pi, Grok Build, Cursor Agent, Generic
- Per-agent detect / spawn / resume / session-files / status / cost parsing
- Account switcher with env injection (`account add/env/show`, `config show/theme`)

### Mission control
- Tabs: Agents · Git (status, log graph, PRs, issues) · Diff (syntax colors, scroll, n/p hunk nav, s side-by-side) · Files (status tint, scroll) · Orchestration (tasks, leases, gates, merge, kanban, fanout, filter) · Settings
- PTY + vt100 emulation, scrollback, resize
- zellij-style split (`S`/`w`), zoom (`z`), pane reorder (`{` `}`)
- Attention badges + terminal bell, approval inbox (`a`), help overlay (`?`)
- Mobile-width mode (≤64 cols), themes, search with result navigation

### Sessions
- Resume, fork, inline `=` delegation, broadcast
- Session persistence: auto-save on quit, `term --restore`, `session save/clear`, `export session`
- Per-pane session ID display

### Orchestration
- Task board with deps, worktrees, path leases, quality gates, merge; JSON persistence
- Sub-agent tree (parent/child panes)

### Modules
- stdio module system w/ hot-refresh, `TERM_JSON` live-state env protocol
- `module install/list/remove` marketplace commands

### Infrastructure
- CI (fmt/clippy/build/test on 3 OS) + Release (5 targets) + manual crates.io publish
- Homebrew tap, install.sh, docs/, SECURITY.md, weekly cargo audit
