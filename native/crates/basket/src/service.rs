//! gRPC surface — the port of `Grpc/BasketService.cs`.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::auth::TokenValidator;
use crate::model::{BasketItem as BasketDocumentItem, CustomerBasket};
use crate::proto::basket_server::Basket;
use crate::proto::{
    BasketItem, CustomerBasketResponse, DeleteBasketRequest, DeleteBasketResponse, GetBasketRequest,
    UpdateBasketRequest,
};
use crate::repository::BasketRepository;

pub struct BasketService {
    repository: BasketRepository,
    tokens: Arc<TokenValidator>,
}

impl BasketService {
    pub fn new(repository: BasketRepository, tokens: Arc<TokenValidator>) -> Self {
        Self { repository, tokens }
    }

    async fn user_identity<T>(&self, request: &Request<T>) -> Option<String> {
        let header = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        self.tokens.user_id(header).await
    }
}

fn not_authenticated() -> Status {
    Status::unauthenticated("The caller is not authenticated.")
}

/// Unhandled handler exceptions surface from ASP.NET Core gRPC as an opaque
/// `Unknown` when detailed errors are off, which is the default for Basket.
fn handler_failure(error: anyhow::Error) -> Status {
    tracing::error!(%error, "basket handler failed");
    Status::unknown("Exception was thrown by handler.")
}

pub fn map_to_response(basket: &CustomerBasket) -> CustomerBasketResponse {
    CustomerBasketResponse {
        items: basket
            .items
            .iter()
            .map(|item| BasketItem {
                product_id: item.product_id,
                quantity: item.quantity,
            })
            .collect(),
    }
}

pub fn map_to_customer_basket(user_id: &str, request: &UpdateBasketRequest) -> CustomerBasket {
    CustomerBasket {
        buyer_id: Some(user_id.to_string()),
        items: request
            .items
            .iter()
            .map(|item| BasketDocumentItem {
                product_id: item.product_id,
                quantity: item.quantity,
                ..Default::default()
            })
            .collect(),
    }
}

#[tonic::async_trait]
impl Basket for BasketService {
    async fn get_basket(
        &self,
        request: Request<GetBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        let Some(user_id) = self.user_identity(&request).await else {
            return Ok(Response::new(CustomerBasketResponse::default()));
        };

        let basket = self
            .repository
            .get_basket(&user_id)
            .await
            .map_err(handler_failure)?;

        Ok(Response::new(
            basket
                .as_ref()
                .map(map_to_response)
                .unwrap_or_else(CustomerBasketResponse::default),
        ))
    }

    async fn update_basket(
        &self,
        request: Request<UpdateBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        let Some(user_id) = self.user_identity(&request).await else {
            return Err(not_authenticated());
        };

        let basket = map_to_customer_basket(&user_id, request.get_ref());
        let stored = self
            .repository
            .update_basket(&basket)
            .await
            .map_err(handler_failure)?
            .ok_or_else(|| {
                Status::not_found(format!("Basket with buyer id {user_id} does not exist"))
            })?;

        Ok(Response::new(map_to_response(&stored)))
    }

    async fn delete_basket(
        &self,
        request: Request<DeleteBasketRequest>,
    ) -> Result<Response<DeleteBasketResponse>, Status> {
        let Some(user_id) = self.user_identity(&request).await else {
            return Err(not_authenticated());
        };

        self.repository
            .delete_basket(&user_id)
            .await
            .map_err(handler_failure)?;

        Ok(Response::new(DeleteBasketResponse::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_requests_only_carry_product_id_and_quantity() {
        let request = UpdateBasketRequest {
            items: vec![
                BasketItem {
                    product_id: 3,
                    quantity: 2,
                },
                BasketItem {
                    product_id: 5,
                    quantity: 1,
                },
            ],
        };

        let basket = map_to_customer_basket("alice", &request);

        assert_eq!(basket.buyer_id.as_deref(), Some("alice"));
        assert_eq!(basket.items.len(), 2);
        assert_eq!(basket.items[0].product_id, 3);
        assert_eq!(basket.items[0].quantity, 2);
        assert_eq!(basket.items[0].product_name, None);
        assert_eq!(basket.items[0].unit_price.to_string(), "0");
    }

    #[test]
    fn responses_drop_everything_but_product_id_and_quantity() {
        let basket = CustomerBasket::from_json_slice(
            br#"{"BuyerId":"alice","Items":[{"Id":"x","ProductId":4,"ProductName":"Mug","UnitPrice":9.5,"OldUnitPrice":0,"Quantity":7,"PictureUrl":"u"}]}"#,
        )
        .unwrap();

        let response = map_to_response(&basket);

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].product_id, 4);
        assert_eq!(response.items[0].quantity, 7);
    }

    #[test]
    fn an_empty_basket_maps_to_an_empty_response() {
        assert!(map_to_response(&CustomerBasket::new("alice")).items.is_empty());
    }

    #[test]
    fn error_statuses_match_the_dotnet_messages() {
        let status = not_authenticated();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert_eq!(status.message(), "The caller is not authenticated.");

        let missing = Status::not_found("Basket with buyer id alice does not exist");
        assert_eq!(missing.message(), "Basket with buyer id alice does not exist");
    }
}
