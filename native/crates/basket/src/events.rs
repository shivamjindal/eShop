//! RabbitMQ consumer for `OrderStartedIntegrationEvent`.
//!
//! Topology and message shape come from `src/EventBusRabbitMQ/RabbitMQEventBus.cs`: a direct
//! exchange `eshop_event_bus` (transient), one durable queue per service, the CLR type name as the
//! routing key, PascalCase JSON bodies, and an ack even when handling fails.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties, ExchangeKind};
use serde::Deserialize;

use crate::storage::BasketRepository;

const EXCHANGE_NAME: &str = "eshop_event_bus";
const ORDER_STARTED: &str = "OrderStartedIntegrationEvent";

#[derive(Debug, Deserialize)]
struct OrderStartedIntegrationEvent {
    #[serde(rename = "UserId")]
    user_id: Option<String>,
}

pub async fn run(amqp_url: &str, queue_name: &str, repository: BasketRepository) -> Result<()> {
    let connection = Connection::connect(amqp_url, ConnectionProperties::default())
        .await
        .context("could not connect to RabbitMQ")?;
    let channel = connection.create_channel().await?;

    channel
        .exchange_declare(
            EXCHANGE_NAME.into(),
            ExchangeKind::Direct,
            // RabbitMQ.Client's ExchangeDeclareAsync defaults durable to false, and the publisher
            // relies on that default. Declaring it durable here fails with PRECONDITION_FAILED.
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_declare(
            queue_name.into(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_bind(
            queue_name.into(),
            EXCHANGE_NAME.into(),
            ORDER_STARTED.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let mut consumer = channel
        .basic_consume(
            queue_name.into(),
            "basket-service".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    tracing::info!(queue = queue_name, "listening for integration events");

    while let Some(delivery) = consumer.next().await {
        let delivery = match delivery {
            Ok(delivery) => delivery,
            Err(error) => {
                tracing::warn!(%error, "integration event delivery failed");
                continue;
            }
        };

        if delivery.routing_key.as_str() == ORDER_STARTED {
            if let Err(error) = handle_order_started(&delivery.data, &repository).await {
                // Basket.API logs and acks anyway; there is no dead-letter exchange.
                tracing::warn!(%error, "error processing OrderStartedIntegrationEvent");
            }
        } else {
            tracing::warn!(
                routing_key = %delivery.routing_key,
                "unable to resolve event type for event name"
            );
        }

        delivery.ack(BasicAckOptions::default()).await?;
    }

    Ok(())
}

async fn handle_order_started(body: &[u8], repository: &BasketRepository) -> Result<()> {
    let event: OrderStartedIntegrationEvent =
        serde_json::from_slice(body).context("malformed OrderStartedIntegrationEvent")?;

    let Some(user_id) = event.user_id.filter(|user_id| !user_id.is_empty()) else {
        return Ok(());
    };

    tracing::info!(user_id, "handling OrderStartedIntegrationEvent");
    repository.delete(&user_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_deserializes_from_the_published_envelope() {
        let published = br#"{"UserId":"alice","Id":"6b7c0b3e-4c1a-4a2f-9d3f-1e5a2b3c4d5e","CreationDate":"2026-08-12T10:00:00Z"}"#;

        let event: OrderStartedIntegrationEvent = serde_json::from_slice(published).unwrap();

        assert_eq!(event.user_id.as_deref(), Some("alice"));
    }
}
