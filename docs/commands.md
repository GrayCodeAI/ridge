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
- `ssh <host> [args]` — open a remote shell pane over SSH (status detection uses the generic heuristics: idle shell prompts, `(y/n)`/permission requests read as blocked)
- `worktree list / add <branch> / remove <path>` — manage git worktrees
- `config show / config theme <n> / config notify on|off` — show config, set theme, toggle OS notifications (fires on blocked/finished agents; notify-send on Linux, osascript on macOS)
- `task add <title>` — add task
- `task start <id> [agent]` — start task (creates worktree)
- `task done <id>` — mark done
- `task lease <id> <paths...>` — lease paths
- `task gate <id>` — run quality gate
- `task merge <id>` — merge branch
- `task filter <status|text|all>` — filter the board view
- `task from-inbox <n>` — promote inbox item `n` (1 = newest, as numbered in the `o` overlay) into a task; the captured reply is stored in its notes and shown on the board as ` [reply: …]`
- `task check <id> <n>` — toggle item `n` on a task's checklist (1-based); checklists are auto-seeded from `-`/`*` bullet lines in the promoted reply
- `task kanban` — show kanban
- `task fanout <prompt>` — fan out to multiple agents
- `task subagent <parent> <agent>` — spawn subagent
- `race <prompt>` — start a fan-out race: claude, codex, and opencode each get their own git worktree + branch, and the prompt is delegated to them immediately
- `race` / `race status` — show contestants, statuses, gates, and the winner (★)
- `race leaderboard` — aggregated wins per agent from past races (persisted to `<repo>/.term-races.json`, last 50 outcomes) plus the three most recent races
- `race gate <idx>` — run the quality gate in one contestant's worktree (`cargo test` by default for Rust repos; `:race gate-cmd <cmd>` overrides)
- `race winner <idx> [force]` — gate the contestant, merge its branch, and clean up the losers; a failed gate blocks promotion unless `force` is given
- `race cancel` — close all race panes and remove their worktrees
- `inbox` — open the reply inbox overlay (captured replies from delegations)
- `inbox unread` — count unread captured replies
- `inbox clear` — empty the inbox
- `queue <pane_idx> <msg>` — park a prompt for that pane; it is delivered automatically (and recorded in the inbox) the next time the pane blocks on the user
- `queue list` / `queue drop <id>` / `queue clear` — manage parked prompts
- `timeline` — list each live pane's session log (path, size, paused flag, seconds idleness); logs live under `<data_dir>/timelines/pane-<id>.log`
- `timeline on|off [pane_idx]` — enable/disable timeline capture (on by default)
- `timeline view [pane_idx]` — open a replay pane that scrolls the pane's session log and auto-follows new output (unix)
- `timeline export <file>` — copy the active pane's session log into the repo
- `branch` — show current branch
- `help` — list commands

## Headless

Run `term --headless` (no TUI) to print machine-readable facts for scripts: registered agents, current branch, `git status` lines, and persisted tasks.

## Dashboard JSON

Run `term --dashboard` (no TUI) to print the full mission-control state as pretty JSON: pane list (status/cost/attention/session id), active race with contestants and gate results, queued prompts, inbox counters, task-board summary with ready ids, module count, and race-history length.

## Doctor

Run `term --doctor` to sanity-check the environment and exit: each agent CLI found on PATH (claude, codex, opencode, gemini, copilot, aider, kimi, pi, grok, cursor-agent), git and gh availability, and that the data dir is writable. Exit code stays 0; missing items are reported with hints. Add `--fix` (`term --doctor --fix`) to print concrete copy-paste install steps (`→ [Agent] npm install -g …`) for every failing check.

## Timelines

Every pane's raw output is continuously logged under `<data_dir>/timelines/pane-<id>.log`. A companion `pane-<id>.meta` file records timestamped byte-offset milestones (every 4 KiB of activity or 30 s of silence) so post-mortems can be scrubbed over time:

```sh
less ~/.local/share/term/timelines/pane-1.log     # raw session replay
cat  ~/.local/share/term/timelines/pane-1.meta    # "epoch_ms bytes" milestones
```

Inside Term, `:timeline view <pane>` opens a replay pane that streams the log live.
