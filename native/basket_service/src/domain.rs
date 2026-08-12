//! Pure basket rules ported from `src/Basket.API` (models, key naming, identity gate).
//!
//! Nothing in this module performs I/O, so every rule the .NET characterization tests pin can be
//! exercised with `cargo test`.

use serde::{Deserialize, Serialize};
use serde_json::Number;

/// Redis key prefix used by the .NET service (`RedisBasketRepository.BasketKeyPrefix`).
pub const BASKET_KEY_PREFIX: &str = "/basket/";

/// Validation message from `BasketItem.Validate`.
pub const INVALID_QUANTITY_MESSAGE: &str = "Invalid number of units";

pub fn basket_key(user_id: &str) -> String {
    format!("{BASKET_KEY_PREFIX}{user_id}")
}

fn zero() -> Number {
    Number::from(0)
}

/// Mirrors `eShop.Basket.API.Model.CustomerBasket`.
///
/// Field order and PascalCase names reproduce the System.Text.Json output byte for byte, so a
/// basket written by either implementation stays readable by the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomerBasket {
    #[serde(rename = "BuyerId", alias = "buyerId", default)]
    pub buyer_id: String,
    #[serde(rename = "Items", alias = "items", default)]
    pub items: Vec<BasketItem>,
}

impl CustomerBasket {
    pub fn new(buyer_id: impl Into<String>) -> Self {
        Self {
            buyer_id: buyer_id.into(),
            items: Vec::new(),
        }
    }
}

/// Mirrors `eShop.Basket.API.Model.BasketItem`.
///
/// `UnitPrice` / `OldUnitPrice` are `serde_json::Number` rather than `f64` because .NET writes a
/// `decimal` (`0`, not `0.0`) and the parity harness compares the stored bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasketItem {
    #[serde(rename = "Id", alias = "id", default)]
    pub id: Option<String>,
    #[serde(rename = "ProductId", alias = "productId", default)]
    pub product_id: i32,
    #[serde(rename = "ProductName", alias = "productName", default)]
    pub product_name: Option<String>,
    #[serde(rename = "UnitPrice", alias = "unitPrice", default = "zero")]
    pub unit_price: Number,
    #[serde(rename = "OldUnitPrice", alias = "oldUnitPrice", default = "zero")]
    pub old_unit_price: Number,
    #[serde(rename = "Quantity", alias = "quantity", default)]
    pub quantity: i32,
    #[serde(rename = "PictureUrl", alias = "pictureUrl", default)]
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

/// The only two fields the gRPC contract carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireItem {
    pub product_id: i32,
    pub quantity: i32,
}

/// `BasketService.MapToCustomerBasket`.
pub fn basket_from_wire(buyer_id: &str, items: &[WireItem]) -> CustomerBasket {
    CustomerBasket {
        buyer_id: buyer_id.to_owned(),
        items: items
            .iter()
            .map(|item| BasketItem {
                product_id: item.product_id,
                quantity: item.quantity,
                ..BasketItem::default()
            })
            .collect(),
    }
}

/// `BasketService.MapToCustomerBasketResponse`.
pub fn wire_items(basket: &CustomerBasket) -> Vec<WireItem> {
    basket
        .items
        .iter()
        .map(|item| WireItem {
            product_id: item.product_id,
            quantity: item.quantity,
        })
        .collect()
}

/// `BasketItem.Validate`.
pub fn validate_item(item: &BasketItem) -> Vec<&'static str> {
    if item.quantity < 1 {
        vec![INVALID_QUANTITY_MESSAGE]
    } else {
        Vec::new()
    }
}

/// Result of `ServerCallContext.GetUserIdentity()` plus the `string.IsNullOrEmpty` guard each RPC
/// applies to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Anonymous,
    User(String),
}

impl Caller {
    pub fn from_subject(subject: Option<&str>) -> Self {
        match subject {
            Some(sub) if !sub.is_empty() => Caller::User(sub.to_owned()),
            _ => Caller::Anonymous,
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Caller::User(id) => Some(id),
            Caller::Anonymous => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basket_key_matches_dotnet_prefix() {
        assert_eq!(basket_key("user-1"), "/basket/user-1");
    }

    #[test]
    fn serialized_basket_uses_pascal_case_property_names() {
        let basket = CustomerBasket {
            buyer_id: "user-1".into(),
            items: vec![BasketItem {
                id: Some("item-1".into()),
                product_id: 3,
                product_name: Some("Adventurer GPS Watch".into()),
                unit_price: Number::from_f64(400.5).unwrap(),
                old_unit_price: Number::from(0),
                quantity: 2,
                picture_url: Some("http://example/3.webp".into()),
            }],
        };

        assert_eq!(
            serde_json::to_string(&basket).unwrap(),
            r#"{"BuyerId":"user-1","Items":[{"Id":"item-1","ProductId":3,"ProductName":"Adventurer GPS Watch","UnitPrice":400.5,"OldUnitPrice":0,"Quantity":2,"PictureUrl":"http://example/3.webp"}]}"#
        );
    }

    #[test]
    fn serialized_basket_writes_nulls_for_unset_item_fields() {
        let basket = basket_from_wire(
            "user-1",
            &[WireItem {
                product_id: 1,
                quantity: 1,
            }],
        );

        assert_eq!(
            serde_json::to_string(&basket).unwrap(),
            r#"{"BuyerId":"user-1","Items":[{"Id":null,"ProductId":1,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":1,"PictureUrl":null}]}"#
        );
    }

    #[test]
    fn empty_basket_serializes_to_an_empty_item_array() {
        assert_eq!(
            serde_json::to_string(&CustomerBasket::new("user-1")).unwrap(),
            r#"{"BuyerId":"user-1","Items":[]}"#
        );
    }

    #[test]
    fn deserialization_accepts_camel_case_and_missing_fields() {
        let basket: CustomerBasket =
            serde_json::from_str(r#"{"buyerId":"user-1","items":[{"productId":3,"quantity":2}]}"#)
                .unwrap();

        assert_eq!(basket.buyer_id, "user-1");
        assert_eq!(basket.items.len(), 1);
        assert_eq!(basket.items[0].product_id, 3);
        assert_eq!(basket.items[0].quantity, 2);
    }

    #[test]
    fn deserialization_round_trips_a_dotnet_written_basket() {
        let json = r#"{"BuyerId":"user-1","Items":[{"Id":null,"ProductId":1,"ProductName":null,"UnitPrice":0,"OldUnitPrice":0,"Quantity":1,"PictureUrl":null}]}"#;
        let basket: CustomerBasket = serde_json::from_str(json).unwrap();

        assert_eq!(serde_json::to_string(&basket).unwrap(), json);
    }

    #[test]
    fn wire_mapping_projects_only_product_id_and_quantity() {
        let basket = CustomerBasket {
            buyer_id: "user-1".into(),
            items: vec![BasketItem {
                id: Some("some-id".into()),
                product_id: 7,
                product_name: Some("ignored".into()),
                unit_price: Number::from_f64(12.5).unwrap(),
                quantity: 3,
                picture_url: Some("ignored".into()),
                ..BasketItem::default()
            }],
        };

        assert_eq!(
            wire_items(&basket),
            vec![WireItem {
                product_id: 7,
                quantity: 3
            }]
        );
    }

    #[test]
    fn basket_from_wire_keeps_item_order_and_buyer() {
        let basket = basket_from_wire(
            "user-1",
            &[
                WireItem {
                    product_id: 1,
                    quantity: 2,
                },
                WireItem {
                    product_id: 5,
                    quantity: 9,
                },
            ],
        );

        assert_eq!(basket.buyer_id, "user-1");
        assert_eq!(basket.items.len(), 2);
        assert_eq!(basket.items[0].product_id, 1);
        assert_eq!(basket.items[0].quantity, 2);
        assert_eq!(basket.items[1].product_id, 5);
        assert_eq!(basket.items[1].quantity, 9);
    }

    #[test]
    fn item_validation_rejects_quantities_below_one() {
        assert_eq!(
            validate_item(&BasketItem {
                quantity: 0,
                ..BasketItem::default()
            }),
            vec![INVALID_QUANTITY_MESSAGE]
        );
        assert!(validate_item(&BasketItem {
            quantity: 1,
            ..BasketItem::default()
        })
        .is_empty());
    }

    #[test]
    fn empty_or_missing_subject_is_anonymous() {
        assert_eq!(Caller::from_subject(None), Caller::Anonymous);
        assert_eq!(Caller::from_subject(Some("")), Caller::Anonymous);
        assert_eq!(
            Caller::from_subject(Some("user-1")),
            Caller::User("user-1".into())
        );
    }
}
