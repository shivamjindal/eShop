# .NET → Rust migration scope: Catalog.API

Ticket: [SHIV-20](https://jshivam21.atlassian.net/browse/SHIV-20) — Catalog.API → Rust (primary demo service).

## Definition of done
- [x] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [x] Blast radius for migrating the service documented (local vs cross-cutting)
- [x] Recommended sequence covers the service with sequenced verifiable units
- [x] Each scheduled domain island includes required Rust implementation + .NET→Rust wire + parity
- How to check pass/fail:
  - Reopen this file: every inventoried Catalog.API area appears under **Recommended sequence** with a check command and a Rust + wire + parity path (or an explicit host-adapter deferral that still ends in a green check).
  - First unit is **not** extract-only; acceptance requires Rust on the live path and `./scripts/check-catalog.sh` exit 0.
  - No unit claims “Catalog.API done” after a single island; remaining units stay listed until the inventory is exhausted.

## Inventory

### Assemblies / csproj / TFM
- **Service project:** `src/Catalog.API/Catalog.API.csproj` — SDK-style `Microsoft.NET.Sdk.Web`, **TFM `net10.0`**, nullable enabled.
- **No separate Domain / Infrastructure / Application assemblies** — all live in one project (`Model/`, `Infrastructure/`, `Apis/`, `IntegrationEvents/`, `Services/`).
- **Linked shared sources:** `src/Shared/ActivityExtensions.cs`, `src/Shared/MigrateDbContextExtensions.cs`.
- **Tests:**
  - `tests/Catalog.FunctionalTests/` — HTTP API coverage via `WebApplicationFactory` (Docker/Testcontainers-style fixture).
  - `tests/Catalog.UnitTests/` — characterization suite for stock (`CatalogItemStockTests`); on solution `eShop.slnx`.
- **InternalsVisibleTo:** `Catalog.FunctionalTests`, `Catalog.UnitTests`.

### NuGet / build
- **Central Package Management:** `Directory.Packages.props` (`ManagePackageVersionsCentrally=true`).
- Notable packages on Catalog.API: `Asp.Versioning.Http`, `Aspire.Npgsql.EntityFrameworkCore.PostgreSQL`, `Aspire.Azure.AI.OpenAI`, `CommunityToolkit.Aspire.OllamaSharp`, `Pgvector` / `Pgvector.EntityFrameworkCore`, OpenAPI document generation.
- **Project refs:** `EventBusRabbitMQ`, `IntegrationEventLogEF`, `eShop.ServiceDefaults`.
- Build/run via solution + Aspire AppHost (`src/eShop.AppHost`).

### Hosting (Aspire / Kestrel / workers)
- Aspire resource name: **`catalog-api`** (`eShop.AppHost/Program.cs`).
- Refs: RabbitMQ `eventbus`, Postgres DB **`catalogdb`** (pgvector image `ankane/pgvector`).
- Optional AI: OpenAI / Ollama embedding clients (flags in AppHost; both default off).
- Kestrel minimal APIs; no gRPC; no dedicated worker process — **integration event handlers run in-process** via RabbitMQ subscriptions.
- YARP mobile-bff routes under `/catalog-api/...` forward to this service.
- Content: `Pics/*.webp` copied to output; picture endpoint serves files from disk.

### Public surface (routes, events, CLI)
**HTTP** (`Apis/CatalogApi.cs`, versioned API 1.0 and 2.0 under `api/catalog`):

| Area | Routes (summary) |
|------|------------------|
| List / filter items | `GET /items` (v1, v2 with name/type/brand), `GET /items/by`, `GET /items/{id}`, `GET /items/by/{name}` (v1), type/brand filters (v1) |
| Search (AI) | `GET /items/withsemanticrelevance/{text}` (v1), `GET /items/withsemanticrelevance?text=` (v2) |
| Taxonomy | `GET /catalogtypes`, `GET /catalogbrands` |
| Pictures | `GET /items/{id}/pic` |
| Mutations | `PUT /items` (v1), `PUT /items/{id}` (v2), `POST /items`, `DELETE /items/{id}` |

No CLI/batch entrypoints on this service.

**Integration events — consumed (RabbitMQ):**
- `OrderStatusChangedToAwaitingValidationIntegrationEvent` → stock check → publish confirm/reject
- `OrderStatusChangedToPaidIntegrationEvent` → `CatalogItem.RemoveStock` + `SaveChanges`

**Integration events — produced:**
- `OrderStockConfirmedIntegrationEvent`
- `OrderStockRejectedIntegrationEvent` (+ `ConfirmedOrderStockItem`)
- `ProductPriceChangedIntegrationEvent` (on price-changing updates, via outbox)

### Domain vs adapters
| Kind | Location | Notes |
|------|----------|-------|
| **Domain (pure / near-pure)** | `Model/CatalogItem.RemoveStock`, `AddStock` | In-memory stock math + `CatalogDomainException`; no I/O inside methods |
| **Domain (decision)** | AwaitingValidation handler stock sufficiency (`AvailableStock >= Units`) | Pure comparison once item loaded |
| **Domain-ish helpers** | `GetImageMimeTypeFromImageFileExtension`, `GetFullPath` | Pure string/path |
| **CRUD / query orchestration** | `CatalogApi` handlers | EF queries, pagination, embeddings refresh |
| **Adapters — DB** | `CatalogContext`, EF configs, migrations, seed | Postgres + `vector(384)` + integration event log outbox |
| **Adapters — messaging** | `CatalogIntegrationEventService`, EventBusRabbitMQ | Outbox + publish |
| **Adapters — AI** | `CatalogAI` / `ICatalogAI` | Optional embedding generator; cosine distance via Pgvector |
| **Host shell** | `Program.cs`, service defaults, OpenAPI, API versioning | Stays .NET until late / optional host cutover |

### Events produced/consumed
- **In:** awaiting validation, paid (from Ordering).
- **Out:** stock confirmed/rejected (to Ordering), product price changed (to Webhooks).
- Contracts are **duplicated record types** across services (Catalog / Ordering / Webhooks / WebApp) — not a single shared NuGet contract assembly.

### Dependencies
- **Inbound callers:** `WebApp` (`CatalogService` HTTP + product-image forwarder), `ClientApp` / `HybridApp` catalog clients, YARP `mobile-bff`, Aspire wiring.
- **Outbound:** `catalogdb` (owned schema `Catalog` / brands / types + outbox), RabbitMQ, optional OpenAI/Ollama.
- **Shared kernels:** `EventBus` / `EventBusRabbitMQ`, `IntegrationEventLogEF`, `eShop.ServiceDefaults`, linked `Shared/` helpers.

### Existing Rust crates (`native/`)
- Workspace: `native/Cargo.toml` — members `eshop-core`, **`catalog`**, `basket`, `ordering`.
- **`native/crates/catalog`:** `stock` + `ffi` modules; `crate-type = ["cdylib", "rlib"]`. Live path for RemoveStock/AddStock.
- `./scripts/check-catalog.sh`, `./scripts/check-native.sh`, `./scripts/build-catalog-native.sh` (macOS CLT linker workaround when Xcode license blocks `/usr/bin/cc`).
- Catalog.API MSBuild target `BuildCatalogNative` copies `libcatalog.dylib` / `.so` / `.dll` to host output.

## Dependencies / blast radius

### Inbound
| Edge | Label | Notes |
|------|-------|-------|
| WebApp → `catalog-api` HTTP | cross-cutting | Catalog browse/detail + `/product-images` forward |
| ClientApp / HybridApp → catalog HTTP | cross-cutting | Mobile/desktop clients |
| YARP mobile-bff routes | cross-cutting | Path-prefixed proxy |
| Ordering → RabbitMQ events Catalog consumes | cross-cutting | Stock validation + decrement |
| Aspire AppHost resource graph | local-to-compose | `catalogdb`, `eventbus` |

### Outbound
| Edge | Label | Notes |
|------|-------|-------|
| Postgres `catalogdb` | local (schema owner) | Migrations live in Catalog.API |
| RabbitMQ publish confirm/reject/price | cross-cutting | Ordering + Webhooks consumers |
| OpenAI / Ollama embeddings | cross-cutting (optional) | Feature-flagged |
| EventBus / IntegrationEventLogEF | cross-cutting shared libs | Stay .NET wrappers initially |

### Cross-cutting
- Duplicated integration-event record shapes across services — changing payloads forces multi-service updates.
- Webhooks listens for `ProductPriceChangedIntegrationEvent`.
- Functional tests are the only automated Catalog coverage today and need Docker.

### Service-level migration notes
- Prefer **vertical domain islands in Rust** (`native/crates/catalog`) called from the existing Kestrel host via **P/Invoke `LibraryImport` → `cdylib`**.
- Keep EF, RabbitMQ, OpenAPI, and Aspire hosting on .NET until domain islands are green; do not big-bang rewrite the host first.
- Characterization harness must land in **`tests/Catalog.UnitTests`** (project currently missing) so `./scripts/check-catalog.sh` path A works without Docker.

### Safety fact (first unit)
- **Fact:** `CatalogItem.RemoveStock` / `AddStock` are I/O-free pure stock rules (only mutate in-memory fields / throw `CatalogDomainException`), safe to characterize and port before touching EF or messaging.
- **Status:** **proven**
- **Evidence:** `dotnet test --project tests/Catalog.UnitTests/Catalog.UnitTests.csproj` — 8/8 green on baseline .NET then again on Rust-wired path; `cargo test -p catalog` — 8/8 mirrored cases; `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` exit 0.

## Recommended sequence (covers the service)

Each unit: change → check → green. Domain islands require **Rust + .NET wire + parity** (not extract-only).

1. ~~**Harness + characterize stock rules**~~ **DONE** — `tests/Catalog.UnitTests` covers `RemoveStock` / `AddStock`.  
   - Check: `dotnet test --project tests/Catalog.UnitTests/Catalog.UnitTests.csproj` and/or `./scripts/check-catalog.sh`  
   - Green means: stock behavior locked; safety fact becomes **proven**.

2. ~~**Extract pure stock API in .NET**~~ **DONE** — `CatalogStock` + `CatalogItem` delegates; paid handler still calls `CatalogItem.RemoveStock` (live path → Rust).  
   - Check: `dotnet test --project tests/Catalog.UnitTests/Catalog.UnitTests.csproj`

3. ~~**Rust port — stock island**~~ **DONE** — `native/crates/catalog` (`stock`, `ffi`), cdylib exports `catalog_stock_remove` / `catalog_stock_add`.  
   - Check: `cargo test --manifest-path native/Cargo.toml -p catalog`  
   - Green means: Rust stock rules match characterization vectors.

4. ~~**Wire Catalog.API → Rust stock**~~ **DONE** — `LibraryImport` in `CatalogStock`; MSBuild builds/copies cdylib.  
   - Check: `dotnet build src/Catalog.API/Catalog.API.csproj` + unit tests  
   - Green means: Rust is not dead code.

5. ~~**Parity harness (stock)**~~ **DONE** — characterization against Rust-wired path; `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` exit 0.  
   - Green means: keep stock island.

6. **Stock confirmation decision** *(next)* — port “has stock / reject vs confirm” pure decision used by awaiting-validation handler; characterize → Rust → wire handler → parity.  
   - Check: unit tests + `./scripts/check-catalog.sh`  
   - Host still loads items via EF and publishes events in .NET.

7. **Picture helpers** — port `GetImageMimeTypeFromImageFileExtension` + path join rules; wire picture endpoint helper; parity.  
   - Check: unit tests (no Docker required for pure helpers).

8. **Catalog mutation validation / invariants** — create/update field rules (required name, id presence on v1 update, non-positive id rejection, price-change detection boolean). Characterize → Rust → wire API handlers’ decision points → parity.  
   - EF save + outbox publish remain .NET adapters.

9. **Price-changed event payload shaping** — pure construction of old/new price event inputs; Rust island + wire before outbox write; parity.  
   - Cross-cutting: do **not** change RabbitMQ contract shapes in this unit.

10. **Query filter / pagination pure helpers** — page index/size normalization and filter predicate inputs as pure functions where extractable; Rust → wire list endpoints → parity.  
    - SQL/EF execution stays .NET.

11. **Semantic search fallback policy** — pure policy: AI disabled / null embedding → name search fallback; port decision, keep embedding + Pgvector distance in .NET adapters; parity on policy tests.

12. **Taxonomy + batch-get orchestration thin rules** — any remaining pure validation; otherwise keep as .NET EF adapters with characterization at HTTP level (`Catalog.FunctionalTests` when Docker available).

13. **Adapter freeze / host retention check** — document remaining .NET-only surface (EF migrations, outbox, RabbitMQ subscriptions, OpenAPI, Aspire). Run full harness: `./scripts/check-catalog.sh` + (when Docker up) `dotnet test tests/Catalog.FunctionalTests`.  
    - Green means: Catalog.API domain islands planned above are on Rust; host/adapters explicitly deferred or accepted as .NET shell for the demo end-state.

14. **(Optional later) Host cutover spike** — only after units 1–13 green; not required to close SHIV-20 scoping. Would replace Kestrel host with a Rust HTTP service — separate ticket, large blast radius.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Missing unit/characterization suite (`Catalog.UnitTests` hollow) | Drift during extract/port; harness falls back to Docker functional tests | Create unit project first; fail closed in `check-catalog.sh` | `./scripts/check-catalog.sh` path A missing / exit ≠ 0 |
| Behavioral drift in stock math (partial fill, exceptions, max threshold) | Wrong inventory after paid events; order/catalog inconsistency | Characterize before edit; Rust parity vectors mirrored | Unit + parity tests; ordering integration symptoms |
| Duplicated event contracts across services | Silent deserialize breaks if shapes diverge while “just migrating Catalog” | Freeze payloads; port logic only; shared-contract change = explicit cross-service unit | Consumer tests in Ordering/Webhooks; manual event fixture |
| FFI/ABI + native lib deploy (cdylib next to .NET host) | Runtime load failures in Aspire/containers | Prefer `LibraryImport`; document copy/rpath in Dockerfile/AppHost; CI builds release dylib | Host smoke + `check-catalog` release cargo build |
| Xcode / linker license blocker on macOS agents | Cannot run `cargo test` locally | Agree Xcode license or use Linux CI for Rust checks | `cargo test` link error exit 69 |
| AI/Pgvector path non-determinism | Flaky semantic relevance tests | Keep embeddings in .NET adapter; test fallback policy in Rust; functional tests with AI off | Functional semantic test with AI disabled |
| Unfinished service surface if work stops after stock | Demo looks done but CRUD/events still fully .NET with no plan execution | This plan lists units through host retention; tickets for units 6+ | Checklist in Definition of done |
| Shared EventBus/outbox coupling | Hard to move messaging early | Keep adapters .NET; only pure decisions in Rust | Compiles without changing EventBus packages |

## First unit (first vertical) — COMPLETE

- **Status:** done — ready for **Migration validate**
- **Scope completed:** Characterize `RemoveStock` / `AddStock` → `CatalogStock` extract → Rust `native/crates/catalog` (`stock`/`ffi`) → `LibraryImport` wire → parity.
- **Harness evidence:** `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → exit 0 (`R:cargo-test-workspace`, `R:cargo-build-release-workspace`, `A:Catalog.UnitTests` 8/8).
- **Rust crate path:** `native/crates/catalog` (`mod stock`, `mod ffi`; cdylib name `catalog` / `libcatalog.*`)
- **Boundary:** `LibraryImport` → cdylib (live path from `CatalogItem.RemoveStock` / `AddStock`)
- **Not whole service:** remaining coverage starts at unit 6 (stock confirmation decision).
- **Suggested next tickets:**
  1. `SHIV-20d` — Stock confirmation decision island (awaiting-validation)
  2. `SHIV-20e` — Picture helpers + mutation/price pure rules islands
  3. `SHIV-20f` — Query/AI-fallback islands + functional regression when Docker available
