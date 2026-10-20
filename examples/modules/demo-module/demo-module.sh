#!/usr/bin/env bash
# Demo module for Term - demonstrates the module system.
# Drop this into ~/.config/term/modules/ and run 'module list' in Term.
#
# Receives $TERM_JSON environment variable with live app state.
# Outputs a status line that appears in `module list`.

read -r term_json

echo "demo-module: active panes=$(echo "$term_json" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('panes',0))" 2>/dev/null || echo "?")"
