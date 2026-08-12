namespace eShop.Catalog.UnitTests;

/// <summary>
/// Characterization of CatalogItem RemoveStock / AddStock semantics
/// (now exercised on the Rust-wired live path).
/// </summary>
[TestClass]
[DoNotParallelize]
public class CatalogItemStockTests
{
    private static CatalogItem Item(int available, int max = 100, bool onReorder = false) =>
        new("Trail Shoe")
        {
            AvailableStock = available,
            MaxStockThreshold = max,
            OnReorder = onReorder,
            RestockThreshold = 10
        };

    [TestMethod]
    public void RemoveStock_throws_when_available_stock_is_zero()
    {
        var item = Item(available: 0);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(1));

        Assert.AreEqual("Empty stock, product item Trail Shoe is sold out", ex.Message);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_throws_when_quantity_desired_is_zero()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(0));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_throws_when_quantity_desired_is_negative()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(-3));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_removes_full_quantity_when_stock_sufficient()
    {
        var item = Item(available: 10);

        var removed = item.RemoveStock(4);

        Assert.AreEqual(4, removed);
        Assert.AreEqual(6, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_partial_fill_when_desired_exceeds_available()
    {
        var item = Item(available: 3);

        var removed = item.RemoveStock(10);

        Assert.AreEqual(3, removed);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_adds_full_quantity_when_under_max_threshold()
    {
        var item = Item(available: 10, max: 100, onReorder: true);

        var added = item.AddStock(5);

        Assert.AreEqual(5, added);
        Assert.AreEqual(15, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_caps_at_max_stock_threshold()
    {
        var item = Item(available: 90, max: 100, onReorder: true);

        var added = item.AddStock(25);

        Assert.AreEqual(10, added);
        Assert.AreEqual(100, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_when_already_at_max_adds_zero()
    {
        var item = Item(available: 50, max: 50, onReorder: true);

        var added = item.AddStock(10);

        Assert.AreEqual(0, added);
        Assert.AreEqual(50, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }
}
