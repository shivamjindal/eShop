using System.Linq;
using System.Text;
using System.Text.Json;
using eShop.Basket.API.Model;
using eShop.Basket.API.Repositories;

namespace eShop.Basket.UnitTests;

/// <summary>
/// Characterization tests for the bytes Basket.API writes to Redis. The Rust port has to produce
/// and accept exactly this shape so a basket written by either implementation stays readable
/// during cutover (see plan.md, "Redis value drift").
/// </summary>
[TestClass]
public class BasketStorageContractTests
{
    [TestMethod]
    public void SerializedBasketUsesPascalCasePropertyNames()
    {
        var basket = new CustomerBasket("user-1")
        {
            Items =
            [
                new BasketItem
                {
                    Id = "item-1",
                    ProductId = 3,
                    ProductName = "Adventurer GPS Watch",
                    UnitPrice = 400.5m,
                    OldUnitPrice = 0m,
                    Quantity = 2,
                    PictureUrl = "http://example/3.webp"
                }
            ]
        };

        var json = Encoding.UTF8.GetString(
            JsonSerializer.SerializeToUtf8Bytes(basket, BasketSerializationContext.Default.CustomerBasket));

        Assert.AreEqual(
            """{"BuyerId":"user-1","Items":[{"Id":"item-1","ProductId":3,"ProductName":"Adventurer GPS Watch","UnitPrice":400.5,"OldUnitPrice":0,"Quantity":2,"PictureUrl":"http://example/3.webp"}]}""",
            json);
    }

    [TestMethod]
    public void SerializedBasketWritesNullsForUnsetItemFields()
    {
        var basket = new CustomerBasket("user-1")
        {
            Items = [new BasketItem { ProductId = 1, Quantity = 1 }]
        };

        var json = Encoding.UTF8.GetString(
            JsonSerializer.SerializeToUtf8Bytes(basket, BasketSerializationContext.Default.CustomerBasket));

        Assert.AreEqual(
            """{"BuyerId":"user-1","Items":[{"Id":null,"ProductId":1,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":1,"PictureUrl":null}]}""",
            json);
    }

    [TestMethod]
    public void EmptyBasketSerializesToAnEmptyItemArray()
    {
        var json = Encoding.UTF8.GetString(
            JsonSerializer.SerializeToUtf8Bytes(new CustomerBasket("user-1"), BasketSerializationContext.Default.CustomerBasket));

        Assert.AreEqual("""{"BuyerId":"user-1","Items":[]}""", json);
    }

    [TestMethod]
    public void DeserializationIsCaseInsensitive()
    {
        var json = """{"buyerid":"user-1","items":[{"productId":3,"quantity":2}]}"""u8;

        var basket = JsonSerializer.Deserialize(json, BasketSerializationContext.Default.CustomerBasket);

        Assert.AreEqual("user-1", basket.BuyerId);
        Assert.HasCount(1, basket.Items);
        Assert.AreEqual(3, basket.Items[0].ProductId);
        Assert.AreEqual(2, basket.Items[0].Quantity);
    }

    [TestMethod]
    public void ItemValidationRejectsQuantitiesBelowOne()
    {
        var results = new BasketItem { Quantity = 0 }.Validate(new(new object())).ToList();

        Assert.HasCount(1, results);
        Assert.AreEqual("Invalid number of units", results[0].ErrorMessage);
        Assert.IsEmpty(new BasketItem { Quantity = 1 }.Validate(new(new object())));
    }
}
