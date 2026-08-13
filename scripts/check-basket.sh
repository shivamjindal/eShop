#!/usr/bin/env bash
set -euo pipefail

# Lever for the Basket.API .NET -> Rust migration.
# Runs the Rust workspace (tests + release build, because Aspire launches the release
# binary) and any .NET Basket tests that still exist. Behavioral parity against the
# recorded transcript lives in ./scripts/parity-basket.sh, which needs Docker.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NATIVE_MANIFEST="native/Cargo.toml"
UNIT_PROJ="tests/Basket.UnitTests/Basket.UnitTests.csproj"
TRANSCRIPT="scripts/parity/basket-transcript.jsonl"
# When set to 1, fail if the Rust workspace is missing (the migration skills require it).
MIGRATION_REQUIRE_RUST="${MIGRATION_REQUIRE_RUST:-1}"

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

if [[ ! -f "$NATIVE_MANIFEST" ]]; then
  if [[ "$MIGRATION_REQUIRE_RUST" == "1" ]]; then
    echo "check-basket: path=R:rust-required-missing exit_code=1" >&2
    echo "No Cargo workspace at $NATIVE_MANIFEST; see native/README.md" >&2
    exit 1
  fi
  echo "check-basket: no Rust workspace at $NATIVE_MANIFEST (skipping)"
else
  if ! command -v cargo >/dev/null 2>&1; then
    echo "check-basket: path=R:rust-missing-toolchain exit_code=2" >&2
    echo "Rust workspace found at $NATIVE_MANIFEST but cargo is not on PATH." >&2
    exit 2
  fi
  if ! run_and_report "R:cargo-test-workspace" cargo test --manifest-path "$NATIVE_MANIFEST" --workspace; then
    echo "check-basket: Rust tests failed" >&2
    exit 1
  fi
  if ! run_and_report "R:cargo-build-release" cargo build --release --manifest-path "$NATIVE_MANIFEST" --bin basket-service --bin basket-parity; then
    echo "check-basket: Rust release build failed" >&2
    exit 1
  fi
fi

if [[ ! -s "$TRANSCRIPT" ]]; then
  echo "check-basket: path=P:transcript-missing exit_code=1" >&2
  echo "Expected a recorded parity transcript at $TRANSCRIPT" >&2
  exit 1
fi
echo "check-basket: path=P:transcript-present ($(grep -c . "$TRANSCRIPT") recorded cases)"

if [[ -f "$UNIT_PROJ" ]]; then
  set +e
  run_and_report "A:Basket.UnitTests" dotnet test "$UNIT_PROJ"
  ec=$?
  set -e
  exit "$ec"
fi

echo "check-basket: no .NET Basket project left; Rust owns the service"
echo "check-basket: run ./scripts/parity-basket.sh (needs Docker) to replay the transcript"
exit 0
