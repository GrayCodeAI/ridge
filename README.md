# Term

Mission control for your AI agents. Terminal-native TUI. Part of [GrayCodeAI](https://github.com/GrayCodeAI).

[![CI](https://github.com/GrayCodeAI/term/actions/workflows/ci.yml/badge.svg)](https://github.com/GrayCodeAI/term/actions/workflows/ci.yml)
[![Release](https://github.com/GrayCodeAI/term/actions/workflows/release.yml/badge.svg)](https://github.com/GrayCodeAI/term/actions/workflows/release.yml)
[![Security Audit](https://github.com/GrayCodeAI/term/actions/workflows/audit.yml/badge.svg)](https://github.com/GrayCodeAI/term/actions/workflows/audit.yml)
[![Crates.io](https://img.shields.io/crates/v/term.svg)](https://crates.io/crates/term)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Status

Full mission control: 11 agent adapters (Claude, Codex, OpenCode, Gemini, Copilot, Aider, Kimi, Pi, Grok, Cursor, Generic), PTY terminal emulation, task orchestration with worktrees/leases/quality-gates/merge, session persistence, cost tracking, modules, themes, and mobile-width mode. 23 tests, CI on 3 OS. See `CHANGELOG.md`.

## Docs

/md book site under `docs/` (mdBook) — built in CI; open `docs/book/index.html` or run `mdbook build docs` locally.

## Build

```sh
cargo build --release
cargo run
```

## Install

```sh
# From source
cargo install --path .

# Via install script
curl -fsSL https://gryacodeai.com/term/install.sh | sh

# Via Homebrew (once tap is published)
brew install gryacodeai/tap/term
# or restore last session's panes
term --restore
```

## CI / Release

- CI runs on every push/PR: fmt, clippy, build, test (Linux/macOS/Windows).
- Tagging `v*` triggers a release build across 5 targets with auto-generated release notes.

## Keybindings

- `q` / `Ctrl+C` — quit
- `Tab` / `←` `→` — switch tabs
- `1`–`6` — jump to tab
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

Agents (panes + status), Git, Diff, Files, Orchestration, Settings

## License

MIT
Unread badges: panes that enter **Blocked**/**Done** while unfocused get a ⚠ badge in the sidebar until focused (clears on `[`, `]`, or `a`).

Extra: `z` zoom, `{`/`}` reorder panes, `S`/`w` zellij-style split, `?` help overlay.
