//! Integration event consumer — port of `OrderStartedIntegrationEventHandler` plus the
//! `EventBusRabbitMQ` topology it relies on.
//!
//! The topology has to match `src/EventBusRabbitMQ/RabbitMQEventBus.cs` exactly (exchange
//! `eshop_event_bus`, direct, non-durable; durable queue named after
//! `EventBus:SubscriptionClientName`; routing key = event type name), otherwise RabbitMQ rejects
//! the declaration or the events never arrive.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_lite::StreamExt;
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
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct OrderStartedIntegrationEvent {
    #[serde(rename = "Id", alias = "id", default)]
    id: Option<String>,
    #[serde(rename = "UserId", alias = "userId")]
    user_id: String,
}

/// Consumes integration events until the process shuts down, reconnecting on failure.
pub async fn run_consumer<R: BasketRepository>(
    amqp_url: String,
    queue_name: String,
    repository: Arc<R>,
) {
    loop {
        if let Err(error) = consume(&amqp_url, &queue_name, repository.clone()).await {
            tracing::error!(error = %error, "event bus consumer stopped; reconnecting");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn consume<R: BasketRepository>(
    amqp_url: &str,
    queue_name: &str,
    repository: Arc<R>,
) -> Result<()> {
    let connection = Connection::connect(amqp_url, ConnectionProperties::default())
        .await
        .context("connecting to the event bus")?;
    let channel = connection.create_channel().await?;

    channel
        .exchange_declare(
            EXCHANGE_NAME,
            ExchangeKind::Direct,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("declaring the event bus exchange")?;

    channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .context("declaring the subscription queue")?;

    channel
        .queue_bind(
            queue_name,
            EXCHANGE_NAME,
            ORDER_STARTED,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("binding the subscription queue")?;

    let mut consumer = channel
        .basic_consume(
            queue_name,
            "basket-service",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("starting the consumer")?;

    tracing::info!(queue = %queue_name, "subscribed to integration events");

    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.context("receiving an integration event")?;

        handle(
            delivery.routing_key.as_str(),
            &delivery.data,
            repository.as_ref(),
        )
        .await;

        // The .NET bus acknowledges even when the handler throws (there is no dead-letter
        // exchange), so a poison message must not block the queue here either.
        delivery.ack(BasicAckOptions::default()).await?;
    }

    Ok(())
}

async fn handle<R: BasketRepository + ?Sized>(routing_key: &str, body: &[u8], repository: &R) {
    if routing_key != ORDER_STARTED {
        tracing::warn!(routing_key, "unable to resolve event type for event name");
        return;
    }

    let event: OrderStartedIntegrationEvent = match serde_json::from_slice(body) {
        Ok(event) => event,
        Err(error) => {
            tracing::error!(error = %error, "could not deserialize OrderStartedIntegrationEvent");
            return;
        }
    };

    tracing::info!(
        integration_event_id = ?event.id,
        user_id = %event.user_id,
        "Handling integration event: OrderStartedIntegrationEvent"
    );

    if let Err(error) = repository.delete_basket(&event.user_id).await {
        tracing::error!(error = %error, "could not delete the basket for the started order");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{basket_from_wire, WireItem};
    use crate::storage::MemoryBasketRepository;

    #[tokio::test]
    async fn order_started_deletes_the_basket() {
        let repository = MemoryBasketRepository::new();
        repository
            .update_basket(&basket_from_wire(
                "user-1",
                &[WireItem {
                    product_id: 1,
                    quantity: 1,
                }],
            ))
            .await
            .unwrap();

        let body = br#"{"Id":"c5e8f2e0-1f3a-4b1e-9a5a-0f6f4e2b0a11","CreationDate":"2026-08-12T03:34:00.1234567Z","UserId":"user-1"}"#;
        handle(ORDER_STARTED, body, &repository).await;

        assert!(repository.get_basket("user-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn events_for_other_routing_keys_are_ignored() {
        let repository = MemoryBasketRepository::new();
        repository
            .update_basket(&basket_from_wire("user-1", &[]))
            .await
            .unwrap();

        handle(
            "SomeOtherIntegrationEvent",
            br#"{"UserId":"user-1"}"#,
            &repository,
        )
        .await;

        assert!(repository.get_basket("user-1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn malformed_payloads_do_not_touch_storage() {
        let repository = MemoryBasketRepository::new();
        repository
            .update_basket(&basket_from_wire("user-1", &[]))
            .await
            .unwrap();

        handle(ORDER_STARTED, b"not json", &repository).await;

        assert!(repository.get_basket("user-1").await.unwrap().is_some());
    }
}
