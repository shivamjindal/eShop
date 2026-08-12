# Catalog.API stock boundary (SHIV-20)

## Slice

Pure `RemoveStock` / `AddStock` rules formerly embedded in `CatalogItem`.
HTTP/OpenAPI, EF persistence, embeddings, and integration-event plumbing stay in .NET.

## Blast radius

| Surface | Touches stock rules? |
| --- | --- |
| `CatalogItem.RemoveStock` / `AddStock` | Yes. Now delegates to Rust via `CatalogStock` + wasm. |
| `OrderStatusChangedToPaidIntegrationEventHandler` | Yes. Calls `RemoveStock` on paid order lines. |
| `OrderStatusChangedToAwaitingValidationIntegrationEventHandler` | Reads `AvailableStock` only. No mutation rules. |
| Catalog HTTP CRUD (`CatalogApi`) | Sets `AvailableStock` / thresholds as fields. Does not call Remove/Add. |
| Other services | Consume catalog over HTTP/events. No direct rule ownership. |

## Domain shape

Inputs for remove: `available_stock`, `quantity_desired` → removed units + new available, or empty/invalid errors.
Inputs for add: `available_stock`, `max_stock_threshold`, `quantity` → added units + new available + `on_reorder=false`.
Packed `i64` ABI for the wasm host. Exception messages stay in .NET so callers see the same text.

## Risks

- Partial fills when desired exceeds available.
- Empty stock and non-positive quantity throw.
- Add clamps to `MaxStockThreshold` and always clears `OnReorder`.
- Missing `catalog_stock.wasm` next to the assembly fails hard (no silent .NET fallback).
- Host native `cargo test` needs a working C toolchain; CI should use wasm build + .NET characterization, or Docker for Rust tests.

## Sequence

1. Characterize .NET semantics (SHIV-21).
2. Extract portable boundary (SHIV-22).
3. Implement Rust + wire live path (SHIV-23).
4. Parity keep/revert (SHIV-24).

## Open questions

- Prefer Wasmtime-in-process long term, or a native cdylib once macOS/Linux CI linkers are uniform?
- Should RestockThreshold / reorder request generation move into Rust later, or stay unused as today?
