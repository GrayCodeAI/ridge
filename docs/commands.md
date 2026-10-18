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
- `task kanban` — show kanban
- `task fanout <prompt>` — fan out to multiple agents
- `task subagent <parent> <agent>` — spawn subagent
- `race <prompt>` — start a fan-out race: claude, codex, and opencode each get their own git worktree + branch, and the prompt is delegated to them immediately
- `race winner <idx>` — merge the winning contestant's branch and clean up the losing panes/worktrees
- `race cancel` — close all race panes and remove their worktrees
- `race` / `race status` — show contestants, statuses, and the winner (★)
- `branch` — show current branch
- `help` — list commands

## Headless

Run `term --headless` (no TUI) to print machine-readable facts for scripts: registered agents, current branch, `git status` lines, and persisted tasks.
