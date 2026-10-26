#!/bin/sh
# Example Ridge module. Place in ~/.config/ridge/modules/ and chmod +x.
case "$1" in
  status)
    echo "hello from module"
    ;;
  *)
    echo "usage: $0 status"
    ;;
esac
