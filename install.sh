#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"

echo "Building flow workspace in release mode..."
cargo build --workspace --release

mkdir -p "$INSTALL_DIR"
cp "target/release/flow" "$INSTALL_DIR/flow"
cp "target/release/flow-cli" "$INSTALL_DIR/flow-cli"

BOARD_DIR="$HOME/.config/flow/boards/default"
mkdir -p "$BOARD_DIR"
cp -r boards/demo/* "$BOARD_DIR/"

echo "Installed flow and flow-cli to $INSTALL_DIR"
echo "Default board copied to $BOARD_DIR"
