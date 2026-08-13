//! RabbitMQ subscription — the port of `OrderStartedIntegrationEventHandler`.
//!
//! Topology has to match `EventBusRabbitMQ` exactly: the direct exchange
//! `eshop_event_bus` is declared **non-durable** there (the eShop call site relies on
//! `IChannelExtensions.ExchangeDeclareAsync` defaulting `durable` to false), while the
//! per-service queue is durable. Declaring the exchange durable gets PRECONDITION_FAILED.

use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties, ExchangeKind};
use serde_json::Value;

use crate::repository::BasketRepository;

pub const EXCHANGE: &str = "eshop_event_bus";
pub const ORDER_STARTED: &str = "OrderStartedIntegrationEvent";

/// Reconnects forever: the event bus outliving a broker restart is the .NET behavior
/// the AppHost depends on.
pub async fn run_consumer(amqp_uri: String, queue_name: String, repository: BasketRepository) {
    loop {
        if let Err(error) = consume(&amqp_uri, &queue_name, &repository).await {
            tracing::error!(%error, "event bus consumer stopped; reconnecting");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn consume(amqp_uri: &str, queue_name: &str, repository: &BasketRepository) -> Result<()> {
    let connection = Connection::connect(amqp_uri, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;

    channel
        .exchange_declare(
            EXCHANGE.into(),
            ExchangeKind::Direct,
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
            EXCHANGE.into(),
            ORDER_STARTED.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let mut consumer = channel
        .basic_consume(
            queue_name.into(),
            format!("{queue_name}-rust").into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    tracing::info!(queue = queue_name, "subscribed to {ORDER_STARTED}");

    while let Some(delivery) = consumer.next().await {
        let delivery = delivery?;
        let routing_key = delivery.routing_key.to_string();

        if routing_key == ORDER_STARTED {
            match user_id_from_event(&delivery.data) {
                Some(user_id) => {
                    // The .NET handler logs and acks even when the side effect fails.
                    if let Err(error) = repository.delete_basket(&user_id).await {
                        tracing::error!(%error, %user_id, "failed to delete basket for order");
                    } else {
                        tracing::info!(%user_id, "deleted basket after {ORDER_STARTED}");
                    }
                }
                None => tracing::error!(%routing_key, "event payload has no UserId"),
            }
        } else {
            tracing::warn!(%routing_key, "ignoring event with no subscription");
        }

        delivery.ack(BasicAckOptions::default()).await?;
    }

    Ok(())
}

/// The .NET event bus deserializes with case-insensitive property matching.
pub fn user_id_from_event(payload: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let object = value.as_object()?;
    object
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("userid"))
        .and_then(|(_, value)| value.as_str())
        .map(str::to_string)
        .filter(|user_id| !user_id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_user_id_from_the_dotnet_envelope() {
        let payload = br#"{"UserId":"alice","Id":"0a2c8f1e","CreationDate":"2026-01-01T00:00:00Z"}"#;
        assert_eq!(user_id_from_event(payload).as_deref(), Some("alice"));
    }

    #[test]
    fn matches_property_names_case_insensitively() {
        assert_eq!(
            user_id_from_event(br#"{"userId":"bob"}"#).as_deref(),
            Some("bob")
        );
    }

    #[test]
    fn rejects_payloads_without_a_user() {
        assert_eq!(user_id_from_event(br#"{"Id":"1"}"#), None);
        assert_eq!(user_id_from_event(br#"{"UserId":""}"#), None);
        assert_eq!(user_id_from_event(b"not json"), None);
    }
}
