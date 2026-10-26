#!/usr/bin/env bash
# ridge install script
# Installs the Ridge TUI mission control for AI agents.
set -euo pipefail

VERSION="${RIDGE_VERSION:-1.0.0}"
REPO="GrayCodeAI/ridge"
BINARY_NAME="ridge"

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
INSTALL_DIR="${RIDGE_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR"

# Download from GitHub releases
ASSET_NAME="ridge-${PLATFORM}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ASSET_NAME}"

echo "Downloading ridge ${VERSION} for ${PLATFORM}..."
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
if [ -f "$TMPDIR/ridge" ]; then
  BIN_PATH="$TMPDIR/ridge"
elif [ -f "$TMPDIR/ridge-${VERSION}-${PLATFORM}/ridge" ]; then
  BIN_PATH="$TMPDIR/ridge-${VERSION}-${PLATFORM}/ridge"
else
  echo "Could not find ridge binary in archive" >&2; exit 1
fi

cp "$BIN_PATH" "$INSTALL_DIR/ridge"
chmod +x "$INSTALL_DIR/ridge"

echo "Installed ridge to $INSTALL_DIR/ridge"
echo ""
echo "Add $INSTALL_DIR to your PATH if it's not already:"
echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
echo ""
echo "Run 'ridge --help' to get started."
