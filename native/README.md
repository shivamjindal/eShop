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

One crate per service under `native/crates/<service>/`. Migration units land as
modules inside that crate. Catalog stock (`catalog::stock` + `ffi` cdylib) is the
first wired island — Catalog.API loads `libcatalog` via `LibraryImport`. Add crates
for other services only when work on them starts.

## Checks

```bash
./scripts/check-native.sh
# cargo test --manifest-path native/Cargo.toml --workspace

./scripts/check-catalog.sh   # workspace Rust + Catalog .NET tests
```
