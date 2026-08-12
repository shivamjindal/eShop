# .NET → Rust migration scope: Basket.API

Ticket: SHIV-21 (*Basket.API → Rust*), under epic SHIV-19 (*eShop — backend to Rust, frontend to React*).

Direction is fixed: **.NET → Rust**. Rust is the required end state for every unit below, not an
optional spike.

## Definition of done

- [x] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [x] Blast radius for migrating the service documented (local vs cross-cutting)
- [x] Recommended sequence covers the whole service with sequenced verifiable units
- [x] Each scheduled island includes required Rust implementation + wire + parity
- [x] First unit names a harness command that fails closed before mass edits

How to check pass/fail:

- `./scripts/check-basket.sh` exits 0 (builds + tests the Rust crate, runs the .NET Basket tests
  while they exist, and runs the dual-run parity harness when Docker is available).
- `MIGRATION_REQUIRE_RUST=1 ./scripts/check-basket.sh` fails if `native/basket_service` is missing.
- Every unit below has a named check command and a "green means" statement.

## Inventory

Everything under `src/Basket.API` (the whole service), as of the baseline commit.

- **Assemblies / csproj / TFM:** single project `src/Basket.API/Basket.API.csproj`,
  SDK-style `Microsoft.NET.Sdk.Web`, `net10.0`, `PublishAot` behind `EnableAotPublishing`.
  Test project `tests/Basket.UnitTests` (MSTest.Sdk, NSubstitute) references it.
- **NuGet / build:** Central Package Management (`Directory.Packages.props`).
  Direct: `Aspire.StackExchange.Redis`, `Grpc.AspNetCore`. Project refs:
  `eShop.ServiceDefaults`, `EventBusRabbitMQ`. Built through `eShop.slnx` / `eShop.Web.slnf`.
- **Hosting:** Kestrel with `Kestrel:EndpointDefaults:Protocols = Http2` (h2c only, no TLS in dev),
  single `http` endpoint (`http://localhost:5221` in `launchSettings.json`).
  Registered in Aspire as `builder.AddProject<Projects.Basket_API>("basket-api")` with references
  to `redis` and `eventbus` (RabbitMQ) and `Identity__Url`.
  `MapDefaultEndpoints()` maps `/health` and `/alive` in Development.
- **Public surface:** gRPC only — `BasketApi.Basket` from `src/Basket.API/Proto/basket.proto`:
  - `GetBasket(GetBasketRequest) -> CustomerBasketResponse`
  - `UpdateBasket(UpdateBasketRequest) -> CustomerBasketResponse`
  - `DeleteBasket(DeleteBasketRequest) -> DeleteBasketResponse`
  No HTTP/REST routes, no CLI/batch entrypoints.
- **Domain vs adapters:**
  - Domain (pure, I/O-free): identity gate per RPC (`GetBasket` returns empty for anonymous,
    `UpdateBasket`/`DeleteBasket` throw `Unauthenticated`), proto ↔ model mapping
    (only `product_id` and `quantity` cross the wire), Redis key naming `/basket/{userId}`,
    `CustomerBasket`/`BasketItem` JSON shape (PascalCase System.Text.Json), `BasketItem.Validate`
    (quantity < 1 → "Invalid number of units"; never invoked on the gRPC path).
  - Adapters: `RedisBasketRepository` (StackExchange.Redis string get/set/delete),
    JWT bearer authentication from `eShop.ServiceDefaults/AuthenticationExtensions.cs`
    (`Authority = Identity:Url`, `ValidateAudience = false`, `sub` claim not remapped),
    RabbitMQ consumer from `EventBusRabbitMQ`, OpenTelemetry/health from ServiceDefaults.
- **Events produced/consumed:** produces none. Consumes `OrderStartedIntegrationEvent`
  (exchange `eshop_event_bus`, type `direct`, durable queue `Basket`, routing key =
  `OrderStartedIntegrationEvent`, body `{"Id","CreationDate","UserId"}`, manual ack) → deletes
  `/basket/{UserId}`. Published by Ordering.API through its outbox.
- **Dependencies:** Redis (Aspire `redis`), RabbitMQ (Aspire `eventbus`), Identity.API for JWKS,
  ServiceDefaults (telemetry/health/auth), EventBusRabbitMQ.
- **Existing Rust crates:** `native/catalog_stock` (Catalog stock island from the earlier
  Catalog demo). New crate for this service: `native/basket_service`.

## Dependencies / blast radius

- **Inbound (who calls Basket.API):**
  - `src/WebApp` — `AddGrpcClient<Basket.BasketClient>(o => o.Address = new("http://basket-api"))`
    with `.AddAuthToken()`; consumed by `WebApp/Services/BasketService.cs` → `BasketState` →
    `CartPage`, `CartMenu`, `Checkout`, `ItemPage`, `Chatbot`. *Cross-cutting* (contract must not
    change; the proto is the seam).
  - `src/ClientApp` (MAUI) — keeps its **own copy** of the proto at
    `src/ClientApp/Services/Basket/Protos/basket.proto`. *Local to ClientApp*, unaffected.
  - `src/Identity.API` — only receives `BasketApiClient` = basket http endpoint for a Swagger
    redirect URI. *Local*, endpoint name `basket-api` must stay stable.
  - Aspire AppHost — owns the resource named `basket-api`. *Cross-cutting*: the resource must keep
    the same name and expose an `http` endpoint so `services__basket-api__http__0` still resolves.
- **Outbound:**
  - Redis string keys `/basket/{userId}` (JSON value).
  - RabbitMQ `eshop_event_bus` exchange, queue `Basket`.
  - Identity.API OIDC discovery + JWKS for token validation.
- **Cross-cutting:** the gRPC contract (`basket.proto`), the Redis value format (a basket written
  by one implementation must be readable by the other during any dual-run), the RabbitMQ queue
  name (`Basket`) and routing key, and the Aspire resource name.
- **Service-level migration notes:** Basket.API has no database schema, no EF migrations, and no
  outbound integration events, which makes it the cleanest whole-service port in the estate. The
  only shared .NET types it consumes are ServiceDefaults/EventBus infrastructure, not domain
  contracts, so removing the .NET project does not force changes in other services.
- **Safety fact (first unit):** *The Redis keyspace `/basket/{userId}` is written and read only by
  Basket.API — no other service in the repo opens a Redis connection.*
  - Status: **proven**
  - Evidence: `grep -rn "AddRedisClient\|IConnectionMultiplexer" --include=*.cs src/` returns only
    `src/Basket.API/Extensions/Extensions.cs:14` and
    `src/Basket.API/Repositories/RedisBasketRepository.cs:6`; the AppHost attaches the `redis`
    resource to `basket-api` only (`src/eShop.AppHost/Program.cs:30-34`). Confirmed at runtime by
    scanning the running Redis instance and seeing only `/basket/*` keys.

## Recommended sequence (covers the service)

Units 1–7 together cover the entire inventoried surface (3 RPCs, auth adapter, Redis adapter,
event consumer, hosting/observability, callers).

1. **Characterize the .NET gRPC surface** — extend `tests/Basket.UnitTests` to lock all three RPCs
   (anonymous vs authenticated, mapping, `Unauthenticated`, `NotFound`) plus the exact Redis JSON
   contract produced by `BasketSerializationContext`.
   Check: `dotnet test tests/Basket.UnitTests` → green means current behavior is pinned before any
   port, and the Rust port has an executable specification to match.
2. **Rust domain core** — `native/basket_service` crate (`cdylib` + `rlib`) with the pure rules:
   identity gate per RPC, proto ↔ model mapping, `/basket/{id}` key naming, PascalCase
   `CustomerBasket` JSON, quantity validation.
   Check: `cargo test --manifest-path native/basket_service/Cargo.toml` → green means the pure
   island matches the characterized cases with no I/O involved.
3. **Rust adapters** — Redis repository (`redis` crate), JWT/JWKS validation against Identity.API
   discovery (`jsonwebtoken`), RabbitMQ consumer of `OrderStartedIntegrationEvent` (`lapin`).
   Check: `cargo test` (adapter-level unit tests) + `cargo build --release` → green means the
   adapters compile and their pure parts are covered.
4. **Rust gRPC host** — `tonic` server implementing `BasketApi.Basket` over h2c on the port Aspire
   assigns, plus `grpc.health.v1` health.
   Check: `cargo build --release` + harness smoke → green means the binary serves the contract.
5. **Wire eShop to Rust** — AppHost registers `basket-api` as the Rust executable, WebApp resolves
   it through the unchanged `http://basket-api` service-discovery name, proto moves to
   `native/basket_service/proto/basket.proto` as the single source of truth.
   Check: `./scripts/check-basket.sh` + Aspire run → green means Rust is on the live path.
6. **Dual-run parity harness** — `scripts/parity-basket.sh` runs the .NET service and the Rust
   service side by side against one Redis (separate DB indexes) and one RabbitMQ with a throwaway
   OIDC issuer, replays the characterization matrix against both, and compares gRPC responses,
   status codes and the resulting Redis bytes.
   Check: `./scripts/parity-basket.sh` → green means the two implementations are observably
   identical on every characterized case.
7. **Retire the .NET service** — delete `src/Basket.API` and `tests/Basket.UnitTests`, update
   `eShop.slnx`, `eShop.Web.slnf`, `WebApp.csproj`, AppHost.
   Check: `dotnet build eShop.slnx` + `./scripts/check-basket.sh` + Playwright `e2e` run → green
   means no duplicated legacy path is left behind and the store still works end to end.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Redis value drift (property casing, number/format) breaks baskets written by the other implementation | Silent data loss for in-flight carts during cutover | Serde model mirrors System.Text.Json PascalCase output; parity harness compares raw Redis bytes | `scripts/parity-basket.sh` byte comparison; characterization test pinning the .NET JSON |
| JWT validation differences (issuer, audience, clock skew, unmapped `sub`) | Users appear anonymous → empty carts, or unauthorized errors | Mirror `AddDefaultAuthentication`: validate signature + issuer, do **not** validate audience, read raw `sub` | Parity cases with valid / missing / invalid / wrong-issuer tokens |
| gRPC status-code drift (`Unauthenticated` vs `NotFound` vs `Internal`) | WebApp error handling changes | Characterization tests assert status codes; Rust maps the same way | Parity harness compares status codes, not just payloads |
| h2c / HTTP-2 negotiation with the Aspire endpoint | WebApp cannot reach the service at all | Serve h2c only, same as Kestrel `Protocols: Http2`; keep the resource name and `http` endpoint | Playwright e2e + gRPC smoke against the Aspire-assigned port |
| RabbitMQ consumer differences (queue name, ack semantics, JSON casing) | Baskets not cleared after checkout | Same exchange/queue/routing key, manual ack after handling, PascalCase payload parse | Parity case that publishes `OrderStartedIntegrationEvent` and asserts both baskets are deleted |
| Rust toolchain absent on a developer machine or CI agent | Build/run of the whole AppHost fails | `check-basket.sh` fails closed with a clear message; documented prerequisite in the crate README | `./scripts/check-basket.sh` exit code 2 |
| Losing OpenTelemetry/health parity from ServiceDefaults | Reduced observability after cutover | Structured `tracing` logs to stdout collected by Aspire, gRPC health service | Aspire dashboard shows the resource healthy with logs |

## First unit (first vertical to implement)

- **Scope:** characterize → Rust port of the pure basket rules → Rust adapters + tonic host →
  wire AppHost/WebApp to Rust → dual-run parity. (Units 1–6 above; unit 7 retires the .NET
  service once parity is green.)
- **Why first:** Basket.API is the smallest complete service in the estate (3 RPCs, one key-value
  adapter, one event consumer, no schema), so it is the first place a *whole service* can be moved
  to Rust and proven end to end rather than a single extracted rule.
- **Harness:** `./scripts/check-basket.sh` — run before mass edits; fails closed when the Rust
  crate is missing (`MIGRATION_REQUIRE_RUST=1`), when `cargo test` fails, when the .NET Basket
  tests fail (while they exist), or when the parity harness reports a mismatch.
- **Rust crate path:** `native/basket_service` (`rlib` + binaries `basket-service` and
  `parity-client`).
- **Boundary:** the service boundary itself — the Rust binary *is* the `basket-api` resource and
  speaks the same gRPC contract. There is no FFI shim, so no dead Rust code.
- **Acceptance:**
  - `cargo test --manifest-path native/basket_service/Cargo.toml` green
  - `./scripts/parity-basket.sh` green (identical responses, statuses and Redis bytes)
  - `./scripts/check-basket.sh` exit 0
  - eShop runs under Aspire with `basket-api` served by Rust and the store's add/update/remove
    cart flows work in the browser
- **Remaining service coverage:** none after unit 7 — the whole service is Rust. Follow-on work for
  the epic is the other services in `Recommended sequence` terms (Catalog.API, Ordering.API), which
  are out of scope for SHIV-21.
- **Suggested tickets:** "Characterize Basket.API gRPC surface", "Rust basket domain + adapters",
  "Rust tonic basket host", "Wire AppHost to Rust basket", "Dual-run parity harness",
  "Retire .NET Basket.API".

## Status

| Unit | State | Evidence |
|------|-------|----------|
| 1. Characterize .NET gRPC surface | done | 16 tests green on the baseline (`dotnet test tests/Basket.UnitTests`), then folded into the Rust suite and the recorded transcript when the .NET project was removed |
| 2. Rust domain core | done | `cargo test --manifest-path native/basket_service/Cargo.toml` (34 tests) |
| 3. Rust adapters | done | same suite (redis url parsing, event handling, token parsing) + parity harness against real Redis/RabbitMQ |
| 4. Rust gRPC host | done | `scripts/parity-basket.sh` drives the running binary over gRPC |
| 5. Wire eShop to Rust | done | AppHost `basket-api` = `native/basket_service`; `npx playwright test` green against the running store |
| 6. Dual-run parity harness | done | `./scripts/parity-basket.sh --dual` → 47/47 identical observations; transcript committed at `scripts/parity/basket-dotnet.transcript` |
| 7. Retire .NET Basket.API | done | `src/Basket.API` and `tests/Basket.UnitTests` deleted; `dotnet build eShop.Web.slnf` green |
