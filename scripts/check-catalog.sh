#!/usr/bin/env bash
set -euo pipefail

# Lever for Catalog migration slices. Prefer unit/characterization tests;
# fall back to functional tests only when Docker is available.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UNIT_PROJ="tests/Catalog.UnitTests/Catalog.UnitTests.csproj"
FUNC_PROJ="tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj"
RUST_CRATE="crates/catalog_stock/Cargo.toml"

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
  if [[ "$ec" -ne 0 ]]; then
    exit "$ec"
  fi
}

# Prove the Rust stock crate builds for the host integration target (wasm).
# Host `cargo test` needs a working native linker; skip it when that fails
# (common on macOS before Xcode license acceptance). Docker cargo test is optional.
if [[ -f "$RUST_CRATE" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    run_and_report "R:catalog_stock(wasm)" \
      cargo build --manifest-path "$RUST_CRATE" --target wasm32-unknown-unknown
  fi

  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    run_and_report "R:catalog_stock(docker-test)" docker run --rm \
      -v "$ROOT/crates/catalog_stock:/crate" \
      -w /crate rust:1.88-bookworm \
      cargo test
  fi
fi

if [[ -f "$UNIT_PROJ" ]]; then
  run_and_report "A:Catalog.UnitTests" dotnet test --project "$UNIT_PROJ"
  exit 0
fi

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  if [[ -f "$FUNC_PROJ" ]]; then
    run_and_report "B:Catalog.FunctionalTests" dotnet test --project "$FUNC_PROJ"
    exit 0
  fi
fi

echo "check-catalog: path=C:unavailable exit_code=2" >&2
echo "No Catalog unit tests found and Docker is unavailable for functional tests." >&2
exit 2
