#!/bin/sh
# Example Term module. Place in ~/.config/term/modules/ and chmod +x.
case "$1" in
  status)
    echo "hello from module"
    ;;
  *)
    echo "usage: $0 status"
    ;;
esac
