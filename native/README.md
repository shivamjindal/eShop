# native/ — Rust backend workspace

Structural Cargo workspace for eShop backend → Rust migration demos.

## Tree

```text
native/
  Cargo.toml
  crates/
    eshop-core/    # shared prelude (empty)
    catalog/       # ← src/Catalog.API (landing zone)
    basket/        # the Basket service — migrated, .NET project deleted
    ordering/      # ← src/Ordering.API / Ordering.Domain (landing zone)
```

## Convention

One crate per service under `native/crates/<service>/`. Migration units land as
modules inside that crate. Add crates for other services only when work on them starts.

## basket

Owns the `basket-api` resource in the Aspire AppHost: a tonic gRPC server on h2c
(`BasketApi.Basket`), the `/basket/{userId}` Redis document, and the
`OrderStartedIntegrationEvent` subscription. Aspire starts it with
`builder.AddRustService("basket-api", "basket-service")`, which runs
`cargo run --release --bin basket-service` and passes the allocated port in `PORT`.
`proto/basket.proto` is the contract WebApp's gRPC client compiles against.

Binaries: `basket-service` (the service) and `basket-parity` (record/replay harness).

## Checks

```bash
./scripts/check-native.sh    # cargo test --workspace
./scripts/check-basket.sh    # Rust tests + release build + transcript presence
./scripts/parity-basket.sh   # replay the recorded .NET transcript against Rust (needs Docker)
./scripts/check-catalog.sh   # workspace Rust + Catalog .NET tests
```
