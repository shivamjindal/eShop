#!/usr/bin/env bash
set -euo pipefail

# Lever for the native/ Cargo workspace (structural Rust landing zones).
# For Catalog .NET + Rust end-to-end, use ./scripts/check-catalog.sh instead.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Prefer Command Line Tools when the full Xcode license is not accepted (macOS linker).
if [[ -d /Library/Developer/CommandLineTools ]] && ! xcodebuild -license check >/dev/null 2>&1; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Library/Developer/CommandLineTools}"
fi

NATIVE_MANIFEST="native/Cargo.toml"

if [[ ! -f "$NATIVE_MANIFEST" ]]; then
  echo "check-native: path=R:native-missing exit_code=1" >&2
  echo "Expected workspace manifest at $NATIVE_MANIFEST" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-native: path=R:rust-missing-toolchain exit_code=2" >&2
  echo "cargo is not on PATH." >&2
  exit 2
fi

echo "check-native: path=R:cargo-test-workspace"
echo "check-native: running: cargo test --manifest-path $NATIVE_MANIFEST --workspace"
set +e
cargo test --manifest-path "$NATIVE_MANIFEST" --workspace
ec=$?
set -e
echo "check-native: path=R:cargo-test-workspace exit_code=$ec"
exit "$ec"
