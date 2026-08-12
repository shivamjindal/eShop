# Cursor skills — Intellias migration demo pack

Cold-agent playbooks for a **.NET → Rust** brownfield slice. No external framework knowledge required.

**Finish line:** Rust on the runtime path for the slice + green parity. A .NET-only extract is intermediate, not success.

**Order of use**

1. **Scope .NET → Rust** — Point at one .NET service. Inventory it, map who depends on it, write a checkable definition of done, and pick a first small slice whose acceptance includes **Rust on the runtime path** → `plan.md`.
2. **Migrate slice to Rust** — Characterize current .NET behavior, extract if needed, implement the rules in Rust (`native/catalog_stock/` for Catalog stock), wire .NET to call Rust, prove parity. Default implementation skill. (**Characterize then extract** is a redirect stub — extract-only only if the user explicitly stops before Rust.)
3. **`./scripts/check-catalog.sh` / Verify Catalog** — Prove Catalog.API still behaves; when Rust wiring is present, exercise that path (or say inconclusive). Capture real exit codes.
4. **Migration validate** — Rank evidence honestly. For the Catalog inventory slice, **keep/merge requires Rust implementation + parity evidence**, not extract-only. Decide **keep/merge**, **do not merge**, or **inconclusive**. Cap fix attempts at 3 → `validate.md`.
5. **Migration decision trail** — Append-only `migrations/decisions.tsv` at scope / implement / validate so a reviewer can see what was decided and why.
