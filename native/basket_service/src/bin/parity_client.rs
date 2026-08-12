//! Parity harness driver: replays the characterized basket cases against one running basket
//! service and writes a deterministic transcript of what it observed.
//!
//! `scripts/parity-basket.sh` records a transcript from the .NET service and then diffs the
//! transcript produced by the Rust service against it, so the comparison survives the removal of
//! the .NET project. Every case records both the gRPC outcome (status code, message, payload) and
//! the bytes the service left in Redis.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use basket_service::proto::basket_client::BasketClient;
use basket_service::proto::{BasketItem, DeleteBasketRequest, GetBasketRequest, UpdateBasketRequest};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Connection, ConnectionProperties, ExchangeKind};
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;

const EXCHANGE_NAME: &str = "eshop_event_bus";
const ORDER_STARTED: &str = "OrderStartedIntegrationEvent";

struct Options {
    endpoint: String,
    redis_url: String,
    user: String,
    token: String,
    expired_token: Option<String>,
    foreign_token: Option<String>,
    amqp_url: Option<String>,
    output: Option<String>,
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
            endpoint: required("endpoint")?,
            redis_url: required("redis")?,
            user: required("user")?,
            token: required("token")?,
            expired_token: args.get("expired-token").cloned(),
            foreign_token: args.get("foreign-token").cloned(),
            amqp_url: args.get("amqp").cloned(),
            output: args.get("output").cloned(),
        })
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
    async fn connect(endpoint: &str, redis_url: &str) -> Result<Self> {
        let client = BasketClient::connect(endpoint.to_owned())
            .await
            .with_context(|| format!("connecting to the basket service at {endpoint}"))?;
        let redis = redis::Client::open(redis_url)
            .with_context(|| format!("opening {redis_url}"))?
            .get_multiplexed_async_connection()
            .await
            .with_context(|| format!("connecting to {redis_url}"))?;

        Ok(Self { client, redis })
    }

    async fn call(&mut self, call: &Call, token: Option<&str>) -> String {
        match call {
            Call::Get => match self
                .client
                .get_basket(authorize(Request::new(GetBasketRequest {}), token))
                .await
            {
                Ok(response) => describe_items(&response.into_inner().items),
                Err(status) => describe_status(&status),
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
                    Ok(response) => describe_items(&response.into_inner().items),
                    Err(status) => describe_status(&status),
                }
            }
            Call::Delete => match self
                .client
                .delete_basket(authorize(Request::new(DeleteBasketRequest {}), token))
                .await
            {
                Ok(_) => "OK".to_owned(),
                Err(status) => describe_status(&status),
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

    async fn clear(&mut self, user: &str) -> Result<()> {
        let _: i64 = redis::cmd("DEL")
            .arg(format!("/basket/{user}"))
            .query_async(&mut self.redis)
            .await?;
        Ok(())
    }
}

fn authorize<T>(mut request: Request<T>, token: Option<&str>) -> Request<T> {
    if let Some(token) = token {
        let value: MetadataValue<_> = format!("Bearer {token}").parse().expect("ascii token");
        request.metadata_mut().insert("authorization", value);
    }
    request
}

fn describe_items(items: &[BasketItem]) -> String {
    let items = items
        .iter()
        .map(|item| format!("({},{})", item.product_id, item.quantity))
        .collect::<Vec<_>>()
        .join(",");
    format!("OK items=[{items}]")
}

fn describe_status(status: &tonic::Status) -> String {
    format!("ERR {:?}: {}", status.code(), status.message())
}

struct Transcript {
    lines: Vec<String>,
}

impl Transcript {
    fn record(&mut self, case: &str, aspect: &str, value: &str) {
        let line = format!("{case}\t{aspect}\t{value}");
        println!("{line}");
        self.lines.push(line);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::from_args()?;

    let mut implementation = Implementation::connect(&options.endpoint, &options.redis_url).await?;
    implementation.clear(&options.user).await?;

    let mut transcript = Transcript { lines: Vec::new() };
    let token = Some(options.token.as_str());

    let mut cases: Vec<(&str, Call, Option<&str>)> = vec![
        ("get/anonymous", Call::Get, None),
        ("get/authenticated-empty", Call::Get, token),
        ("update/anonymous", Call::Update(vec![(1, 2)]), None),
        ("update/two-items", Call::Update(vec![(1, 2), (5, 9)]), token),
        ("get/after-update", Call::Get, token),
        ("update/replaces-basket", Call::Update(vec![(3, 1)]), token),
        ("get/after-replace", Call::Get, token),
        ("update/empty-item-list", Call::Update(vec![]), token),
        ("get/after-empty-update", Call::Get, token),
        ("update/zero-quantity", Call::Update(vec![(4, 0)]), token),
        ("update/negative-quantity", Call::Update(vec![(4, -1)]), token),
        ("delete/anonymous", Call::Delete, None),
        ("get/before-delete", Call::Get, token),
        ("delete/authenticated", Call::Delete, token),
        ("get/after-delete", Call::Get, token),
        ("delete/no-basket", Call::Delete, token),
        ("get/garbage-token", Call::Get, Some("not-a-jwt")),
        ("update/garbage-token", Call::Update(vec![(1, 1)]), Some("not-a-jwt")),
    ];

    if let Some(expired) = options.expired_token.as_deref() {
        cases.push(("get/expired-token", Call::Get, Some(expired)));
        cases.push(("update/expired-token", Call::Update(vec![(1, 1)]), Some(expired)));
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
        let outcome = implementation.call(call, *token).await;
        transcript.record(name, "response", &outcome);

        let stored = implementation.stored_basket(&options.user).await?;
        transcript.record(name, "redis", &stored);
    }

    if let Some(amqp_url) = options.amqp_url.as_deref() {
        let case = "event/order-started-clears-basket";
        implementation.call(&Call::Update(vec![(2, 4)]), token).await;
        transcript.record(
            case,
            "redis-before",
            &implementation.stored_basket(&options.user).await?,
        );

        publish_order_started(amqp_url, &options.user)
            .await
            .context("publishing OrderStartedIntegrationEvent")?;

        let stored = wait_for_absent_basket(&mut implementation, &options.user).await?;
        transcript.record(case, "redis-after", &stored);
        transcript.record(
            case,
            "response",
            &implementation.call(&Call::Get, token).await,
        );
    }

    if let Some(path) = options.output.as_deref() {
        std::fs::write(path, format!("{}\n", transcript.lines.join("\n")))
            .with_context(|| format!("writing {path}"))?;
        eprintln!("parity: wrote {} observations to {path}", transcript.lines.len());
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

    // The payload Ordering.API puts on the wire: PascalCase, no envelope.
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
