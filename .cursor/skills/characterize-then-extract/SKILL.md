---
name: Characterize then extract
description: Use when implementing the first .NET→Rust-friendly migration slice for a service — write failing/characterization tests first, extract pure domain rules, delete duplicated legacy logic in the same change, then run the check-catalog lever.
---

# Characterize then extract

Implement the first **.NET → Rust-friendly** migration slice for a service: characterize current behavior with tests, extract pure domain rules, migrate callers then delete legacy in the same change, and green the check-catalog lever.

Approach: write tests that lock current behavior first; extract the pure rules; update all callers and delete the old duplicated logic in the same change; use `scripts/check-catalog.sh` as the shared check.

## Default demo target

**CatalogItem stock rules** — `RemoveStock` / `AddStock` in `src/Catalog.API/Model/CatalogItem.cs` — unless the user points elsewhere.

## When to use

- After **Scope .NET → Rust** names a first harness-backed slice (pure domain rules + tests)
- Before claiming the slice is implemented for **Verify Catalog** / **Migration validate**
- When you need a small, demo-honest extract without a full Rust port

## Steps

1. **Brief `how`**  
   State current behavior in **3–5 bullets** (no long essay). Cover empty stock, qty ≤ 0, partial fill, max threshold, `OnReorder` as they apply to the target.

2. **Add characterization tests that lock CURRENT behavior**  
   Prefer adding `tests/Catalog.UnitTests/` if missing; otherwise extend an existing project. Cover at least: empty stock, qty ≤ 0, partial fill, max threshold, `OnReorder`. Tests must be runnable **without Docker** when possible.

3. **Run tests on baseline BEFORE extract**  
   Confirm characterization tests **pass against current code** (they characterize, they do not drive a greenfield rewrite).

4. **Extract pure rules**  
   Move pure domain rules to a clear module/type (same assembly is OK for the demo). `CatalogItem` becomes a thin wrapper **or** callers use the extracted API.

5. **Migrate callers then delete legacy**  
   No forever shim / duplicated logic left behind in the same PR. Callers move; old inlined/duplicated path is deleted in this change.

6. **Run the lever**  
   From repo root:

   ```bash
   ./scripts/check-catalog.sh
   ```

   Must be green (exit 0).

7. **Hand off**  
   Hand off to **Verify Catalog** + **Migration validate**; append decision-trail rows (`implement`, then validate as appropriate).

## Guardrails

- No behavior change intended — characterization locks current semantics.
- No fake metrics — exit codes and failing/passing assertions only.
- Keep the diff **demo-small** — extract one pure rules surface, not a full service rewrite.
- Do not leave duplicated legacy + extracted logic side by side after the PR.
