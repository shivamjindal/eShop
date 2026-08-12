//! `basket-api` process: the Aspire resource that replaced `src/Basket.API`.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use basket::auth::TokenValidator;
use basket::config::Config;
use basket::proto::basket_server::BasketServer;
use basket::service::BasketGrpcService;
use basket::storage::BasketRepository;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let repository = BasketRepository::connect(&config.redis_url).await?;

    let tokens = match config.identity_url.as_deref() {
        Some(identity_url) => TokenValidator::new(identity_url)?,
        None => {
            tracing::warn!("Identity__Url is not set; every caller is anonymous");
            TokenValidator::disabled()
        }
    };

    if let Some(amqp_url) = config.amqp_url.clone() {
        let queue_name = config.queue_name.clone();
        let repository = repository.clone();
        tokio::spawn(async move {
            // Basket.API starts its consumer on a background thread and only logs failures; the
            // gRPC surface stays up either way.
            if let Err(error) = basket::events::run(&amqp_url, &queue_name, repository).await {
                tracing::error!(%error, "integration event consumer stopped");
            }
        });
    } else {
        tracing::warn!("ConnectionStrings__eventbus is not set; not consuming integration events");
    }

    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!(%address, "basket-api listening (gRPC over h2c)");

    tonic::transport::Server::builder()
        .add_service(BasketServer::new(BasketGrpcService::new(repository, tokens)))
        .serve_with_shutdown(address, shutdown())
        .await
        .context("gRPC server failed")?;

    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
