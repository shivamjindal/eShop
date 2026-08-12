# catalog_stock (Rust island)

Pure Catalog stock rules (`RemoveStock` / `AddStock`) for the eShop **.NET → Rust** demo.

| Path | Role |
|------|------|
| `native/catalog_stock/` | Stock mutation semantics + `extern "C"` FFI |
| `crate-type` | `cdylib` + `rlib` — P/Invoke from .NET, `cargo test` for parity |
| .NET wire | `CatalogStockNative` → `CatalogItem.RemoveStock` / `AddStock` |

## Checks

```bash
# Prefer CLT on macOS if Xcode license blocks linking:
export DEVELOPER_DIR=/Library/Developer/CommandLineTools

cargo test --manifest-path native/catalog_stock/Cargo.toml
cargo build --release --manifest-path native/catalog_stock/Cargo.toml
./scripts/check-catalog.sh
MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh
```

`Catalog.API` / `Catalog.UnitTests` import `CatalogStock.Native.targets` to build the release cdylib and copy it beside the managed output.
