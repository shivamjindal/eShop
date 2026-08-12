#!/usr/bin/env bash
# Build the Catalog Rust cdylib (native/crates/catalog) for .NET P/Invoke.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/native/Cargo.toml"
OUT_DIR="${1:-}"

# Prefer Command Line Tools when the full Xcode license is not accepted.
if [[ -d /Library/Developer/CommandLineTools ]] && ! xcodebuild -license check >/dev/null 2>&1; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Library/Developer/CommandLineTools}"
fi

if ! command -v cargo >/dev/null 2>&1; then
  # Keep `dotnet build` green on hosts/CI images without a Rust toolchain.
  # Stock P/Invoke still needs the cdylib at runtime when that path is exercised.
  echo "build-catalog-native: cargo not on PATH; skipping native build" >&2
  exit 0
fi

cargo build --release --manifest-path "$MANIFEST" -p catalog

TARGET_DIR="$ROOT/native/target/release"
if [[ "$(uname -s)" == "Darwin" ]]; then
  LIB="$TARGET_DIR/libcatalog.dylib"
elif [[ "$(uname -s)" == "Linux" ]]; then
  LIB="$TARGET_DIR/libcatalog.so"
else
  LIB="$TARGET_DIR/catalog.dll"
fi

if [[ ! -f "$LIB" ]]; then
  echo "build-catalog-native: expected library missing at $LIB" >&2
  exit 1
fi

if [[ -n "$OUT_DIR" ]]; then
  mkdir -p "$OUT_DIR"
  cp "$LIB" "$OUT_DIR/"
  echo "build-catalog-native: staged $(basename "$LIB") -> $OUT_DIR"
fi
