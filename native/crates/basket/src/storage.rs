//! Redis-backed basket storage (`RedisBasketRepository` in Basket.API).

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::domain::{basket_key, CustomerBasket};

#[derive(Clone)]
pub struct BasketRepository {
    connection: ConnectionManager,
}

impl BasketRepository {
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url).context("invalid Redis connection string")?;
        let connection = ConnectionManager::new(client)
            .await
            .context("could not connect to Redis")?;
        Ok(Self { connection })
    }

    pub async fn get(&self, buyer_id: &str) -> Result<Option<CustomerBasket>> {
        let mut connection = self.connection.clone();
        let stored: Option<Vec<u8>> = connection.get(basket_key(buyer_id)).await?;

        match stored {
            Some(bytes) if !bytes.is_empty() => Ok(Some(CustomerBasket::from_json(&bytes)?)),
            _ => Ok(None),
        }
    }

    /// Writes the basket and reads it back, exactly like `UpdateBasketAsync`.
    pub async fn update(&self, basket: &CustomerBasket) -> Result<Option<CustomerBasket>> {
        let buyer_id = match basket.buyer_id.as_deref() {
            Some(buyer_id) => buyer_id,
            None => return Ok(None),
        };

        let mut connection = self.connection.clone();
        let _: () = connection.set(basket_key(buyer_id), basket.to_json()).await?;
        self.get(buyer_id).await
    }

    pub async fn delete(&self, buyer_id: &str) -> Result<bool> {
        let mut connection = self.connection.clone();
        let removed: i64 = connection.del(basket_key(buyer_id)).await?;
        Ok(removed > 0)
    }
}
