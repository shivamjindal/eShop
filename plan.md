# .NET → Rust migration scope: Basket.API

Ticket: SHIV-21 (`Basket.API → Rust`), under epic SHIV-19.
Service: `src/Basket.API` (gRPC basket service, .NET 10).

## Definition of done

- [ ] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [ ] Blast radius for migrating the service documented (local vs cross-cutting)
- [ ] Recommended sequence covers the service with sequenced verifiable units
- [ ] Each scheduled domain island includes required Rust implementation + .NET→Rust wire + parity
- [ ] End state: `basket-api` in the Aspire app model is the Rust binary; no .NET Basket.API project remains
- How to check pass/fail:
  - `./scripts/check-basket.sh` exits 0 (builds + tests the Rust basket crate; runs the .NET Basket
    characterization tests while the .NET project still exists)
  - `./scripts/parity-basket.sh` exits 0 (replays the recorded .NET transcript against the Rust
    service over real gRPC + Redis + RabbitMQ and diffs it)
  - `grep -r Basket.API src --include=*.csproj` finds no project reference to a Basket .NET project
  - The shop works end to end in a browser (sign in → add to cart → update quantity → checkout)
    with `basket-api` running as the Rust binary

## Inventory

### Assemblies / csproj / TFM
- `src/Basket.API/Basket.API.csproj` — `Microsoft.NET.Sdk.Web`, `net10.0`, SDK-style, Central Package
  Management (`Directory.Packages.props`), optional `PublishAot`.
- `tests/Basket.UnitTests/Basket.UnitTests.csproj` — `MSTest.Sdk`, 3 tests over `BasketService`.
- Listed in `eShop.slnx` and `eShop.Web.slnf`; referenced by `src/eShop.AppHost/eShop.AppHost.csproj`.

### NuGet / build
- `Aspire.StackExchange.Redis`, `Grpc.AspNetCore`; project refs `eShop.ServiceDefaults`,
  `EventBusRabbitMQ`.
- Protobuf codegen: `<Protobuf Include="Proto\basket.proto" GrpcServices="Server" />`.
- CI builds `eShop.Web.slnf` (`.github/workflows/pr-validation.yml`); `eShop.slnx` also contains the
  MAUI `ClientApp`, which does not build in this environment.

### Hosting
- `Program.cs`: `AddBasicServiceDefaults()` + `AddApplicationServices()` + `AddGrpc()`, `MapDefaultEndpoints()`,
  `MapGrpcService<BasketService>()`.
- Kestrel `Protocols: Http2` (h2c — plaintext HTTP/2) from `appsettings.json`.
- Aspire AppHost resource `basket-api`: `AddProject<Projects.Basket_API>` with references to `redis`
  and `eventbus`, plus `Identity__Url`.

### Public surface
gRPC package `BasketApi`, service `Basket` (`Proto/basket.proto`):

| RPC | Request | Response | Behavior |
|-----|---------|----------|----------|
| `GetBasket` | `GetBasketRequest{}` | `CustomerBasketResponse{repeated BasketItem items=1}` | `[AllowAnonymous]`; empty response when unauthenticated or no basket |
| `UpdateBasket` | `UpdateBasketRequest{repeated BasketItem items=2}` | `CustomerBasketResponse` | `Unauthenticated` when no `sub`; writes then re-reads the basket |
| `DeleteBasket` | `DeleteBasketRequest{}` | `DeleteBasketResponse{}` | `Unauthenticated` when no `sub`; deletes the key |

`BasketItem` on the wire is only `{ int32 product_id = 2; int32 quantity = 6; }` — the response drops
every other stored field.

No HTTP/REST surface. `MapDefaultEndpoints` adds `/health` + `/alive` only in Development; nothing in
the app model depends on a Basket health check.

### Domain vs adapters
- Domain (pure): identity extraction (`sub` claim), request→`CustomerBasket` mapping,
  `CustomerBasket`→response mapping, `BasketItem.Validate` (quantity ≥ 1 — declared but never invoked
  on this path).
- Adapters: `RedisBasketRepository` (StackExchange.Redis), JWT bearer auth via
  `eShop.ServiceDefaults.AddDefaultAuthentication`, RabbitMQ consumer via `EventBusRabbitMQ`.

### Storage contract (Redis)
- Key `/basket/{userId}` (string), value = UTF-8 `System.Text.Json` of `CustomerBasket`.
- PascalCase, property order `BuyerId`, `Items[{Id, ProductId, ProductName, UnitPrice, OldUnitPrice,
  Quantity, PictureUrl}]`; `decimal` writes as `0` (not `0.0`); unset strings write as `null`.
- Deserialization is case-insensitive (`PropertyNameCaseInsensitive = true`).

### Events
- Consumed: `OrderStartedIntegrationEvent { UserId }` — deletes that user's basket.
  Exchange `eshop_event_bus` (direct), queue `Basket`, routing key = CLR type name, durable queue,
  always ack, body = PascalCase JSON.
- Produced: none.

### Auth contract
- JWT bearer, authority/issuer = `Identity__Url`, `ValidateAudience = false`, `sub` claim carries the
  user id, default 5 minute clock skew, `RequireHttpsMetadata = false`.
- The gRPC methods declare no authorization requirement, so a bad token does **not** produce
  `Unauthenticated` at the transport level — the caller is simply anonymous, and each method decides.

### Existing Rust crates
- `native/` Cargo workspace with landing zones `crates/eshop-core`, `crates/catalog`, `crates/basket`
  (empty), `crates/ordering`. Basket lands in `native/crates/basket`.

## Dependencies / blast radius

### Inbound (who calls Basket.API)
- **WebApp** (`src/WebApp`) — *local*. `AddGrpcClient<Basket.BasketClient>(o => o.Address = new("http://basket-api")).AddAuthToken()`,
  resolved by Aspire service discovery. Generates its client stubs from `..\Basket.API\Proto\basket.proto`
  (`src/WebApp/WebApp.csproj`), so deleting the .NET project moves that path.
- **ClientApp** (MAUI) — *not affected*. Has its own copy of the proto
  (`src/ClientApp/Services/Basket/Protos/basket.proto`) and calls basket-api directly; `mobile-bff`
  does not proxy basket.
- **Identity.API** — *local*. Receives `BasketApiClient` = basket endpoint for the `basketswaggerui`
  client redirect URIs only.
- **Ordering.API** — *no direct call*. It has its own `CustomerBasket`/`BasketItem` DTOs for the
  checkout payload; they are unrelated types that never cross the Basket gRPC boundary.

### Outbound
- Redis (`redis` resource) — *local*: the `/basket/*` keyspace is owned only by this service.
- RabbitMQ (`eventbus`) — *cross-cutting contract*: the `Basket` queue binding and the
  `OrderStartedIntegrationEvent` JSON shape are shared with Ordering.API (publisher).
- Identity.API JWKS — *cross-cutting contract*: token issuer and `sub` claim.

### Cross-cutting shared .NET code that does **not** survive the port
`eShop.ServiceDefaults` (OTEL, health, auth), `EventBusRabbitMQ`, `Aspire.StackExchange.Redis`. The Rust
service must re-implement the parts it needs (JWT validation, Redis access, AMQP consumer) rather than
share them, so the wire contracts above are what must be preserved byte for byte.

### Test hotspots / untested paths
- Covered today: 3 MSTest cases over `BasketService.GetBasket` only.
- Untested today: `UpdateBasket`, `DeleteBasket`, the Redis JSON contract, and the
  `OrderStartedIntegrationEvent` handler. Those need characterization before the port.

### Safety fact (first unit)
**The Redis `/basket/{userId}` keyspace and its JSON encoding are owned only by Basket.API — no other
service reads or writes those keys — so a Rust re-implementation only has to match Basket.API's own
serialization to be safe.**

- Status: **proven**
- Evidence: `rg -n "/basket/|BasketKeyPrefix|GetBasketKey" src --glob '!**/bin/**' --glob '!**/obj/**'`
  matches only `src/Basket.API/Repositories/RedisBasketRepository.cs`; `rg -l "AddRedisClient|IConnectionMultiplexer" src`
  matches only `src/Basket.API`. Byte-level encoding is pinned by the characterization test
  `RedisBasketRepositoryTests.SerializedBasketMatchesStoredContract` (unit 1) and re-checked against
  the live Rust service by `./scripts/parity-basket.sh`.

## Recommended sequence (covers the service)

1. **Characterize the .NET surface** — add MSTest coverage for `UpdateBasket`, `DeleteBasket`, the
   unauthenticated paths, the response projection, the Redis JSON byte contract, and the
   `OrderStartedIntegrationEvent` handler.
   Check: `dotnet test tests/Basket.UnitTests/Basket.UnitTests.csproj` → green means current behavior
   is locked before anything moves.
2. **Record a behavioral transcript from the live .NET service** — drive real gRPC calls against
   `Basket.API` with real Redis + RabbitMQ + a local OIDC stub, and record request/response/Redis-state
   into `scripts/parity/basket-dotnet.transcript`.
   Check: `./scripts/parity-basket.sh record --dotnet` → green means the baseline is captured; this
   file is what makes deleting the .NET project safe.
3. **Rust port: domain + storage island** — `native/crates/basket`: `CustomerBasket`/`BasketItem`
   model, the exact Redis key + JSON encoding, and the request/response mapping.
   Check: `cargo test -p basket` → green means the pure rules and the storage encoding match the
   characterized .NET behavior.
4. **Rust port: auth island** — JWT bearer validation against the Identity JWKS (issuer check,
   audience ignored, `sub` extraction, 5 minute leeway, anonymous-on-invalid semantics).
   Check: `cargo test -p basket` → green means identity resolution matches.
5. **Rust port: gRPC surface + event consumer** — tonic server for the three RPCs and a lapin consumer
   on `eshop_event_bus`/`Basket`/`OrderStartedIntegrationEvent`.
   Check: `./scripts/check-basket.sh` → green means the service builds and all Rust tests pass.
6. **Wire the app model to Rust** — `AddRustService("basket-api", …)` in `src/eShop.AppHost`, move the
   proto that `WebApp` compiles to `native/crates/basket/proto/basket.proto`, keep the same env
   contract (`ConnectionStrings__redis`, `ConnectionStrings__eventbus`, `Identity__Url`).
   Check: `dotnet build eShop.Web.slnf` + app starts and WebApp reaches basket-api → green means Rust
   is on the live path.
7. **Parity + retire the .NET project** — replay the recorded transcript against the Rust service,
   then delete `src/Basket.API` and `tests/Basket.UnitTests` and drop them from `eShop.slnx` /
   `eShop.Web.slnf` / AppHost project references.
   Check: `./scripts/parity-basket.sh` and `./scripts/check-basket.sh` → green means the Rust service
   reproduces the recorded .NET behavior with no .NET Basket left.
8. **End-to-end validation** — sign in as a seeded user in the WebApp, add items, change quantities,
   check out, confirm the basket is emptied by `OrderStartedIntegrationEvent`.
   Check: Playwright `e2e/` run + a recorded walkthrough → green means the shop behaves the same on
   the Rust backend.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Redis JSON drift (casing, field order, `0` vs `0.0`, nulls) | Baskets written by one impl unreadable/lossy in the other; silent cart loss on rollback | Port the exact `System.Text.Json` encoding; assert raw bytes in both test suites | `RedisBasketRepositoryTests` byte assertion + transcript `redis_after` diff in `parity-basket.sh` |
| Auth semantics drift (invalid token → `Unauthenticated` instead of anonymous) | WebApp gets errors where it used to get an empty basket | Characterize the anonymous-on-invalid behavior and mirror it in Rust | Transcript cases `get_no_token`, `get_bad_token`, `update_no_token` |
| Event contract drift (queue name, routing key, JSON casing) | Checkout stops clearing the cart | Reuse exchange/queue/routing key and PascalCase JSON; consume with a durable queue and always-ack | Transcript case `order_started_clears_basket` |
| gRPC field numbers / h2c protocol mismatch | WebApp cannot talk to the service at all | Compile the Rust server from the same `.proto` and keep WebApp compiling from that one file | `dotnet build` + E2E browser run |
| Rust binary is not built when the AppHost starts | `basket-api` fails to start in a clean checkout | `AddRustService` runs `cargo run --release`, and `check-basket.sh` builds the release binary | AppHost logs + `./scripts/check-basket.sh` |
| Losing .NET behavior after the project is deleted | No way to re-derive the baseline | Record the transcript **before** deletion and commit it | `scripts/parity/basket-dotnet.transcript` is in git |
| Rust ops/runtime skill gap for the team | Slower incident response on basket | Keep the crate small and idiomatic, document env contract in `native/crates/basket/README.md` | Code review |

## First unit (first vertical to implement)

- Scope: units 1–3 above — characterize the .NET surface, record the transcript, then port the
  domain + Redis storage island to Rust. Not extract-only: unit 3 lands real Rust.
- Why first: the Redis encoding is the one contract that outlives the process boundary, it is pure
  (no I/O in the encode/decode step), and everything else in the service is a thin shell around it.
- **Harness:** `./scripts/check-basket.sh` — run before mass edits; fails closed (non-zero) when the
  Rust crate fails to build or test, or when the .NET characterization tests fail while they exist.
  Transcript parity: `./scripts/parity-basket.sh`.
- Rust crate path: `native/crates/basket` (existing landing zone in the `native/` workspace).
- Boundary: the Rust crate ships a `basket-service` binary that **replaces** the .NET process as the
  `basket-api` Aspire resource, speaking the same gRPC contract to unmodified callers. (P/Invoke does
  not apply — the unit of migration here is a whole network service, not an in-process rule.)
- Acceptance: Rust on the live path; every recorded transcript case matches; `check-basket.sh` exit 0.
- Remaining service coverage: units 4–8 above, all landed in this same change since the service is
  small enough to keep one green step per unit.
- Suggested tickets: "Characterize Basket.API gRPC + Redis contract", "Record Basket.API parity
  transcript", "Port Basket domain/storage to Rust", "Port Basket auth to Rust", "Rust basket gRPC
  server + event consumer", "Cut basket-api over to Rust and retire the .NET project".
