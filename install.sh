#!/bin/sh
# Install Term — mission control for your AI agents.
# Usage: curl -fsSL https://gryacodeai.com/term/install.sh | sh
set -eu

REPO="GrayCodeAI/term"
BIN="term"
INSTALL_DIR="${TERM_INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Linux)  OS_SLUG="unknown-linux" ;;
  Darwin) OS_SLUG="apple-darwin" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac
case "$ARCH" in
  x86_64|amd64) ARCH_SLUG="x86_64" ;;
  aarch64|arm64) ARCH_SLUG="aarch64" ;;
  *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac

if [ "$OS" = "Linux" ]; then
  TARGET="${ARCH_SLUG}-unknown-linux-musl"
else
  TARGET="${ARCH_SLUG}-${OS_SLUG}"
fi

TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest"   | sed -n 's/.*"tag_name": *"\([^"]*\)".*//p')"
if [ -z "$TAG" ]; then
  echo "Could not determine latest release." >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/term-${TARGET}.tar.gz"
echo "Downloading Term ${TAG} (${TARGET})..."
mkdir -p "$INSTALL_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP/term.tar.gz"
tar xzf "$TMP/term.tar.gz" -C "$TMP"
install -m 0755 "$TMP/term" "$INSTALL_DIR/$BIN"

echo "Installed $BIN to $INSTALL_DIR/$BIN"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add $INSTALL_DIR to your PATH to use 'term'." ;;
esac
