---
name: Migration decision trail
description: Use during migration scoping/implementation/validation to keep an append-only decisions.tsv audit trail.
---

# Migration decision trail

Append-only audit trail for migration work (scope → implement → validate). Every meaningful call gets a row with evidence a reviewer can open — not a slide, not a rewritten history.

## Why append-only

- **Append rows; never edit prior rows in place.** If a decision was wrong, add a new row that supersedes it.  
- Reviewers (and later agents) can see *what* was decided, *why*, and *what evidence* backed keep/merge vs do-not-merge.  
- The trail does **not** replace **Migration validate** — it records the gate’s outcome.

## When to use

- During **Scope .NET → Rust**, **Migrate slice to Rust** (implement), or **Migration validate**
- When a demo/PR needs to show *why* a keep/merge or do-not-merge was issued
- When multiple agents touch the same slice and need a shared history

## Default path

Prefer (create parent dirs if missing):

1. `migrations/decisions.tsv` — default for demo/PR confidence  
2. `.audit/migration-decisions.tsv` — alternative if the repo already uses `.audit/`

Commit the file when the demo or PR should show the trail. Template (header only):  
`.cursor/skills/migration-decision-trail/decisions-template.tsv`

## Format (TSV)

Columns (tab-separated, one header row, then append-only data rows):

| Column | Meaning |
|--------|---------|
| `ts` | ISO-8601 UTC timestamp |
| `phase` | `scope` \| `implement` \| `validate` \| other short label |
| `decision` | What was decided (short) |
| `why` | Rationale in one line |
| `evidence` | Command, path, PR URL, or log pointer |
| `result` | Outcome (`plan.md written`, `tests green`, `keep/merge`, `do not merge`, `inconclusive`, …). Legacy `Go` / `No-go` labels are OK if the plain meaning is clear. |

Do **not** rewrite history. Correct mistakes with a new row that supersedes the prior decision.

## Steps

1. **Ensure file exists**  
   If missing, copy the template header:

   ```bash
   mkdir -p migrations
   cp .cursor/skills/migration-decision-trail/decisions-template.tsv migrations/decisions.tsv
   ```

2. **Append on the milestones**  
   Append **one row** at each of:
   - Scope finishes (`phase=scope`) — e.g. first slice chosen as characterize → Rust → wire → parity; safety fact proven/unproven  
   - Slice lands (`phase=implement`) — e.g. Rust island + .NET wire + parity / `./scripts/check-catalog.sh` green  
   - Validate verdict (`phase=validate`) — keep/merge, do not merge, or inconclusive, with evidence paths (including Rust + parity for Catalog stock)

3. **Keep rows honest**  
   Evidence must point at something a reviewer can open (command + exit code, file path, CI URL). No fabricated metrics.

4. **Commit when needed**  
   Include `migrations/decisions.tsv` in the demo PR when confidence/audit is part of the story.

## Example rows

```tsv
ts	phase	decision	why	evidence	result
2026-08-12T00:00:00Z	scope	First slice = Catalog stock → Rust island	E2E .NET→Rust not extract-only	plan.md#first-demo-able-slice	plan.md written; safety fact unproven
2026-08-12T00:30:00Z	implement	Port stock rules to native/catalog_stock + wire	Parity cases green via Rust path	./scripts/check-catalog.sh; cargo test	tests green; Rust on path
2026-08-12T01:00:00Z	validate	Keep/merge	Unit + Rust parity + Verify Catalog green; safety fact proven	validate.md; migrations/decisions.tsv	keep/merge
```

## Guardrails

- Append-only; never edit prior rows in place.  
- One row minimum at scope end, slice land, and validate verdict.  
- Tabs between columns; no commas-as-separators (values may contain commas).  
- Pair with **Migration validate** — the trail does not replace the gate.
