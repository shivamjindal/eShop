# basket

The eShop basket service, migrated from `src/Basket.API` (.NET) to Rust. It is the `basket-api`
resource in the Aspire app model, and `src/WebApp` and the MAUI `ClientApp` call it unchanged.

## Binaries

| Binary | Purpose |
|--------|---------|
| `basket-service` | The service. gRPC (`BasketApi.Basket`) over h2c. |
| `basket-parity` | Records/replays the behavioral transcript used by `scripts/parity-basket.sh`. |

## Contracts it must not break

- **gRPC** — `proto/basket.proto` is the single copy `src/WebApp` compiles its client from.
- **Redis** — key `/basket/{userId}`, PascalCase `System.Text.Json` document; see `domain.rs`.
- **RabbitMQ** — direct exchange `eshop_event_bus` (transient), durable queue `Basket`, routing key
  `OrderStartedIntegrationEvent`.
- **Auth** — JWT bearer from `Identity__Url`; audience is not validated; the user id is the `sub`
  claim; an unusable token makes the caller anonymous instead of failing the call.

## Environment

Injected by the Aspire AppHost, matching what the .NET project consumed:

| Variable | Meaning |
|----------|---------|
| `PORT` | Listen port (from `WithHttpEndpoint(env: "PORT")`) |
| `ConnectionStrings__redis` | StackExchange format (`host:port,password=…`) or a `redis://` URL |
| `ConnectionStrings__eventbus` | `amqp://user:pass@host:port`; without it the event consumer is off |
| `Identity__Url` | Token issuer; without it every caller is anonymous |
| `EventBus__SubscriptionClientName` | Queue name, defaults to `Basket` |

## Checks

```bash
./scripts/check-basket.sh     # cargo test + release build (Docker-free)
./scripts/parity-basket.sh    # replay the recorded .NET transcript (needs Docker)
```
