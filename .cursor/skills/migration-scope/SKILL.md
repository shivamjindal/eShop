---
name: Scope .NET → Rust
description: Use when scoping a .NET → Rust migration for one service the user points at (or named in a ticket). Inventory, blast radius, checkable done definition, safety fact, first harness-backed slice that ends in a Rust island + wire + parity.
---

# Scope .NET → Rust

Playbook for scoping a **.NET → Rust** brownfield migration slice for **one service**. Tickets already describe the work — the user points at a service; you scope it. Do not ask them to fill a source/target parameter table.

**Fixed direction:** always migrate **from .NET toward Rust**. The only input is which service.

## Input

The **service** (or project) the user points at — folder, project name, ticket link, or @ mention.

If unclear, infer once from the open ticket / chat / workspace context (e.g. Catalog.API in eShop). Do **not** interrogate for stack parameters, entrypoints, or repo metadata. If still ambiguous after one inference pass, ask only: *which .NET service?*

Optional context (take if offered, don’t block on it): constraints (latency, dual-run, regulatory), ticket system, Aspire/hosting notes.

## When to use

- User (or ticket) names a .NET service to migrate toward Rust
- Need inventory → blast radius → sequence → first safe slice before production edits
- Demo or engagement where the stack is already .NET → Rust

## Ideas this skill expects you to follow (explained here — no outside glossary)

- **Checkable definition of done** — Before you finish scoping, write concrete pass/fail bullets for *this run*. Someone reopening the artifact must be able to say pass or fail. No vibes. **Must include** a required unit that lands a Rust implementation of the first pure domain island, a .NET→Rust boundary, and parity checks — not “extract only” and not “optional later spike.”
- **Safety fact** — Name the *one* fact the first slice depends on. Mark it **proven** only if you ran code/tests/commands; otherwise **unproven**. Design docs and narration do not count.
- **Small units that each end green** — Sequence work as change → run a check command → green, then the next unit. Never one big-bang rewrite as the first move. Units that stop after a .NET extract are incomplete for this demo.
- **Harness / tests before structural change** — Prefer a characterization test suite or script that fails closed on drift *before* you restructure, extract, or port.

## Steps

1. **Resolve service + checkable definition of done**  
   Name the service (`SERVICE`). Success = velocity of *safe* slices that actually cross into Rust, not “rewrite the estate” and not “stop after C# extract.”  
   Write a **definition of done** for *this scoping run* — checkable, e.g.:
   - `plan.md` names inbound/outbound edges for `SERVICE` with local vs cross-cutting labels
   - Recommended sequence is ≥3 verifiable units, each ending in a green check, and **includes required units** for: (a) Rust implementation of the first pure domain island, (b) .NET→Rust wiring for that island, (c) parity harness green
   - First slice lists a harness command that can fail before mass edits (`./scripts/check-catalog.sh` for Catalog)  
   Someone must be able to reopen the artifact and say pass/fail. No vibes.

2. **Inventory `SERVICE` (.NET-aware)**  
   - Assemblies / projects under the service: API, Domain, Infrastructure, tests (`.csproj`, solution filters).  
   - TFM(s), SDK-style vs legacy, NuGet / Central Package Management, build entrypoints.  
   - Hosting & boundaries: Aspire AppHost, Kestrel endpoints, gRPC, workers, message consumers.  
   - Public surface: HTTP/gRPC routes, events, CLI/batch.  
   - Domain core vs adapters (EF/DB, brokers, auth, third-party).  
   - Note any existing `native/` Rust crates (e.g. `native/catalog_stock`) if present.  
   Write under **Inventory** in `plan.md`.

3. **Map dependencies & blast radius — one safety fact**  
   - Inbound: who calls `SERVICE` (other services, UI, jobs, Aspire references).  
   - Outbound: DB schemas, brokers, shared .NET libs/contracts.  
   - Shared types that would force multi-service changes.  
   - Test hotspots and untested critical paths (`dotnet test` projects).  
   Label each edge: *local* vs *cross-cutting*.  
   **Safety fact:** the *one* fact the first slice depends on (e.g. “Catalog stock rules are I/O-free”, “endpoint Y has characterization coverage”, “schema Z owned only by SERVICE”).  
   Mark **proven** (you ran code/tests/commands) or **unproven** (assumption only). Design docs don’t count. If unproven, the first unit must prove it or the slice is blocked.  
   Write under **Dependencies / blast radius**.

4. **Propose .NET → Rust sequence as verifiable units**  
   Prefer vertical slices that compile, test, and ship independently. Each unit: *change → check (command) → green* before the next.  
   **Required progression for the first demo-able slice** (adapt names; do not drop the Rust units):
   1. Characterize current .NET behavior (`dotnet test` / characterization suite).  
   2. Extract pure .NET domain rules if still embedded (migrate callers, delete legacy) — keep tests green.  
   3. **Rust port of those rules** (crate under `native/catalog_stock` or equivalent) — `cargo test` green.  
   4. **Wire `SERVICE` to call Rust** for that island (preferred: P/Invoke / `LibraryImport` to a `cdylib`; acceptable for parity proof: Rust CLI if FFI is too heavy — but prefer a real library call from the .NET wrapper). Rust must not remain dead code.  
   5. **Parity harness** — same characterization cases pass against the Rust path (and/or the .NET wrapper that delegates to Rust).  
   Avoid big-bang .NET→Rust rewrite of all of `SERVICE` as the first move — but **do not** treat the Rust port as optional or “later spike.”  
   Write under **Recommended sequence**.

5. **Call out risks**  
   For each major risk: trigger, impact, mitigation, detection (test/metric/alert).  
   Themes to adapt (no fake numbers): behavioral drift, shared-kernel / NuGet coupling, serialization & TFM quirks, missing characterization tests, Rust ops/runtime skill gap, FFI/ABI or process-boundary deploy complexity, parity gaps between .NET and Rust.  
   Write under **Risks**.

6. **Pick the first demo-able slice — end-to-end to Rust**  
   Smallest slice that:
   - is mostly pure or well-bounded in .NET,
   - can gain characterization tests *before* behavior changes,
   - proves scope → characterize/extract → **Rust port → wire → parity** → validate,
   - does **not** require rewriting all of `SERVICE` in Rust,  
   - is **not** “extract only” — extract (if needed) is a stepping stone to the Rust island.  
   **Harness before change:** write or identify a characterization harness / script that fails closed on drift, *then* do structural edit, Rust port, and wiring.  
   Example (Catalog.API): characterize `RemoveStock` / `AddStock` → extract `CatalogStock` if needed → implement rules in `native/catalog_stock` → wire Catalog.API / CatalogStock wrapper to call Rust → green parity + `./scripts/check-catalog.sh`.  
   Write under **First demo-able slice** with acceptance checks, harness command, suggested ticket titles.

7. **Emit artifact**  
   Write `plan.md` at the workspace/migration root (or path the user specifies). Actionable for agent or human. No fake metrics. Re-check the definition of done before calling the run complete — including the required Rust + wire + parity units.

## Output: `plan.md` template

```markdown
# .NET → Rust migration scope: {SERVICE}

## Definition of done
- [ ] ...
- [ ] Recommended sequence includes required Rust implementation + .NET→Rust wire + parity units
- How to check pass/fail: ...

## Inventory
- Assemblies / csproj / TFM: ...
- NuGet / build: ...
- Hosting (Aspire/Kestrel/workers): ...
- Public surface: ...
- Domain vs adapters: ...
- Existing Rust crates (`native/`): ...

## Dependencies / blast radius
- Inbound: ...
- Outbound: ...
- Cross-cutting: ...
- **Safety fact:** ...
  - Status: proven | unproven
  - Evidence: {command/result} or “assumption only”

## Recommended sequence
1. Unit: characterize current .NET behavior → check: `{dotnet test ...}` → green means: ...
2. Unit: extract pure rules if embedded (migrate callers, delete legacy) → check: `{command}` → green means: ...
3. Unit: **Rust port of those rules** (`native/...`) → check: `cargo test` → green means: ...
4. Unit: **wire SERVICE to call Rust** for the island → check: `{build + smoke}` → green means: Rust is on the live path (not dead code)
5. Unit: **parity harness** (same cases vs Rust path / delegating wrapper) → check: `./scripts/check-catalog.sh` → green means: ...

## Risks
| Risk | Impact | Mitigation | Detection |
|------|--------|------------|-----------|
| ... | ... | ... | ... |

## First demo-able slice
- Scope: characterize → extract (if needed) → Rust port → wire → parity (NOT extract-only)
- Why first: ...
- **First unit harness:** `{command or path}` — run before mass edits; fails closed on drift
- Rust crate path (proposed): `native/catalog_stock` (or ...)
- Boundary: P/Invoke LibraryImport to cdylib (preferred) | Rust CLI for parity (acceptable if FFI too heavy)
- Acceptance: Rust on the path; characterization cases green against Rust-wired path; `./scripts/check-catalog.sh` exit 0
- Suggested tickets: ...
```

## Guardrails

- Direction is fixed: **.NET → Rust**. Input is the **service**, not a parameter table.  
- Catalog.API / eShop are examples only — any .NET service the user points at.  
- The first demo-able slice **must** land a Rust implementation + wiring + parity for the island. Extract-only is incomplete.  
- Prefer a **lever** (harness/script) before mass edits.  
- Prefer executable artifacts over slideware.  
- No fabricated LOC / % coverage / velocity — cite only what you measured.  
- Unproven safety facts block the first slice until proven or explicitly waived with owner + reason.
