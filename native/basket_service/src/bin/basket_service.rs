//! Entry point for the `basket-api` resource: serves the `BasketApi.Basket` gRPC contract over
//! h2c and consumes integration events from RabbitMQ.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use basket_service::auth::JwtAuthenticator;
use basket_service::config::Config;
use basket_service::events;
use basket_service::proto::basket_server::BasketServer;
use basket_service::service::BasketGrpcService;
use basket_service::storage::RedisBasketRepository;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

const REDIS_CONNECT_ATTEMPTS: usize = 30;
const REDIS_CONNECT_DELAY: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().context("reading configuration")?;
    tracing::info!(
        listen_addr = %config.listen_addr,
        subscription_client_name = %config.subscription_client_name,
        identity_url = ?config.identity_url,
        "starting basket service"
    );

    let repository = Arc::new(connect_to_redis(&config.redis_url).await?);

    let authenticator = Arc::new(JwtAuthenticator::new(config.identity_url.clone())?);
    if authenticator.is_enabled() {
        let authenticator = authenticator.clone();
        // Discovery must not block startup: the identity service may still be coming up.
        tokio::spawn(async move {
            if let Err(error) = authenticator.warm_up().await {
                tracing::warn!(error = %error, "could not preload identity signing keys");
            }
        });
    } else {
        tracing::warn!("no Identity url configured; every caller is anonymous");
    }

    if let Some(event_bus_url) = config.event_bus_url.clone() {
        tokio::spawn(events::run_consumer(
            event_bus_url,
            config.subscription_client_name.clone(),
            repository.clone(),
        ));
    } else {
        tracing::warn!("no event bus connection string configured; order events are not consumed");
    }

    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<BasketServer<BasketGrpcService<RedisBasketRepository, JwtAuthenticator>>>()
        .await;

    let basket = BasketGrpcService::new(repository, authenticator).into_server();

    tracing::info!(listen_addr = %config.listen_addr, "basket service listening");
    Server::builder()
        .add_service(health_service)
        .add_service(basket)
        .serve_with_shutdown(config.listen_addr, shutdown_signal())
        .await
        .context("serving grpc")?;

    Ok(())
}

async fn connect_to_redis(redis_url: &str) -> Result<RedisBasketRepository> {
    let mut last_error = None;

    for attempt in 1..=REDIS_CONNECT_ATTEMPTS {
        match RedisBasketRepository::connect(redis_url).await {
            Ok(repository) => return Ok(repository),
            Err(error) => {
                tracing::warn!(attempt, error = %error, "waiting for redis");
                last_error = Some(error);
                tokio::time::sleep(REDIS_CONNECT_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one attempt")).context("connecting to redis")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(error = %error, "could not listen for SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    tracing::info!("shutting down");
}
