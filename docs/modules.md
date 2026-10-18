# Modules

Term supports modules in any language via a stdio JSON protocol.

## Location
Place executable scripts in `~/.config/term/modules/`.

## Protocol
When Term runs a module, it invokes it with a single argument: `status`. The module should print one line to stdout. That line becomes the widget text in the Settings tab and status bar.

## Example
```sh
#!/bin/sh
case "$1" in
  status) echo "hello from module" ;;
  *) echo "usage: $0 status" ;;
esac
```

Make it executable: `chmod +x ~/.config/term/modules/hello.sh`

## Refresh
Modules are refreshed every 20 ticks (~5 seconds). Output is displayed in the Settings tab under "Modules".

## Marketplace
A module registry is planned for v1.0.
