# Term

Mission control for your AI agents. Terminal-native TUI. Part of [Gryacodeai](https://gryacodeai.com).

## Status

Phase 0 — skeleton. PTY/agent detection, git/diff/files/orchestration coming in subsequent phases. See `term-plan.md`.

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
- `:` — command mode (spawn, close, delegate, task add/start/done, branch, help)
- Typed chars/Enter/Backspace — forwarded to active PTY

## Tabs

Agents (panes + status), Git, Diff, Files, Orchestration, Settings

## License

MIT
EOF