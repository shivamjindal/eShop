# .NET → Rust migration scope: Basket.API

Ticket: SHIV-21 (`Basket.API → Rust`, epic SHIV-19). Service: `src/Basket.API`.

Direction is fixed: .NET → Rust. End state for this service is Rust on the live path; the
.NET project is deleted once parity is proven.

**Status:** units 1–6 below are done — `native/crates/basket` serves `basket-api`,
`src/Basket.API` is deleted, and the evidence is in `validate.md`.

## Definition of done

- [x] Whole-service inventory (public surface, domain, adapters, events, deps) complete
- [x] Blast radius for migrating the service documented (local vs cross-cutting)
- [x] Recommended sequence covers the whole service with sequenced verifiable units
- [x] Each scheduled unit includes required Rust implementation + wire + parity
- [x] A harness command exists that fails closed on drift, before mass edits

How to check pass/fail: `./scripts/check-basket.sh` exits 0 (Rust workspace tests + release
build + the .NET tests that still exist), and `./scripts/parity-basket.sh` exits 0 (records a
behavioral transcript from the running service and replays every case against Rust).

## Inventory

Everything below was read out of `src/Basket.API` at the commit this plan was written.

- **Assemblies / csproj / TFM:** single project `src/Basket.API/Basket.API.csproj`,
  `Microsoft.NET.Sdk.Web`, `net10.0`, SDK-style, `PublishAot` when `EnableAotPublishing=true`.
  Test project `tests/Basket.UnitTests` (MSTest.Sdk, NSubstitute) references it.
- **NuGet / build:** Central Package Management (`Directory.Packages.props`);
  `Aspire.StackExchange.Redis`, `Grpc.AspNetCore`; project refs `eShop.ServiceDefaults`,
  `EventBusRabbitMQ`. Built via `eShop.Web.slnf` (the solution filter CI uses).
- **Hosting:** Kestrel with `Kestrel:EndpointDefaults:Protocols=Http2` (h2c, no TLS in-cluster).
  Registered in Aspire as `builder.AddProject<Projects.Basket_API>("basket-api")` with
  `.WithReference(redis)`, `.WithReference(rabbitMq).WaitFor(rabbitMq)` and
  `Identity__Url` pointed at Identity.API. `redis.WithParentRelationship(basketApi)`.
  Health/liveness comes from `AddBasicServiceDefaults` → `MapDefaultEndpoints` (`/health`, `/alive`,
  development only).
- **Public surface:** one gRPC service from `Proto/basket.proto`, `package BasketApi`,
  `service Basket`:
  - `GetBasket(GetBasketRequest) → CustomerBasketResponse` — `[AllowAnonymous]`; anonymous or
    unknown user returns an empty response, never an error.
  - `UpdateBasket(UpdateBasketRequest) → CustomerBasketResponse` — anonymous ⇒
    `Unauthenticated: The caller is not authenticated.`; Redis write failure ⇒
    `NotFound: Basket with buyer id {userId} does not exist`.
  - `DeleteBasket(DeleteBasketRequest) → DeleteBasketResponse` — anonymous ⇒ `Unauthenticated`;
    otherwise deletes the key and returns empty (idempotent, no error for a missing basket).
  The wire messages only carry `product_id` (field 2) and `quantity` (field 6); name/price/picture
  are *not* round-tripped through gRPC even though they exist in the Redis document.
  No HTTP/REST surface, no CLI, no background worker other than the event bus consumer.
- **Domain vs adapters:**
  - Domain (pure): identity extraction (`sub` claim), the request→`CustomerBasket` mapping, the
    `CustomerBasket`→response mapping, the two error conditions. `BasketItem.Validate`
    (`Quantity < 1` ⇒ "Invalid number of units") is dead code — nothing calls it.
  - Adapters: `RedisBasketRepository` (StackExchange.Redis), `AddRabbitMqEventBus` consumer,
    `AddDefaultAuthentication` JWT bearer, ServiceDefaults OpenTelemetry/health.
- **Events:** consumes `OrderStartedIntegrationEvent` (routing key = CLR type name) on the durable
  queue `Basket` bound to the non-durable direct exchange `eshop_event_bus`; the handler deletes
  that user's basket. Publishes nothing.
- **Persistence contract (must be preserved byte-for-byte — WebApp and Ordering read the same
  Redis instance through this service only, but the document shape is what makes replay safe):**
  key `/basket/{userId}`, value = `System.Text.Json` of `CustomerBasket` with PascalCase
  properties `BuyerId`, `Items[{Id, ProductId, ProductName, UnitPrice, OldUnitPrice, Quantity,
  PictureUrl}]`. Decimals serialize as `0`, not `0.0`. Reads are case-insensitive and missing
  properties fall back to CLR defaults.
- **Dependencies:** Redis (`redis` connection string), RabbitMQ (`eventbus`), Identity.API for JWT
  issuer metadata (`Identity__Url`, audience `basket`, `ValidateAudience=false`, issuer must match,
  5 min default clock skew).
- **Existing Rust crates:** `native/` Cargo workspace with empty landing zones
  `crates/eshop-core`, `crates/catalog`, `crates/basket`, `crates/ordering`. Basket lands in
  `native/crates/basket`.

## Dependencies / blast radius

- **Inbound (local):** `src/WebApp` — `AddGrpcClient<Basket.BasketClient>` at `http://basket-api`
  with `.AddAuthToken()`, over Aspire service discovery, h2c. `WebApp.csproj` compiles the proto
  straight out of `..\Basket.API\Proto\basket.proto`, so deleting the .NET project forces a
  one-line csproj repoint.
- **Inbound (cross-cutting, not affected):** `src/ClientApp` (MAUI) keeps its own copy of the proto
  and calls basket-api directly; `mobile-bff` (YARP) does **not** proxy basket, so the BFF routes
  are untouched. Ordering.API has its own unrelated `CustomerBasket`/`BasketItem` DTOs and is not a
  caller.
- **Outbound:** Redis (owned solely by this service), RabbitMQ exchange `eshop_event_bus` +
  queue `Basket`, Identity.API discovery document/JWKS.
- **Cross-cutting shared code dropped by the port:** `eShop.ServiceDefaults`
  (OpenTelemetry/health/auth) and `EventBusRabbitMQ` stay in the repo for the other services;
  the Rust service reimplements only what basket used.
- **Service-level migration notes:** the gRPC contract, the Redis document and the event
  subscription are the three contracts that must not drift. Everything else (DI, logging,
  telemetry plumbing) is internal.
- **Safety fact (first unit):** *Basket.API owns the `/basket/*` Redis keyspace and no other
  service reads or writes those keys, so the whole service can be swapped as one vertical as long
  as the JSON document shape is preserved.*
  - Status: **proven**
  - Evidence: `rg -n "/basket/" src/` matches only `src/Basket.API/Repositories/RedisBasketRepository.cs`;
    `rg -n "AddRedisClient|IConnectionMultiplexer" src/` matches only Basket.API; the Aspire graph
    gives `redis` to `basket-api` alone (`src/eShop.AppHost/Program.cs`). Baseline
    `dotnet test tests/Basket.UnitTests/Basket.UnitTests.csproj` → 3/3 passed before any edit.

## Recommended sequence (covers the service)

Basket.API is small enough that the honest unit boundary is the whole service; splitting the three
RPCs across units would leave a half-Rust gRPC server that cannot serve WebApp. So unit 1 is the
service vertical, and the later units are the coverage/cleanup work that the vertical does not do.

1. **Unit 1 — characterize the live .NET service.** Keep `tests/Basket.UnitTests` green on the
   baseline, then record a behavioral transcript from the *running* .NET service:
   `basket-parity record` drives real gRPC calls against real Redis/RabbitMQ with real JWTs and
   captures status code, response items and the raw Redis document per case.
   Check: `./scripts/parity-basket.sh --record-only` → green means the transcript is reproducible.
2. **Unit 2 — Rust port of the service** into `native/crates/basket`: tonic gRPC server, Redis
   repository with byte-identical JSON, JWT validation against Identity's JWKS, lapin consumer for
   `OrderStartedIntegrationEvent`. Check: `cargo test --manifest-path native/Cargo.toml --workspace`.
3. **Unit 3 — parity replay.** `basket-parity replay` runs every recorded case against the Rust
   server and diffs status, items and Redis bytes. Check: `./scripts/parity-basket.sh` exit 0.
4. **Unit 4 — wire Aspire to Rust.** `builder.AddRustService("basket-api", "basket-service")`
   replaces `AddProject<Projects.Basket_API>`, keeping the `redis`/`eventbus`/`Identity__Url`
   wiring. Check: `dotnet build eShop.Web.slnf` + the app boots and WebApp reaches basket-api.
5. **Unit 5 — delete the .NET service.** Remove `src/Basket.API` and `tests/Basket.UnitTests`,
   repoint `WebApp.csproj` at the proto now owned by the crate, drop the solution/AppHost refs.
   Check: `./scripts/check-basket.sh` exit 0 (no .NET basket left to test, Rust must be green).
6. **Unit 6 — end-to-end proof.** Run the full Aspire stack, log in as a seeded user, add to
   basket, checkout (which exercises the `OrderStartedIntegrationEvent` consumer), and run the
   Playwright basket specs. Check: `npx playwright test` basket specs pass against the live stack.

## Risks

| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| Redis JSON drift (decimal formatting, missing fields, key casing) | WebApp shows wrong prices or an empty basket; old documents fail to load | Serialize with `serde_json::Number`, never `f64`; case-insensitive key normalization + defaults for absent fields | Parity replay diffs the raw Redis document, not just the gRPC response |
| gRPC status drift (anonymous handling) | WebApp error pages instead of an empty basket, or silent data loss | Characterize all four token states (none/invalid/expired/wrong issuer) before porting | Transcript cases assert exact `code` + `message` |
| RabbitMQ topology mismatch | `PRECONDITION_FAILED` on boot; checkout no longer clears the basket | Declare exchange `durable=false` exactly as `IChannelExtensions` does; queue `Basket` durable | Parity case publishes `OrderStartedIntegrationEvent` and asserts the key is gone |
| JWT validation differences (issuer, clock skew, `sub`) | Users silently anonymous ⇒ empty baskets | Mirror `AddDefaultAuthentication`: issuer = `Identity__Url`, audience not validated, 300s leeway, identity from `sub` | Transcript includes expired and wrong-issuer tokens |
| Aspire can't inject service discovery into a non-project resource | WebApp cannot resolve `http://basket-api` | `AddRustService` registers an `ExecutableResource` with an HTTP endpoint; consumers use `.WithReference(basketApi.GetEndpoint("http"))` | AppHost build + WebApp gRPC call in the smoke run |
| Deleting the .NET project loses the proto that WebApp compiles | WebApp build break | Move the proto into the crate and repoint `WebApp.csproj` in the same change | `dotnet build eShop.Web.slnf` |

## First unit (first vertical to implement)

- **Scope:** the Basket vertical — characterize → Rust port → wire Aspire → parity → delete .NET.
  Not extract-only; not "the estate is done".
- **Why first:** it is the only unit; Basket.API is a single-project service with three RPCs, one
  adapter and one event handler, and it owns its datastore (safety fact above).
- **Harness:** `./scripts/check-basket.sh` (Rust workspace tests + release build + remaining .NET
  basket tests) and `./scripts/parity-basket.sh` (record from .NET, replay against Rust). Both are
  committed; both fail closed.
- **Rust crate path:** `native/crates/basket` with `[[bin]] basket-service` (the server) and
  `[[bin]] basket-parity` (record/replay tool).
- **Boundary:** process boundary, not FFI. Basket.API is a network service, so the honest
  replacement is a Rust gRPC server that Aspire starts in place of the .NET project — Rust is on
  the live request path for every WebApp basket call.
- **Acceptance:** parity replay green on every recorded case; `check-basket.sh` exit 0; Aspire
  boots `basket-api` from the crate; WebApp add-to-basket and checkout work in a browser;
  `src/Basket.API` no longer exists.
- **Remaining service coverage:** units 2–6 above; nothing in the inventory is left unplanned.
- **Suggested tickets:** SHIV-21 (this one) covers units 1–6. Follow-ups for the epic:
  Catalog.API → Rust, Ordering.API → Rust using the same record/replay recipe.
