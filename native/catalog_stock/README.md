# catalog_stock (Rust island)

Skeleton crate for the eShop **.NET → Rust** Catalog stock-slice demo.

| Path | Role |
|------|------|
| `native/catalog_stock/` | Pure stock rules (`RemoveStock` / `AddStock` semantics) |
| `crate-type` | `cdylib` + `rlib` — P/Invoke from .NET, `cargo test` for parity |

## What agents should do

Follow **Migrate slice to Rust** (`.cursor/skills/migrate-slice-to-rust/`):

1. Characterize current .NET behavior and keep those tests green.
2. Extract pure rules in .NET if still embedded.
3. **Port the same rules into this crate** (replace the skeleton).
4. **Wire Catalog.API / CatalogStock** to call this library (preferred: `LibraryImport` to the release `cdylib`).
5. Prove parity; run `./scripts/check-catalog.sh` (builds/tests this crate when present).

Strict mode (skills / CI that require the island):

```bash
MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh
```

## Local checks

```bash
cargo test --manifest-path native/catalog_stock/Cargo.toml
cargo build --release --manifest-path native/catalog_stock/Cargo.toml
```

Do not leave this crate as unused dead code once the slice is claimed done.
