# Catalog stock Rust parity checklist (SHIV-24)

## Claim

Catalog.API stock mutations for the chosen slice run through `crates/catalog_stock` (wasm), and behavior matches the characterization suite that locked the old .NET rules.

## Commands

```bash
cargo build --manifest-path crates/catalog_stock/Cargo.toml --target wasm32-unknown-unknown
./scripts/check-catalog.sh
```

Optional when Docker works:

```bash
docker run --rm -v "$PWD/crates/catalog_stock:/crate" -w /crate rust:1.88-bookworm cargo test
```

## Evidence to attach

- `check-catalog` exit code 0
- `catalog_stock.wasm` present under the unit-test / Catalog.API output directory
- Characterization cases green: empty stock, invalid qty, partial fill, full fill, max clamp, OnReorder clear

## Rollback

1. Restore `CatalogItem.RemoveStock` / `AddStock` bodies from git history.
2. Drop Wasmtime package + `CatalogStock*.cs` + Native.targets import if unused.
3. Re-run `./scripts/check-catalog.sh`.

## Go / no-go

- **Keep** if characterization + wasm build are green and the paid-order handler still calls `RemoveStock` (no bypass).
- **Revert** if any characterized edge differs, or if wasm is missing at runtime and callers hit `FileNotFoundException` in environments that must stay up.
