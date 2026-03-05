#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="flow"
INSTALL_DIR="$HOME/.local/bin"

echo "Building $BINARY_NAME in release mode..."
cargo build --release

mkdir -p "$INSTALL_DIR"
cp "target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"

BOARD_DIR="$HOME/.config/flow/boards/default"
mkdir -p "$BOARD_DIR"
cp -r boards/demo/* "$BOARD_DIR/"

echo "Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"
echo "Default board copied to $BOARD_DIR"
