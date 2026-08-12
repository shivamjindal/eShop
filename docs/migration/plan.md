# .NET → Rust migration scope: Catalog.API

Ticket: [SHIV-20](https://jshivam21.atlassian.net/browse/SHIV-20) — Catalog.API → Rust (primary demo service).

## Definition of done
- [x] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [x] Blast radius for migrating the service documented (local vs cross-cutting)
- [x] Recommended sequence covers the service with sequenced verifiable units
- [x] Each scheduled domain island includes required Rust implementation + .NET→Rust wire + parity
- How to check pass/fail:
  - This artifact inventories **all** Catalog.API surface below (HTTP, events, domain, adapters).
  - Recommended sequence has units that **together** cover that surface; every domain island requires Rust + wire + parity (not extract-only).
  - First unit names harness `./scripts/check-catalog.sh` (fails closed on drift once unit/characterization tests exist).
  - Reopen this file and tick each checkbox; if any inventory area or later unit is missing, fail the scoping run.

## Inventory

### Assemblies / csproj / TFM
| Piece | Path | Notes |
|-------|------|--------|
| Service host | `src/Catalog.API/Catalog.API.csproj` | SDK-style Web, **net10.0**, nullable |
| Functional tests | `tests/Catalog.FunctionalTests/` | Aspire AppHost SDK host; refs Catalog.API; Docker/Postgres |
| Unit / characterization | `tests/Catalog.UnitTests/` | **Present** — stock characterization (8 tests); harness path A |
| Linked shared compile | `src/Shared/ActivityExtensions.cs`, `MigrateDbContextExtensions.cs` | Linked into Catalog.API |
| Aspire AppHost | `src/eShop.AppHost/` | Registers `catalog-api` + `catalogdb` |

~39 `.cs` files under `src/Catalog.API` (API, Model, Infrastructure, IntegrationEvents, Services).

### NuGet / build
- **Central Package Management:** `Directory.Packages.props` (`ManagePackageVersionsCentrally`, DotnetPackagesVersion `10.0.5`).
- Key packages (Catalog.API): `Asp.Versioning.Http`, `Aspire.Npgsql.EntityFrameworkCore.PostgreSQL`, `CommunityToolkit.Aspire.OllamaSharp`, `Aspire.Azure.AI.OpenAI`, `Pgvector` / `Pgvector.EntityFrameworkCore`, EF Tools, OpenAPI document generation.
- Project refs: `EventBusRabbitMQ`, `IntegrationEventLogEF`, `eShop.ServiceDefaults`.
- InternalsVisibleTo: `Catalog.FunctionalTests`.
- OpenAPI snapshots: `Catalog.API.json`, `Catalog.API_v2.json`.

### Hosting (Aspire / Kestrel / workers)
- Aspire project resource name: **`catalog-api`**.
- Outbound refs: RabbitMQ `eventbus`, Postgres DB `catalogdb` (pgvector image `ankane/pgvector`).
- Kestrel minimal APIs via `MapCatalogApi()`; service defaults + OpenAPI + status pages.
- No separate Catalog worker process — integration handlers run in-process on the RabbitMQ event bus.
- Optional AI: OpenAI or Ollama embedding generator (AppHost flags `useOpenAI` / `useOllama`, default off).

### Public surface (routes, events, CLI)

**HTTP** (`api/catalog`, API versions 1.0 and 2.0) — from `Apis/CatalogApi.cs`:

| Area | Routes |
|------|--------|
| List / filter items | `GET /items` (v1 plain page; v2 + `name`/`type`/`brand`), `GET /items/by`, `GET /items/{id}`, `GET /items/by/{name}` (v1), `GET /items/type/{typeId}/brand/{brandId?}` (v1), `GET /items/type/all/brand/{brandId?}` (v1) |
| Search (AI) | `GET /items/withsemanticrelevance/{text}` (v1), `GET /items/withsemanticrelevance?text=` (v2) |
| Taxonomy | `GET /catalogtypes`, `GET /catalogbrands` |
| Media | `GET /items/{id}/pic` |
| Mutations | `PUT /items` (v1), `PUT /items/{id}` (v2), `POST /items`, `DELETE /items/{id}` |

No Catalog CLI/batch entrypoint.

**YARP / BFF:** `mobile-bff` proxies `/catalog-api/api/catalog/...` and `/api/catalog/{*any}` to Catalog.

### Domain vs adapters

**Domain (in-process rules / entities)**
- `CatalogItem` — stock: `RemoveStock`, `AddStock`; fields: price, thresholds, reorder flag, brand/type ids.
- Stock availability check in `OrderStatusChangedToAwaitingValidationIntegrationEventHandler` (`AvailableStock >= Units` → confirm/reject).
- Pure helpers: image mime map + pic path (`GetImageMimeTypeFromImageFileExtension`, `GetFullPath`).
- `CatalogDomainException` for stock rule violations.
- Pagination DTO shaping (`PaginatedItems`, `PaginationRequest`) — thin.

**Adapters**
- **EF Core / Postgres / pgvector:** `CatalogContext`, entity configs, migrations, seed (`CatalogContextSeed`).
- **Outbox / integration log:** `IntegrationEventLogEF` via `UseIntegrationEventLogs`, `CatalogIntegrationEventService`.
- **RabbitMQ event bus:** subscriptions + publish.
- **AI embeddings:** `CatalogAI` / `ICatalogAI` (OpenAI or Ollama).
- **Static pics:** `Pics/*.webp` content files.
- **Shared host:** `eShop.ServiceDefaults`, Aspire wiring.

### Events produced / consumed

| Direction | Event | Peer |
|-----------|--------|------|
| Consume | `OrderStatusChangedToAwaitingValidationIntegrationEvent` | Ordering.API |
| Consume | `OrderStatusChangedToPaidIntegrationEvent` | Ordering.API (calls `RemoveStock`) |
| Produce | `OrderStockConfirmedIntegrationEvent` | Ordering.API |
| Produce | `OrderStockRejectedIntegrationEvent` | Ordering.API |
| Produce | `ProductPriceChangedIntegrationEvent` | Webhooks.API (duplicate contract type locally) |

Event record types are **duplicated per service** (not a shared NuGet contracts package) — shape drift is a real cross-cutting risk.

### Dependencies
- **Inbound HTTP:** WebApp (`CatalogService` → `https+http://catalog-api`), HybridApp, ClientApp (HTTP catalog clients), mobile-bff YARP, WebApp product-image forwarder.
- **Inbound events:** Ordering status → catalog validation / paid stock decrement.
- **Outbound:** `catalogdb`, RabbitMQ, optional embedding model.
- **Shared kernels:** EventBusRabbitMQ, IntegrationEventLogEF, ServiceDefaults, Shared linked files.

### Existing Rust crates (`native/`)
| Path | State |
|------|--------|
| `native/catalog_stock/` | **Implemented** — pure `remove_stock` / `add_stock` + `extern "C"` FFI; 8 parity tests |
| Wiring from .NET | **Live** — `CatalogStockNative` (`LibraryImport`) called from `CatalogItem.RemoveStock` / `AddStock` |
| `tests/Catalog.UnitTests` | **Present** — characterization against Rust-wired path |
| Harness | `scripts/check-catalog.sh` — cargo test/build + `dotnet test --project` Catalog.UnitTests; auto-sets `DEVELOPER_DIR` to CLT when Xcode license blocks linking |

## Dependencies / blast radius

### Inbound
| Edge | Local vs cross-cutting |
|------|------------------------|
| WebApp / HybridApp / ClientApp HTTP catalog clients | **Cross-cutting** (contract/URL stable; behavior must stay) |
| mobile-bff YARP routes | **Cross-cutting** |
| Ordering → AwaitingValidation / Paid events | **Cross-cutting** (stock confirm/reject / decrement) |
| Catalog.FunctionalTests | **Local** (characterization of HTTP) |

### Outbound
| Edge | Local vs cross-cutting |
|------|------------------------|
| Postgres `catalogdb` schema + migrations | **Local** to Catalog (owned schema) |
| RabbitMQ publish confirm/reject/price-changed | **Cross-cutting** |
| Embedding provider | **Local** adapter (optional) |
| Pic filesystem | **Local** |

### Cross-cutting
- Duplicated integration-event record types across Ordering / Catalog / Webhooks / WebApp — changing payloads forces multi-service edits.
- Price-change → Webhooks; stock confirm/reject → Ordering order state machine.
- Aspire composition couples Catalog to postgres + rabbit lifetime.

### Service-level migration notes
- Keep **.NET as the host** (Kestrel + Aspire + EF + RabbitMQ) while migrating **domain islands** to Rust via `cdylib` + `LibraryImport`.
- Do **not** big-bang rewrite HTTP/EF/AI in the first units.
- Later units can move more pure logic (validation, mime/path, availability) into crates; adapters stay .NET until an optional host-cutover unit.
- Canonical Rust path for stock: `native/catalog_stock` (do not invent a second crate root).

### Safety fact (first unit)
**Fact:** `CatalogItem.RemoveStock` / `AddStock` are pure in-memory mutations (no I/O) and are safe to characterize and port before touching EF/RabbitMQ.

- **Status:** proven
- **Evidence:** Inspected `src/Catalog.API/Model/CatalogItem.cs` — methods only read/write `AvailableStock` / `MaxStockThreshold` / `OnReorder` / `Name` and throw `CatalogDomainException`; `rg` for `async|Await|DbContext|Http|File\.|SaveChanges|ILogger` in that file returned no matches. Sole production caller of `RemoveStock` is `OrderStatusChangedToPaidIntegrationEventHandler` (persistence is outside the methods). `AddStock` has no callers outside `CatalogItem` today (still part of the public domain API to port).

**First unit status:** complete — characterization + Rust port + wire + parity; `./scripts/check-catalog.sh` exit 0 (including `MIGRATION_REQUIRE_RUST=1`).

## Recommended sequence (covers the service)

Each unit: *change → check → green* before the next. Domain islands always include **Rust + wire + parity**.

1. **Unit: Stock rules island (first vertical)** — ✅ **DONE** — Characterize `RemoveStock` / `AddStock` → port to `native/catalog_stock` → `LibraryImport` wire from `CatalogItem` → parity.  
   - Check: `./scripts/check-catalog.sh` (and `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh`) — **exit 0**  
   - Green means: cargo tests encode parity; .NET characterization green; paid-handler path uses Rust-backed stock mutation (not dead code).

2. **Unit: Stock availability decision** — Characterize confirm/reject rule (`AvailableStock >= Units` → `OrderStockConfirmed` vs `OrderStockRejected` payload shaping for the pure decision). Extract pure function → Rust crate (extend `catalog_stock` or `native/catalog_stock_availability`) → wire handler → parity.  
   - Check: unit tests + `cargo test` + `./scripts/check-catalog.sh`  
   - Green means: handler calls Rust for the boolean/decision; Ordering contracts unchanged.

3. **Unit: Catalog item write invariants** — Characterize create/update validation (id required on v1 update, not-found, price-change detection inputs) as pure predicates where separable from EF. Rust port + wire for pure parts; EF SaveChanges / outbox remain .NET.  
   - Check: `dotnet test tests/Catalog.UnitTests` + targeted functional update/create/delete tests  
   - Green means: invariants enforced via Rust-backed helpers on the live path; `ProductPriceChanged` still published only when price modified.

4. **Unit: Picture path + MIME helpers** — Characterize `GetImageMimeTypeFromImageFileExtension` / path join → Rust → wire from `GetItemPictureById`.  
   - Check: unit tests + `cargo test` + functional `GetCatalogItemPicWithId`  
   - Green means: pic endpoint still returns correct Content-Type; helpers not duplicated only in .NET.

5. **Unit: Query / pagination pure helpers (optional extract)** — If filtering/pagination policy is extracted from EF `IQueryable` (page bounds, name prefix policy), port that policy to Rust; keep SQL in EF.  
   - Check: unit tests for policy + functional list/filter tests (`GetCatalogItemsRespectsPageSize`, by-name, type/brand)  
   - Green means: HTTP list behavior unchanged; Rust on policy path or explicit deferral documented if left in LINQ-only form with owner waiver.

6. **Unit: Integration-handler orchestration hardening** — Keep RabbitMQ/outbox in .NET; ensure both Catalog handlers only call Rust islands for domain decisions/mutations; add characterization around handler pure cores; regression via functional + (where possible) handler-level tests.  
   - Check: `./scripts/check-catalog.sh` + Ordering contract smoke (manual or existing Ordering tests if available)  
   - Green means: no remaining inline stock math in handlers.

7. **Unit: AI / semantic search adapter boundary** — Leave embedding provider in .NET; characterize fallback-to-name-search when AI disabled; no Rust required for vendor SDK unless a pure ranking post-process is identified. If a pure post-process appears, port that only.  
   - Check: functional `GetCatalogItemWithsemanticrelevance` (AI off → name path)  
   - Green means: fallback behavior locked; AI SDK remains adapter.

8. **Unit: Host & schema remain .NET (explicit non-rewrite)** — Document keep: Kestrel host, EF migrations, Aspire registration, OpenAPI generation. Optional future spike (out of SHIV-20 demo critical path): Rust HTTP sidecar — **not** required to close Catalog domain migration if islands 1–6 are done and adapters stay .NET.  
   - Check: AppHost still starts `catalog-api`; functional suite green with Docker  
   - Green means: service operable; domain islands Rust-backed; adapters accounted for.

… Inventoried HTTP + events + domain + adapters are covered by units 1–8. No Catalog surface left unplanned.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Behavioral drift on stock (partial remove, empty stock, max threshold) | Wrong inventory / order fulfillment | Characterization tests before extract; Rust parity cases mirrored; wire before claiming done | `./scripts/check-catalog.sh`; unit failures |
| Missing `Catalog.UnitTests` | Harness skips to Docker functional only; slow/fragile | Add unit tests in first unit before mass edits | Harness path `A:Catalog.UnitTests` vs `B:Functional` / `C:unavailable` |
| FFI / cdylib deploy (native lib next to Catalog.API) | Runtime load failures in Aspire/containers | Document copy/`NativeLibrary` path; build release in CI; smoke paid-handler path | Process start + integration test calling RemoveStock path |
| Duplicated event contracts across services | Silent serialize/deserialize breaks | Freeze event shapes during Catalog units; change Ordering/Webhooks in lockstep if needed | Ordering/Webhooks handlers + bus integration tests |
| Xcode / linker / platform toolchain gaps | `cargo test` fails locally/CI | Document rustup + platform C toolchain; CI image with Rust | Harness Rust steps exit ≠ 0 |
| Leaving Rust as skeleton dead code | Fake “migrated” state | Acceptance requires LibraryImport on live path; validate skill checks wire | Code search for `LibraryImport` / native calls; migration-validate |
| AI / pgvector / outbox complexity pulled into early units | Huge blast radius, blocked demos | Sequence adapters last; first units stay I/O-free | Review unit scope against this plan |
| Unfinished service surface | Demo stops after stock only | Whole-service sequence above; SHIV-20 follows units 1+ | Plan checklist + ticket breakdown |

## First unit (first vertical) — COMPLETE

- **Scope:** Characterize `RemoveStock` / `AddStock` → **Rust port in `native/catalog_stock`** → **wire via `CatalogStockNative` / `LibraryImport`** → **parity** → harness green.
- **Delivered:**
  - `tests/Catalog.UnitTests/CatalogItemStockTests.cs` (8 characterization cases)
  - `native/catalog_stock` pure + FFI (`catalog_stock_remove` / `catalog_stock_add`)
  - `CatalogItem` delegates to Rust; paid handler unchanged (still calls `RemoveStock` then EF save)
  - `CatalogStock.Native.targets` builds/copies release cdylib into output
  - `./scripts/check-catalog.sh` and `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → **exit 0**
- **Next unit to implement:** Unit 2 — Stock availability decision (confirm/reject rule).
- **Remaining service coverage:** Units 2–8 in Recommended sequence.
- **Hand-off:** ready for **Migration validate** on this stock island before/while starting unit 2.
