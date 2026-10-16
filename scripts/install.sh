#!/usr/bin/env bash
# Installs the flow binaries to ~/.local/bin.
set -euo pipefail

cd "$(dirname "$0")/.."

INSTALL_DIR="$HOME/.local/bin"

echo "Building flow workspace in release mode..."
cargo build --workspace --release

mkdir -p "$INSTALL_DIR"
cp "target/release/flow" "$INSTALL_DIR/flow"
cp "target/release/flow-cli" "$INSTALL_DIR/flow-cli"

echo "Installed flow and flow-cli to $INSTALL_DIR"
echo "The database is created automatically at ~/.config/flow/flow.db on first run."
