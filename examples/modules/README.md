# Ridge Example Modules

This directory contains example modules demonstrating Ridge's module system
(see `src/core/modules.rs`).

## Installing a Module

Copy a module directory to `~/.config/ridge/modules/`:

```bash
cp -r examples/modules/demo-module ~/.config/ridge/modules/
```

Then in Ridge, run:

```
:module list
:module run say-hello
:module run show-panes
```

## Module System

Modules are executables or scripts dropped into
`~/.config/ridge/modules/`. Each module can declare a `ridge-module.toml`
manifest with:

- **startup** hooks - commands run when the module loads
- **actions** - named commands invokable with `:module run <action-id>`
- **events** - commands fired on app events (pane_spawn, agent_blocked, etc.)
- **docks** - sidebar panel contributions
- **bars** - compact status bar widgets
- **settings** - user-editable configuration
- **worktree_provider** - custom worktree create/remove via JSON protocol

Modules receive `$RIDGE_JSON` (live app state as JSON) on their stdin/stdout.

## Example: demo-module

The `demo-module` demonstrates all manifest features:

```
:module list                          # List installed modules
:module run say-hello                 # Run an action
:module run show-panes                # Run an action with RIDGE_JSON
```

See `src/core/modules.rs` for the full module API specification.
