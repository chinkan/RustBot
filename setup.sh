#!/usr/bin/env bash
# RustFox setup wizard entry point.
#
# Usage:
#   ./setup.sh          # Opens browser-based wizard on http://localhost:8719
#   ./setup.sh --cli    # Interactive terminal wizard

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="$SCRIPT_DIR/target/release/setup"

# Build the setup binary if it isn't present or is older than its source.
if [[ ! -f "$BINARY" ]] || [[ "$SCRIPT_DIR/src/bin/setup.rs" -nt "$BINARY" ]] || [[ "$SCRIPT_DIR/setup/index.html" -nt "$BINARY" ]]; then
    echo "Building setup wizard…"
    cargo build --release --bin setup --manifest-path "$SCRIPT_DIR/Cargo.toml"
fi

RUSTFOX_ROOT="$SCRIPT_DIR" "$BINARY" "$@"
