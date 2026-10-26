#!/bin/sh
# Install Ridge — mission control for your AI agents.
# Usage: curl -fsSL https://raw.githubusercontent.com/GrayCodeAI/ridge/main/install.sh | sh
set -eu

REPO="GrayCodeAI/ridge"
BIN="ridge"
INSTALL_DIR="${RIDGE_INSTALL_DIR:-$HOME/.local/bin}"

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

TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')"
if [ -z "$TAG" ]; then
  echo "Could not determine latest release." >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/ridge-${TARGET}.tar.gz"
echo "Downloading Ridge ${TAG} (${TARGET})..."
mkdir -p "$INSTALL_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
if ! curl -fsSL "$URL" -o "$TMP/ridge.tar.gz"; then
  echo "No Ridge release asset found. Build from source: cargo install --git https://github.com/GrayCodeAI/ridge" >&2
  exit 1
fi
tar xzf "$TMP/ridge.tar.gz" -C "$TMP"
install -m 0755 "$TMP/ridge" "$INSTALL_DIR/$BIN"

echo "Installed $BIN to $INSTALL_DIR/$BIN"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add $INSTALL_DIR to your PATH to use 'ridge'." ;;
esac
