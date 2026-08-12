# native/ — Rust backend workspace

Structural Cargo workspace for eShop backend → Rust migration demos.

## Tree

```text
native/
  Cargo.toml
  crates/
    eshop-core/    # shared prelude (empty)
    catalog/       # ← src/Catalog.API
    basket/        # ← src/Basket.API
    ordering/      # ← src/Ordering.API / Ordering.Domain
```

## Convention

One crate per service under `native/crates/<service>/`. Future migration units
land as modules inside that crate (e.g. `catalog::stock`). Add crates for other
services only when work on them starts.

## Catalog stock (live path)

`crates/catalog` builds as `cdylib` + `rlib`. `Catalog.API` loads `libcatalog`
via `LibraryImport` (`CatalogStock` → `catalog_remove_stock` / `catalog_add_stock`).
Stage/build helper: `./scripts/build-catalog-native.sh`.

## Checks

```bash
./scripts/check-native.sh
# cargo test --manifest-path native/Cargo.toml --workspace

./scripts/check-catalog.sh   # workspace Rust + Catalog .NET tests
MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh
```
