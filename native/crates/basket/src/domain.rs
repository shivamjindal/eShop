//! Basket model and the JSON document Basket.API stores in Redis.
//!
//! The encoding is a contract, not an implementation detail: baskets written by the previous .NET
//! service are still in Redis, and `src/WebApp` reads them back through this service.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

/// A basket as stored under `/basket/{buyer_id}`.
///
/// Field order and PascalCase names mirror `System.Text.Json`'s output for
/// `eShop.Basket.API.Model.CustomerBasket`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomerBasket {
    #[serde(rename = "BuyerId", default)]
    pub buyer_id: Option<String>,
    #[serde(rename = "Items", default)]
    pub items: Vec<BasketItem>,
}

/// Missing properties fall back to CLR defaults, matching `System.Text.Json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasketItem {
    #[serde(rename = "Id")]
    pub id: Option<String>,
    #[serde(rename = "ProductId")]
    pub product_id: i32,
    #[serde(rename = "ProductName")]
    pub product_name: Option<String>,
    /// Kept as a raw JSON number so a decimal read from Redis is written back byte for byte
    /// (.NET writes `0`, never `0.0`).
    #[serde(rename = "UnitPrice")]
    pub unit_price: Number,
    #[serde(rename = "OldUnitPrice")]
    pub old_unit_price: Number,
    #[serde(rename = "Quantity")]
    pub quantity: i32,
    #[serde(rename = "PictureUrl")]
    pub picture_url: Option<String>,
}

impl Default for BasketItem {
    fn default() -> Self {
        Self {
            id: None,
            product_id: 0,
            product_name: None,
            unit_price: Number::from(0),
            old_unit_price: Number::from(0),
            quantity: 0,
            picture_url: None,
        }
    }
}

impl CustomerBasket {
    pub fn new(buyer_id: impl Into<String>) -> Self {
        Self {
            buyer_id: Some(buyer_id.into()),
            items: Vec::new(),
        }
    }

    /// Builds the basket persisted by `UpdateBasket`, which only carries product id and quantity.
    pub fn from_quantities(buyer_id: impl Into<String>, items: &[(i32, i32)]) -> Self {
        Self {
            buyer_id: Some(buyer_id.into()),
            items: items
                .iter()
                .map(|&(product_id, quantity)| BasketItem {
                    product_id,
                    quantity,
                    ..BasketItem::default()
                })
                .collect(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("basket is always serializable")
    }

    /// Reads a stored basket. Basket.API deserialized with `PropertyNameCaseInsensitive = true`,
    /// so documents written with different casing must still load.
    pub fn from_json(raw: &[u8]) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_slice(raw)?;
        serde_json::from_value(normalize_casing(value, &CANONICAL_KEYS))
    }
}

const CANONICAL_KEYS: [&str; 9] = [
    "BuyerId",
    "Items",
    "Id",
    "ProductId",
    "ProductName",
    "UnitPrice",
    "OldUnitPrice",
    "Quantity",
    "PictureUrl",
];

fn normalize_casing(value: Value, canonical: &[&str]) -> Value {
    match value {
        Value::Object(map) => {
            let mut normalized = Map::with_capacity(map.len());
            for (key, child) in map {
                let canonical_key = canonical
                    .iter()
                    .find(|candidate| candidate.eq_ignore_ascii_case(&key))
                    .map_or(key, |candidate| (*candidate).to_owned());
                normalized.insert(canonical_key, normalize_casing(child, canonical));
            }
            Value::Object(normalized)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| normalize_casing(item, canonical))
                .collect(),
        ),
        other => other,
    }
}

/// Redis key for a buyer's basket (`/basket/{buyer_id}` in Basket.API).
pub fn basket_key(buyer_id: &str) -> String {
    format!("/basket/{buyer_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors BasketStorageContractTests.SerializedBasketMatchesStoredContract in
    // tests/Basket.UnitTests (the .NET characterization suite).
    #[test]
    fn serialized_basket_matches_stored_contract() {
        let basket = CustomerBasket {
            buyer_id: Some("alice".into()),
            items: vec![
                BasketItem {
                    product_id: 7,
                    quantity: 2,
                    ..BasketItem::default()
                },
                BasketItem {
                    id: Some("item-2".into()),
                    product_id: 9,
                    product_name: Some("Roslyn Red Sheet".into()),
                    unit_price: Number::from_f64(8.5).unwrap(),
                    old_unit_price: Number::from(9),
                    quantity: 1,
                    picture_url: Some("http://example/pic.png".into()),
                },
            ],
        };

        assert_eq!(
            basket.to_json(),
            r#"{"BuyerId":"alice","Items":[{"Id":null,"ProductId":7,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null},{"Id":"item-2","ProductId":9,"ProductName":"Roslyn Red Sheet","UnitPrice":8.5,"OldUnitPrice":9,"Quantity":1,"PictureUrl":"http://example/pic.png"}]}"#
        );
    }

    #[test]
    fn empty_basket_round_trips() {
        assert_eq!(
            CustomerBasket::new("alice").to_json(),
            r#"{"BuyerId":"alice","Items":[]}"#
        );
    }

    #[test]
    fn deserialization_is_case_insensitive() {
        let basket =
            CustomerBasket::from_json(br#"{"buyerid":"alice","items":[{"productId":7,"QUANTITY":2}]}"#)
                .unwrap();

        assert_eq!(basket.buyer_id.as_deref(), Some("alice"));
        assert_eq!(basket.items.len(), 1);
        assert_eq!(basket.items[0].product_id, 7);
        assert_eq!(basket.items[0].quantity, 2);
    }

    #[test]
    fn stored_document_round_trips_byte_for_byte() {
        let stored = r#"{"BuyerId":"alice","Items":[{"Id":null,"ProductId":7,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null}]}"#;

        let basket = CustomerBasket::from_json(stored.as_bytes()).unwrap();

        assert_eq!(basket.to_json(), stored);
    }

    #[test]
    fn update_basket_persists_only_product_id_and_quantity() {
        let basket = CustomerBasket::from_quantities("alice", &[(7, 2), (9, 1)]);

        assert_eq!(
            basket.to_json(),
            r#"{"BuyerId":"alice","Items":[{"Id":null,"ProductId":7,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null},{"Id":null,"ProductId":9,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":1,"PictureUrl":null}]}"#
        );
    }

    #[test]
    fn basket_key_matches_dotnet_prefix() {
        assert_eq!(basket_key("alice"), "/basket/alice");
    }
}
