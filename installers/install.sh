#!/bin/sh
set -e

# Defaults
TAG=""
GALFUS_DIR="${HOME}/.galfus"
BIN_DIR="${GALFUS_DIR}/bin"
MANIFEST_URL="https://storage.galfus.com/cli/manifest.json" # Correct storage URL

# Check dependencies
if ! command -v curl >/dev/null 2>&1; then
    echo "Error: curl is required to install Galfus."
    exit 1
fi

if ! command -v grep >/dev/null 2>&1; then
    echo "Error: grep is required to install Galfus."
    exit 1
fi

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --tag)
            TAG="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1"
            echo "Usage: $0 [--tag <tag>]"
            exit 1
            ;;
    esac
done

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux)  TARGET_OS="linux" ;;
    Darwin) TARGET_OS="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) TARGET_ARCH="x64" ;;
    arm64|aarch64) TARGET_ARCH="arm64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

echo "=> Fetching manifest from ${MANIFEST_URL}..."
MANIFEST_JSON=$(curl -sSL "$MANIFEST_URL")

if [ -z "$TAG" ]; then
    # Extract latest_tag from JSON manually (since jq might not be available)
    LATEST_TAG=$(echo "$MANIFEST_JSON" | grep '"latest_tag"' | sed -n -E 's/.*"latest_tag"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p')
    if [ -z "$LATEST_TAG" ]; then
        echo "Error: Could not determine latest_tag from manifest."
        exit 1
    fi
    TAG="$LATEST_TAG"
    echo "=> Selected tag: ${TAG} (latest)"
else
    echo "=> Selected tag: ${TAG}"
fi

# Extract version for the selected tag
VERSION=$(echo "$MANIFEST_JSON" | grep -A 10 '"tags"' | sed -n -E 's/.*"'"$TAG"'"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p')

if [ -z "$VERSION" ]; then
    echo "Error: Tag '${TAG}' not found in the manifest."
    exit 1
fi

echo "=> Version: ${VERSION}"

DOWNLOAD_URL="https://storage.galfus.com/cli/${TAG}/${VERSION}/${TARGET_OS}/${TARGET_ARCH}/galfus-cli-${TARGET_OS}-${TARGET_ARCH}"
echo "=> Downloading from ${DOWNLOAD_URL}..."

mkdir -p "$BIN_DIR"
TMP_BIN="/tmp/galfus-cli"

curl -sSL "$DOWNLOAD_URL" -o "$TMP_BIN"

# Check if downloaded file is an HTML error page or similar
if head -n 1 "$TMP_BIN" | grep -q "^<"; then
    echo "Error: Failed to download binary (File not found on CDN)."
    rm -f "$TMP_BIN"
    exit 1
fi

chmod +x "$TMP_BIN"
mv "$TMP_BIN" "${BIN_DIR}/galfus"

echo "=> Installed galfus to ${BIN_DIR}/galfus"

# Try to update PATH
PROFILE_ADDED=""
for profile in "${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.profile"; do
    if [ -f "$profile" ]; then
        if ! grep -q "GALFUS_HOME" "$profile"; then
            echo "" >> "$profile"
            echo 'export GALFUS_HOME="$HOME/.galfus"' >> "$profile"
            echo 'export PATH="$GALFUS_HOME/bin:$PATH"' >> "$profile"
            PROFILE_ADDED="$profile"
        fi
    fi
done

if [ -n "$PROFILE_ADDED" ]; then
    echo "=> Added ${BIN_DIR} to PATH in ${PROFILE_ADDED}"
    echo "=> Please restart your terminal or run: source ${PROFILE_ADDED}"
else
    echo "=> Note: Make sure to add ${BIN_DIR} to your PATH."
fi

echo "=> Galfus installation complete!"
