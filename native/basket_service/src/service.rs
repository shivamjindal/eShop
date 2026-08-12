//! gRPC surface — port of `src/Basket.API/Grpc/BasketService.cs`.

use std::sync::Arc;

use async_trait::async_trait;
use tonic::{Request, Response, Status};

use crate::auth::JwtAuthenticator;
use crate::domain::{basket_from_wire, wire_items, Caller, WireItem};
use crate::proto::basket_server::{Basket, BasketServer};
use crate::proto::{
    BasketItem as WireBasketItem, CustomerBasketResponse, DeleteBasketRequest,
    DeleteBasketResponse, GetBasketRequest, UpdateBasketRequest,
};
use crate::storage::BasketRepository;

const NOT_AUTHENTICATED: &str = "The caller is not authenticated.";
/// What ASP.NET Core gRPC returns for an unhandled handler exception.
const HANDLER_FAILED: &str = "Exception was thrown by handler.";

/// Resolves the calling user from request metadata.
#[async_trait]
pub trait CallerIdentity: Send + Sync + 'static {
    async fn caller(&self, authorization_header: Option<&str>) -> Caller;
}

#[async_trait]
impl CallerIdentity for JwtAuthenticator {
    async fn caller(&self, authorization_header: Option<&str>) -> Caller {
        Caller::from_subject(self.subject(authorization_header).await.as_deref())
    }
}

pub struct BasketGrpcService<R, I> {
    repository: Arc<R>,
    identity: Arc<I>,
}

impl<R, I> BasketGrpcService<R, I>
where
    R: BasketRepository,
    I: CallerIdentity,
{
    pub fn new(repository: Arc<R>, identity: Arc<I>) -> Self {
        Self {
            repository,
            identity,
        }
    }

    pub fn into_server(self) -> BasketServer<Self> {
        BasketServer::new(self)
    }

    async fn caller<T>(&self, request: &Request<T>) -> Caller {
        let header = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok());

        self.identity.caller(header).await
    }
}

fn response_from(items: Vec<WireItem>) -> CustomerBasketResponse {
    CustomerBasketResponse {
        items: items
            .into_iter()
            .map(|item| WireBasketItem {
                product_id: item.product_id,
                quantity: item.quantity,
            })
            .collect(),
    }
}

fn handler_failed(error: impl std::fmt::Display) -> Status {
    tracing::error!(error = %error, "basket request failed");
    Status::unknown(HANDLER_FAILED)
}

#[async_trait]
impl<R, I> Basket for BasketGrpcService<R, I>
where
    R: BasketRepository,
    I: CallerIdentity,
{
    async fn get_basket(
        &self,
        request: Request<GetBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        let Caller::User(user_id) = self.caller(&request).await else {
            return Ok(Response::new(CustomerBasketResponse::default()));
        };

        tracing::debug!(user_id = %user_id, "GetBasket");

        match self
            .repository
            .get_basket(&user_id)
            .await
            .map_err(handler_failed)?
        {
            Some(basket) => Ok(Response::new(response_from(wire_items(&basket)))),
            None => Ok(Response::new(CustomerBasketResponse::default())),
        }
    }

    async fn update_basket(
        &self,
        request: Request<UpdateBasketRequest>,
    ) -> Result<Response<CustomerBasketResponse>, Status> {
        let Caller::User(user_id) = self.caller(&request).await else {
            return Err(Status::unauthenticated(NOT_AUTHENTICATED));
        };

        tracing::debug!(user_id = %user_id, "UpdateBasket");

        let items: Vec<WireItem> = request
            .into_inner()
            .items
            .into_iter()
            .map(|item| WireItem {
                product_id: item.product_id,
                quantity: item.quantity,
            })
            .collect();

        let basket = basket_from_wire(&user_id, &items);

        match self
            .repository
            .update_basket(&basket)
            .await
            .map_err(handler_failed)?
        {
            Some(stored) => Ok(Response::new(response_from(wire_items(&stored)))),
            None => Err(Status::not_found(format!(
                "Basket with buyer id {user_id} does not exist"
            ))),
        }
    }

    async fn delete_basket(
        &self,
        request: Request<DeleteBasketRequest>,
    ) -> Result<Response<DeleteBasketResponse>, Status> {
        let Caller::User(user_id) = self.caller(&request).await else {
            return Err(Status::unauthenticated(NOT_AUTHENTICATED));
        };

        tracing::debug!(user_id = %user_id, "DeleteBasket");

        self.repository
            .delete_basket(&user_id)
            .await
            .map_err(handler_failed)?;

        Ok(Response::new(DeleteBasketResponse::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryBasketRepository;
    use tonic::Code;

    struct StaticIdentity(Option<&'static str>);

    #[async_trait]
    impl CallerIdentity for StaticIdentity {
        async fn caller(&self, _authorization_header: Option<&str>) -> Caller {
            Caller::from_subject(self.0)
        }
    }

    fn service(
        repository: MemoryBasketRepository,
        user: Option<&'static str>,
    ) -> BasketGrpcService<MemoryBasketRepository, StaticIdentity> {
        BasketGrpcService::new(Arc::new(repository), Arc::new(StaticIdentity(user)))
    }

    fn update_request(items: &[(i32, i32)]) -> Request<UpdateBasketRequest> {
        Request::new(UpdateBasketRequest {
            items: items
                .iter()
                .map(|(product_id, quantity)| WireBasketItem {
                    product_id: *product_id,
                    quantity: *quantity,
                })
                .collect(),
        })
    }

    #[tokio::test]
    async fn get_basket_returns_empty_for_no_user() {
        let service = service(MemoryBasketRepository::new(), None);

        let response = service
            .get_basket(Request::new(GetBasketRequest::default()))
            .await
            .unwrap();

        assert!(response.into_inner().items.is_empty());
    }

    #[tokio::test]
    async fn get_basket_returns_empty_when_no_basket_is_stored() {
        let service = service(MemoryBasketRepository::new(), Some("user-1"));

        let response = service
            .get_basket(Request::new(GetBasketRequest::default()))
            .await
            .unwrap();

        assert!(response.into_inner().items.is_empty());
    }

    #[tokio::test]
    async fn get_basket_returns_stored_items_for_the_authenticated_user() {
        let service = service(MemoryBasketRepository::new(), Some("user-1"));
        service
            .update_basket(update_request(&[(7, 3)]))
            .await
            .unwrap();

        let response = service
            .get_basket(Request::new(GetBasketRequest::default()))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].product_id, 7);
        assert_eq!(response.items[0].quantity, 3);
    }

    #[tokio::test]
    async fn update_basket_is_unauthenticated_without_a_user() {
        let service = service(MemoryBasketRepository::new(), None);

        let status = service
            .update_basket(update_request(&[(1, 1)]))
            .await
            .unwrap_err();

        assert_eq!(status.code(), Code::Unauthenticated);
        assert_eq!(status.message(), NOT_AUTHENTICATED);
    }

    #[tokio::test]
    async fn update_basket_persists_items_in_order() {
        let repository = Arc::new(MemoryBasketRepository::new());
        let service =
            BasketGrpcService::new(repository.clone(), Arc::new(StaticIdentity(Some("user-1"))));

        let response = service
            .update_basket(update_request(&[(1, 2), (5, 9)]))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.items.len(), 2);
        assert_eq!(response.items[1].product_id, 5);
        assert_eq!(response.items[1].quantity, 9);

        let stored = repository.get_basket("user-1").await.unwrap().unwrap();
        assert_eq!(stored.buyer_id, "user-1");
        assert_eq!(stored.items[0].product_id, 1);
        assert_eq!(stored.items[0].quantity, 2);
    }

    #[tokio::test]
    async fn update_basket_accepts_an_empty_item_list() {
        let service = service(MemoryBasketRepository::new(), Some("user-1"));

        let response = service.update_basket(update_request(&[])).await.unwrap();

        assert!(response.into_inner().items.is_empty());
    }

    #[tokio::test]
    async fn update_basket_is_not_found_when_the_write_does_not_stick() {
        let service = service(MemoryBasketRepository::rejecting_writes(), Some("user-1"));

        let status = service
            .update_basket(update_request(&[(1, 1)]))
            .await
            .unwrap_err();

        assert_eq!(status.code(), Code::NotFound);
        assert_eq!(
            status.message(),
            "Basket with buyer id user-1 does not exist"
        );
    }

    #[tokio::test]
    async fn delete_basket_is_unauthenticated_without_a_user() {
        let service = service(MemoryBasketRepository::new(), None);

        let status = service
            .delete_basket(Request::new(DeleteBasketRequest::default()))
            .await
            .unwrap_err();

        assert_eq!(status.code(), Code::Unauthenticated);
        assert_eq!(status.message(), NOT_AUTHENTICATED);
    }

    #[tokio::test]
    async fn delete_basket_succeeds_even_when_no_basket_exists() {
        let service = service(MemoryBasketRepository::new(), Some("user-1"));

        service
            .delete_basket(Request::new(DeleteBasketRequest::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_basket_removes_the_stored_basket() {
        let repository = Arc::new(MemoryBasketRepository::new());
        let service =
            BasketGrpcService::new(repository.clone(), Arc::new(StaticIdentity(Some("user-1"))));
        service
            .update_basket(update_request(&[(1, 1)]))
            .await
            .unwrap();

        service
            .delete_basket(Request::new(DeleteBasketRequest::default()))
            .await
            .unwrap();

        assert!(repository.get_basket("user-1").await.unwrap().is_none());
    }
}
