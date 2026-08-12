# Migration validate: Basket.API → Rust (SHIV-21)

Unit under validation: the whole Basket.API service (plan.md units 1–7). Baseline: `src/Basket.API`
on `main`. After: `native/basket_service` registered as the Aspire `basket-api` resource.

## Claim

The Rust basket service is observably identical to the .NET Basket.API it replaces: same gRPC
contract and status codes, same identity rules, the same bytes in Redis, and the same reaction to
`OrderStartedIntegrationEvent` — and the eShop store works end to end against it.

## Blast-radius safety fact

- **Fact:** the Redis keyspace `/basket/{userId}` is owned exclusively by the basket service; no
  other service in the repo opens a Redis connection.
- **Status:** proven
- **Evidence (commands run):**
  - Before the migration: `grep -rn "AddRedisClient\|IConnectionMultiplexer" --include=*.cs src/`
    matched only `src/Basket.API/Extensions/Extensions.cs:14` and
    `src/Basket.API/Repositories/RedisBasketRepository.cs:6`.
  - After the migration: the same grep over `src/` and `tests/` (extended with
    `StackExchange.Redis` and `*.csproj`) exits 1 — no .NET project talks to Redis at all.
  - At runtime: scanning the live Redis instance while the store was exercised showed only
    `/basket/{sub}` keys, written by the Rust process (`target/release/basket-service`).

## Evidence level

**ran-real-tests + runtime** — every claim below comes from a command with a recorded exit code,
plus a live Aspire run of the full application.

## Artifact ladder

| Step | Command | Result |
|------|---------|--------|
| Characterization on the baseline | `dotnet test tests/Basket.UnitTests/Basket.UnitTests.csproj` | 16/16 passed against the .NET service before any port (exit 0) |
| Rust unit suite | `cargo test --manifest-path native/basket_service/Cargo.toml` | 34/34 passed (exit 0) |
| Release build | `cargo build --release --manifest-path native/basket_service/Cargo.toml` | exit 0 |
| Lever | `./scripts/check-basket.sh` | exit 0 (cargo test → release build → parity replay) |
| Dual-run parity | `./scripts/parity-basket.sh --dual` | 47/47 observations identical; transcript committed at `scripts/parity/basket-dotnet.transcript` |
| Parity replay after the .NET service was deleted | `./scripts/parity-basket.sh` | `PASS — Rust matches the recorded .NET transcript (47 observations)` (exit 0) |
| Solution build | `dotnet build eShop.Web.slnf` | exit 0 (`eShop.slnx` additionally needs the MAUI workloads, unrelated to this change) |
| Runtime, whole app | `ESHOP_USE_HTTP_ENDPOINTS=1 dotnet run --project src/eShop.AppHost` | all resources start; `basket-api` runs `cargo run --release --quiet --bin basket-service` |
| Runtime, browser | `USERNAME1=alice PASSWORD=… npx playwright test` | 4/4 passed (browse, login, add item to cart, remove item from cart) |
| Runtime, manual | recorded walkthrough | add to bag → quantity 3 → checkout → order submitted → basket cleared by the integration event |

## Parity

- [x] Characterization tests existed and passed on the baseline (16 MSTest cases covering all three
      RPCs, the identity gate, the `NotFound` path and the exact Redis JSON bytes).
- [x] The same behavior passes against the Rust path. The 47-observation transcript covers
      anonymous / valid / garbage / expired / wrong-issuer tokens, item mapping, empty and negative
      quantities, replace-basket semantics, delete semantics and the `OrderStartedIntegrationEvent`
      flow, comparing both the gRPC outcome and the stored Redis bytes.
- [x] Contract check: `basket.proto` is byte-identical to the file the .NET service served; the
      WebApp gRPC client compiles against it unchanged and resolves the same `http://basket-api`
      service-discovery name.

## Fix-forward attempts

- Count: 2 (both while bringing the harness up, not behavioral drift)
  1. `jsonwebtoken` 11 needs an explicit crypto provider feature — enabled `aws_lc_rs`.
  2. The parity harness raced RabbitMQ startup — it now waits for the broker's
     "Server startup complete" log line.
- Stopped because: success.

## Structure encoding

- `scripts/check-basket.sh` is the fail-closed lever (Rust tests + release build + parity replay);
  `MIGRATION_REQUIRE_RUST=1` makes a missing Rust crate an error.
- `scripts/parity/basket-dotnet.transcript` keeps the .NET behavior executable after the .NET
  project is gone, so drift in the Rust service fails a command rather than being noticed in
  production.
- CI runs `cargo test` on pull requests and prebuilds the binary before the Playwright job.

## Rollback

- **Triggers:** `./scripts/check-basket.sh` failing on mainline; the Playwright cart tests failing;
  a panic or 5xx/`Unknown` status from `basket-api` under normal traffic; baskets not clearing
  after checkout.
- **Action:** revert this PR. Nothing outside `basket-api` changed shape — the proto, the Redis key
  format, the queue name and the Aspire resource name are all unchanged — so the previous .NET
  service can be restored and started against the same Redis and RabbitMQ without data migration.

## Verdict

- [x] **Keep / merge** — the claim holds at the "ran real tests + runtime" evidence level: Rust is
      the only implementation on the path, the parity transcript matches on all 47 observations,
      the lever and the browser end-to-end tests are green, and the blast-radius fact is proven by
      commands rather than prose.
- [ ] Do not merge
- [ ] Inconclusive
