//! eShop Basket service in Rust: the gRPC contract, Redis storage, JWT identity and integration
//! event consumer previously implemented by `src/Basket.API`.

pub mod auth;
pub mod config;
pub mod domain;
pub mod events;
pub mod service;
pub mod storage;

pub mod proto {
    tonic::include_proto!("basket_api");
}
