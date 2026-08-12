#!/usr/bin/env bash
set -euo pipefail

# Lever for Basket migration units (.NET → Rust), named in plan.md.
# Fails closed: the Rust basket crate must build and test, and the .NET characterization suite must
# pass for as long as src/Basket.API still exists.
#
# Behavioral parity against the recorded .NET transcript lives in ./scripts/parity-basket.sh
# (needs Docker); this script is the fast, Docker-free check.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NATIVE_MANIFEST="native/Cargo.toml"
UNIT_PROJ="tests/Basket.UnitTests/Basket.UnitTests.csproj"

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
  echo "check-basket: path=R:native-missing exit_code=1" >&2
  echo "Expected the Rust workspace manifest at $NATIVE_MANIFEST" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-basket: path=R:rust-missing-toolchain exit_code=2" >&2
  echo "cargo is not on PATH; the basket service is a Rust binary." >&2
  exit 2
fi

if ! run_and_report "R:cargo-test-basket" cargo test --manifest-path "$NATIVE_MANIFEST" -p basket; then
  echo "check-basket: Rust tests failed" >&2
  exit 1
fi

# The AppHost starts basket-api from the release profile, so prove that binary builds.
if ! run_and_report "R:cargo-build-release-basket" \
  cargo build --release --manifest-path "$NATIVE_MANIFEST" -p basket; then
  echo "check-basket: Rust release build failed" >&2
  exit 1
fi

if [[ -f "$UNIT_PROJ" ]]; then
  if ! run_and_report "A:Basket.UnitTests" dotnet test "$UNIT_PROJ"; then
    echo "check-basket: .NET Basket characterization tests failed" >&2
    exit 1
  fi
else
  echo "check-basket: path=A:dotnet-basket-retired (Basket.API was migrated to Rust)"
fi

echo "check-basket: path=OK exit_code=0"
