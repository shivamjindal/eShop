//! C ABI for .NET LibraryImport.

use crate::stock::{add_stock, remove_stock, StockError};

/// 0 = ok, 1 = empty stock, 2 = invalid quantity.
#[no_mangle]
pub unsafe extern "C" fn catalog_stock_remove(
    available_stock: i32,
    quantity_desired: i32,
    out_available: *mut i32,
    out_removed: *mut i32,
) -> i32 {
    match remove_stock(available_stock, quantity_desired) {
        Ok((available, removed)) => {
            if !out_available.is_null() {
                *out_available = available;
            }
            if !out_removed.is_null() {
                *out_removed = removed;
            }
            0
        }
        Err(StockError::Empty) => 1,
        Err(StockError::InvalidQuantity) => 2,
    }
}

/// 0 = ok (AddStock has no failure modes in current .NET semantics).
#[no_mangle]
pub unsafe extern "C" fn catalog_stock_add(
    available_stock: i32,
    max_stock_threshold: i32,
    quantity: i32,
    out_available: *mut i32,
    out_added: *mut i32,
) -> i32 {
    let (available, added) = add_stock(available_stock, max_stock_threshold, quantity);
    if !out_available.is_null() {
        *out_available = available;
    }
    if !out_added.is_null() {
        *out_added = added;
    }
    0
}
