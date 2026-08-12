---
name: Migrate slice to Rust
description: Use when implementing an end-to-end .NET → Rust migration slice for a service — characterize, extract if needed, implement the rules in Rust, wire .NET to call Rust, and prove parity. Not done at a .NET-only extract.
---

# Migrate slice to Rust

Implement an **end-to-end .NET → Rust** migration slice for one service. This is the default implementation skill after **Scope .NET → Rust**.

**Done means Rust is on the runtime path for the slice and parity checks are green.**  
A .NET-only extract (pure module + characterization tests) is an **intermediate step**, not success. Do not stop there unless the user explicitly says to stop before Rust.

## Default demo target

**CatalogItem stock rules** — `RemoveStock` / `AddStock` in `src/Catalog.API/Model/CatalogItem.cs` — unless the user points elsewhere.  
Rust crate for this demo: `native/catalog_stock/` (cdylib preferred so .NET can call it; rlib is fine alongside for `cargo test`).

## When to use

- After **Scope .NET → Rust** names a first harness-backed slice
- When the goal is a **demo-able .NET → Rust** slice with Rust on the live call path
- Before claiming the slice is ready for **Verify Catalog** / **Migration validate**
- Prefer this over **Characterize then extract** unless the user explicitly asks for extract-only

## What “done” looks like (checkable)

Someone reopening the branch must be able to say pass or fail:

- [ ] Characterization tests lock current .NET behavior and were green on baseline
- [ ] Pure rules live in a clear module (extract if still embedded); callers migrated; legacy deleted
- [ ] Same rules implemented in a Rust crate (`native/catalog_stock/` or analogous path)
- [ ] .NET calls that Rust implementation on the runtime path (LibraryImport / PInvoke to cdylib preferred; boundary documented)
- [ ] Parity tests cover the same cases through the **Rust-wired** path and are green
- [ ] `./scripts/check-catalog.sh` is green (script builds/tests Rust when the crate is present)
- [ ] Handed to **Verify Catalog** + **Migration validate** with evidence paths

## Steps

1. **How (current .NET behavior)**  
   State current behavior in **3–5 bullets** (plain English, no long essay). For Catalog stock, cover at least:
   - empty / insufficient stock
   - qty ≤ 0
   - partial fill / exact fill
   - max stock / threshold behavior
   - `OnReorder` (or equivalent flags) as they apply today

2. **Characterization tests on current .NET (green first)**  
   Prefer `tests/Catalog.UnitTests/` if missing; otherwise extend an existing project. Cover the same cases from step 1. Tests should run **without Docker** when possible.  
   **Confirm they pass against current code before any extract or Rust work.** They lock today’s semantics; they do not invent new behavior.

3. **Extract pure module if still embedded**  
   If rules are still buried in entities / handlers:
   - Move pure domain rules to a clear module/type (same assembly is OK for the demo)
   - Migrate callers onto the extracted API
   - Delete the legacy inlined / duplicated path in the same change (no forever shim)
   - Keep characterization tests green after the extract  
   Treat this as scaffolding for the Rust wire-up — **not** the finish line.

4. **Implement the same rules in Rust**  
   Create or extend a crate at `native/catalog_stock/` (Catalog stock demo) or an analogous `native/<slice>/` path.  
   Mirror the characterized rules 1:1. Prefer `[lib] crate-type = ["cdylib", "rlib"]` so .NET can call it and `cargo test` can exercise logic. Keep the crate focused on the pure slice — no drive-by rewrite of the whole service. Run `cargo test` in that crate — must be green.

5. **Wire .NET to call Rust**  
   Prefer `LibraryImport` / PInvoke into the cdylib. Document the chosen boundary in the PR or a short note next to the FFI surface (who owns allocation, error codes, string/buffer rules).  
   **The production/runtime path for the slice must go through Rust** — not a dead parallel implementation that nothing calls. Feature flags are OK only if the demo default path is the Rust-wired one and tests exercise it.

6. **Parity tests through the Rust-wired path**  
   Cover the **same cases** as characterization, but assert behavior when .NET is calling Rust. Fail closed on drift. Do not claim parity from “we looked at the Rust source.”

7. **Run the lever**  
   From repo root:

   ```bash
   ./scripts/check-catalog.sh
   ```

   Must exit 0. When `native/catalog_stock/Cargo.toml` (or `native/*/Cargo.toml`) exists, the script **must** `cargo test` that crate (and `cargo build --release` for a cdylib). Then unit tests / functional fallback as the script documents. Print which path ran.

8. **Hand to Verify Catalog + Migration validate**  
   Hand to **Verify Catalog** + **Migration validate**. Append decision-trail rows (`implement`, then validate as appropriate). Validate for this Catalog inventory slice **requires** Rust implementation + parity evidence — a .NET-only extract is not enough for keep/merge.

## Boundary notes (Catalog stock)

| Piece | Expectation |
|-------|-------------|
| .NET entry | Thin wrapper on `CatalogItem` / extracted module that calls native |
| Rust crate | `native/catalog_stock/` with pure stock rules |
| Preferred link | `LibraryImport` → cdylib |
| Check command | `./scripts/check-catalog.sh` |

## Guardrails

- **Rust on the runtime path is mandatory for “done.”** A .NET-only extract is intermediate, not success.
- Do not treat Rust as optional. Do not stop at extract-only for the demo finish line.
- No behavior change intended vs characterized baseline — parity locks current semantics.
- No fake metrics — exit codes and failing/passing assertions only.
- Keep the diff **demo-small** — one pure rules surface through Rust, not a full service rewrite.
- Do not leave duplicated legacy + extracted + Rust paths all live without a single source of truth on the call path.
- Plain English. No outside glossary required.
