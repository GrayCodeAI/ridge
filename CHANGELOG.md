# Changelog

## v0.5.0

- Race leaderboard: race outcomes persisted to `<repo>/.term-races.json` (last 50), `:race leaderboard` aggregates wins per agent and lists the three most recent races (with `[forced]` markers)
- Reply checklists: promoting an inbox reply into a task (`:task from-inbox`) seeds a per-task checklist from `-`/`*` bullet lines; `:task check <id> <n>` toggles items; the board renders `[x]`/`[ ]`
- Runtime artifacts (`.term-*.json`, `.term-worktrees/`) added to `.gitignore`
- 54 unit tests (leaderboard cap/sort/roundtrip, checklist derivation/toggle/persistence, `:race leaderboard` + `:task check` command paths)

## v0.4.0

- Race quality gates (`:race gate <idx>` / `:race winner <idx> [force]` / `:race gate-cmd <cmd>`): winner promotion is gated — `cargo test` by default in Rust worktrees; gate ✓/✗ shown per contestant; `force` overrides
- Reply → task promotion: `:task from-inbox <n>` turns a captured reply into an orchestration task; new persisted `Task.notes` field, board shows ` [reply: …]`
- Wait-for-human queue (`:queue <pane> <msg>`): prompts parked per pane are delivered automatically the next time that agent blocks, recorded in the reply inbox; `:queue list | drop | clear`, Orchestration-tab Queue section, stale prompts auto-dropped
- 44 unit tests (race gate integration in real temp git repos, inbox-threading, queue lifecycle/delivery)

## v0.3.0

- Fan-out races (`:race <prompt>`): claude/codex/opencode each race in their own git worktree + branch; `:race winner <idx>` merges the winner and cleans up losers; statuses mirrored live in the Orchestration tab
- Reply inbox (`o`): delegations (`=<pane>`, `:delegate`, `:broadcast`) recorded; agent replies captured automatically when they block or finish; `:inbox unread`, `:inbox clear`
- `merge_branch` helper in worktree API with branch validation
- 32 unit tests (race render, inbox lifecycle/predicate/trim, plus prior suite)

## v0.2.0

- Side-by-side diff view (`s`), hunk navigation (`n`/`p`), cached rendering
- zellij-style split panes (`S`/`w`), zoom (`z`), pane reorder (`{`/`}`)
- Help overlay (`?`), searchable results navigation
- Task filter (`task filter`), git log graph view, per-pane session IDs
- Module marketplace commands (`module install/list/remove`), `TERM_JSON` module protocol
- Remote panes over SSH (`ssh <host>`), worktree commands
- Account env injection, `config show/theme`, session export, attention badges + bell
- Headless mode (`term --headless`) and session restore (`term --restore`)
- Security: worktree path validation, weekly cargo audit, SECURITY.md
- Distribution: crates.io metadata, MIT LICENSE, mdBook docs, README badges
- Homebrew tap auto-update job (runs when HOMEBREW_TAP_TOKEN is set)

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
