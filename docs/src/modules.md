# Modules

Ridge supports modules in any language via a stdio JSON protocol.

## Location
Place executable scripts in `~/.config/ridge/modules/`.

## Protocol
When Ridge runs a module, it invokes it with a single argument: `status`. The module should print one line to stdout. That line becomes the widget text in the Settings tab and status bar.

## Example
```sh
#!/bin/sh
case "$1" in
  status) echo "hello from module" ;;
  *) echo "usage: $0 status" ;;
esac
```

Make it executable: `chmod +x ~/.config/ridge/modules/hello.sh`

## Environment

Modules receive live state via the `RIDGE_JSON` environment variable:

```sh
#!/bin/sh
echo "panes=$RIDGE_JSON"   # {"panes":2,"blocked":1,"tasks":5}
```

## Refresh
Modules are refreshed every 20 ticks (~5 seconds). Output is displayed in the Settings tab under "Modules".

## Marketplace

`:module install <raw-url> [name]` downloads a script into ~/.config/ridge/modules/ (via curl, made executable). `:module list` / `:module remove <name>` manage them. Only install scripts you have read and trust — modules run with your user privileges.
A module registry is planned for v1.0.
