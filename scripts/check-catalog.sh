#!/usr/bin/env bash
set -euo pipefail

# Lever for Catalog migration units (.NET → Rust stock island).
# Builds/tests Rust when present, then prefers unit/characterization tests;
# falls back to functional tests only when Docker is available.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UNIT_PROJ="tests/Catalog.UnitTests/Catalog.UnitTests.csproj"
FUNC_PROJ="tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj"
RUST_CRATE_DIR="native/catalog_stock"
# When set to 1, fail if the expected Rust crate is missing (skills require it).
MIGRATION_REQUIRE_RUST="${MIGRATION_REQUIRE_RUST:-0}"

# Prefer Command Line Tools when Xcode.app is selected but its license blocks `cc` linking.
if [[ -z "${DEVELOPER_DIR:-}" && -d /Library/Developer/CommandLineTools ]]; then
  if ! xcodebuild -checkFirstLaunchStatus >/dev/null 2>&1; then
    export DEVELOPER_DIR=/Library/Developer/CommandLineTools
    echo "check-catalog: using DEVELOPER_DIR=$DEVELOPER_DIR (Xcode license / first-launch not ready)"
  fi
fi

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

# --- Rust island (required by Migrate to Rust when present / when forced) ---
if [[ -f "$RUST_CRATE_DIR/Cargo.toml" ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "check-catalog: path=R:rust-missing-toolchain exit_code=2" >&2
    echo "Rust crate found at $RUST_CRATE_DIR but cargo is not on PATH." >&2
    exit 2
  fi
  echo "check-catalog: Rust crate detected at $RUST_CRATE_DIR"
  if ! run_and_report "R:cargo-test" cargo test --manifest-path "$RUST_CRATE_DIR/Cargo.toml"; then
    echo "check-catalog: Rust tests failed" >&2
    exit 1
  fi
  if ! run_and_report "R:cargo-build-release" cargo build --release --manifest-path "$RUST_CRATE_DIR/Cargo.toml"; then
    echo "check-catalog: Rust release build failed" >&2
    exit 1
  fi
elif [[ "$MIGRATION_REQUIRE_RUST" == "1" ]]; then
  echo "check-catalog: path=R:rust-required-missing exit_code=1" >&2
  echo "MIGRATION_REQUIRE_RUST=1 but no Cargo.toml at $RUST_CRATE_DIR." >&2
  echo "Add the Rust island per .cursor/skills/migrate-to-rust/SKILL.md" >&2
  exit 1
else
  echo "check-catalog: no Rust crate at $RUST_CRATE_DIR (skip; set MIGRATION_REQUIRE_RUST=1 to require it)"
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
