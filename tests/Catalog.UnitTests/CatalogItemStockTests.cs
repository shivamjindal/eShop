namespace eShop.Catalog.UnitTests;

[TestClass]
public class CatalogItemStockTests
{
    private static CatalogItem CreateItem(
        int availableStock,
        int maxStockThreshold = 100,
        bool onReorder = false,
        string name = "Test Item")
    {
        return new CatalogItem(name)
        {
            AvailableStock = availableStock,
            MaxStockThreshold = maxStockThreshold,
            RestockThreshold = 10,
            OnReorder = onReorder
        };
    }

    [TestMethod]
    public void RemoveStock_empty_stock_throws()
    {
        var item = CreateItem(availableStock: 0);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(1));
        Assert.Contains("sold out", ex.Message);
    }

    [TestMethod]
    public void RemoveStock_quantity_zero_or_negative_throws()
    {
        var item = CreateItem(availableStock: 5);

        Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(0));
        Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(-3));
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_partial_fill_when_desired_exceeds_available()
    {
        var item = CreateItem(availableStock: 4);

        var removed = item.RemoveStock(10);

        Assert.AreEqual(4, removed);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_full_fill_when_stock_sufficient()
    {
        var item = CreateItem(availableStock: 10);

        var removed = item.RemoveStock(3);

        Assert.AreEqual(3, removed);
        Assert.AreEqual(7, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_clamps_to_max_stock_threshold()
    {
        var item = CreateItem(availableStock: 80, maxStockThreshold: 100);

        var added = item.AddStock(50);

        Assert.AreEqual(20, added);
        Assert.AreEqual(100, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_within_threshold_adds_full_quantity()
    {
        var item = CreateItem(availableStock: 10, maxStockThreshold: 100);

        var added = item.AddStock(5);

        Assert.AreEqual(5, added);
        Assert.AreEqual(15, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_clears_on_reorder()
    {
        var item = CreateItem(availableStock: 10, maxStockThreshold: 100, onReorder: true);

        _ = item.AddStock(1);

        Assert.IsFalse(item.OnReorder);
    }
}
