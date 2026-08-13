//! Redis adapter — the port of `RedisBasketRepository`.

use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::model::CustomerBasket;

#[derive(Clone)]
pub struct BasketRepository {
    connection: ConnectionManager,
}

fn basket_key(user_id: &str) -> String {
    format!("/basket/{user_id}")
}

impl BasketRepository {
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            connection: ConnectionManager::new(client).await?,
        })
    }

    pub async fn get_basket(&self, user_id: &str) -> Result<Option<CustomerBasket>> {
        match self.get_raw(user_id).await? {
            Some(bytes) if !bytes.is_empty() => Ok(Some(CustomerBasket::from_json_slice(&bytes)?)),
            _ => Ok(None),
        }
    }

    pub async fn get_raw(&self, user_id: &str) -> Result<Option<Vec<u8>>> {
        let mut connection = self.connection.clone();
        Ok(connection.get(basket_key(user_id)).await?)
    }

    /// `StringSetAsync` then re-read, exactly like the .NET repository.
    pub async fn update_basket(&self, basket: &CustomerBasket) -> Result<Option<CustomerBasket>> {
        let Some(buyer_id) = basket.buyer_id.clone() else {
            return Ok(None);
        };

        let mut connection = self.connection.clone();
        let _: () = connection
            .set(basket_key(&buyer_id), basket.to_json_bytes())
            .await?;

        self.get_basket(&buyer_id).await
    }

    pub async fn delete_basket(&self, user_id: &str) -> Result<bool> {
        let mut connection = self.connection.clone();
        let deleted: i64 = connection.del(basket_key(user_id)).await?;
        Ok(deleted > 0)
    }
}
