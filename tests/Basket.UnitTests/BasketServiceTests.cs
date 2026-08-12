using System.Linq;
using System.Security.Claims;
using Grpc.Core;
using eShop.Basket.API.Repositories;
using eShop.Basket.API.Grpc;
using eShop.Basket.API.Model;
using eShop.Basket.UnitTests.Helpers;
using Microsoft.Extensions.Logging.Abstractions;
using BasketItem = eShop.Basket.API.Model.BasketItem;
using GrpcBasketItem = eShop.Basket.API.Grpc.BasketItem;

namespace eShop.Basket.UnitTests;

[TestClass]
public class BasketServiceTests
{
    public TestContext TestContext { get; set; }

    [TestMethod]
    public async Task GetBasketReturnsEmptyForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);
        var serverCallContext = TestServerCallContext.Create(cancellationToken: TestContext.CancellationToken);
        serverCallContext.SetUserState("__HttpContext", new DefaultHttpContext());

        var response = await service.GetBasket(new GetBasketRequest(), serverCallContext);

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task GetBasketReturnsItemsForValidUserId()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items = [new BasketItem { Id = "some-id" }];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);
        var serverCallContext = TestServerCallContext.Create(cancellationToken: TestContext.CancellationToken);
        var httpContext = new DefaultHttpContext();
        httpContext.User = new ClaimsPrincipal(new ClaimsIdentity([new Claim("sub", "1")]));
        serverCallContext.SetUserState("__HttpContext", httpContext);

        var response = await service.GetBasket(new GetBasketRequest(), serverCallContext);

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.HasCount(1, response.Items);
    }

    [TestMethod]
    public async Task GetBasketReturnsEmptyForInvalidUserId()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items = [new BasketItem { Id = "some-id" }];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);
        var serverCallContext = TestServerCallContext.Create(cancellationToken: TestContext.CancellationToken);
        var httpContext = new DefaultHttpContext();
        serverCallContext.SetUserState("__HttpContext", httpContext);

        var response = await service.GetBasket(new GetBasketRequest(), serverCallContext);

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task GetBasketReturnsEmptyWhenRepositoryHasNoBasket()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult<CustomerBasket>(null));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var response = await service.GetBasket(new GetBasketRequest(), CreateContext("1"));

        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task GetBasketProjectsOnlyProductIdAndQuantity()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items =
        [
            new BasketItem { Id = "some-id", ProductId = 42, ProductName = "Roslyn Red Sheet", UnitPrice = 8.5m, OldUnitPrice = 9m, Quantity = 3, PictureUrl = "http://example/pic" }
        ];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var response = await service.GetBasket(new GetBasketRequest(), CreateContext("1"));

        Assert.HasCount(1, response.Items);
        Assert.AreEqual(42, response.Items[0].ProductId);
        Assert.AreEqual(3, response.Items[0].Quantity);
    }

    [TestMethod]
    public async Task UpdateBasketThrowsUnauthenticatedForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.UpdateBasket(new UpdateBasketRequest(), CreateContext(userId: null)));

        Assert.AreEqual(StatusCode.Unauthenticated, exception.StatusCode);
        Assert.AreEqual("The caller is not authenticated.", exception.Status.Detail);
        await mockRepository.DidNotReceiveWithAnyArgs().UpdateBasketAsync(default);
    }

    [TestMethod]
    public async Task UpdateBasketPersistsRequestItemsUnderCallerIdentity()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        CustomerBasket persisted = null;
        mockRepository.UpdateBasketAsync(Arg.Do<CustomerBasket>(basket => persisted = basket))
            .Returns(callInfo => Task.FromResult(callInfo.Arg<CustomerBasket>()));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);
        var request = new UpdateBasketRequest
        {
            Items = { new GrpcBasketItem { ProductId = 7, Quantity = 2 }, new GrpcBasketItem { ProductId = 9, Quantity = 1 } }
        };

        var response = await service.UpdateBasket(request, CreateContext("alice"));

        Assert.AreEqual("alice", persisted.BuyerId);
        Assert.HasCount(2, persisted.Items);
        Assert.AreEqual(7, persisted.Items[0].ProductId);
        Assert.AreEqual(2, persisted.Items[0].Quantity);
        Assert.HasCount(2, response.Items);
        Assert.AreEqual(9, response.Items[1].ProductId);
    }

    [TestMethod]
    public async Task UpdateBasketThrowsNotFoundWhenPersistFails()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.UpdateBasketAsync(Arg.Any<CustomerBasket>()).Returns(Task.FromResult<CustomerBasket>(null));
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.UpdateBasket(new UpdateBasketRequest(), CreateContext("alice")));

        Assert.AreEqual(StatusCode.NotFound, exception.StatusCode);
        Assert.AreEqual("Basket with buyer id alice does not exist", exception.Status.Detail);
    }

    [TestMethod]
    public async Task DeleteBasketThrowsUnauthenticatedForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.DeleteBasket(new DeleteBasketRequest(), CreateContext(userId: null)));

        Assert.AreEqual(StatusCode.Unauthenticated, exception.StatusCode);
        await mockRepository.DidNotReceiveWithAnyArgs().DeleteBasketAsync(default);
    }

    [TestMethod]
    public async Task DeleteBasketDeletesCallerBasket()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = new BasketService(mockRepository, NullLogger<BasketService>.Instance);

        var response = await service.DeleteBasket(new DeleteBasketRequest(), CreateContext("alice"));

        Assert.IsInstanceOfType<DeleteBasketResponse>(response);
        await mockRepository.Received(1).DeleteBasketAsync("alice");
    }

    private static TestServerCallContext CreateContext(string userId)
    {
        var context = TestServerCallContext.Create();
        var httpContext = new DefaultHttpContext();
        if (userId is not null)
        {
            httpContext.User = new ClaimsPrincipal(new ClaimsIdentity([new Claim("sub", userId)]));
        }

        context.SetUserState("__HttpContext", httpContext);
        return context;
    }
}
