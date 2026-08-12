# Migration validate: Basket.API / whole service → Rust

Ticket: SHIV-21. Plan: [`plan.md`](plan.md). Units 1–8 of the recommended sequence, all landed.

## Claim

The Rust `basket-api` (`native/crates/basket`) reproduces the behavior of the deleted .NET
`src/Basket.API` for its whole surface — the three gRPC RPCs, the Redis document, the JWT identity
rules, and the `OrderStartedIntegrationEvent` consumer — so unmodified callers (`src/WebApp`, the MAUI
`ClientApp`) and previously stored baskets keep working.

## Blast-radius safety fact

- Fact: the Redis `/basket/{userId}` keyspace and its JSON encoding are owned only by Basket.API, so
  matching Basket.API's own serialization is sufficient.
- Status: **proven**
- Evidence:
  - `rg -n "/basket/|BasketKeyPrefix|GetBasketKey" src --glob '!**/bin/**' --glob '!**/obj/**'` matched
    only `src/Basket.API/Repositories/RedisBasketRepository.cs`, and `rg -l "AddRedisClient|IConnectionMultiplexer" src`
    matched only `src/Basket.API` (run on the pre-migration tree).
  - The encoding itself is not assumed: the transcript recorded from the live .NET service captured the
    raw Redis document per case, and the Rust replay reproduces those bytes
    (`./scripts/parity-basket.sh` → `15 cases match`, exit 0).

## Evidence level

**ran-real-tests** plus **runtime** (the Aspire stack was running and driven through a browser).

## Artifact ladder

- [x] Characterization/unit (harness from `plan.md`): `./scripts/check-basket.sh` — exit 0
  (`R:cargo-test-basket` 16 Rust tests, `R:cargo-build-release-basket`,
  `A:dotnet-basket-retired`). Before the cutover the same lever also ran
  `A:Basket.UnitTests` → 15 passed.
- [x] Rust island + parity: crate `native/crates/basket`; `cargo test -p basket` → 16 passed;
  `./scripts/parity-basket.sh` → `basket-parity: 15 cases match`, exit 0. Rust is not dead code — it
  **is** the `basket-api` resource (`AddRustService("basket-api", "basket-service")` in
  `src/eShop.AppHost/Program.cs`); no .NET Basket process exists any more
  (`rg -n "Basket.API" src --glob '*.csproj'` → no matches, `src/Basket.API` deleted).
- [x] Service client-style evidence: `dotnet test --solution eShop.Web.slnf` → 82 passed, 0 failed.
  Playwright (`USERNAME1=alice PASSWORD=… npx playwright test`) → 4 passed, including
  `AddItemTest` and `RemoveItemTest`, which drive the basket through WebApp's gRPC client.
- [x] Optional runtime: ran. `ESHOP_USE_HTTP_ENDPOINTS=1 dotnet run --project src/eShop.AppHost`, then a
  browser walkthrough as seeded user `alice`: add item → quantity 3 → checkout → order `Submitted` →
  bag empty. The Rust service logged
  `basket-api listening (gRPC over h2c)`, `loaded Identity signing keys count=1` (real Identity.API
  JWKS) and `handling OrderStartedIntegrationEvent user_id="5cf73d17-…"`, and Redis held
  `{"BuyerId":"5cf73d17-…","Items":[]}` under `/basket/{sub}` before the order cleared the key.

## Parity

- [x] Characterization tests exist and passed on the .NET baseline before any structural change
      (`tests/Basket.UnitTests`, 3 → 15 tests, green pre-change).
- [x] The same behavior passes against the Rust path. Two independent forms:
      Rust unit tests mirroring each characterization case (`cargo test -p basket`), and a transcript
      recorded from the **running** .NET service (`scripts/parity/basket-dotnet.transcript`, 15 cases
      covering anonymous/invalid/expired/wrong-issuer tokens, create/replace/empty update, delete
      idempotency, per-user isolation, and the integration event) replayed against the running Rust
      service over real gRPC + Redis + RabbitMQ.
- [x] Contract checks: `src/WebApp` compiles its gRPC client from the same
      `native/crates/basket/proto/basket.proto`; `dotnet build eShop.Web.slnf` succeeds and the browser
      run exercises the wire contract end to end.

## Fix-forward attempts

- Count: 0 for the migration itself. Two environment issues were worked around without changing
  migration behavior: the VM's default Rust toolchain was 1.83 (`rustup default stable`), and
  Identity.API failed its first cold boot with `database "identitydb" already exists` — a pre-existing
  EF/Aspire startup race, unrelated to basket, that cleared on restart.

## Structure encoding

- `scripts/check-basket.sh` is the committed Docker-free lever; it fails closed when the Rust crate
  cannot build or test, and it degrades cleanly now that the .NET test project is gone.
- `scripts/parity-basket.sh` + `scripts/parity/basket-dotnet.transcript` keep the deleted .NET
  behavior executable rather than described.
- CI runs both the Rust tests (`pr-validation.yml`) and a release prebuild of the service before
  Playwright (`playwright.yml`), so `basket-api` is covered now that `dotnet test` cannot see it.
- `native/.gitignore` re-includes `crates/*/src/bin/`; the repo-wide .NET `.gitignore` excludes every
  `bin/` directory and would otherwise silently drop Cargo binary sources.

## Rollback

- Trigger: `./scripts/parity-basket.sh` or `./scripts/check-basket.sh` red on mainline; WebApp unable
  to reach `basket-api`; a panic in the Rust service under normal shopping traffic; any basket
  document in Redis that the service cannot read.
- Action: revert this PR. That restores `src/Basket.API`, its AppHost project reference, and the
  WebApp proto path in one step; the Redis document is unchanged by design, so baskets written by the
  Rust service are readable by the restored .NET service.

## Verdict

- [x] **Keep / merge** — the whole scoped service is in Rust and on the live path; unit, parity,
      solution-wide and browser evidence are all green from real commands; the blast-radius fact is
      proven by replaying recorded .NET behavior rather than asserted; no open rollback triggers.
- [ ] Do not merge
- [ ] Inconclusive
