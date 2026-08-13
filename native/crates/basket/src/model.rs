//! The Redis document shape, byte-compatible with the .NET `CustomerBasket`.
//!
//! The .NET side serialized with `System.Text.Json` source generation and
//! `PropertyNameCaseInsensitive = true`, so: PascalCase names, declaration order,
//! `null` for absent strings, case-insensitive reads and CLR defaults for missing
//! properties. Money uses `serde_json::Number` because .NET writes `decimal` zero as
//! `0`, which an `f64` would render as `0.0`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

fn zero() -> Number {
    Number::from(0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasketItem {
    #[serde(rename = "Id", default)]
    pub id: Option<String>,
    #[serde(rename = "ProductId", default)]
    pub product_id: i32,
    #[serde(rename = "ProductName", default)]
    pub product_name: Option<String>,
    #[serde(rename = "UnitPrice", default = "zero")]
    pub unit_price: Number,
    #[serde(rename = "OldUnitPrice", default = "zero")]
    pub old_unit_price: Number,
    #[serde(rename = "Quantity", default)]
    pub quantity: i32,
    #[serde(rename = "PictureUrl", default)]
    pub picture_url: Option<String>,
}

impl Default for BasketItem {
    fn default() -> Self {
        Self {
            id: None,
            product_id: 0,
            product_name: None,
            unit_price: zero(),
            old_unit_price: zero(),
            quantity: 0,
            picture_url: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CustomerBasket {
    #[serde(rename = "BuyerId", default)]
    pub buyer_id: Option<String>,
    #[serde(rename = "Items", default, deserialize_with = "items_or_empty")]
    pub items: Vec<BasketItem>,
}

fn items_or_empty<'de, D>(deserializer: D) -> Result<Vec<BasketItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<BasketItem>>::deserialize(deserializer)?.unwrap_or_default())
}

impl CustomerBasket {
    pub fn new(buyer_id: impl Into<String>) -> Self {
        Self {
            buyer_id: Some(buyer_id.into()),
            items: Vec::new(),
        }
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("CustomerBasket is always serializable")
    }

    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_slice(bytes)?;
        serde_json::from_value(normalize_keys(value))
    }
}

/// Mirrors `PropertyNameCaseInsensitive = true`: JSON written by another casing
/// convention still binds to the PascalCase fields above.
fn normalize_keys(value: Value) -> Value {
    const KNOWN: [&str; 9] = [
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

    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, item) in map {
                let canonical = KNOWN
                    .iter()
                    .find(|known| known.eq_ignore_ascii_case(&key))
                    .map(|known| (*known).to_string())
                    .unwrap_or(key);
                out.insert(canonical, normalize_keys(item));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_keys).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(product_id: i32, quantity: i32) -> BasketItem {
        BasketItem {
            product_id,
            quantity,
            ..Default::default()
        }
    }

    #[test]
    fn serializes_exactly_like_system_text_json() {
        let basket = CustomerBasket {
            buyer_id: Some("alice".into()),
            items: vec![item(1, 2)],
        };

        assert_eq!(
            String::from_utf8(basket.to_json_bytes()).unwrap(),
            r#"{"BuyerId":"alice","Items":[{"Id":null,"ProductId":1,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":2,"PictureUrl":null}]}"#
        );
    }

    #[test]
    fn writes_decimal_zero_without_a_fraction() {
        let json = String::from_utf8(CustomerBasket::new("bob").to_json_bytes()).unwrap();
        assert_eq!(json, r#"{"BuyerId":"bob","Items":[]}"#);

        let one_item = CustomerBasket {
            buyer_id: Some("bob".into()),
            items: vec![item(7, 1)],
        };
        assert!(String::from_utf8(one_item.to_json_bytes())
            .unwrap()
            .contains(r#""UnitPrice":0,"OldUnitPrice":0"#));
    }

    #[test]
    fn round_trips_a_document_written_by_dotnet() {
        let dotnet = br#"{"BuyerId":"alice","Items":[{"Id":"abc","ProductId":3,"ProductName":"Hat","UnitPrice":19.5,"OldUnitPrice":0,"Quantity":4,"PictureUrl":"http://x/1.png"}]}"#;
        let basket = CustomerBasket::from_json_slice(dotnet).unwrap();

        assert_eq!(basket.buyer_id.as_deref(), Some("alice"));
        assert_eq!(basket.items[0].product_name.as_deref(), Some("Hat"));
        assert_eq!(basket.items[0].unit_price.to_string(), "19.5");
        assert_eq!(basket.to_json_bytes(), dotnet.to_vec());
    }

    #[test]
    fn reads_case_insensitively_and_defaults_missing_properties() {
        let loose = br#"{"buyerid":"alice","items":[{"productid":9,"quantity":2}]}"#;
        let basket = CustomerBasket::from_json_slice(loose).unwrap();

        assert_eq!(basket.buyer_id.as_deref(), Some("alice"));
        assert_eq!(basket.items[0].product_id, 9);
        assert_eq!(basket.items[0].quantity, 2);
        assert_eq!(basket.items[0].id, None);
        assert_eq!(basket.items[0].unit_price.to_string(), "0");
    }

    #[test]
    fn ignores_unknown_properties_like_system_text_json() {
        let extra = br#"{"BuyerId":"alice","Items":[],"Whatever":123}"#;
        assert_eq!(
            CustomerBasket::from_json_slice(extra).unwrap(),
            CustomerBasket::new("alice")
        );
    }
}
