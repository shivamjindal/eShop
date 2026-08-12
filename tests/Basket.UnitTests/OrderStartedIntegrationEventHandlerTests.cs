using System.Text;
using System.Text.Json;
using eShop.Basket.API.IntegrationEvents.EventHandling;
using eShop.Basket.API.IntegrationEvents.EventHandling.Events;
using eShop.Basket.API.Repositories;
using Microsoft.Extensions.Logging.Abstractions;

namespace eShop.Basket.UnitTests;

[TestClass]
public class OrderStartedIntegrationEventHandlerTests
{
    [TestMethod]
    public async Task HandleDeletesTheBasketOfTheUserWhoStartedTheOrder()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var handler = new OrderStartedIntegrationEventHandler(
            mockRepository, NullLogger<OrderStartedIntegrationEventHandler>.Instance);

        await handler.Handle(new OrderStartedIntegrationEvent("alice"));

        await mockRepository.Received(1).DeleteBasketAsync("alice");
    }

    [TestMethod]
    public void EventDeserializesFromThePublishedEnvelope()
    {
        // Ordering.API publishes PascalCase JSON with the CLR type name as the routing key.
        const string published = """{"UserId":"alice","Id":"6b7c0b3e-4c1a-4a2f-9d3f-1e5a2b3c4d5e","CreationDate":"2026-08-12T10:00:00Z"}""";

        var @event = JsonSerializer.Deserialize<OrderStartedIntegrationEvent>(Encoding.UTF8.GetBytes(published));

        Assert.AreEqual("alice", @event.UserId);
    }
}
