#!/usr/bin/env bash
# term install script
# Installs the Term TUI mission control for AI agents.
set -euo pipefail

VERSION="${TERM_VERSION:-1.0.0}"
REPO="GrayCodeAI/term"
BINARY_NAME="term"

# Detect platform
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

case "$OS" in
  linux) PLATFORM="${ARCH}-unknown-linux-gnu" ;;
  darwin) PLATFORM="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

# Determine install dir
INSTALL_DIR="${TERM_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR"

# Download from GitHub releases
ASSET_NAME="term-${VERSION}-${PLATFORM}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ASSET_NAME}"

echo "Downloading term ${VERSION} for ${PLATFORM}..."
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL -o "$TMPDIR/$ASSET_NAME" "$DOWNLOAD_URL"
elif command -v wget >/dev/null 2>&1; then
  wget -q -O "$TMPDIR/$ASSET_NAME" "$DOWNLOAD_URL"
else
  echo "Neither curl nor wget is available" >&2; exit 1
fi

tar -xzf "$TMPDIR/$ASSET_NAME" -C "$TMPDIR"

# Find the binary
BIN_PATH=""
if [ -f "$TMPDIR/term" ]; then
  BIN_PATH="$TMPDIR/term"
elif [ -f "$TMPDIR/term-${VERSION}-${PLATFORM}/term" ]; then
  BIN_PATH="$TMPDIR/term-${VERSION}-${PLATFORM}/term"
else
  echo "Could not find term binary in archive" >&2; exit 1
fi

cp "$BIN_PATH" "$INSTALL_DIR/term"
chmod +x "$INSTALL_DIR/term"

echo "Installed term to $INSTALL_DIR/term"
echo ""
echo "Add $INSTALL_DIR to your PATH if it's not already:"
echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
echo ""
echo "Run 'term --help' to get started."
