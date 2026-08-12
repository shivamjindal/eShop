//! Pure Catalog stock rules (`RemoveStock` / `AddStock`) for the eShop .NET → Rust demo.
//!
//! Exposes an `extern "C"` surface for .NET `LibraryImport` / P/Invoke (`cdylib`)
//! and keeps `cargo test` parity cases on the same pure functions (`rlib`).

/// Empty stock — mirrors CatalogDomainException "Empty stock...".
pub const ERR_EMPTY_STOCK: i32 = 1;
/// quantityDesired <= 0 — mirrors CatalogDomainException "Item units desired...".
pub const ERR_NON_POSITIVE_QTY: i32 = 2;
pub const OK: i32 = 0;
const ERR_NULL_PTR: i32 = -1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockError {
    EmptyStock,
    NonPositiveQuantity,
}

/// Pure RemoveStock: returns `(new_available, removed)`.
pub fn remove_stock(available: i32, quantity_desired: i32) -> Result<(i32, i32), StockError> {
    if available == 0 {
        return Err(StockError::EmptyStock);
    }
    if quantity_desired <= 0 {
        return Err(StockError::NonPositiveQuantity);
    }
    let removed = quantity_desired.min(available);
    Ok((available - removed, removed))
}

/// Pure AddStock: returns `(new_available, added)`. `on_reorder` becomes false.
pub fn add_stock(available: i32, max_stock_threshold: i32, quantity: i32) -> (i32, i32) {
    let original = available;
    let new_available = if available + quantity > max_stock_threshold {
        available + (max_stock_threshold - available)
    } else {
        available + quantity
    };
    (new_available, new_available - original)
}

/// FFI: mutate `available_stock` in place; write units removed to `removed_out`.
/// Returns OK / ERR_EMPTY_STOCK / ERR_NON_POSITIVE_QTY.
#[no_mangle]
pub unsafe extern "C" fn catalog_stock_remove(
    available_stock: *mut i32,
    quantity_desired: i32,
    removed_out: *mut i32,
) -> i32 {
    if available_stock.is_null() || removed_out.is_null() {
        return ERR_NULL_PTR;
    }
    match remove_stock(*available_stock, quantity_desired) {
        Ok((new_stock, removed)) => {
            *available_stock = new_stock;
            *removed_out = removed;
            OK
        }
        Err(StockError::EmptyStock) => ERR_EMPTY_STOCK,
        Err(StockError::NonPositiveQuantity) => ERR_NON_POSITIVE_QTY,
    }
}

/// FFI: mutate `available_stock` and clear `on_reorder` (u8 0/1); write units added.
#[no_mangle]
pub unsafe extern "C" fn catalog_stock_add(
    available_stock: *mut i32,
    max_stock_threshold: i32,
    on_reorder: *mut u8,
    quantity: i32,
    added_out: *mut i32,
) -> i32 {
    if available_stock.is_null() || on_reorder.is_null() || added_out.is_null() {
        return ERR_NULL_PTR;
    }
    let (new_stock, added) = add_stock(*available_stock, max_stock_threshold, quantity);
    *available_stock = new_stock;
    *on_reorder = 0;
    *added_out = added;
    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_empty_stock_errors() {
        assert_eq!(remove_stock(0, 1), Err(StockError::EmptyStock));
    }

    #[test]
    fn remove_non_positive_errors() {
        assert_eq!(remove_stock(5, 0), Err(StockError::NonPositiveQuantity));
        assert_eq!(remove_stock(5, -3), Err(StockError::NonPositiveQuantity));
    }

    #[test]
    fn remove_full_quantity() {
        assert_eq!(remove_stock(10, 4), Ok((6, 4)));
    }

    #[test]
    fn remove_partial_fill() {
        assert_eq!(remove_stock(3, 10), Ok((0, 3)));
    }

    #[test]
    fn add_under_max() {
        assert_eq!(add_stock(10, 100, 5), (15, 5));
    }

    #[test]
    fn add_caps_at_max() {
        assert_eq!(add_stock(90, 100, 25), (100, 10));
    }

    #[test]
    fn add_at_max_adds_zero() {
        assert_eq!(add_stock(50, 50, 10), (50, 0));
    }

    #[test]
    fn ffi_remove_and_add_roundtrip() {
        let mut available = 10;
        let mut removed = 0;
        let rc = unsafe { catalog_stock_remove(&mut available, 4, &mut removed) };
        assert_eq!(rc, OK);
        assert_eq!(available, 6);
        assert_eq!(removed, 4);

        let mut on_reorder: u8 = 1;
        let mut added = 0;
        let rc = unsafe { catalog_stock_add(&mut available, 100, &mut on_reorder, 5, &mut added) };
        assert_eq!(rc, OK);
        assert_eq!(available, 11);
        assert_eq!(added, 5);
        assert_eq!(on_reorder, 0);
    }
}
