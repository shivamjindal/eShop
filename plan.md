# .NET → Rust migration scope: Catalog.API

Ticket: [SHIV-20](https://jshivam21.atlassian.net/browse/SHIV-20) (Epic: SHIV-19 — eShop backend to Rust)

## Definition of done
- [x] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [x] Blast radius for migrating the service documented (local vs cross-cutting)
- [x] Recommended sequence covers the service with sequenced verifiable units
- [x] Each scheduled domain island includes required Rust implementation + .NET→Rust wire + parity
- How to check pass/fail: reopen this file; every checkbox above must stay true; **Recommended sequence** must name units that together cover Inventory (HTTP surface, stock/domain rules, event handlers, AI search, pics/file adapter, outbox/event bus wiring); each domain island unit must require Rust + wire + parity (not extract-only); **First unit** must name `./scripts/check-catalog.sh` as the harness.

## Inventory

### Assemblies / csproj / TFM
| Artifact | Path | Notes |
|----------|------|-------|
| Service host | `src/Catalog.API/Catalog.API.csproj` | SDK-style Web; **TFM `net10.0`**; single project (no separate Domain/Infrastructure assemblies — all inlined) |
| Functional tests | `tests/Catalog.FunctionalTests/` | Aspire AppHost SDK 13.2.0; WebApplicationFactory; xunit v3; needs Docker/Postgres |
| Unit / characterization | `tests/Catalog.UnitTests/` | **Present** — stock characterization + Rust-wired parity (`CatalogItemStockTests`) |
| Shared compile links | `src/Shared/ActivityExtensions.cs`, `MigrateDbContextExtensions.cs` | Linked into Catalog.API |

### NuGet / build
- **Central Package Management:** `Directory.Packages.props` (`ManagePackageVersionsCentrally=true`); Aspire **13.2.0**, .NET packages **10.0.5**
- **Direct packages:** Asp.Versioning.Http, Aspire.Npgsql.EntityFrameworkCore.PostgreSQL, CommunityToolkit.Aspire.OllamaSharp, EF Core Tools, OpenAPI doc server, Aspire.Azure.AI.OpenAI, Pgvector / Pgvector.EntityFrameworkCore
- **Project refs:** `EventBusRabbitMQ`, `IntegrationEventLogEF`, `eShop.ServiceDefaults`
- **InternalsVisibleTo:** `Catalog.FunctionalTests`
- Build entry: `dotnet build src/Catalog.API/Catalog.API.csproj`; solution via AppHost

### Hosting (Aspire / Kestrel / workers)
- Aspire resource name: **`catalog-api`** in `src/eShop.AppHost/Program.cs`
- Refs: RabbitMQ `eventbus`, Postgres DB `catalogdb` (pgvector image `ankane/pgvector`)
- Kestrel minimal API host (`Program.cs` → `MapCatalogApi()`); OpenAPI generation; default Aspire endpoints/health
- No gRPC, no separate worker process — **integration event handlers run in-process** via RabbitMQ subscriptions
- Optional AI: Ollama embedding client or Azure OpenAI `textEmbeddingModel` connection string

### Public surface (HTTP — api-version 1.0 and 2.0)

Base group: `api/catalog`

| Method | Route (v1 / v2 notes) | Handler |
|--------|----------------------|---------|
| GET | `/items` | List/paginate; v2 adds `name`/`type`/`brand` query filters |
| GET | `/items/by?ids=` | Batch get by ids |
| GET | `/items/{id}` | Get one (+ brand include) |
| GET | `/items/by/{name}` | v1 name search (v2 via `/items?name=`) |
| GET | `/items/{id}/pic` | Static picture from `Pics/` |
| GET | `/items/withsemanticrelevance/{text}` (v1) / `?text=` (v2) | AI embedding search; falls back to name search if AI off |
| GET | `/items/type/{typeId}/brand/{brandId?}` | v1 type+brand filter |
| GET | `/items/type/all/brand/{brandId?}` | v1 brand-only filter |
| GET | `/catalogtypes`, `/catalogbrands` | Reference data |
| PUT | `/items` (v1) / `/items/{id}` (v2) | Update; may publish `ProductPriceChangedIntegrationEvent` |
| POST | `/items` | Create (+ optional embedding) |
| DELETE | `/items/{id}` | Delete |

OpenAPI snapshots: `Catalog.API.json`, `Catalog.API_v2.json`. YARP mobile-bff routes under `/catalog-api/...` and `/api/catalog/{*any}`.

### Domain vs adapters

**Domain (in `Model/` + exceptions):**
- `CatalogItem` — stock rules `RemoveStock` / `AddStock`; price/stock fields; embedding property (persistence concern attached to entity)
- `CatalogBrand`, `CatalogType`, `PaginatedItems`, `PaginationRequest`
- `CatalogDomainException`

**Adapters / infrastructure:**
- **EF / Postgres:** `CatalogContext` + entity configs + migrations (Initial, RemoveHiLo, Outbox); pgvector extension; integration event log tables via `UseIntegrationEventLogs()`
- **Seed:** `CatalogContextSeed` + `Setup/catalog.json` + `Pics/*.webp`
- **Event bus:** RabbitMQ via `EventBusRabbitMQ`; outbox through `IntegrationEventLogEF` / `CatalogIntegrationEventService`
- **AI:** `CatalogAI` / `ICatalogAI` (Ollama or Azure OpenAI embeddings)
- **Files:** picture serving from content-root `Pics/`
- **Host DI:** `Extensions.AddApplicationServices`, `CatalogServices` parameter object, `CatalogOptions`

### Events produced / consumed

| Direction | Event | Role |
|-----------|-------|------|
| Consumed | `OrderStatusChangedToAwaitingValidationIntegrationEvent` | Check stock availability → confirm/reject |
| Consumed | `OrderStatusChangedToPaidIntegrationEvent` | Call `RemoveStock` per line; `SaveChanges` |
| Produced | `OrderStockConfirmedIntegrationEvent` | All lines have stock |
| Produced | `OrderStockRejectedIntegrationEvent` + `ConfirmedOrderStockItem` | At least one line short |
| Produced | `ProductPriceChangedIntegrationEvent` | Price changed on PUT |

Contracts are **duplicated** across Catalog / Ordering / Webhooks (not a shared NuGet contract assembly). Shape must stay wire-compatible.

### Dependencies
- **Shared kernels:** `EventBus` / `EventBusRabbitMQ`, `IntegrationEventLogEF`, `eShop.ServiceDefaults`, linked `Shared/` helpers
- **External:** Postgres+pgvector (`catalogdb`), RabbitMQ, optional Ollama/OpenAI
- **Callers (HTTP):** WebApp (`CatalogService`), ClientApp (gateway → catalog), HybridApp / mobile-bff (YARP), WebApp picture forwarder
- **Callers (events):** Ordering.API publishes awaiting-validation / paid; consumes stock confirmed/rejected. Webhooks.API subscribes to `ProductPriceChangedIntegrationEvent`

### Existing Rust crates (`native/`)
- Workspace: `native/Cargo.toml` — members `eshop-core`, `catalog`, `basket`, `ordering`
- **Catalog landing zone:** `native/crates/catalog` — `stock` module live (`cdylib` + `rlib`); exports `catalog_remove_stock` / `catalog_add_stock`
- Harness: `./scripts/check-catalog.sh` (cargo workspace test+release build, then Catalog.UnitTests)
- Toolchain note: if Xcode license is unaccepted, scripts set `DEVELOPER_DIR=/Library/Developer/CommandLineTools`

## Dependencies / blast radius

### Inbound
| Edge | Kind | Notes |
|------|------|-------|
| WebApp HTTP → catalog-api | local to Catalog contract | Product list, item, brands/types, semantic search, pics |
| ClientApp / Hybrid via BFF → catalog | local | Same HTTP surface (often api-version 2.0) |
| YARP mobile-bff route table | local | Must stay aligned with Catalog routes |
| Ordering → RabbitMQ → Catalog handlers | **cross-cutting** | Stock validation + deduction path |
| Aspire AppHost wiring | local | `catalog-api` + `catalogdb` + eventbus |

### Outbound
| Edge | Kind | Notes |
|------|------|-------|
| Postgres `catalogdb` schema + outbox | local (owned by Catalog) | Migrations live in Catalog.API |
| RabbitMQ publish (stock + price events) | **cross-cutting** | Ordering + Webhooks consumers depend on payload shape |
| Embedding providers | local | Optional; search degrades to name match |
| `Pics/` filesystem | local | |

### Cross-cutting
- **Duplicated integration event DTOs** in Ordering/Webhooks — renaming/serializing fields is multi-service
- **EventBus / IntegrationEventLogEF** shared projects — keep .NET host for bus/outbox until a later platform unit; first units should not rewrite the bus
- Functional tests require Docker — unit harness must exist before relying on CI without containers

### Service-level migration notes
- Catalog is a **single fat Web project**; extract pure domain into testable .NET types (or straight to Rust FFI wrappers) rather than inventing a full Domain project unless needed for clarity
- Prefer **vertical islands**: stock rules → stock event decisions → CRUD/query helpers → AI ranking policy → keep I/O adapters in .NET until late
- Rust lands under `native/crates/catalog` modules; wire via **P/Invoke `LibraryImport` → `cdylib`** (preferred). Host remains Catalog.API until a later cutover

### Safety fact (first unit)
**Fact:** `CatalogItem.RemoveStock` and `CatalogItem.AddStock` are pure in-memory domain rules (mutate fields / throw `CatalogDomainException` only; no DB, bus, or file I/O).

- **Status:** proven
- **Evidence:** `dotnet test --project tests/Catalog.UnitTests/Catalog.UnitTests.csproj` — 8/8 characterization cases green on baseline before Rust wire; same 8/8 green after Rust `catalog` cdylib on the live path. `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` exit 0.

## Recommended sequence (covers the service)

Each unit: change → check → green before the next. Domain islands require **Rust + .NET wire + parity**.

1. **DONE — Unit: harness + characterize stock rules** — `tests/Catalog.UnitTests` covers `RemoveStock` / `AddStock` (success, partial fill, empty stock, non-positive qty, max threshold / OnReorder).  
   Check: `./scripts/check-catalog.sh` (path A: Catalog.UnitTests) → green (exit 0).

2. **DONE — Unit: extract pure stock API** — `CatalogStock` façade; `CatalogItem.RemoveStock` / `AddStock` delegate to it; paid handler still calls `CatalogItem.RemoveStock`.  
   Check: `./scripts/check-catalog.sh` → green.

3. **DONE — Unit: Rust port of stock** — `native/crates/catalog` module `stock` + `cdylib` exports `catalog_remove_stock` / `catalog_add_stock`.  
   Check: `cargo test --manifest-path native/Cargo.toml -p catalog` → 8/8 green.

4. **DONE — Unit: wire Catalog.API → Rust stock** — lazy `NativeLibrary` load of `libcatalog` from `CatalogStock`; paid handler path uses Rust via `CatalogItem.RemoveStock`.  
   Check: `dotnet build src/Catalog.API/Catalog.API.csproj` + harness → green (Rust on live path).

5. **DONE — Unit: parity harness for stock** — characterization cases against Rust-wired path.  
   Check: `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → exit 0.

6. **Unit: stock availability decision (awaiting-validation)** — characterize confirm vs reject decision (`AvailableStock >= Units` aggregation); Rust port decision pure function; wire handler; parity.  
   Check: unit tests + `./scripts/check-catalog.sh` → green means confirm/reject rule on Rust path; event payload shape unchanged (cross-cutting safe).

7. **Unit: catalog item query/filter helpers** — characterize pagination + name/type/brand filter predicates (pure pieces); Rust for filter/predicate helpers where beneficial; .NET keeps EF materialization; wire + parity for helpers used by list endpoints.  
   Check: characterization + harness → green means list filtering helpers parity; HTTP contract unchanged.

8. **Unit: item validation / id guards** — characterize id≤0 bad-request and create/update field defaults; Rust validation helpers; wire create/update/get guards; parity.  
   Check: unit tests + harness → green means validation island on Rust.

9. **Unit: picture path / MIME mapping** — characterize `GetImageMimeTypeFromImageFileExtension` + `GetFullPath`; Rust port; wire picture endpoint helpers; parity (file I/O stays in .NET).  
   Check: unit tests + harness → green means MIME/path helpers on Rust.

10. **Unit: semantic search fallback policy** — characterize “AI disabled / null vector → name search” branching (pure control policy); Rust port policy enum/decision; wire; parity. Embedding generation + pgvector SQL stay .NET adapters.  
    Check: unit tests + harness → green means search policy on Rust without requiring live LLM in unit tests.

11. **Unit: price-change detection side-effect boundary** — characterize when price change implies outbox event (boolean / payload field selection); Rust for “should publish + event field snapshot” pure part; .NET keeps outbox/transaction; wire + parity.  
    Check: unit tests + harness → green means price-change decision parity; Webhooks contract unchanged.

12. **Unit: remaining HTTP surface smoke / functional** — with Docker: `dotnet test tests/Catalog.FunctionalTests`; ensure CRUD + query + pic + semantic routes still pass against Rust-wired host.  
    Check: functional suite green → green means full public HTTP surface still works after domain islands migrated.

13. **Unit (later / optional cutover): host-shaped Rust service** — only after islands above are green; evaluate replacing Kestrel host. Out of first-wave demo scope unless SHIV-20 expands; until then .NET host + Rust domain libs is the required end state for planned islands.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Behavioral drift on stock (`RemoveStock` partial fill / exceptions) | Wrong inventory or order paid with bad stock | Characterize before port; parity harness; keep messages identical | `./scripts/check-catalog.sh`; paid-path integration tests |
| Missing unit harness (only functional/Docker) | Migration blocked or false confidence | Create `Catalog.UnitTests` in unit 1 before structural edits | Script path A vs B/C in check-catalog |
| Duplicated event contracts (Ordering/Webhooks) | Silent consumer break on field rename/JSON shape | Do not change event DTOs in early units; parity on decision outputs only | Ordering/Webhooks handlers + manual contract review |
| FFI/ABI / native lib deploy | Runtime load failures on macOS/Linux | Prefer `cdylib` + explicit pack/copy to output; document `DYLD`/`LD` paths for Aspire | Host smoke after wire unit; release `cargo build --release` in harness |
| Xcode/license or missing `cargo` in agent env | Cannot prove Rust green locally | Document toolchain; CI image with Rust; fail closed when `MIGRATION_REQUIRE_RUST=1` | Harness exit 2 on missing cargo; linker errors |
| AI/pgvector coupling | Flaky tests / env-specific search | Keep embeddings in .NET adapter; only port policy; unit-test fallback path | Functional semantic test + unit policy tests |
| Unfinished surface left as “stock only” | Demo looks done but SHIV-20 whole service incomplete | Sequence units 6–12 cover events, queries, pics, AI policy, price event | Inventory checklist vs sequence |
| Shared EventBus/outbox rewrite too early | Cross-service outage | Keep bus/outbox in .NET through planned units | No EventBus API changes in Catalog stock PRs |

## First unit (first vertical) — COMPLETE

- **Scope:** characterize `RemoveStock` / `AddStock` → `CatalogStock` façade → Rust `native/crates/catalog` (`stock`) → wire live path → parity. **Not** “whole Catalog done.”
- **Harness:** `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → exit 0 (recorded).
- **Rust crate path:** `native/crates/catalog` (module `stock`); build helper `./scripts/build-catalog-native.sh`.
- **Boundary:** lazy `NativeLibrary.Load` + exports `catalog_remove_stock` / `catalog_add_stock` (cdylib). Paid handler → `CatalogItem.RemoveStock` → Rust.
- **Acceptance:** all met (safety fact proven; Rust on path; 8/8 parity; harness exit 0).
- **Next unit:** 6 — stock availability decision (awaiting-validation).
- **Suggested tickets (remaining):**
  1. `Catalog: awaiting-validation stock decision → Rust + parity`
  2. `Catalog: query/filter + validation + pic MIME helpers → Rust + parity`
  3. `Catalog: semantic fallback + price-change decision → Rust; functional suite green`
