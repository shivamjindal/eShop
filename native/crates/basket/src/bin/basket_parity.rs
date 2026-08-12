//! Behavioral transcript recorder/replayer for the basket service.
//!
//! `record` drives a fixed script of gRPC calls (plus one integration event) against a running
//! basket service and writes what it observed — status codes, response items, and the raw Redis
//! document — to a transcript. `replay` runs the same script against another implementation and
//! fails on the first difference.
//!
//! The transcript in `scripts/parity/basket-dotnet.transcript` was recorded from the .NET
//! `src/Basket.API` before it was deleted; it is the baseline the Rust service must reproduce.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use basket::proto::basket_client::BasketClient;
use basket::proto::{BasketItem, DeleteBasketRequest, GetBasketRequest, UpdateBasketRequest};
use jsonwebtoken::{encode, EncodingKey, Header};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Connection, ConnectionProperties, ExchangeKind};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;

const ALICE: &str = "alice-user-id";
const BOB: &str = "bob-user-id";
const EXCHANGE_NAME: &str = "eshop_event_bus";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Observation {
    case: String,
    /// gRPC status code name, e.g. `Ok`, `Unauthenticated`, `NotFound`.
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// `product_id:quantity` pairs from the response, in wire order.
    items: Vec<String>,
    /// Raw Redis document for the keys the case touches, or `absent`.
    redis: BTreeMap<String, String>,
}

struct Options {
    endpoint: String,
    redis_url: String,
    amqp_url: String,
    signing_key: Vec<u8>,
    key_id: String,
    issuer: String,
    transcript: String,
    record: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let options = parse_args()?;
    let observations = run_script(&options).await?;

    if options.record {
        let mut rendered = String::new();
        for observation in &observations {
            rendered.push_str(&serde_json::to_string(observation)?);
            rendered.push('\n');
        }
        std::fs::write(&options.transcript, rendered)
            .with_context(|| format!("could not write {}", options.transcript))?;
        println!(
            "basket-parity: recorded {} cases to {}",
            observations.len(),
            options.transcript
        );
        return Ok(());
    }

    let recorded = std::fs::read_to_string(&options.transcript)
        .with_context(|| format!("could not read {}", options.transcript))?;
    let expected: Vec<Observation> = recorded
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("malformed transcript")?;

    let mut failures = 0;
    for (index, expectation) in expected.iter().enumerate() {
        match observations.get(index) {
            Some(actual) if actual == expectation => {
                println!("basket-parity: case={} match", expectation.case);
            }
            Some(actual) => {
                failures += 1;
                println!("basket-parity: case={} MISMATCH", expectation.case);
                println!("  expected: {}", serde_json::to_string(expectation)?);
                println!("  actual:   {}", serde_json::to_string(actual)?);
            }
            None => {
                failures += 1;
                println!("basket-parity: case={} MISSING", expectation.case);
            }
        }
    }

    if observations.len() != expected.len() {
        failures += 1;
        println!(
            "basket-parity: case count differs (recorded {}, replayed {})",
            expected.len(),
            observations.len()
        );
    }

    if failures > 0 {
        bail!("{failures} parity failure(s) against {}", options.transcript);
    }

    println!("basket-parity: {} cases match", expected.len());
    Ok(())
}

async fn run_script(options: &Options) -> Result<Vec<Observation>> {
    let channel = Channel::from_shared(options.endpoint.clone())?
        .connect_timeout(Duration::from_secs(10))
        .connect()
        .await
        .with_context(|| format!("could not connect to {}", options.endpoint))?;
    let mut client = BasketClient::new(channel);

    let redis_client = redis::Client::open(options.redis_url.as_str())?;
    let mut redis = redis_client.get_multiplexed_async_connection().await?;
    for user in [ALICE, BOB] {
        let _: () = redis.del(format!("/basket/{user}")).await?;
    }

    let alice = mint(options, ALICE, 3600)?;
    let bob = mint(options, BOB, 3600)?;
    let expired = mint(options, ALICE, -3600)?;
    let wrong_issuer = mint_with_issuer(options, ALICE, 3600, "http://not-the-identity-server")?;

    let mut observations = Vec::new();

    observations.push(
        observe("get_no_token", client.get_basket(request(GetBasketRequest {}, None)).await)
            .with_redis(&mut redis, &[ALICE])
            .await?,
    );

    observations.push(
        observe(
            "get_bad_token",
            client
                .get_basket(request(GetBasketRequest {}, Some("not-a-jwt")))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "get_wrong_issuer_token",
            client
                .get_basket(request(GetBasketRequest {}, Some(&wrong_issuer)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "get_expired_token",
            client
                .get_basket(request(GetBasketRequest {}, Some(&expired)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "get_empty_basket",
            client
                .get_basket(request(GetBasketRequest {}, Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "update_no_token",
            client
                .update_basket(request(update(&[(7, 2)]), None))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "update_creates_basket",
            client
                .update_basket(request(update(&[(7, 2), (9, 1)]), Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "get_after_update",
            client
                .get_basket(request(GetBasketRequest {}, Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "get_is_scoped_to_the_caller",
            client
                .get_basket(request(GetBasketRequest {}, Some(&bob)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE, BOB])
        .await?,
    );

    observations.push(
        observe(
            "update_replaces_all_items",
            client
                .update_basket(request(update(&[(7, 5)]), Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "update_with_no_items_empties_the_basket",
            client
                .update_basket(request(update(&[]), Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "delete_no_token",
            client
                .delete_basket(request(DeleteBasketRequest {}, None))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "delete_removes_the_basket",
            client
                .delete_basket(request(DeleteBasketRequest {}, Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    observations.push(
        observe(
            "delete_is_idempotent",
            client
                .delete_basket(request(DeleteBasketRequest {}, Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    // Checkout path: Ordering.API publishes OrderStartedIntegrationEvent and the basket is cleared.
    let _ = client
        .update_basket(request(update(&[(7, 2)]), Some(&alice)))
        .await?;
    publish_order_started(&options.amqp_url, ALICE).await?;
    wait_for_absence(&mut redis, ALICE).await;
    observations.push(
        observe(
            "order_started_clears_the_basket",
            client
                .get_basket(request(GetBasketRequest {}, Some(&alice)))
                .await,
        )
        .with_redis(&mut redis, &[ALICE])
        .await?,
    );

    Ok(observations)
}

struct Pending {
    case: String,
    status: String,
    message: Option<String>,
    items: Vec<String>,
}

impl Pending {
    async fn with_redis(
        self,
        redis: &mut redis::aio::MultiplexedConnection,
        users: &[&str],
    ) -> Result<Observation> {
        let mut documents = BTreeMap::new();
        for user in users {
            let key = format!("/basket/{user}");
            let stored: Option<String> = redis.get(&key).await?;
            documents.insert(key, stored.unwrap_or_else(|| "absent".to_owned()));
        }

        Ok(Observation {
            case: self.case,
            status: self.status,
            message: self.message,
            items: self.items,
            redis: documents,
        })
    }
}

trait Items {
    fn as_items(&self) -> Vec<String>;
}

impl Items for basket::proto::CustomerBasketResponse {
    fn as_items(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| format!("{}:{}", item.product_id, item.quantity))
            .collect()
    }
}

impl Items for basket::proto::DeleteBasketResponse {
    fn as_items(&self) -> Vec<String> {
        Vec::new()
    }
}

fn observe<T: Items>(case: &str, result: Result<tonic::Response<T>, tonic::Status>) -> Pending {
    match result {
        Ok(response) => Pending {
            case: case.to_owned(),
            status: "Ok".to_owned(),
            message: None,
            items: response.get_ref().as_items(),
        },
        Err(status) => Pending {
            case: case.to_owned(),
            status: format!("{:?}", status.code()),
            message: Some(status.message().to_owned()),
            items: Vec::new(),
        },
    }
}

fn request<T>(message: T, token: Option<&str>) -> Request<T> {
    let mut request = Request::new(message);
    if let Some(token) = token {
        let value: MetadataValue<_> = format!("Bearer {token}").parse().expect("valid header");
        request.metadata_mut().insert("authorization", value);
    }
    request
}

fn update(items: &[(i32, i32)]) -> UpdateBasketRequest {
    UpdateBasketRequest {
        items: items
            .iter()
            .map(|&(product_id, quantity)| BasketItem {
                product_id,
                quantity,
            })
            .collect(),
    }
}

fn mint(options: &Options, subject: &str, lifetime_seconds: i64) -> Result<String> {
    mint_with_issuer(options, subject, lifetime_seconds, &options.issuer)
}

fn mint_with_issuer(
    options: &Options,
    subject: &str,
    lifetime_seconds: i64,
    issuer: &str,
) -> Result<String> {
    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        scope: &'a str,
        iat: i64,
        nbf: i64,
        exp: i64,
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    // Well past the 5 minute clock skew so an "expired" token really is expired.
    let issued_at = if lifetime_seconds < 0 { now + lifetime_seconds } else { now };

    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(options.key_id.clone());

    encode(
        &header,
        &Claims {
            iss: issuer,
            sub: subject,
            aud: "basket",
            scope: "basket",
            iat: issued_at,
            nbf: issued_at,
            exp: now + lifetime_seconds,
        },
        &EncodingKey::from_rsa_pem(&options.signing_key)?,
    )
    .context("could not mint a test token")
}

async fn publish_order_started(amqp_url: &str, user_id: &str) -> Result<()> {
    let connection = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;
    channel
        .exchange_declare(
            EXCHANGE_NAME.into(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let body = format!(
        r#"{{"UserId":"{user_id}","Id":"11111111-2222-3333-4444-555555555555","CreationDate":"2026-08-12T10:00:00Z"}}"#
    );

    channel
        .basic_publish(
            EXCHANGE_NAME.into(),
            "OrderStartedIntegrationEvent".into(),
            BasicPublishOptions::default(),
            body.as_bytes(),
            BasicProperties::default().with_delivery_mode(2),
        )
        .await?
        .await?;

    Ok(())
}

async fn wait_for_absence(redis: &mut redis::aio::MultiplexedConnection, user: &str) {
    let key = format!("/basket/{user}");
    for _ in 0..100 {
        let exists: bool = redis.exists(&key).await.unwrap_or(true);
        if !exists {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn parse_args() -> Result<Options> {
    let mut endpoint = None;
    let mut redis_url = None;
    let mut amqp_url = None;
    let mut key_path = None;
    let mut key_id = "parity-key".to_owned();
    let mut issuer = None;
    let mut transcript = None;
    let mut record = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| anyhow!("missing value for {arg}"))
        };
        match arg.as_str() {
            "record" => record = true,
            "replay" => record = false,
            "--endpoint" => endpoint = Some(value()?),
            "--redis" => redis_url = Some(value()?),
            "--amqp" => amqp_url = Some(value()?),
            "--signing-key" => key_path = Some(value()?),
            "--key-id" => key_id = value()?,
            "--issuer" => issuer = Some(value()?),
            "--transcript" => transcript = Some(value()?),
            other => bail!("unexpected argument {other}"),
        }
    }

    let key_path = key_path.ok_or_else(|| anyhow!("--signing-key is required"))?;

    Ok(Options {
        endpoint: endpoint.ok_or_else(|| anyhow!("--endpoint is required"))?,
        redis_url: redis_url.ok_or_else(|| anyhow!("--redis is required"))?,
        amqp_url: amqp_url.ok_or_else(|| anyhow!("--amqp is required"))?,
        signing_key: std::fs::read(&key_path)
            .with_context(|| format!("could not read {key_path}"))?,
        key_id,
        issuer: issuer.ok_or_else(|| anyhow!("--issuer is required"))?,
        transcript: transcript.ok_or_else(|| anyhow!("--transcript is required"))?,
        record,
    })
}
