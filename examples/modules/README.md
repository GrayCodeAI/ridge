# Term Example Modules

This directory contains example modules demonstrating Term's module system
(see `src/core/modules.rs`).

## Installing a Module

Copy a module directory to `~/.config/term/modules/`:

```bash
cp -r examples/modules/demo-module ~/.config/term/modules/
```

Then in Term, run:

```
:module list
:module run say-hello
:module run show-panes
```

## Module System

Modules are executables or scripts dropped into
`~/.config/term/modules/`. Each module can declare a `term-module.toml`
manifest with:

- **startup** hooks - commands run when the module loads
- **actions** - named commands invokable with `:module run <action-id>`
- **events** - commands fired on app events (pane_spawn, agent_blocked, etc.)
- **docks** - sidebar panel contributions
- **bars** - compact status bar widgets
- **settings** - user-editable configuration
- **worktree_provider** - custom worktree create/remove via JSON protocol

Modules receive `$TERM_JSON` (live app state as JSON) on their stdin/stdout.

## Example: demo-module

The `demo-module` demonstrates all manifest features:

```
:module list                          # List installed modules
:module run say-hello                 # Run an action
:module run show-panes                # Run an action with TERM_JSON
```

See `src/core/modules.rs` for the full module API specification.
