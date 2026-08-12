# basket_service

The eShop Basket service, in Rust. It replaces `src/Basket.API` (removed) and is registered in the
Aspire AppHost as the `basket-api` resource, so `WebApp` keeps talking to `http://basket-api` over
the unchanged gRPC contract.

## What it does

| Concern | Implementation |
|---------|----------------|
| gRPC contract | `proto/basket.proto` (source of truth for `WebApp` too), served with `tonic` over h2c |
| Storage | Redis string `/basket/{userId}`, System.Text.Json-compatible PascalCase payload |
| Identity | Bearer JWT validated against the Identity authority's OIDC discovery + JWKS; `sub` is the basket owner |
| Integration events | RabbitMQ `eshop_event_bus` (direct), queue from `EventBus:SubscriptionClientName`, `OrderStartedIntegrationEvent` clears the basket |
| Health | `grpc.health.v1` |

## Configuration

The same environment variables Aspire injects for a .NET service:

| Variable | Meaning | Default |
|----------|---------|---------|
| `ConnectionStrings__redis` | StackExchange.Redis style (`host:port,password=...`) or a `redis://` URL | `localhost` |
| `ConnectionStrings__eventbus` | `amqp://user:pass@host:port` | none (event consumption disabled) |
| `EventBus__SubscriptionClientName` | RabbitMQ queue name | `Basket` |
| `Identity__Url` | OIDC authority used for token validation | none (every caller anonymous) |
| `PORT` / `ASPNETCORE_URLS` / `BASKET_LISTEN_ADDR` | listen address | `0.0.0.0:5221` |
| `RUST_LOG` | `tracing` filter | `info` |

The service reaches the Identity authority over plain HTTP in the Aspire `http` profile
(`ESHOP_USE_HTTP_ENDPOINTS=1`). With the `https` profile the ASP.NET Core development certificate
has to be trusted by the OS certificate store, exactly as Kestrel's JWT bearer handler requires.

## Build and test

```bash
cargo test                 # unit + rule parity tests
cargo build --release      # produces target/release/basket-service
```

From the repository root:

```bash
./scripts/check-basket.sh  # cargo test + release build + parity harness (needs Docker)
```

`parity-client` is the dual-run harness driver used by `scripts/parity-basket.sh`.
