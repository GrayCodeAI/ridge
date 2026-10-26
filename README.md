# Ridge

Mission control for your AI agents. Terminal-native TUI. Part of [GrayCodeAI](https://github.com/GrayCodeAI).

Formerly **Term**. The command and package are now `ridge`. Existing Term configuration, session data, task boards, race history, worktrees, and module manifests are read when their Ridge equivalents do not exist.

[![CI](https://github.com/GrayCodeAI/ridge/actions/workflows/ci.yml/badge.svg)](https://github.com/GrayCodeAI/ridge/actions/workflows/ci.yml)
[![Release](https://github.com/GrayCodeAI/ridge/actions/workflows/release.yml/badge.svg)](https://github.com/GrayCodeAI/ridge/actions/workflows/release.yml)
[![Security Audit](https://github.com/GrayCodeAI/ridge/actions/workflows/audit.yml/badge.svg)](https://github.com/GrayCodeAI/ridge/actions/workflows/audit.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Status

Full mission control: 12 agent adapters (Reflect, Claude, Codex, OpenCode, Gemini, Copilot, Aider, Kimi, Pi, Grok, Cursor, Generic), PTY terminal emulation, task orchestration with worktrees/leases/quality-gates/merge, session persistence, cost tracking, modules, themes, and mobile-width mode. See `CHANGELOG.md`.

## Docs

/md book site under `docs/` (mdBook) — built in CI; open `docs/book/index.html` or run `mdbook build docs` locally.

## Scripting & diagnostics

- `ridge --headless` — machine-readable agent/branch/status/task facts
- `ridge --dashboard` — full mission-control state as JSON
- `ridge --doctor` — check which agent CLIs / tools are installed

## Session timelines

Every pane's raw PTY output is continuously logged to `<data_dir>/timelines/pane-<id>.log` — replay what an agent did while you were away with `less ~/.local/share/ridge/timelines/pane-1.log`, `:timeline view <pane>` (live replay pane inside Ridge, follows new output), or `:timeline export postmortem.log`. A companion `.meta` file records timestamped byte-offset milestones for time-scanning. SSH/remote panes get status detection (blocked/idle/working) via the generic heuristics.

## Build

```sh
cargo build --release
cargo run
```

## Install

```sh
# From source
cargo install --path .

# Via install script (after the first Ridge release)
curl -fsSL https://raw.githubusercontent.com/GrayCodeAI/ridge/main/install.sh | sh

# or restore last session's panes
ridge --restore
```

## CI / Release

- CI runs on every push/PR: fmt, clippy, build, test (Linux/macOS/Windows).
- Tagging `v*` triggers a release build across 5 targets with auto-generated release notes.

## Keybindings

- `q` / `Ctrl+C` — quit
- `Tab` / `←` `→` — switch tabs
- `1`–`7` — jump to tab (7 = dashboard)
- `n` — spawn Claude Code pane
- `x` — close active pane
- `r` — resume last session
- `f` — fork active session
- `[` / `]` — switch active pane
- `:` — command mode (spawn, close, delegate, task add/start/done, race, branch, help)
- `:race <prompt>` — fan out to claude/codex/opencode, each in its own worktree + branch; `:race winner <idx>` merges the winner and cleans up losers
- `o` — reply inbox: delegations (`=` or `:delegate`) are tracked, agent replies captured when they block/finish
- Typed chars/Enter/Backspace — forwarded to active PTY

## Tabs

Dashboard (mission control summary), Agents (panes + status), Git, Diff, Files, Orchestration, Settings

## License

MIT
Unread badges: panes that enter **Blocked**/**Done** while unfocused get a ⚠ badge in the sidebar until focused (clears on `[`, `]`, or `a`).

Extra: `z` zoom, `{`/`}` reorder panes, `S`/`w` zellij-style split, `?` help overlay.
