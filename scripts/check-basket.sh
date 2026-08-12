#!/usr/bin/env bash
set -euo pipefail

# Lever for the Basket.API .NET -> Rust migration (plan.md).
# Builds and tests the Rust service, runs the .NET Basket tests while they still exist, and
# replays the recorded parity transcript against the Rust service when Docker is available.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUST_CRATE_DIR="native/basket_service"
DOTNET_TEST_PROJ="tests/Basket.UnitTests/Basket.UnitTests.csproj"
PARITY_SCRIPT="scripts/parity-basket.sh"
# When set to 1, fail if the expected Rust crate is missing (the skills require it).
MIGRATION_REQUIRE_RUST="${MIGRATION_REQUIRE_RUST:-0}"
# When set to 1, skip the parity replay (it needs Docker and takes ~30s).
BASKET_SKIP_PARITY="${BASKET_SKIP_PARITY:-0}"

run_and_report() {
  local path_label="$1"
  shift
  echo "check-basket: path=$path_label"
  echo "check-basket: running: $*"
  set +e
  "$@"
  local ec=$?
  set -e
  echo "check-basket: path=$path_label exit_code=$ec"
  return "$ec"
}

# --- Rust service (the migration target) ---
if [[ -f "$RUST_CRATE_DIR/Cargo.toml" ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "check-basket: path=R:rust-missing-toolchain exit_code=2" >&2
    echo "Rust crate found at $RUST_CRATE_DIR but cargo is not on PATH." >&2
    exit 2
  fi
  echo "check-basket: Rust crate detected at $RUST_CRATE_DIR"
  if ! run_and_report "R:cargo-test" cargo test --manifest-path "$RUST_CRATE_DIR/Cargo.toml"; then
    echo "check-basket: Rust tests failed" >&2
    exit 1
  fi
  if ! run_and_report "R:cargo-build-release" cargo build --release --manifest-path "$RUST_CRATE_DIR/Cargo.toml"; then
    echo "check-basket: Rust release build failed" >&2
    exit 1
  fi
elif [[ "$MIGRATION_REQUIRE_RUST" == "1" ]]; then
  echo "check-basket: path=R:rust-required-missing exit_code=1" >&2
  echo "MIGRATION_REQUIRE_RUST=1 but no Cargo.toml at $RUST_CRATE_DIR." >&2
  echo "Add the Rust service per .cursor/skills/migrate-to-rust/SKILL.md" >&2
  exit 1
else
  echo "check-basket: no Rust crate at $RUST_CRATE_DIR (skip; set MIGRATION_REQUIRE_RUST=1 to require it)"
fi

# --- .NET Basket tests (only while the .NET service exists) ---
if [[ -f "$DOTNET_TEST_PROJ" ]]; then
  if ! run_and_report "A:Basket.UnitTests" dotnet test "$DOTNET_TEST_PROJ"; then
    echo "check-basket: .NET Basket tests failed" >&2
    exit 1
  fi
else
  echo "check-basket: no .NET Basket test project (the service is fully migrated)"
fi

# --- Parity replay against the recorded .NET transcript ---
if [[ "$BASKET_SKIP_PARITY" == "1" ]]; then
  echo "check-basket: path=P:parity-skipped exit_code=0 (BASKET_SKIP_PARITY=1)"
  exit 0
fi

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  echo "check-basket: path=P:parity-unavailable exit_code=2" >&2
  echo "Docker is required for the parity replay; set BASKET_SKIP_PARITY=1 to skip it." >&2
  exit 2
fi

run_and_report "P:parity-replay" "$PARITY_SCRIPT"
