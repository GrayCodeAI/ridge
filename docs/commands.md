# Commands

Type `:` to enter command mode.

- `spawn <agent>` — spawn an agent pane
- `close` — close active pane
- `delegate <pane> <msg>` — send message to a pane
- `broadcast <msg>` — send to all panes
- `session save` / `session clear` — persist or delete saved panes
- `export costs` — cost CSV
- `export session <file>` — open-pane snapshot as JSON — export cost CSV
- `account list / add <agent> <name> / env <agent> KEY=VAL / show <agent>`
- `module list / install <url> [name] / remove <name>` — literally install module scripts
- `ssh <host> [args]` — open a remote shell pane over SSH
- `worktree list / add <branch> / remove <path>` — manage git worktrees
- `config show / config theme <n>`
- `task add <title>` — add task
- `task start <id> [agent]` — start task (creates worktree)
- `task done <id>` — mark done
- `task lease <id> <paths...>` — lease paths
- `task gate <id>` — run quality gate
- `task merge <id>` — merge branch
- `task filter <status|text|all>` — filter the board view
- `task from-inbox <n>` — promote inbox item `n` (1 = newest, as numbered in the `o` overlay) into a task; the captured reply is stored in its notes and shown on the board as ` [reply: …]`
- `task kanban` — show kanban
- `task fanout <prompt>` — fan out to multiple agents
- `task subagent <parent> <agent>` — spawn subagent
- `race <prompt>` — start a fan-out race: claude, codex, and opencode each get their own git worktree + branch, and the prompt is delegated to them immediately
- `race` / `race status` — show contestants, statuses, gates, and the winner (★)
- `race gate <idx>` — run the quality gate in one contestant's worktree (`cargo test` by default for Rust repos; `:race gate-cmd <cmd>` overrides)
- `race winner <idx> [force]` — gate the contestant, merge its branch, and clean up the losers; a failed gate blocks promotion unless `force` is given
- `race cancel` — close all race panes and remove their worktrees
- `inbox` — open the reply inbox overlay (captured replies from delegations)
- `inbox unread` — count unread captured replies
- `inbox clear` — empty the inbox
- `branch` — show current branch
- `help` — list commands

## Headless

Run `term --headless` (no TUI) to print machine-readable facts for scripts: registered agents, current branch, `git status` lines, and persisted tasks.
