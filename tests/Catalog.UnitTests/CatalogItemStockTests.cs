using eShop.Catalog.API.Infrastructure.Exceptions;
using eShop.Catalog.API.Model;

namespace eShop.Catalog.UnitTests;

/// <summary>
/// Characterization tests for CatalogItem stock rules (baseline .NET semantics).
/// </summary>
[TestClass]
public class CatalogItemStockTests
{
    private static CatalogItem Item(int available, int maxThreshold = 100, bool onReorder = false) =>
        new("Trail Shoe")
        {
            AvailableStock = available,
            MaxStockThreshold = maxThreshold,
            OnReorder = onReorder,
            RestockThreshold = 10,
        };

    [TestMethod]
    public void RemoveStock_SufficientStock_RemovesDesiredAndReturnsIt()
    {
        var item = Item(available: 10);

        var removed = item.RemoveStock(3);

        Assert.AreEqual(3, removed);
        Assert.AreEqual(7, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_PartialStock_RemovesAllAvailable()
    {
        var item = Item(available: 4);

        var removed = item.RemoveStock(10);

        Assert.AreEqual(4, removed);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_EmptyStock_ThrowsCatalogDomainException()
    {
        var item = Item(available: 0);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(1));

        Assert.AreEqual("Empty stock, product item Trail Shoe is sold out", ex.Message);
        Assert.AreEqual(0, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_ZeroDesired_ThrowsCatalogDomainException()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(0));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void RemoveStock_NegativeDesired_ThrowsCatalogDomainException()
    {
        var item = Item(available: 5);

        var ex = Assert.ThrowsExactly<CatalogDomainException>(() => item.RemoveStock(-2));

        Assert.AreEqual("Item units desired should be greater than zero", ex.Message);
        Assert.AreEqual(5, item.AvailableStock);
    }

    [TestMethod]
    public void AddStock_BelowMax_AddsFullQuantityAndClearsOnReorder()
    {
        var item = Item(available: 10, maxThreshold: 100, onReorder: true);

        var added = item.AddStock(5);

        Assert.AreEqual(5, added);
        Assert.AreEqual(15, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_ExceedsMax_CapsAtMaxThreshold()
    {
        var item = Item(available: 90, maxThreshold: 100, onReorder: true);

        var added = item.AddStock(25);

        Assert.AreEqual(10, added);
        Assert.AreEqual(100, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }

    [TestMethod]
    public void AddStock_AlreadyAtMax_AddsZeroAndClearsOnReorder()
    {
        var item = Item(available: 100, maxThreshold: 100, onReorder: true);

        var added = item.AddStock(5);

        Assert.AreEqual(0, added);
        Assert.AreEqual(100, item.AvailableStock);
        Assert.IsFalse(item.OnReorder);
    }
}
