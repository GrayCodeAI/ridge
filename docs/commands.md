# Commands

Type `:` to enter command mode.

- `spawn <agent>` — spawn an agent pane
- `close` — close active pane
- `delegate <pane> <msg>` — send message to a pane
- `broadcast <msg>` — send to all panes
- `session save` / `session clear` — persist or delete saved panes
- `export costs` — export cost CSV
- `account list` / `account add <agent> <name>`
- `task add <title>` — add task
- `task start <id> [agent]` — start task (creates worktree)
- `task done <id>` — mark done
- `task lease <id> <paths...>` — lease paths
- `task gate <id>` — run quality gate
- `task merge <id>` — merge branch
- `task kanban` — show kanban
- `task fanout <prompt>` — fan out to multiple agents
- `task subagent <parent> <agent>` — spawn subagent
- `branch` — show current branch
- `help` — list commands
