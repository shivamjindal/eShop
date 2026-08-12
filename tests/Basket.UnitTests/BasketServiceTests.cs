using System.Linq;
using System.Security.Claims;
using eShop.Basket.API.Repositories;
using eShop.Basket.API.Grpc;
using eShop.Basket.API.Model;
using eShop.Basket.UnitTests.Helpers;
using Grpc.Core;
using Microsoft.Extensions.Logging.Abstractions;
using BasketItem = eShop.Basket.API.Model.BasketItem;

namespace eShop.Basket.UnitTests;

/// <summary>
/// Characterization tests for the Basket.API gRPC surface. They pin the behavior the Rust
/// port in native/basket_service has to reproduce (see plan.md, unit 1).
/// </summary>
[TestClass]
public class BasketServiceTests
{
    public TestContext TestContext { get; set; }

    private static ServerCallContext AnonymousContext()
    {
        var context = TestServerCallContext.Create();
        context.SetUserState("__HttpContext", new DefaultHttpContext());
        return context;
    }

    private static ServerCallContext ContextForUser(string userId)
    {
        var context = TestServerCallContext.Create();
        var httpContext = new DefaultHttpContext
        {
            User = new ClaimsPrincipal(new ClaimsIdentity([new Claim("sub", userId)]))
        };
        context.SetUserState("__HttpContext", httpContext);
        return context;
    }

    private static BasketService CreateService(IBasketRepository repository)
        => new(repository, NullLogger<BasketService>.Instance);

    [TestMethod]
    public async Task GetBasketReturnsEmptyForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = CreateService(mockRepository);

        var response = await service.GetBasket(new GetBasketRequest(), AnonymousContext());

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.IsEmpty(response.Items);
        await mockRepository.DidNotReceiveWithAnyArgs().GetBasketAsync(default);
    }

    [TestMethod]
    public async Task GetBasketReturnsItemsForValidUserId()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items = [new BasketItem { Id = "some-id" }];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = CreateService(mockRepository);

        var response = await service.GetBasket(new GetBasketRequest(), ContextForUser("1"));

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.HasCount(1, response.Items);
    }

    [TestMethod]
    public async Task GetBasketReturnsEmptyForInvalidUserId()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items = [new BasketItem { Id = "some-id" }];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = CreateService(mockRepository);

        var response = await service.GetBasket(new GetBasketRequest(), AnonymousContext());

        Assert.IsInstanceOfType<CustomerBasketResponse>(response);
        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task GetBasketReturnsEmptyWhenRepositoryHasNoBasket()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult<CustomerBasket>(null));
        var service = CreateService(mockRepository);

        var response = await service.GetBasket(new GetBasketRequest(), ContextForUser("1"));

        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task GetBasketProjectsOnlyProductIdAndQuantity()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        List<BasketItem> items =
        [
            new BasketItem { Id = "some-id", ProductId = 7, Quantity = 3, ProductName = "ignored", UnitPrice = 12.5m, PictureUrl = "ignored" }
        ];
        mockRepository.GetBasketAsync("1").Returns(Task.FromResult(new CustomerBasket { BuyerId = "1", Items = items }));
        var service = CreateService(mockRepository);

        var response = await service.GetBasket(new GetBasketRequest(), ContextForUser("1"));

        Assert.HasCount(1, response.Items);
        Assert.AreEqual(7, response.Items[0].ProductId);
        Assert.AreEqual(3, response.Items[0].Quantity);
    }

    [TestMethod]
    public async Task UpdateBasketThrowsUnauthenticatedForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = CreateService(mockRepository);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.UpdateBasket(new UpdateBasketRequest(), AnonymousContext()));

        Assert.AreEqual(StatusCode.Unauthenticated, exception.StatusCode);
        await mockRepository.DidNotReceiveWithAnyArgs().UpdateBasketAsync(default);
    }

    [TestMethod]
    public async Task UpdateBasketPersistsItemsForTheAuthenticatedUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.UpdateBasketAsync(Arg.Any<CustomerBasket>())
            .Returns(callInfo => Task.FromResult(callInfo.Arg<CustomerBasket>()));
        var service = CreateService(mockRepository);

        var request = new UpdateBasketRequest();
        request.Items.Add(new eShop.Basket.API.Grpc.BasketItem { ProductId = 1, Quantity = 2 });
        request.Items.Add(new eShop.Basket.API.Grpc.BasketItem { ProductId = 5, Quantity = 9 });

        var response = await service.UpdateBasket(request, ContextForUser("user-1"));

        var persisted = mockRepository.ReceivedCalls().Single().GetArguments().OfType<CustomerBasket>().Single();
        Assert.AreEqual("user-1", persisted.BuyerId);
        Assert.HasCount(2, persisted.Items);
        Assert.AreEqual(1, persisted.Items[0].ProductId);
        Assert.AreEqual(2, persisted.Items[0].Quantity);

        Assert.HasCount(2, response.Items);
        Assert.AreEqual(5, response.Items[1].ProductId);
        Assert.AreEqual(9, response.Items[1].Quantity);
    }

    [TestMethod]
    public async Task UpdateBasketAcceptsAnEmptyItemList()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.UpdateBasketAsync(Arg.Any<CustomerBasket>())
            .Returns(callInfo => Task.FromResult(callInfo.Arg<CustomerBasket>()));
        var service = CreateService(mockRepository);

        var response = await service.UpdateBasket(new UpdateBasketRequest(), ContextForUser("user-1"));

        Assert.IsEmpty(response.Items);
    }

    [TestMethod]
    public async Task UpdateBasketThrowsNotFoundWhenRepositoryReturnsNull()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.UpdateBasketAsync(Arg.Any<CustomerBasket>()).Returns(Task.FromResult<CustomerBasket>(null));
        var service = CreateService(mockRepository);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.UpdateBasket(new UpdateBasketRequest(), ContextForUser("user-1")));

        Assert.AreEqual(StatusCode.NotFound, exception.StatusCode);
        Assert.Contains("user-1", exception.Status.Detail);
    }

    [TestMethod]
    public async Task DeleteBasketThrowsUnauthenticatedForNoUser()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        var service = CreateService(mockRepository);

        var exception = await Assert.ThrowsExactlyAsync<RpcException>(
            () => service.DeleteBasket(new DeleteBasketRequest(), AnonymousContext()));

        Assert.AreEqual(StatusCode.Unauthenticated, exception.StatusCode);
        await mockRepository.DidNotReceiveWithAnyArgs().DeleteBasketAsync(default);
    }

    [TestMethod]
    public async Task DeleteBasketSucceedsEvenWhenNoBasketExists()
    {
        var mockRepository = Substitute.For<IBasketRepository>();
        mockRepository.DeleteBasketAsync("user-1").Returns(Task.FromResult(false));
        var service = CreateService(mockRepository);

        var response = await service.DeleteBasket(new DeleteBasketRequest(), ContextForUser("user-1"));

        Assert.IsInstanceOfType<DeleteBasketResponse>(response);
        await mockRepository.Received(1).DeleteBasketAsync("user-1");
    }
}
