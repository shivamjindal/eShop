# .NET → Rust migration scope: Catalog.API

Ticket: [SHIV-19](https://jshivam21.atlassian.net/browse/SHIV-19) — Catalog.API: migrate inventory/stock domain toward Rust.

## Definition of done

- [x] `plan.md` names inbound/outbound edges for Catalog.API with local vs cross-cutting labels
- [x] Recommended sequence is ≥2 verifiable units, each ending in a green check
- [x] First slice lists a harness command that can fail before mass edits
- [x] Safety fact stated with proven/unproven status and evidence
- [x] Rust island of stock rules wired into the live CatalogItem path (wasm)

- How to check pass/fail: reopen this file; checkboxes must hold; `./scripts/check-catalog.sh` must exit 0 (paths R + A).

## Inventory

- Assemblies / csproj / TFM: `src/Catalog.API` → `net10.0` (single web project)
- NuGet / build: Central Package Management; Wasmtime hosts Rust wasm
- Hosting: Aspire `catalog-api` + Postgres `catalogdb` + RabbitMQ
- Public surface: `api/catalog/*`; stock mutations via paid-order integration event → `RemoveStock`
- Domain vs adapters: stock rules in Rust `crates/catalog_stock` (wasm); `CatalogStock` / `CatalogItem` are thin .NET adapters

## Dependencies / blast radius

- Inbound: WebApp / BFF / clients (HTTP); Ordering events (paid / awaiting validation) — *cross-cutting* for events, *local* for HTTP DTOs
- Outbound: `catalogdb`, RabbitMQ stock confirm/reject — *local* schema ownership
- **Safety fact:** Stock Remove/Add rules are pure and now execute in Rust wasm with characterization coverage on both sides.
  - Status: **proven**
  - Evidence: `./scripts/check-catalog.sh` → `R:catalog_stock(docker) exit_code=0` (7) and `A:Catalog.UnitTests exit_code=0` (7)

## Recommended sequence

1. Characterize + extract on .NET → green unit harness
2. Port rules to Rust wasm; wire live path through Wasmtime → green R+A
3. Validate (Verify Catalog + keep/merge gate) → `validate.md`
4. Out of epic scope: full Catalog.API rewrite in Rust

## First demo-able slice (completed as full SHIV-19 stock vertical)

- Scope: `RemoveStock` / `AddStock` → Rust `catalog_stock` wasm via `CatalogStock` / Wasmtime; characterization on both sides
- **Harness:** `./scripts/check-catalog.sh`
