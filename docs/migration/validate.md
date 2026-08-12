# Migration validate: Catalog stock rules (`RemoveStock` / `AddStock`)

| Field | Value |
|-------|--------|
| Baseline | .NET `CatalogItem.RemoveStock` / `AddStock` semantics (pre-port, characterized) |
| After | `native/catalog_stock` + `CatalogStockNative` (`LibraryImport`) on live path |
| Unit | Stock rules island (plan unit 1 / SHIV-20 first vertical) |
| Repo | `/Users/shivam/repos/eshop` |
| Date | 2026-08-11 |

## Claim

Catalog stock rules match pre-migration behavior for the characterized cases when Catalog.API uses the Rust-wired path (`CatalogItem` → `CatalogStockNative` → `libcatalog_stock`).

## Blast-radius safety fact

- **Fact:** Stock mutation rules remain I/O-free in-memory operations (no EF/HTTP/SaveChanges inside the rule path); persistence stays outside in the paid handler.
- **Status:** proven
- **Evidence:**
  - `rg -n "async|Await|DbContext|Http|File\.|SaveChanges|ILogger" src/Catalog.API/Model/CatalogItem.cs` → no matches (stock wrappers only call native + throw `CatalogDomainException`).
  - Sole production caller still `OrderStatusChangedToPaidIntegrationEventHandler` → `RemoveStock` then `SaveChangesAsync` (persistence outside rules).
  - Characterization suite exercises mutations without Docker/DB: `dotnet test --project tests/Catalog.UnitTests` via harness → exit 0.
  - Note: `CatalogStockNative` uses `File.Exists` only to locate the cdylib for load — not domain I/O.

## Evidence level

**ran-real-tests** (floor for keep/merge met)

Also pointed-at-code: `CatalogItem.cs` → `CatalogStockNative.cs` → `native/catalog_stock/src/lib.rs`.

## Artifact ladder

- [x] Characterization/unit (`./scripts/check-catalog.sh`):  
  - Command: `./scripts/check-catalog.sh`  
  - Result: **exit 0**  
    - `R:cargo-test` exit 0 (8 passed)  
    - `R:cargo-build-release` exit 0  
    - `A:Catalog.UnitTests` exit 0 (8 passed, 0 failed)  
  - Strict: `MIGRATION_REQUIRE_RUST=1 ./scripts/check-catalog.sh` → **exit 0**  
  - Evidence: command output in this validation run (2026-08-11)

- [x] Rust island + parity (migrated unit):  
  - Crate: `native/catalog_stock` (`cdylib` + `rlib`)  
  - Wired from .NET: **yes** — `CatalogStockNative.catalog_stock_remove` / `catalog_stock_add` via `LibraryImport`; called from `CatalogItem.RemoveStock` / `AddStock`  
  - Native artifact present beside test host: `tests/Catalog.UnitTests/bin/Debug/net10.0/libcatalog_stock.dylib`  
  - Parity: same 8 cases in `tests/Catalog.UnitTests/CatalogItemStockTests.cs` and `native/catalog_stock` `#[cfg(test)]` (empty / non-positive / full remove / partial / add under max / cap / at-max / reorder clear)

- [x] Optional runtime: **N/A** — Aspire AppHost / Docker functional path not required for this pure-domain unit; harness took path A (unit tests). No waiver needed for keep/merge of stock rules island.

## Parity

- [x] Characterization tests exist (`tests/Catalog.UnitTests/CatalogItemStockTests.cs`) and passed on baseline before port (prior migrate-to-rust run)
- [x] Same tests pass after unit on Rust-wired path (`A:Catalog.UnitTests` exit 0)
- [x] Contract/API: HTTP/event contracts unchanged; paid handler still calls `RemoveStock` then EF save — no Ordering payload shape change in this unit

## Fix-forward attempts (if any)

- Count: **0**
- Stopped because: N/A (checks green on first validation pass)

## Structure encoding (if recurring failure)

- Xcode license blocking `cc` appeared during migration; encoded into `scripts/check-catalog.sh` (auto `DEVELOPER_DIR=/Library/Developer/CommandLineTools`) and `CatalogStock.Native.targets` — not left as prompt-only knowledge.

## Rollback

- **Trigger:** Characterization or `cargo test` / `./scripts/check-catalog.sh` failing on mainline CI; native library load failure at runtime under normal inputs; stock behavior drift vs characterized cases.
- **Action:** Revert the PR (or restore prior `CatalogItem` stock method bodies and drop `CatalogStockNative` + crate wire); keep characterization tests as the regression net. No feature flag present — revert is the rollback.

## Verdict

- [x] **Keep / merge** — evidence: `./scripts/check-catalog.sh` exit 0; `MIGRATION_REQUIRE_RUST=1` exit 0; Rust crate tested; `LibraryImport` on live path; blast-radius fact proven by inspection + green unit suite; 0 fix attempts; no open rollback triggers observed in this run.
- [ ] Do not merge
- [ ] Inconclusive

**Scope note:** This verdict covers **plan unit 1 (stock rules island) only**. Units 2–8 remain open per `docs/migration/plan.md`.
