using System.Text;
using System.Text.Json;
using eShop.Basket.API.Model;
using eShop.Basket.API.Repositories;

namespace eShop.Basket.UnitTests;

/// <summary>
/// Locks the bytes Basket.API writes to Redis under <c>/basket/{userId}</c>. Any reimplementation of
/// the service has to produce the same document or previously stored baskets become unreadable.
/// </summary>
[TestClass]
public class BasketStorageContractTests
{
    [TestMethod]
    public void SerializedBasketMatchesStoredContract()
    {
        var basket = new CustomerBasket("alice")
        {
            Items =
            [
                new BasketItem { ProductId = 7, Quantity = 2 },
                new BasketItem
                {
                    Id = "item-2",
                    ProductId = 9,
                    ProductName = "Roslyn Red Sheet",
                    UnitPrice = 8.5m,
                    OldUnitPrice = 9m,
                    Quantity = 1,
                    PictureUrl = "http://example/pic.png"
                }
            ]
        };

        var json = Encoding.UTF8.GetString(
            JsonSerializer.SerializeToUtf8Bytes(basket, BasketSerializationContext.Default.CustomerBasket));

        Assert.AreEqual(
            """
            {"BuyerId":"alice","Items":[{"Id":null,"ProductId":7,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null},{"Id":"item-2","ProductId":9,"ProductName":"Roslyn Red Sheet","UnitPrice":8.5,"OldUnitPrice":9,"Quantity":1,"PictureUrl":"http://example/pic.png"}]}
            """,
            json);
    }

    [TestMethod]
    public void DeserializationIsCaseInsensitive()
    {
        const string stored = """{"buyerid":"alice","items":[{"productId":7,"QUANTITY":2}]}""";

        var basket = JsonSerializer.Deserialize(
            Encoding.UTF8.GetBytes(stored), BasketSerializationContext.Default.CustomerBasket);

        Assert.AreEqual("alice", basket.BuyerId);
        Assert.HasCount(1, basket.Items);
        Assert.AreEqual(7, basket.Items[0].ProductId);
        Assert.AreEqual(2, basket.Items[0].Quantity);
    }

    [TestMethod]
    public void EmptyBasketRoundTrips()
    {
        var json = Encoding.UTF8.GetString(
            JsonSerializer.SerializeToUtf8Bytes(new CustomerBasket("alice"), BasketSerializationContext.Default.CustomerBasket));

        Assert.AreEqual("""{"BuyerId":"alice","Items":[]}""", json);
    }
}
