#!/usr/bin/env bash
set -euo pipefail

# Build in release mode
cargo build --release

# Ensure local bin exists
install_dir="${HOME}/.local/bin"
mkdir -p "$install_dir"

# Copy binary
cp target/release/stash "$install_dir/"

echo "Installed stash to $install_dir/stash"
