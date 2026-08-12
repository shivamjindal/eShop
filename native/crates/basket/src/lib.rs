//! eShop basket service, migrated from `src/Basket.API`.
//!
//! Serves the same `BasketApi.Basket` gRPC contract over h2c, stores baskets in the same Redis
//! keyspace with the same JSON document, and consumes `OrderStartedIntegrationEvent` from the same
//! RabbitMQ topology, so `src/WebApp` and the MAUI `ClientApp` talk to it unchanged.

pub mod auth;
pub mod config;
pub mod domain;
pub mod events;
pub mod service;
pub mod storage;

pub mod proto {
    // prost names the generated module after the snake_cased proto package (`package BasketApi`).
    tonic::include_proto!("basket_api");
}
