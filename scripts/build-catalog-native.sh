#!/usr/bin/env bash
set -euo pipefail

# Build the Catalog Rust cdylib (release) for .NET LibraryImport.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" == "Darwin" && -x /Library/Developer/CommandLineTools/usr/bin/clang ]]; then
  export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER:-/Library/Developer/CommandLineTools/usr/bin/clang}"
  if [[ -d /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk ]]; then
    export SDKROOT="${SDKROOT:-/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk}"
  fi
fi

cargo build --release --manifest-path native/Cargo.toml -p catalog
