//! Pure Catalog stock mutation rules — parity with eShop CatalogItem RemoveStock / AddStock.
//!
//! Host ABI (native cdylib or wasm32): packed `i64` results so callers need no guest pointers.
//! - Remove/Add success: high 32 bits = new_available_stock, low 32 bits = units changed
//! - Remove errors: `-1` empty stock, `-2` invalid quantity

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockError {
    EmptyStock,
    InvalidQuantity,
}

/// Decrements available stock. Partial fill when desired exceeds available.
pub fn remove_stock(
    available_stock: i32,
    quantity_desired: i32,
) -> Result<(i32 /* removed */, i32 /* new_available */), StockError> {
    if available_stock == 0 {
        return Err(StockError::EmptyStock);
    }
    if quantity_desired <= 0 {
        return Err(StockError::InvalidQuantity);
    }
    let removed = quantity_desired.min(available_stock);
    Ok((removed, available_stock - removed))
}

/// Increments available stock, clamping to max threshold. Always clears on-reorder.
pub fn add_stock(
    available_stock: i32,
    max_stock_threshold: i32,
    quantity: i32,
) -> (i32 /* added */, i32 /* new_available */, bool /* on_reorder */) {
    let original = available_stock;
    let new_available = if available_stock + quantity > max_stock_threshold {
        available_stock + (max_stock_threshold - available_stock)
    } else {
        available_stock + quantity
    };
    (new_available - original, new_available, false)
}

fn pack_ok(changed: i32, new_available: i32) -> i64 {
    ((new_available as i64) << 32) | (changed as u32 as i64)
}

pub const ERR_EMPTY: i64 = -1;
pub const ERR_INVALID_QTY: i64 = -2;

#[no_mangle]
pub extern "C" fn catalog_stock_remove(available_stock: i32, quantity_desired: i32) -> i64 {
    match remove_stock(available_stock, quantity_desired) {
        Ok((removed, new_available)) => pack_ok(removed, new_available),
        Err(StockError::EmptyStock) => ERR_EMPTY,
        Err(StockError::InvalidQuantity) => ERR_INVALID_QTY,
    }
}

#[no_mangle]
pub extern "C" fn catalog_stock_add(
    available_stock: i32,
    max_stock_threshold: i32,
    quantity: i32,
) -> i64 {
    let (added, new_available, _) = add_stock(available_stock, max_stock_threshold, quantity);
    pack_ok(added, new_available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_empty_stock_errors() {
        assert_eq!(remove_stock(0, 1), Err(StockError::EmptyStock));
        assert_eq!(catalog_stock_remove(0, 1), ERR_EMPTY);
    }

    #[test]
    fn remove_quantity_zero_or_negative_errors() {
        assert_eq!(remove_stock(5, 0), Err(StockError::InvalidQuantity));
        assert_eq!(remove_stock(5, -3), Err(StockError::InvalidQuantity));
        assert_eq!(catalog_stock_remove(5, 0), ERR_INVALID_QTY);
    }

    #[test]
    fn remove_partial_fill_when_desired_exceeds_available() {
        assert_eq!(remove_stock(4, 10), Ok((4, 0)));
        let packed = catalog_stock_remove(4, 10);
        assert_eq!(packed as i32, 4);
        assert_eq!((packed >> 32) as i32, 0);
    }

    #[test]
    fn remove_full_fill_when_stock_sufficient() {
        assert_eq!(remove_stock(10, 3), Ok((3, 7)));
        let packed = catalog_stock_remove(10, 3);
        assert_eq!(packed as i32, 3);
        assert_eq!((packed >> 32) as i32, 7);
    }

    #[test]
    fn add_clamps_to_max_stock_threshold() {
        assert_eq!(add_stock(80, 100, 50), (20, 100, false));
        let packed = catalog_stock_add(80, 100, 50);
        assert_eq!(packed as i32, 20);
        assert_eq!((packed >> 32) as i32, 100);
    }

    #[test]
    fn add_within_threshold_adds_full_quantity() {
        assert_eq!(add_stock(10, 100, 5), (5, 15, false));
    }

    #[test]
    fn add_clears_on_reorder() {
        let (_, _, on_reorder) = add_stock(10, 100, 1);
        assert!(!on_reorder);
    }
}
