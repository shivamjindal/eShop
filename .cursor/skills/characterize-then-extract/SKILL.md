---
name: Characterize then extract
description: Use only for a .NET extract-only stop when the user explicitly says to stop before Rust. Superseded by Migrate slice to Rust for the default E2E path.
---

# Characterize then extract

**This skill is superseded by [Migrate slice to Rust](../migrate-slice-to-rust/SKILL.md).**

**Default:** use **Migrate slice to Rust** for every Catalog (or analogous) migration slice. That skill includes characterization and extract as early steps, then **requires** Rust on the runtime path and green parity.

**Only use this skill** when the user **explicitly** says to stop before Rust (extract-only / .NET-only). In that case:

1. Brief `how` (3–5 bullets of current .NET behavior)
2. Characterization tests green on baseline
3. Extract pure module; migrate callers; delete legacy; keep tests green
4. Run `./scripts/check-catalog.sh`
5. Say clearly in the hand-off that **Rust was deferred by explicit user request** — do not call the migration slice “done” for the E2E demo

For the Intellias / eShop demo finish line, **E2E Rust is required**. See **Migrate slice to Rust**.
