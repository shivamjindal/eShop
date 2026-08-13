//! eShop Basket service, migrated from `src/Basket.API` (.NET) to Rust.
//!
//! The service keeps three contracts identical to the .NET original: the
//! `BasketApi.Basket` gRPC surface, the `/basket/{userId}` Redis document, and the
//! `OrderStartedIntegrationEvent` subscription on the `eshop_event_bus` exchange.

pub mod auth;
pub mod config;
pub mod events;
pub mod model;
pub mod repository;
pub mod service;

pub mod proto {
    // prost names the module after the snake_cased proto package (`package BasketApi`).
    tonic::include_proto!("basket_api");
}
