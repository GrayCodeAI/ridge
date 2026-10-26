#!/usr/bin/env bash
# Demo module for Ridge - demonstrates the module system.
# Drop this into ~/.config/ridge/modules/ and run 'module list' in Ridge.
#
# Receives $RIDGE_JSON environment variable with live app state.
# Outputs a status line that appears in `module list`.

read -r term_json

echo "demo-module: active panes=$(echo "$term_json" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('panes',0))" 2>/dev/null || echo "?")"
