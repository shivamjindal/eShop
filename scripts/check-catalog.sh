#!/usr/bin/env bash
set -euo pipefail

# Catalog check lever for eShop migration demos.
# If a Rust crate exists under native/, cargo test (+ release build for cdylib) first.
# Then Catalog.UnitTests if present; else functional tests when Docker is up; else exit 2.
# Prints which path ran.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

ran_rust=0
ran_unit=0
ran_functional=0

rust_crate=""
if [[ -f "native/catalog_stock/Cargo.toml" ]]; then
  rust_crate="native/catalog_stock"
else
  shopt -s nullglob
  for cand in native/*/Cargo.toml; do
    rust_crate="$(dirname "${cand}")"
    break
  done
  shopt -u nullglob
fi

if [[ -n "${rust_crate}" ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "path=rust-missing-toolchain crate=${rust_crate}" >&2
    echo "Rust crate found but cargo is not on PATH." >&2
    exit 2
  fi
  echo "path=rust crate=${rust_crate}"
  (
    cd "${rust_crate}"
    cargo test
    if grep -Eq 'crate-type|cdylib' Cargo.toml; then
      if grep -q 'cdylib' Cargo.toml; then
        echo "path=rust-cdylib-release crate=${rust_crate}"
        cargo build --release
      fi
    fi
  )
  ran_rust=1
fi

if [[ -f "tests/Catalog.UnitTests/Catalog.UnitTests.csproj" ]]; then
  echo "path=unit-tests project=tests/Catalog.UnitTests"
  dotnet test tests/Catalog.UnitTests/Catalog.UnitTests.csproj
  ran_unit=1
elif [[ -d "tests/Catalog.UnitTests" ]]; then
  shopt -s nullglob
  csprojs=(tests/Catalog.UnitTests/*.csproj)
  shopt -u nullglob
  if [[ ${#csprojs[@]} -gt 0 ]]; then
    echo "path=unit-tests project=${csprojs[0]}"
    dotnet test "${csprojs[0]}"
    ran_unit=1
  fi
fi

if [[ "${ran_unit}" -eq 1 ]]; then
  echo "path=summary rust=${ran_rust} unit=1 functional=0"
  echo "exit_code=0"
  exit 0
fi

if [[ -f "tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj" ]]; then
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    echo "path=functional-tests project=tests/Catalog.FunctionalTests docker=yes"
    dotnet test tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj
    ran_functional=1
    echo "path=summary rust=${ran_rust} unit=0 functional=1"
    echo "exit_code=0"
    exit 0
  fi
fi

echo "path=none No Catalog.UnitTests project and Docker unavailable for Catalog.FunctionalTests." >&2
if [[ "${ran_rust}" -eq 1 ]]; then
  echo "Rust checks ran, but Catalog .NET tests are still required for this lever." >&2
fi
echo "Add tests/Catalog.UnitTests (preferred) or start Docker, then re-run." >&2
echo "exit_code=2"
exit 2
