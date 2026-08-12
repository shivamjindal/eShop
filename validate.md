# Migration validate: Catalog.API / stock (RemoveStock · AddStock)

## Claim
Catalog.API stock rules (`RemoveStock` / `AddStock`) match pre-migration behavior for the characterized cases when the service uses the Rust-wired path (`CatalogItem` → `CatalogStock` → `libcatalog` cdylib).

## Blast-radius safety fact
- Fact: Stock mutation rules are I/O-free pure functions (in-memory field updates + `CatalogDomainException` only); safe to port without EF/RabbitMQ coupling for this island.
- Status: **proven**
- Evidence: `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → exit **0** (re-run this validation); path `A:Catalog.UnitTests` 8/8 against live Rust-wired `CatalogItem` API; `cargo test -p catalog` stock module 8/8. Live caller remains `OrderStatusChangedToPaidIntegrationEventHandler` → `CatalogItem.RemoveStock` (no direct I/O inside stock math).

## Evidence level
**ran-real-tests**  
(floor for keep/merge: ran-real-tests)

Also pointed-at-code for the live boundary:
- `src/Catalog.API/Model/CatalogItem.cs` → `CatalogStock`
- `src/Catalog.API/Model/CatalogStock.cs` → `LibraryImport` `catalog_stock_remove` / `catalog_stock_add`
- `native/crates/catalog/src/{stock,ffi}.rs`
- `artifacts/bin/Catalog.API/debug/libcatalog.dylib` present after host build

## Artifact ladder
- [x] Characterization/unit (harness from plan.md): `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` — result: **exit 0** — evidence:
  - `R:cargo-test-workspace` exit 0 (catalog stock 8 tests ok)
  - `R:cargo-build-release-workspace` exit 0
  - `A:Catalog.UnitTests` exit 0 — total 8, failed 0, succeeded 8
- [x] Rust island + parity (migrated units): crate path: `native/crates/catalog` — `cargo test` via harness + `crate-type = ["cdylib", "rlib"]` — wired from .NET: **yes** (`LibraryImport` + MSBuild `BuildCatalogNative` / `scripts/build-catalog-native.sh`) — evidence: harness log above; dylib at `native/target/release/libcatalog.dylib` and host/test outputs
- [x] Service client-style evidence: `dotnet test --project tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj` — result: **exit 2** (30 total: 28 passed, 2 failed) — evidence: agent-tools log `5859b722-b896-4f17-9c00-01c09b996f1f.txt`
  - Failures: `GetCatalogItemsRespectsPageSize` v1/v2 — Expected count **103**, Actual **101** (fixture assumes AddCatalogItem already ran; order/parallel race on shared Aspire DB). **Does not exercise `RemoveStock`/`AddStock` FFI.**
  - Update paths that mutate `AvailableStock` via HTTP PUT (EF `SetValues`, not stock island) were among the 28 passes.
  - Transient Npgsql stream errors during fixture warmup observed in log; suite continued.
- [ ] Optional runtime: **N/A** — Aspire AppHost not left running for dual-run/canary; functional fixture covers host+DB briefly.

## Parity
- [x] Characterization tests exist and pass on baseline (locked before structural change in migrate run; 8 cases)
- [x] Same tests pass after unit (Rust-wired path) — harness `A:Catalog.UnitTests` 8/8
- [x] Contract/API checks (if applicable) — no RabbitMQ/HTTP contract change in this unit; event payloads untouched. Functional HTTP surface mostly green; page-count flake unrelated to stock.

## Fix-forward attempts (if any)
- Count: **0**
- Stopped because: **N/A** (no corrections during this validate gate)
- Notes for human (if capped): n/a

## Structure encoding (if recurring failure)
- Xcode license / `/usr/bin/cc` link failure on macOS already encoded into `scripts/check-catalog.sh`, `scripts/check-native.sh`, and `scripts/build-catalog-native.sh` (CLT clang + `SDKROOT`).
- Functional `GetCatalogItemsRespectsPageSize` expecting 103 is order-dependent on `AddCatalogItem`; recommend hardening that test (isolate seed count or remove cross-test coupling) outside this unit — **not** a stock rollback trigger.

## Rollback
- Trigger:
  - `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` non-zero on mainline CI
  - Characterization / Rust stock tests fail or diverge
  - Native load failure at runtime (`DllNotFoundException` / missing `libcatalog.*` beside host)
  - Wrong stock exception semantics or inventory drift under paid-order path
- Action:
  1. Revert the stock unit PR (or restore prior `CatalogItem` method bodies and drop `CatalogStock` / cdylib wire).
  2. Keep `tests/Catalog.UnitTests` characterization if still green against restored .NET so future ports stay locked.
  3. No feature flag today — rollback = code revert.

## Verdict
- [x] **Keep / merge** — evidence: harness exit 0; safety fact proven by real commands; Rust cdylib on live path from `CatalogItem`/`paid` handler; parity 8/8 .NET + 8/8 Rust; functional failures are unrelated page-count race (documented), not stock semantics.
- [ ] Do not merge — blockers: …
- [ ] Inconclusive — missing evidence: …

(Inconclusive ≠ keep/merge)

### Scope note
Keep/merge applies to the **stock island only**. Remaining Catalog.API units (confirmation decision, picture helpers, etc.) stay open per `plan.md`.
