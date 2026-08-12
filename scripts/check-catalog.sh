#!/usr/bin/env bash
set -euo pipefail

# Lever for Catalog migration units (.NET → Rust).
# Builds/tests the native/ Rust workspace when present, then prefers
# unit/characterization tests; falls back to functional tests only when Docker
# is available.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UNIT_PROJ="tests/Catalog.UnitTests/Catalog.UnitTests.csproj"
FUNC_PROJ="tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj"
# Rust workspace under native/ (Catalog landing zone: native/crates/catalog).
# Formerly native/catalog_stock/ — absorbed into the workspace layout.
NATIVE_MANIFEST="native/Cargo.toml"
# When set to 1, fail if the expected Rust workspace is missing (skills require it).
MIGRATION_REQUIRE_RUST="${MIGRATION_REQUIRE_RUST:-0}"

run_and_report() {
  local path_label="$1"
  shift
  echo "check-catalog: path=$path_label"
  echo "check-catalog: running: $*"
  set +e
  "$@"
  local ec=$?
  set -e
  echo "check-catalog: path=$path_label exit_code=$ec"
  return "$ec"
}

# Prefer Command Line Tools clang when Xcode license is not accepted (macOS).
if [[ "$(uname -s)" == "Darwin" && -x /Library/Developer/CommandLineTools/usr/bin/clang ]]; then
  export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER:-/Library/Developer/CommandLineTools/usr/bin/clang}"
  if [[ -d /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk ]]; then
    export SDKROOT="${SDKROOT:-/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk}"
  fi
fi

# --- Rust workspace (required by Migrate to Rust when present / when forced) ---
if [[ -f "$NATIVE_MANIFEST" ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "check-catalog: path=R:rust-missing-toolchain exit_code=2" >&2
    echo "Rust workspace found at $NATIVE_MANIFEST but cargo is not on PATH." >&2
    exit 2
  fi
  echo "check-catalog: Rust workspace detected at $NATIVE_MANIFEST"
  if ! run_and_report "R:cargo-test-workspace" cargo test --manifest-path "$NATIVE_MANIFEST" --workspace; then
    echo "check-catalog: Rust tests failed" >&2
    exit 1
  fi
  if ! run_and_report "R:cargo-build-release-workspace" cargo build --release --manifest-path "$NATIVE_MANIFEST" --workspace; then
    echo "check-catalog: Rust release build failed" >&2
    exit 1
  fi
elif [[ "$MIGRATION_REQUIRE_RUST" == "1" ]]; then
  echo "check-catalog: path=R:rust-required-missing exit_code=1" >&2
  echo "MIGRATION_REQUIRE_RUST=1 but no Cargo.toml at $NATIVE_MANIFEST." >&2
  echo "Add the Rust workspace per native/README.md" >&2
  exit 1
else
  echo "check-catalog: no Rust workspace at $NATIVE_MANIFEST (skip; set MIGRATION_REQUIRE_RUST=1 to require it)"
fi

# --- .NET Catalog tests ---
if [[ -f "$UNIT_PROJ" ]]; then
  set +e
  run_and_report "A:Catalog.UnitTests" dotnet test --project "$UNIT_PROJ"
  ec=$?
  set -e
  exit "$ec"
fi

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  if [[ -f "$FUNC_PROJ" ]]; then
    set +e
    run_and_report "B:Catalog.FunctionalTests" dotnet test --project "$FUNC_PROJ"
    ec=$?
    set -e
    exit "$ec"
  fi
fi

echo "check-catalog: path=C:unavailable exit_code=2" >&2
echo "No Catalog unit tests found and Docker is unavailable for functional tests." >&2
echo "Add unit/characterization tests via the Migrate to Rust skill," >&2
echo "or start Docker and re-run for Catalog.FunctionalTests." >&2
exit 2
