//! Catalog stock rules island — skeleton for the eShop .NET → Rust demo.
//!
//! Agents implementing **Migrate slice to Rust** should port the characterized
//! `RemoveStock` / `AddStock` semantics here, expose a `cdylib` surface for
//! .NET `LibraryImport` / P/Invoke, and keep `cargo test` green with parity cases.
//!
//! This crate is intentionally minimal scaffolding so skills and
//! `scripts/check-catalog.sh` have a concrete path (`native/catalog_stock`).

/// Placeholder so `cargo test` / `cargo build` succeed before the real port lands.
pub fn skeleton_ok() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_builds() {
        assert!(skeleton_ok());
    }
}
