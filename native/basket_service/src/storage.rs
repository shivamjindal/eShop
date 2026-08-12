//! Redis-backed basket storage — port of `src/Basket.API/Repositories/RedisBasketRepository.cs`.

use async_trait::async_trait;
use redis::aio::ConnectionManager;

use crate::domain::{basket_key, CustomerBasket};

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("stored basket is not valid json: {0}")]
    Deserialization(#[from] serde_json::Error),
}

#[async_trait]
pub trait BasketRepository: Send + Sync + 'static {
    async fn get_basket(
        &self,
        customer_id: &str,
    ) -> Result<Option<CustomerBasket>, RepositoryError>;

    /// Writes the basket and reads it back, like the .NET repository. `Ok(None)` means the write
    /// did not stick, which the gRPC layer turns into `NotFound`.
    async fn update_basket(
        &self,
        basket: &CustomerBasket,
    ) -> Result<Option<CustomerBasket>, RepositoryError>;

    async fn delete_basket(&self, customer_id: &str) -> Result<bool, RepositoryError>;
}

#[derive(Clone)]
pub struct RedisBasketRepository {
    connection: ConnectionManager,
}

impl RedisBasketRepository {
    pub async fn connect(redis_url: &str) -> Result<Self, RepositoryError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            connection: ConnectionManager::new(client).await?,
        })
    }
}

#[async_trait]
impl BasketRepository for RedisBasketRepository {
    async fn get_basket(
        &self,
        customer_id: &str,
    ) -> Result<Option<CustomerBasket>, RepositoryError> {
        let mut connection = self.connection.clone();
        let data: Option<Vec<u8>> = redis::cmd("GET")
            .arg(basket_key(customer_id))
            .query_async(&mut connection)
            .await?;

        match data {
            Some(bytes) if !bytes.is_empty() => Ok(Some(serde_json::from_slice(&bytes)?)),
            _ => Ok(None),
        }
    }

    async fn update_basket(
        &self,
        basket: &CustomerBasket,
    ) -> Result<Option<CustomerBasket>, RepositoryError> {
        let mut connection = self.connection.clone();
        let payload = serde_json::to_vec(basket)?;
        let _: () = redis::cmd("SET")
            .arg(basket_key(&basket.buyer_id))
            .arg(payload)
            .query_async(&mut connection)
            .await?;

        self.get_basket(&basket.buyer_id).await
    }

    async fn delete_basket(&self, customer_id: &str) -> Result<bool, RepositoryError> {
        let mut connection = self.connection.clone();
        let deleted: i64 = redis::cmd("DEL")
            .arg(basket_key(customer_id))
            .query_async(&mut connection)
            .await?;

        Ok(deleted > 0)
    }
}

#[cfg(test)]
pub struct MemoryBasketRepository {
    baskets: std::sync::Mutex<std::collections::HashMap<String, CustomerBasket>>,
    fail_writes: bool,
}

#[cfg(test)]
impl Default for MemoryBasketRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MemoryBasketRepository {
    pub fn new() -> Self {
        Self {
            baskets: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail_writes: false,
        }
    }

    /// Reproduces the .NET path where `StringSetAsync` reports the write did not happen.
    pub fn rejecting_writes() -> Self {
        Self {
            baskets: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail_writes: true,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl BasketRepository for MemoryBasketRepository {
    async fn get_basket(
        &self,
        customer_id: &str,
    ) -> Result<Option<CustomerBasket>, RepositoryError> {
        Ok(self.baskets.lock().unwrap().get(customer_id).cloned())
    }

    async fn update_basket(
        &self,
        basket: &CustomerBasket,
    ) -> Result<Option<CustomerBasket>, RepositoryError> {
        if self.fail_writes {
            return Ok(None);
        }
        self.baskets
            .lock()
            .unwrap()
            .insert(basket.buyer_id.clone(), basket.clone());
        Ok(Some(basket.clone()))
    }

    async fn delete_basket(&self, customer_id: &str) -> Result<bool, RepositoryError> {
        Ok(self.baskets.lock().unwrap().remove(customer_id).is_some())
    }
}
