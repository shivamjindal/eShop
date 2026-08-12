//! Catalog stock mutation rules (parity with CatalogItem.RemoveStock / AddStock).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockError {
    Empty,
    InvalidQuantity,
}

/// Returns `(new_available, removed)`.
pub fn remove_stock(available: i32, quantity_desired: i32) -> Result<(i32, i32), StockError> {
    if available == 0 {
        return Err(StockError::Empty);
    }
    if quantity_desired <= 0 {
        return Err(StockError::InvalidQuantity);
    }

    let removed = quantity_desired.min(available);
    Ok((available - removed, removed))
}

/// Returns `(new_available, added)`. Always clears reorder on the .NET side.
pub fn add_stock(available: i32, max_stock_threshold: i32, quantity: i32) -> (i32, i32) {
    let original = available;
    let new_available = if available + quantity > max_stock_threshold {
        max_stock_threshold
    } else {
        available + quantity
    };
    (new_available, new_available - original)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_sufficient() {
        assert_eq!(remove_stock(10, 4).unwrap(), (6, 4));
    }

    #[test]
    fn remove_partial() {
        assert_eq!(remove_stock(3, 10).unwrap(), (0, 3));
    }

    #[test]
    fn remove_empty() {
        assert_eq!(remove_stock(0, 1), Err(StockError::Empty));
    }

    #[test]
    fn remove_zero_qty() {
        assert_eq!(remove_stock(5, 0), Err(StockError::InvalidQuantity));
    }

    #[test]
    fn remove_negative_qty() {
        assert_eq!(remove_stock(5, -2), Err(StockError::InvalidQuantity));
    }

    #[test]
    fn add_under_max() {
        assert_eq!(add_stock(10, 50, 5), (15, 5));
    }

    #[test]
    fn add_clamps_to_max() {
        assert_eq!(add_stock(40, 50, 20), (50, 10));
    }

    #[test]
    fn add_at_max() {
        assert_eq!(add_stock(50, 50, 5), (50, 0));
    }
}
