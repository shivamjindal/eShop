# Migration validate: Basket.API / whole-service vertical → Rust

Unit from `plan.md`: the Basket vertical (characterize → Rust port → wire Aspire → parity →
delete .NET). Every command below was run on this branch; exit codes are what the shell
reported, not estimates.

## Claim

The Rust `basket-api` behaves identically to the deleted .NET Basket.API for every recorded
case — gRPC status codes and messages, response items, and the raw `/basket/{userId}` Redis
document — and it still clears the basket when Ordering publishes `OrderStartedIntegrationEvent`.

## Blast-radius safety fact

- **Fact:** Basket.API owns the `/basket/*` Redis keyspace; no other service reads or writes it,
  so the service can be swapped whole as long as the JSON document shape is preserved.
- **Status:** proven
- **Evidence:**
  - `rg -n "/basket/" src/` → only `src/Basket.API/Repositories/RedisBasketRepository.cs`
    (before deletion); `rg -n "AddRedisClient|IConnectionMultiplexer" src/` → only Basket.API.
  - The parity harness diffs the raw Redis document per case, so a shape change fails closed:
    `scripts/parity/basket-transcript.jsonl` carries the exact bytes the .NET service wrote,
    e.g. `{"BuyerId":"parity-user-1","Items":[{"Id":null,"ProductId":1,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null}]}`.
  - After the browser demo, the document the **Rust** service wrote for the signed-in user read
    `{"BuyerId":"3fb863bc-…","Items":[]}` — same shape, decimals still integral.

## Evidence level

**runtime/deploy** — the full Aspire stack ran with the Rust service on the live path, plus
ran-real-tests below.

## Artifact ladder

- [x] **Characterization / unit (harness from `plan.md`)** — `./scripts/check-basket.sh` → exit 0.
      Runs `cargo test --manifest-path native/Cargo.toml --workspace` (17 tests, all green),
      `cargo build --release --bin basket-service --bin basket-parity`, and asserts the recorded
      transcript is present (22 cases). Baseline before any edit:
      `dotnet test tests/Basket.UnitTests/Basket.UnitTests.csproj` → 3/3 passed (that project has
      since been deleted with the .NET service it tested).
- [x] **Rust island + parity** — crate `native/crates/basket` (bins `basket-service`,
      `basket-parity`). `./scripts/parity-basket.sh` → exit 0, `parity: 22/22 cases identical`.
      The harness records from the real .NET service (`--record-only`, run while
      `src/Basket.API` still existed) and replays against the Rust server on throwaway
      Redis/RabbitMQ containers with a stub OIDC provider. Rust is not dead code: Aspire starts
      it as the `basket-api` resource via `builder.AddRustService("basket-api", "basket-service")`.
- [x] **Runtime** — `ESHOP_USE_HTTP_ENDPOINTS=1 dotnet run --project src/eShop.AppHost` with the
      whole stack up:
      - Playwright against the running app: `npx playwright test` → **4 passed** (login setup,
        Browse Items, Add item to the cart, Remove item from cart). Chromium could not be
        downloaded in this environment, so the run used the system Google Chrome via a config
        outside the repo; the specs themselves are unmodified.
      - Browser demo as seeded user `alice`: added two products, changed a quantity
        ($329.98 → $529.97), checked out, order recorded as Paid, basket emptied.
      - Rust service log for that checkout:
        `INFO basket::events: deleted basket after OrderStartedIntegrationEvent user_id=3fb863bc-…`
        — the lapin consumer handled the event Ordering published.
- [x] **Build** — `dotnet build eShop.Web.slnf` → 0 errors after `src/Basket.API`,
      `tests/Basket.UnitTests` and their solution entries were removed and `WebApp.csproj` was
      repointed at `native/crates/basket/proto/basket.proto`.

## Parity

- [x] Characterization existed and passed on the baseline before any structural change
      (`Basket.UnitTests` 3/3, plus the transcript recorded from the running .NET service).
- [x] The same 22 cases pass against the Rust-wired path (`parity: 22/22 cases identical`).
      They cover: anonymous / garbage / expired / wrong-issuer tokens, `Unauthenticated` on
      update and delete, create/replace/empty update, non-positive quantities, duplicate product
      ids, per-user isolation, delete idempotency, and the `OrderStartedIntegrationEvent` path.
- [x] Contract checks: the proto moved with the service and WebApp compiles against it; the Redis
      document is byte-compared in every case; the RabbitMQ exchange is declared non-durable to
      match `EventBusRabbitMQ` (a durable declare gets `PRECONDITION_FAILED`).

## Fix-forward attempts

- Count: 1 (during harness bring-up, before any parity result existed).
- What happened: RabbitMQ kept dying with
  `Error when reading /var/lib/rabbitmq/.erlang.cookie: eacces`. Root cause was the harness's own
  readiness probe — `docker exec rabbitmq-diagnostics` runs as root, whose `HOME` inside the image
  is `/var/lib/rabbitmq`, so Erlang wrote a root-owned cookie the broker (uid 999) could no longer
  read. The probe now waits on the broker's own "Server startup complete" log line and any later
  `docker exec` uses `-u rabbitmq`.
- Parity itself passed on its first run; no corrections were needed to the Rust port.

## Structure encoding

- The readiness bug above is fixed in `scripts/parity-basket.sh` (committed) rather than in
  prose, so the next service migration inherits the working probe.
- `native/.gitignore` re-includes `crates/*/src/bin/`: the repo-wide .NET `.gitignore` excludes
  every `bin/` directory, which would silently drop Cargo binary sources.

## Rollback

- Trigger: `./scripts/parity-basket.sh` failing on mainline, WebApp gRPC errors against
  `basket-api`, or a panic in the Rust service under normal traffic.
- Action: revert this PR. Basket.API was deleted in a single commit on top of an otherwise
  unchanged service, so the revert restores the .NET project, the AppHost `AddProject`
  registration, the solution entries and the WebApp proto path together.

## Verdict

- [x] **Keep / merge** — parity is 22/22 against a transcript recorded from the real .NET
      service, the Rust binary is what Aspire runs for `basket-api`, the full stack shops and
      checks out in a browser, and the Playwright basket specs pass unmodified.
- [ ] Do not merge
- [ ] Inconclusive
