namespace eShop.Catalog.API.Model;

public static class CatalogStock
{
    public static int Remove(int availableStock, string itemName, int quantityDesired, out int newAvailableStock)
    {
        var packed = CatalogStockWasm.Remove(availableStock, quantityDesired);
        if (packed == CatalogStockWasm.ErrEmpty)
        {
            throw new CatalogDomainException($"Empty stock, product item {itemName} is sold out");
        }

        if (packed == CatalogStockWasm.ErrInvalidQty)
        {
            throw new CatalogDomainException($"Item units desired should be greater than zero");
        }

        if (packed < 0)
        {
            throw new CatalogDomainException($"Unexpected catalog_stock_remove status {packed}");
        }

        newAvailableStock = (int)(packed >> 32);
        return (int)packed;
    }

    public static int Add(
        int availableStock,
        int maxStockThreshold,
        int quantity,
        out int newAvailableStock,
        out bool onReorder)
    {
        var packed = CatalogStockWasm.Add(availableStock, maxStockThreshold, quantity);
        if (packed < 0)
        {
            throw new CatalogDomainException($"Unexpected catalog_stock_add status {packed}");
        }

        newAvailableStock = (int)(packed >> 32);
        onReorder = false;
        return (int)packed;
    }
}
