using eShop.Catalog.API.Infrastructure.Exceptions;
using eShop.Catalog.API.Model;

namespace eShop.Catalog.UnitTests;

/// <summary>
/// Characterization tests for CatalogItem stock rules (RemoveStock / AddStock).
/// Locks current .NET semantics before extract / Rust port.
/// </summary>
[TestClass]
public class CatalogItemStockTests
{
    private static CatalogItem Item(int available, int max = 100, string name = "Trail Shoe", bool onReorder = false) =>
        new(name)
        {
            AvailableStock = available,
            MaxStockThreshold = max,
            OnReorder = onReorder,
            RestockThreshold = 10,
        };

    [TestMethod]
    public void RemoveStock_WhenSufficient_RemovesExactQuantity()
    {
        var item = Item(available: 10);

        var removed = item.RemoveStock(4);

        Assert.AreEqual(4, removed);
        Assert.AreEqual(6, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_WhenPartialStock_RemovesOnlyAvailable()
    {
        var item = Item(available: 3);

        var removed = item.RemoveStock(10);

        Assert.AreEqual(3, removed);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_WhenEmpty_ThrowsSoldOut()
    {
        var item = Item(available: 0, name: "Wanderer Boots");

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(1));

        Assert.AreEqual("Empty stock, product item Wanderer Boots is sold out", ex.Message);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_WhenQuantityZero_Throws()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(0));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_WhenQuantityNegative_Throws()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(-2));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_WhenUnderMax_AddsFullQuantity()
    {
        var item = Item(available: 10, max: 50, onReorder: true);

        var added = item.AddStock(5);

        Assert.AreEqual(5, added);
        Assert.AreEqual(15, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_WhenExceedsMax_ClampsToThreshold()
    {
        var item = Item(available: 40, max: 50, onReorder: true);

        var added = item.AddStock(20);

        Assert.AreEqual(10, added);
        Assert.AreEqual(50, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_WhenAlreadyAtMax_AddsZero()
    {
        var item = Item(available: 50, max: 50, onReorder: true);

        var added = item.AddStock(5);

        Assert.AreEqual(0, added);
        Assert.AreEqual(50, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }
}
