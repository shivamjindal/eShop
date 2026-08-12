//! The `BasketApi.Basket` gRPC surface (`Grpc/BasketService.cs` in Basket.API).

use tonic::{Request, Response, Status};

use crate::auth::TokenValidator;
use crate::domain::CustomerBasket;
use crate::proto::basket_server::Basket;
use crate::proto::{
    BasketItem as ProtoBasketItem, CustomerBasketResponse, DeleteBasketRequest,
    DeleteBasketResponse, GetBasketRequest, UpdateBasketRequest,
};
use crate::storage::BasketRepository;

pub struct BasketGrpcService {
    repository: BasketRepository,
    tokens: TokenValidator,
}

impl BasketGrpcService {
    pub fn new(repository: BasketRepository, tokens: TokenValidator) -> Self {
        Self { repository, tokens }
    }

    async fn user_id<T>(&self, request: &Request<T>) -> Option<String> {
        let authorization = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        self.tokens.user_id(authorization).await
    }
}

#[tonic::async_trait]
impl Basket for BasketGrpcService {
    async fn get_basket(
        &self,
        request: Request<GetBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        // GetBasket is [AllowAnonymous]: an unknown caller gets an empty basket, not an error.
        let Some(user_id) = self.user_id(&request).await else {
            return Ok(Response::new(CustomerBasketResponse::default()));
        };

        let basket = self
            .repository
            .get(&user_id)
            .await
            .map_err(internal_error)?;

        Ok(Response::new(
            basket.map(to_response).unwrap_or_default(),
        ))
    }

    async fn update_basket(
        &self,
        request: Request<UpdateBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        let user_id = self.user_id(&request).await.ok_or_else(not_authenticated)?;

        let quantities: Vec<(i32, i32)> = request
            .get_ref()
            .items
            .iter()
            .map(|item| (item.product_id, item.quantity))
            .collect();

        let updated = self
            .repository
            .update(&CustomerBasket::from_quantities(&user_id, &quantities))
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                Status::not_found(format!("Basket with buyer id {user_id} does not exist"))
            })?;

        Ok(Response::new(to_response(updated)))
    }

    async fn delete_basket(
        &self,
        request: Request<DeleteBasketRequest>,
    ) -> Result<Response<DeleteBasketResponse>, Status> {
        let user_id = self.user_id(&request).await.ok_or_else(not_authenticated)?;

        self.repository
            .delete(&user_id)
            .await
            .map_err(internal_error)?;

        Ok(Response::new(DeleteBasketResponse::default()))
    }
}

/// The wire response carries only product id and quantity; everything else stays in Redis.
fn to_response(basket: CustomerBasket) -> CustomerBasketResponse {
    CustomerBasketResponse {
        items: basket
            .items
            .into_iter()
            .map(|item| ProtoBasketItem {
                product_id: item.product_id,
                quantity: item.quantity,
            })
            .collect(),
    }
}

fn not_authenticated() -> Status {
    Status::unauthenticated("The caller is not authenticated.")
}

fn internal_error(error: anyhow::Error) -> Status {
    tracing::error!(%error, "basket request failed");
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::BasketItem;
    use serde_json::Number;

    #[test]
    fn response_projects_only_product_id_and_quantity() {
        let basket = CustomerBasket {
            buyer_id: Some("alice".into()),
            items: vec![BasketItem {
                id: Some("some-id".into()),
                product_id: 42,
                product_name: Some("Roslyn Red Sheet".into()),
                unit_price: Number::from_f64(8.5).unwrap(),
                old_unit_price: Number::from(9),
                quantity: 3,
                picture_url: Some("http://example/pic".into()),
            }],
        };

        let response = to_response(basket);

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].product_id, 42);
        assert_eq!(response.items[0].quantity, 3);
    }

    #[test]
    fn unauthenticated_status_matches_dotnet() {
        let status = not_authenticated();

        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert_eq!(status.message(), "The caller is not authenticated.");
    }
}
