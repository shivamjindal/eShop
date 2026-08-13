//! Basket.API host: gRPC server + event bus consumer.
//!
//! Reads the same environment Aspire gave the .NET project: `ConnectionStrings__redis`,
//! `ConnectionStrings__eventbus`, `Identity__Url`, `EventBus__SubscriptionClientName`,
//! and `PORT` for the endpoint Aspire allocated.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use basket::auth::TokenValidator;
use basket::config::{connection_string, env_or, redis_url_from_connection_string};
use basket::events;
use basket::proto::basket_server::BasketServer;
use basket::repository::BasketRepository;
use basket::service::BasketService;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let port: u16 = env_or("PORT", "8080")
        .parse()
        .context("PORT must be a port number")?;
    let address: SocketAddr = format!("0.0.0.0:{port}").parse()?;

    let redis_connection_string = connection_string("redis").unwrap_or_else(|| "localhost".into());
    let redis_url = redis_url_from_connection_string(&redis_connection_string)?;
    let repository = BasketRepository::connect(&redis_url)
        .await
        .context("connecting to redis")?;

    let identity_url = std::env::var("Identity__Url")
        .ok()
        .filter(|value| !value.is_empty());
    if identity_url.is_none() {
        tracing::warn!("Identity__Url is not set; every caller will be anonymous");
    }
    let tokens = Arc::new(TokenValidator::new(identity_url));

    if let Some(amqp_uri) = connection_string("eventbus") {
        let queue = env_or("EventBus__SubscriptionClientName", "Basket");
        tokio::spawn(events::run_consumer(amqp_uri, queue, repository.clone()));
    } else {
        tracing::warn!("ConnectionStrings__eventbus is not set; not subscribing to integration events");
    }

    tracing::info!(%address, "basket-api (rust) listening for gRPC over h2c");

    Server::builder()
        .add_service(BasketServer::new(BasketService::new(repository, tokens)))
        .serve_with_shutdown(address, async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;

    Ok(())
}
