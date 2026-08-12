---
name: Migration validate
description: Use when validating that a migration slice worked (typically after a .NET→Rust scoped slice) — rank evidence honestly, prove blast-radius safety by running code, require Rust implementation + parity for Catalog inventory slices, Verify Catalog, and decide keep/merge vs do-not-merge vs inconclusive, with a decisions.tsv row.
---

# Migration validate

Gate after a migration slice. Prove the slice is safe enough to **keep and merge** (or roll back). Parameterize every run; no fake metrics. When fixing failures while validating, stop after **3** correction attempts — this skill remains the gate, not an infinite fix loop.

For the **Catalog inventory / stock rules** demo slice (and analogous pure-rule slices scoped for E2E Rust): **keep/merge requires Rust implementation + parity evidence**, not only a .NET extract. A .NET-only extract without Rust on the runtime path is **do not merge** for the E2E demo finish line (unless the user explicitly waived Rust with owner + reason).

## What you need for this run

| Item | Meaning | Examples |
|------|---------|----------|
| Baseline | Pre-slice stack / behavior you must not regress | `.NET` Catalog domain before extract/Rust |
| After-slice | Implementation under validation | Rust-wired path (LibraryImport/PInvoke → `native/catalog_stock/`), not extract-only |
| Slice | Concrete change under validation | CatalogItem stock rules through Rust + parity |
| Repo | Codebase path | local checkout |

Optional: CI job names, test commands, dual-run flag, rollback owner, trail path (default `migrations/decisions.tsv`).

## When to use

- After implementing a scoped migration slice via **Migrate slice to Rust**
- Before merging / demoing “slice done”
- When a team needs a repeatable keep-or-not gate
- When validating alongside iterative agent fixes (enforce the 3-attempt cap)

## Ideas this skill expects you to follow (explained here — no outside glossary)

- **Evidence ranking** — How strong is the claim?
  1. Self-report (“it works”) — weakest; never enough alone to keep/merge  
  2. Pointed at code — cited paths/diffs that *should* imply correctness  
  3. Ran real tests/commands — exit codes and output (**required floor to keep/merge**)  
  4. Optional runtime/deploy — dual-run, canary, or staging probe when warranted  
- **Safety fact proven by running code** — Restate the one blast-radius fact; prove it with a command, not a writeup.  
- **Rust + parity for Catalog inventory slice** — Keep/merge for this demo slice needs: Rust crate present, .NET wired to call it, parity cases green through that path, and `./scripts/check-catalog.sh` green (or equivalent with evidence).  
- **Keep / do-not-merge / inconclusive** — Explicit verdict. **Inconclusive is not keep/merge.**  
- **Stop after 3 fix attempts** — After the third failed correction, stop and hand residual risk to a human.  
- **Encode recurring failures** — Same failure class twice → add a test, script, or lint; do not grow prompt text.  
- **Append-only trail** — One row in `migrations/decisions.tsv` for the verdict (see **Migration decision trail**).

## Evidence ranking (detail)

Rank claims by how they were obtained. Higher wins; never skip the floor for keep/merge:

1. **Self-report** — agent/human says it works (weakest; insufficient alone)
2. **Pointed at code** — cited paths/diffs that *should* imply correctness
3. **Ran real tests/commands** — exit codes and output from the repo’s suite/scripts (**required floor to keep/merge**)
4. **Optional runtime/deploy** — dual-run, canary, or staging probe when the slice warrants it

**Inconclusive ≠ keep/merge.** Missing commands, flaky runs, or “looks fine in the diff” without a green suite ⇒ **inconclusive** or **do not merge**, never pass.

## Artifact ladder (this repo — eShop)

Run in order for the slice; skip only with an explicit waiver + reason in the verdict and trail:

1. **Characterization / unit for the slice** — Prefer `./scripts/check-catalog.sh` (committed lever) as command evidence when present. The script must exercise Rust (`cargo test` / release build) when `native/*/Cargo.toml` exists, then Catalog unit tests. Record exit code and path to evidence.
2. **Rust on path + parity** — Confirm the crate exists, .NET calls it on the runtime path, and parity tests cover the characterized cases through that path. Cite commands/exit codes. **Required for Catalog inventory keep/merge.**
3. **Verify Catalog** — invoke the **Verify Catalog** skill. When Rust wiring is present, verification must exercise that path (or return **inconclusive**). Prefer `./scripts/check-catalog.sh` then documented smoke / `tests/Catalog.FunctionalTests`. If skipped: document why in `validate.md` and the trail.
4. **Optional runtime** — only if the stack is already running (Aspire AppHost / deployed env). Do not invent infra just for the demo.

**Keep/merge requirement for Catalog inventory / stock slices:**
- Characterization green **and**
- Rust implementation + .NET→Rust wire-up on runtime path **and**
- Parity through the Rust-wired path green **and**
- Verify Catalog green (exercising the Rust path when wiring is present) — or an **explicit waiver** with owner + reason

.NET extract-only without Rust ⇒ **do not merge** for the E2E demo finish line (unless user explicitly waived Rust).

Non-Catalog slices: unit/characterization floor still required; Rust requirements follow that slice’s scope plan; Verify Catalog N/A.

## Steps

1. **Freeze the claim**  
   One sentence: what behavioral guarantee this slice must hold. Example: “CatalogItem stock rules match pre-migration behavior for the characterized cases when .NET calls the Rust crate on the runtime path.”

2. **Blast-radius proof (one safety fact)**  
   Identify the *one* safety fact this slice depends on (from scope `plan.md`, or restate it). **Prove it by running real code/script/tests** — not a writeup. Mark **proven** or **unproven**. If unproven, verdict cannot be keep/merge until proven or explicitly waived with owner + reason.

3. **Characterization + parity**  
   - Characterization tests must have been green on baseline before structural / Rust change.  
   - After the slice: same cases must pass through the **Rust-wired** path.  
   - Prefer table-driven cases for pure rules.  
   - Do not invent coverage percentages—report pass/fail and what was exercised.

4. **Confirm Rust on path**  
   - `native/catalog_stock/Cargo.toml` (or analogous) exists  
   - .NET boundary documented (LibraryImport/PInvoke preferred)  
   - Runtime path for the slice invokes Rust (not a dead parallel module)  
   If missing for a Catalog inventory slice scoped for E2E Rust → **do not merge** (or inconclusive if you could not inspect), not keep/merge.

5. **Walk the artifact ladder**  
   Execute the ladder above. Document exact commands, exit codes, and evidence paths. For Catalog slices, invoke **Verify Catalog** rather than inventing ad-hoc HTTP probes.

6. **Optional fix-forward (companion, 3-attempt cap)**  
   If validating while an agent iterates on failures: allow at most **3** correction attempts. After the third failed attempt, **stop** — document failing commands, diffs tried, and residual risk for a human.

7. **Encode recurring failures into structure**  
   If the same failure class appears twice: prefer a lint rule, script, narrow skill, or test harness over growing prompt text. Note the encoding action in the trail.

8. **Rollback criteria**  
   Define objective triggers to revert or feature-flag off:
   - Characterization or parity suite failing on mainline CI  
   - Contract break with known consumers  
   - Panic/crash / undefined behavior in the Rust path under normal inputs  
   State rollback action (revert PR, toggle flag, restore previous artifact).

9. **Verdict: keep/merge · do not merge · inconclusive**  
   - **Keep / merge (Go)** — claim holds; evidence ≥ “ran real tests”; blast-radius fact **proven** (or waived); Catalog inventory slice has Rust on path + parity green; Verify Catalog green (or waived); no open rollback triggers  
   - **Do not merge (No-go)** — failed checks, missing Rust/parity for required slice, broken parity, unproven safety fact without waiver  
   - **Inconclusive** — could not obtain required evidence — **not a pass; do not treat as keep/merge**

10. **Append decision trail**  
    Using **Migration decision trail**, append one row to `migrations/decisions.tsv` with phase=`validate`, the verdict, why, evidence paths, and result.

11. **Emit artifact**  
    Write `validate.md` (or append a **Validation** section to `plan.md`). Include commands actually run, evidence level, Rust/parity status, blast-radius fact status, attempt count if any, Verify Catalog result/waiver, and the verdict.

## Output: checklist template

```markdown
# Migration validate: {SLICE}

## Claim
...

## Blast-radius safety fact
- Fact: ...
- Status: proven | unproven | waived
- Evidence: {command + exit code / path} or waiver owner+reason

## Evidence level
self-report | pointed-at-code | ran-real-tests | runtime/deploy
(floor for keep/merge: ran-real-tests)

## Rust on path (required for Catalog inventory E2E slice)
- [ ] Crate present: `native/...`
- [ ] .NET wires to Rust on runtime path: yes | no
- [ ] Parity through Rust-wired path: green | red | missing
- Evidence: ...

## Artifact ladder
- [ ] Characterization/unit + Rust via `./scripts/check-catalog.sh`: `{command}` — result: ... — evidence: ...
- [ ] Verify Catalog (must exercise Rust path when wired): ran | skipped (reason: ...) — result: ... — evidence: ...
- [ ] Optional runtime: ran | N/A — result: ...

## Parity
- [ ] Characterization tests exist and passed on baseline
- [ ] Same cases pass after slice through Rust-wired path
- [ ] Contract/API checks (if applicable)

## Fix-forward attempts (if any)
- Count: 0–3
- Stopped because: success | hit 3-attempt cap | N/A
- Notes for human (if capped): ...

## Structure encoding (if recurring failure)
- ...

## Rollback
- Trigger: ...
- Action: ...

## Verdict
- [ ] Keep / merge — evidence: ...
- [ ] Do not merge — blockers: ...
- [ ] Inconclusive — missing evidence: ...
(Inconclusive ≠ keep/merge)

## Trail
- Appended row to migrations/decisions.tsv: yes | no
```

## Guardrails

- Works for any baseline → after-slice / slice; eShop Catalog inventory is the primary demo surface.  
- For that surface, **Rust implementation + parity are required for keep/merge** — extract-only is not enough.  
- No fake pass-rates, LOC, or “% parity.”  
- Fail closed: missing characterization or missing Rust/parity for a required E2E slice ⇒ do not merge until added or explicitly waived with reason.  
- Never treat **inconclusive** as **keep/merge**.  
- Catalog-related keep/merge requires Verify Catalog green (exercising Rust when wired) or explicit waiver.  
- Prefer encoding recurring failures into **structure** (lint rule, script, narrow skill) over growing a mega-prompt. For Catalog slices, `./scripts/check-catalog.sh` is preferred command evidence when present.  
- Validation is the gate; fix-forward is optional and capped at 3 attempts.  
- Always leave a trail row on verdict (see **Migration decision trail**).
