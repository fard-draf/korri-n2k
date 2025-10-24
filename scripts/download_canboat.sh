#!/usr/bin/env bash
# Download script for canboat.json
# Run before building: ./scripts/download_canboat.sh

set -e

CANBOAT_URL="https://raw.githubusercontent.com/canboat/canboat/master/docs/canboat.json"
DEST_PATH="build_core/var/canboat.json"

echo "=== Downloading canboat.json from CANboat ==="

# Check if the file already exists
if [ -f "$DEST_PATH" ]; then
    echo "✓ $DEST_PATH already exists."

    # Inspect the current version
    CURRENT_VERSION=$(grep -oP '"Version":"[^"]*"' "$DEST_PATH" | cut -d'"' -f4 || echo "unknown")
    echo "  Current version : $CURRENT_VERSION"

    read -p "Download it again? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Download cancelled."
        exit 0
    fi
fi

# Create the directory if needed
mkdir -p "$(dirname "$DEST_PATH")"

# Download with curl or wget
if command -v curl &> /dev/null; then
    echo "Downloading with curl..."
    curl -fsSL "$CANBOAT_URL" -o "$DEST_PATH.tmp"
elif command -v wget &> /dev/null; then
    echo "Downloading with wget..."
    wget -q "$CANBOAT_URL" -O "$DEST_PATH.tmp"
else
    echo "❌ Error: neither curl nor wget is available."
    exit 1
fi

# Validate the downloaded file (JSON)
if grep -q '"SchemaVersion"' "$DEST_PATH.tmp"; then
    mv "$DEST_PATH.tmp" "$DEST_PATH"

    # Extract and display the version
    VERSION=$(grep -oP '"Version":"[^"]*"' "$DEST_PATH" | cut -d'"' -f4 || echo "unknown")
    SIZE=$(du -h "$DEST_PATH" | cut -f1)

    echo "✓ Download complete!"
    echo "  Version  : $VERSION"
    echo "  Size     : $SIZE"
    echo "  Path     : $DEST_PATH"
else
    rm -f "$DEST_PATH.tmp"
    echo "❌ Error: the downloaded file is not a valid canboat.json."
    exit 1
fi

echo ""
echo "You can now run: cargo build"
