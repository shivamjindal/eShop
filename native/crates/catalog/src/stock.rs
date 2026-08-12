//! Catalog stock rules — parity with `CatalogItem.RemoveStock` / `AddStock`.

pub const ERR_OK: i32 = 0;
pub const ERR_EMPTY_STOCK: i32 = 1;
pub const ERR_INVALID_QUANTITY: i32 = 2;

/// Mirrors .NET `CatalogItem.RemoveStock`.
pub fn remove_stock(available_stock: &mut i32, quantity_desired: i32) -> Result<i32, i32> {
    if *available_stock == 0 {
        return Err(ERR_EMPTY_STOCK);
    }
    if quantity_desired <= 0 {
        return Err(ERR_INVALID_QUANTITY);
    }

    let removed = quantity_desired.min(*available_stock);
    *available_stock -= removed;
    Ok(removed)
}

/// Mirrors .NET `CatalogItem.AddStock`.
pub fn add_stock(
    available_stock: &mut i32,
    max_stock_threshold: i32,
    on_reorder: &mut bool,
    quantity: i32,
) -> i32 {
    let original = *available_stock;

    if (*available_stock + quantity) > max_stock_threshold {
        *available_stock += max_stock_threshold - *available_stock;
    } else {
        *available_stock += quantity;
    }

    *on_reorder = false;
    *available_stock - original
}

/// FFI: remove stock. Writes units removed to `removed` on success.
/// Returns 0 on success, or an error code (`ERR_*`).
///
/// # Safety
/// `available_stock` and `removed` must be valid for writes.
#[no_mangle]
pub unsafe extern "C" fn catalog_remove_stock(
    available_stock: *mut i32,
    quantity_desired: i32,
    removed: *mut i32,
) -> i32 {
    if available_stock.is_null() || removed.is_null() {
        return ERR_INVALID_QUANTITY;
    }
    match remove_stock(&mut *available_stock, quantity_desired) {
        Ok(n) => {
            *removed = n;
            ERR_OK
        }
        Err(code) => code,
    }
}

/// FFI: add stock. Writes units added to `added`.
/// `on_reorder` is 0/1; cleared to 0 on success (always succeeds like .NET).
///
/// # Safety
/// Pointers must be valid for reads/writes.
#[no_mangle]
pub unsafe extern "C" fn catalog_add_stock(
    available_stock: *mut i32,
    max_stock_threshold: i32,
    on_reorder: *mut u8,
    quantity: i32,
    added: *mut i32,
) -> i32 {
    if available_stock.is_null() || on_reorder.is_null() || added.is_null() {
        return ERR_INVALID_QUANTITY;
    }
    let mut reorder = *on_reorder != 0;
    let n = add_stock(
        &mut *available_stock,
        max_stock_threshold,
        &mut reorder,
        quantity,
    );
    *on_reorder = u8::from(reorder);
    *added = n;
    ERR_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_stock_sufficient() {
        let mut available = 10;
        assert_eq!(remove_stock(&mut available, 3), Ok(3));
        assert_eq!(available, 7);
    }

    #[test]
    fn remove_stock_partial() {
        let mut available = 4;
        assert_eq!(remove_stock(&mut available, 10), Ok(4));
        assert_eq!(available, 0);
    }

    #[test]
    fn remove_stock_empty() {
        let mut available = 0;
        assert_eq!(remove_stock(&mut available, 1), Err(ERR_EMPTY_STOCK));
        assert_eq!(available, 0);
    }

    #[test]
    fn remove_stock_zero_desired() {
        let mut available = 5;
        assert_eq!(remove_stock(&mut available, 0), Err(ERR_INVALID_QUANTITY));
        assert_eq!(available, 5);
    }

    #[test]
    fn remove_stock_negative_desired() {
        let mut available = 5;
        assert_eq!(remove_stock(&mut available, -2), Err(ERR_INVALID_QUANTITY));
        assert_eq!(available, 5);
    }

    #[test]
    fn add_stock_below_max() {
        let mut available = 10;
        let mut on_reorder = true;
        assert_eq!(add_stock(&mut available, 100, &mut on_reorder, 5), 5);
        assert_eq!(available, 15);
        assert!(!on_reorder);
    }

    #[test]
    fn add_stock_caps_at_max() {
        let mut available = 90;
        let mut on_reorder = true;
        assert_eq!(add_stock(&mut available, 100, &mut on_reorder, 25), 10);
        assert_eq!(available, 100);
        assert!(!on_reorder);
    }

    #[test]
    fn add_stock_already_at_max() {
        let mut available = 100;
        let mut on_reorder = true;
        assert_eq!(add_stock(&mut available, 100, &mut on_reorder, 5), 0);
        assert_eq!(available, 100);
        assert!(!on_reorder);
    }
}
