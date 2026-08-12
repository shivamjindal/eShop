//! Dual-run parity harness: replays the characterized basket cases against the .NET service and
//! the Rust service and fails when the observable behavior differs.
//!
//! For every case it compares the gRPC outcome (status code, message, payload) *and* the bytes
//! each implementation left in its own Redis database.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use basket_service::proto::basket_client::BasketClient;
use basket_service::proto::{
    BasketItem, DeleteBasketRequest, GetBasketRequest, UpdateBasketRequest,
};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Connection, ConnectionProperties, ExchangeKind};
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;

const EXCHANGE_NAME: &str = "eshop_event_bus";
const ORDER_STARTED: &str = "OrderStartedIntegrationEvent";

struct Options {
    dotnet_endpoint: String,
    rust_endpoint: String,
    dotnet_redis: String,
    rust_redis: String,
    user: String,
    token: String,
    expired_token: Option<String>,
    foreign_token: Option<String>,
    amqp_url: Option<String>,
}

impl Options {
    fn from_args() -> Result<Self> {
        let mut args: BTreeMap<String, String> = BTreeMap::new();
        let mut iter = std::env::args().skip(1);
        while let Some(flag) = iter.next() {
            let Some(name) = flag.strip_prefix("--") else {
                bail!("unexpected argument: {flag}");
            };
            let value = iter
                .next()
                .with_context(|| format!("missing value for --{name}"))?;
            args.insert(name.to_owned(), value);
        }

        let required = |name: &str| -> Result<String> {
            args.get(name)
                .cloned()
                .with_context(|| format!("missing required --{name}"))
        };

        Ok(Self {
            dotnet_endpoint: required("dotnet")?,
            rust_endpoint: required("rust")?,
            dotnet_redis: required("dotnet-redis")?,
            rust_redis: required("rust-redis")?,
            user: required("user")?,
            token: required("token")?,
            expired_token: args.get("expired-token").cloned(),
            foreign_token: args.get("foreign-token").cloned(),
            amqp_url: args.get("amqp").cloned(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome(String);

impl Outcome {
    fn ok_items(items: &[BasketItem]) -> Self {
        let items = items
            .iter()
            .map(|item| format!("({},{})", item.product_id, item.quantity))
            .collect::<Vec<_>>()
            .join(",");
        Outcome(format!("OK items=[{items}]"))
    }

    fn ok_empty() -> Self {
        Outcome("OK".to_owned())
    }

    fn from_status(status: &tonic::Status) -> Self {
        Outcome(format!("ERR {:?}: {}", status.code(), status.message()))
    }
}

enum Call {
    Get,
    Update(Vec<(i32, i32)>),
    Delete,
}

struct Implementation {
    client: BasketClient<Channel>,
    redis: redis::aio::MultiplexedConnection,
}

impl Implementation {
    async fn connect(name: &'static str, endpoint: &str, redis_url: &str) -> Result<Self> {
        let client = BasketClient::connect(endpoint.to_owned())
            .await
            .with_context(|| format!("connecting to the {name} basket service at {endpoint}"))?;
        let redis = redis::Client::open(redis_url)
            .with_context(|| format!("opening {redis_url}"))?
            .get_multiplexed_async_connection()
            .await
            .with_context(|| format!("connecting to {redis_url}"))?;

        Ok(Self { client, redis })
    }

    async fn call(&mut self, call: &Call, token: Option<&str>) -> Outcome {
        match call {
            Call::Get => match self
                .client
                .get_basket(authorize(Request::new(GetBasketRequest {}), token))
                .await
            {
                Ok(response) => Outcome::ok_items(&response.into_inner().items),
                Err(status) => Outcome::from_status(&status),
            },
            Call::Update(items) => {
                let request = UpdateBasketRequest {
                    items: items
                        .iter()
                        .map(|(product_id, quantity)| BasketItem {
                            product_id: *product_id,
                            quantity: *quantity,
                        })
                        .collect(),
                };
                match self
                    .client
                    .update_basket(authorize(Request::new(request), token))
                    .await
                {
                    Ok(response) => Outcome::ok_items(&response.into_inner().items),
                    Err(status) => Outcome::from_status(&status),
                }
            }
            Call::Delete => match self
                .client
                .delete_basket(authorize(Request::new(DeleteBasketRequest {}), token))
                .await
            {
                Ok(_) => Outcome::ok_empty(),
                Err(status) => Outcome::from_status(&status),
            },
        }
    }

    async fn stored_basket(&mut self, user: &str) -> Result<String> {
        let value: Option<String> = redis::cmd("GET")
            .arg(format!("/basket/{user}"))
            .query_async(&mut self.redis)
            .await?;

        Ok(value.unwrap_or_else(|| "<absent>".to_owned()))
    }
}

fn authorize<T>(mut request: Request<T>, token: Option<&str>) -> Request<T> {
    if let Some(token) = token {
        let value: MetadataValue<_> = format!("Bearer {token}").parse().expect("ascii token");
        request.metadata_mut().insert("authorization", value);
    }
    request
}

struct Report {
    failures: usize,
    cases: usize,
}

impl Report {
    fn record(&mut self, case: &str, aspect: &str, dotnet: &str, rust: &str) {
        self.cases += 1;
        if dotnet == rust {
            println!("  MATCH    {case} [{aspect}]: {dotnet}");
        } else {
            self.failures += 1;
            println!("  MISMATCH {case} [{aspect}]");
            println!("      .NET: {dotnet}");
            println!("      Rust: {rust}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::from_args()?;

    let mut dotnet =
        Implementation::connect("dotnet", &options.dotnet_endpoint, &options.dotnet_redis).await?;
    let mut rust =
        Implementation::connect("rust", &options.rust_endpoint, &options.rust_redis).await?;
    let mut report = Report {
        failures: 0,
        cases: 0,
    };

    println!(
        "parity: comparing .NET ({}) with Rust ({})",
        options.dotnet_endpoint, options.rust_endpoint
    );

    let token = Some(options.token.as_str());
    let mut cases: Vec<(&str, Call, Option<&str>)> = vec![
        ("get/anonymous", Call::Get, None),
        ("get/authenticated-empty", Call::Get, token),
        ("update/anonymous", Call::Update(vec![(1, 2)]), None),
        (
            "update/two-items",
            Call::Update(vec![(1, 2), (5, 9)]),
            token,
        ),
        ("get/after-update", Call::Get, token),
        ("update/replaces-basket", Call::Update(vec![(3, 1)]), token),
        ("get/after-replace", Call::Get, token),
        ("update/empty-item-list", Call::Update(vec![]), token),
        ("get/after-empty-update", Call::Get, token),
        ("update/zero-quantity", Call::Update(vec![(4, 0)]), token),
        ("delete/anonymous", Call::Delete, None),
        ("get/before-delete", Call::Get, token),
        ("delete/authenticated", Call::Delete, token),
        ("get/after-delete", Call::Get, token),
        ("delete/no-basket", Call::Delete, token),
        ("get/garbage-token", Call::Get, Some("not-a-jwt")),
        (
            "update/garbage-token",
            Call::Update(vec![(1, 1)]),
            Some("not-a-jwt"),
        ),
    ];

    if let Some(expired) = options.expired_token.as_deref() {
        cases.push(("get/expired-token", Call::Get, Some(expired)));
        cases.push((
            "update/expired-token",
            Call::Update(vec![(1, 1)]),
            Some(expired),
        ));
    }
    if let Some(foreign) = options.foreign_token.as_deref() {
        cases.push(("get/wrong-issuer-token", Call::Get, Some(foreign)));
        cases.push((
            "update/wrong-issuer-token",
            Call::Update(vec![(1, 1)]),
            Some(foreign),
        ));
    }

    for (name, call, token) in &cases {
        let dotnet_outcome = dotnet.call(call, *token).await;
        let rust_outcome = rust.call(call, *token).await;
        report.record(name, "response", &dotnet_outcome.0, &rust_outcome.0);

        let dotnet_stored = dotnet.stored_basket(&options.user).await?;
        let rust_stored = rust.stored_basket(&options.user).await?;
        report.record(name, "redis", &dotnet_stored, &rust_stored);
    }

    if let Some(amqp_url) = options.amqp_url.as_deref() {
        let case = "event/order-started-clears-basket";
        dotnet.call(&Call::Update(vec![(2, 4)]), token).await;
        rust.call(&Call::Update(vec![(2, 4)]), token).await;

        publish_order_started(amqp_url, &options.user)
            .await
            .context("publishing OrderStartedIntegrationEvent")?;

        let dotnet_stored = wait_for_absent_basket(&mut dotnet, &options.user).await?;
        let rust_stored = wait_for_absent_basket(&mut rust, &options.user).await?;
        report.record(case, "redis", &dotnet_stored, &rust_stored);

        let dotnet_outcome = dotnet.call(&Call::Get, token).await;
        let rust_outcome = rust.call(&Call::Get, token).await;
        report.record(case, "response", &dotnet_outcome.0, &rust_outcome.0);
    }

    println!(
        "parity: {} comparisons, {} mismatches",
        report.cases, report.failures
    );

    if report.failures > 0 {
        std::process::exit(1);
    }

    Ok(())
}

async fn wait_for_absent_basket(implementation: &mut Implementation, user: &str) -> Result<String> {
    for _ in 0..40 {
        let stored = implementation.stored_basket(user).await?;
        if stored == "<absent>" {
            return Ok(stored);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    implementation.stored_basket(user).await
}

async fn publish_order_started(amqp_url: &str, user_id: &str) -> Result<()> {
    let connection = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;
    channel
        .exchange_declare(
            EXCHANGE_NAME,
            ExchangeKind::Direct,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Same payload Ordering.API puts on the wire (PascalCase, no envelope).
    let body = serde_json::json!({
        "Id": "00000000-0000-0000-0000-0000000000ff",
        "CreationDate": "2026-08-12T03:34:00.1234567Z",
        "UserId": user_id,
    });

    channel
        .basic_publish(
            EXCHANGE_NAME,
            ORDER_STARTED,
            BasicPublishOptions::default(),
            &serde_json::to_vec(&body)?,
            BasicProperties::default().with_delivery_mode(2),
        )
        .await?
        .await?;

    connection.close(0, "done").await?;
    Ok(())
}
