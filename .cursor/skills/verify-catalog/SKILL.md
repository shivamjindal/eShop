---
name: Verify Catalog
description: Use when proving Catalog.API still behaves after a migration slice — drive the service the way a client would and capture evidence. When Rust wiring is present, verification must exercise that path (or say inconclusive).
---

# Verify Catalog

Project-local verification skill for **eShop Catalog.API**. Written for cold agents who have never seen the app. Goal: prove Catalog still behaves after a migration slice by running real tests/commands and capturing exit codes — no fake metrics.

You do **not** need any external verification framework. Everything you need is in this file and this repo.

## When to use

- After a Catalog-related migration slice (**Migrate slice to Rust**, extract, adapter change)
- When **Migration validate** requires the Verify Catalog rung of the artifact ladder
- Before claiming **keep/merge** on Catalog surface area

## How Catalog runs in this repo (discover, don’t invent)

| Path | What it is |
|------|------------|
| `src/Catalog.API/` | Catalog service (Kestrel API, EF + Postgres/pgvector, domain in `Model/`) |
| `src/eShop.AppHost/` | .NET Aspire AppHost that composes Catalog + dependencies |
| `tests/Catalog.FunctionalTests/` | HTTP functional tests via `WebApplicationFactory` + Aspire Postgres test resource |
| `tests/Catalog.UnitTests/` | Characterization / unit tests (may be added by the slice; prefer when present) |
| `native/catalog_stock/` | Rust crate for Catalog stock rules when the E2E slice is present |
| `tests/README.md` | States functional tests need Docker (Aspire test containers) |
| `eShop.slnx` | Solution entry |

Domain rules historically live on `CatalogItem` in `src/Catalog.API/Model/CatalogItem.cs` (`RemoveStock` / `AddStock`, etc.). An E2E migration slice moves those rules into Rust and wires .NET to call them.

## Rust wiring rule (mandatory when present)

**If Rust wiring is present** (`native/*/Cargo.toml`, LibraryImport/PInvoke, or equivalent):

- Verification **must exercise the Rust-wired path** (via `./scripts/check-catalog.sh`, parity/unit tests that go through the native boundary, or a documented probe that hits that path).
- If you cannot exercise that path (missing native toolchain, library not built, tests bypass Rust): result is **inconclusive** (or red) — **not green**. Say so explicitly for Migration validate.
- Do **not** claim green verification from .NET-only tests that never call Rust when the slice’s success criteria require Rust on path.

## Preferred evidence (in order)

0. **`./scripts/check-catalog.sh`** — **first command when present**. When a Rust crate exists under `native/`, the script runs `cargo test` (and release build for cdylib) **before** .NET tests, and prints which path ran. Capture `path=` + exit code as primary evidence.
1. **Parity / unit tests through the Rust-wired path** — fastest honest check when the E2E slice landed.  
2. **`tests/Catalog.FunctionalTests`** — drives Catalog over HTTP. Requires Docker (Aspire Postgres). Escalate when HTTP/hosting contracts are in blast radius; still insufficient alone if Rust wiring exists but was never exercised.  
3. **Full Aspire AppHost** — only if already running; do not stand up the whole estate just for smoke.

## Steps

1. **Resolve what the slice touched**  
   List changed paths under `src/Catalog.API/`, `native/`, and any new/updated test projects. Note whether HTTP surface, pure domain, Rust FFI, or both are in blast radius. Explicitly record: **Rust wiring present? yes/no**.

2. **Prefer existing harnesses**  
   Do **not** invent a new test host, docker-compose, or mock Catalog if `check-catalog.sh`, parity/unit tests, or `Catalog.FunctionalTests` already cover the claim. Read existing fixtures before adding anything.

3. **Run the lightest honest smoke**

   **0. Lever (preferred when present)** — from repo root:

   ```bash
   ./scripts/check-catalog.sh
   ```

   Record the script’s path line and `exit_code=`. When Rust is present, confirm the script reported a Rust/`cargo` path. If exit 0 and Rust was exercised (or no Rust wiring exists), you have Catalog smoke evidence. If Rust wiring exists but the script did not run cargo / build native, treat verification as **inconclusive** until that is fixed.

   **A. Parity / unit through Rust (no Docker)**  
   Prefer tests that call the .NET API which loads the native library:

   ```bash
   # Example — replace with the actual project the slice introduced:
   dotnet test tests/Catalog.UnitTests/Catalog.UnitTests.csproj
   ```

   Also expect the lever to have run:

   ```bash
   cargo test
   # and for cdylib:
   cargo build --release
   ```

   inside `native/catalog_stock/` (or analogous).

   If Rust wiring is present but tests only hit a pure .NET fallback and never load the crate: **inconclusive** (or red) for E2E verification — do not mark green.

   **B. Functional HTTP smoke (Docker + Aspire test host required)**  
   From repo root, with Docker running:

   ```bash
   dotnet test tests/Catalog.FunctionalTests/Catalog.FunctionalTests.csproj
   ```

   Escalate to B when the slice touches HTTP/API contracts, EF mappings, or Program hosting — or when A is unavailable **and** Rust is not required for this particular change. If Rust wiring is present, B does not replace A/0 unless you prove the functional path also exercises Rust.

   **C. Optional runtime (only if already up)**  
   If AppHost is already running locally, probe a client-style endpoint and record HTTP status. Skip if AppHost is not running.

4. **Observe — real signals only**  
   - Exit codes from `./scripts/check-catalog.sh`, `cargo test`, `dotnet test`  
   - Failed assertion names / messages  
   - Whether the Rust library was actually loaded / exercised  
   - HTTP status codes if probing a live host  
   Do **not** invent latency, coverage %, or “parity scores.”

5. **Checkable definition of done for this skill**  
   This skill is done when:
   - Exact commands run are listed  
   - Each command’s exit code (or skip + reason) is recorded  
   - Rust-wired path exercised **or** explicitly marked inconclusive / not applicable  
   - Evidence paths are noted  
   - Result handed to **Migration validate**: green | red | inconclusive | skipped-with-reason

## Output template

```markdown
# Verify Catalog

## Slice context
- Touched: ...
- Rust wiring present: yes | no
- Rust path exercised: yes | no | n/a

## Commands
- [ ] `./scripts/check-catalog.sh` — path: ... — exit: ... — evidence: ...
- [ ] Rust/cargo: `{command}` — exit: ... — evidence: ...
- [ ] Parity/unit through Rust-wired path: `{command}` — exit: ... — evidence: ...
- [ ] Functional tests: ran | skipped (reason: ...) — exit: ... — evidence: ...
- [ ] Runtime probe: ran | N/A — result: ...

## Observations
- Key assertions / HTTP statuses / native load notes: ...

## Result for Migration validate
green | red | inconclusive | skipped-with-reason
```

## Guardrails

- Cold-agent friendly: use only paths and commands that exist in this repo (or that the slice just added).  
- **When Rust wiring is present, verification must exercise that path or say inconclusive.**  
- Functional tests **require Docker**; if Docker/Aspire is unavailable, say so and fall back to A/0 or skip with reason — do not pretend they passed.  
- Prefer `./scripts/check-catalog.sh` when present; it must build/test Rust when the crate exists.  
- No fake metrics. Exit codes and assertion failures are the evidence.  
- Hand result back to **Migration validate**; append trail rows there (or via **Migration decision trail**), not as a substitute for the gate.
