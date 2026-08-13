//! Behavioral record/replay harness for the Basket migration.
//!
//! `record` drives a fixed sequence of gRPC calls against a **running** Basket service
//! (the .NET one) and writes what it observed — gRPC status, response items and the raw
//! Redis document — as JSON lines. `replay` runs the identical sequence against the Rust
//! service and fails on any difference. The transcript is what makes deleting the .NET
//! project safe: it is evidence from the real service, not from mocks.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use basket::events::{EXCHANGE, ORDER_STARTED};
use basket::proto::basket_client::BasketClient;
use basket::proto::{BasketItem, DeleteBasketRequest, GetBasketRequest, UpdateBasketRequest};
use clap::{Parser, Subcommand};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Connection, ConnectionProperties, ExchangeKind};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use tonic::Request;

const USER_ONE: &str = "parity-user-1";
const USER_TWO: &str = "parity-user-2";
const UNKNOWN_USER: &str = "parity-user-unknown";

#[derive(Parser)]
#[command(name = "basket-parity", about = "Record/replay parity harness for Basket.API")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Drive the cases against a running service and write the transcript.
    Record {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        out: PathBuf,
    },
    /// Re-run the cases and diff against a recorded transcript.
    Replay {
        #[command(flatten)]
        target: Target,
        #[arg(long, name = "in")]
        transcript: PathBuf,
    },
}

#[derive(clap::Args)]
struct Target {
    /// gRPC endpoint of the basket service under test.
    #[arg(long)]
    endpoint: String,
    /// Redis URL holding the `/basket/*` documents.
    #[arg(long)]
    redis: String,
    /// AMQP URI of the event bus.
    #[arg(long)]
    amqp: String,
    /// PEM private key the stub identity provider publishes as JWKS.
    #[arg(long)]
    signing_key: PathBuf,
    /// `kid` of that key.
    #[arg(long, default_value = "parity-key")]
    kid: String,
    /// Issuer the service is configured to trust.
    #[arg(long)]
    issuer: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenKind {
    None,
    Valid(&'static str),
    Garbage,
    Expired(&'static str),
    WrongIssuer(&'static str),
}

#[derive(Debug, Clone)]
enum Action {
    Get(TokenKind),
    Update(TokenKind, Vec<(i32, i32)>),
    Delete(TokenKind),
    PublishOrderStarted(&'static str),
}

struct Case {
    name: &'static str,
    action: Action,
    /// Whose Redis document to capture after the call.
    observe: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Observation {
    case: String,
    code: String,
    message: String,
    items: Vec<ObservedItem>,
    redis_document: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ObservedItem {
    product_id: i32,
    quantity: i32,
}

fn cases() -> Vec<Case> {
    use Action::*;
    use TokenKind::*;

    vec![
        Case { name: "get_anonymous_returns_empty", action: Get(None), observe: Some(USER_ONE) },
        Case { name: "get_with_garbage_token_returns_empty", action: Get(Garbage), observe: Option::None },
        Case { name: "get_with_expired_token_returns_empty", action: Get(Expired(USER_ONE)), observe: Option::None },
        Case { name: "get_with_wrong_issuer_token_returns_empty", action: Get(WrongIssuer(USER_ONE)), observe: Option::None },
        Case { name: "update_anonymous_is_unauthenticated", action: Update(None, vec![(1, 1)]), observe: Some(USER_ONE) },
        Case { name: "delete_anonymous_is_unauthenticated", action: Delete(None), observe: Option::None },
        Case { name: "get_empty_basket_for_valid_user", action: Get(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "update_creates_basket", action: Update(Valid(USER_ONE), vec![(1, 2), (2, 3)]), observe: Some(USER_ONE) },
        Case { name: "get_returns_created_basket", action: Get(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "update_replaces_items", action: Update(Valid(USER_ONE), vec![(5, 1)]), observe: Some(USER_ONE) },
        Case { name: "update_with_no_items_empties_basket", action: Update(Valid(USER_ONE), vec![]), observe: Some(USER_ONE) },
        Case { name: "update_accepts_non_positive_quantities", action: Update(Valid(USER_ONE), vec![(7, 0), (8, -3)]), observe: Some(USER_ONE) },
        Case { name: "update_keeps_duplicate_product_ids", action: Update(Valid(USER_ONE), vec![(9, 1), (9, 2)]), observe: Some(USER_ONE) },
        Case { name: "second_user_is_isolated", action: Update(Valid(USER_TWO), vec![(4, 4)]), observe: Some(USER_TWO) },
        Case { name: "first_user_basket_unchanged_by_second", action: Get(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "delete_removes_basket", action: Delete(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "delete_is_idempotent", action: Delete(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "get_after_delete_returns_empty", action: Get(Valid(USER_ONE)), observe: Some(USER_ONE) },
        Case { name: "order_started_event_deletes_basket", action: PublishOrderStarted(USER_TWO), observe: Some(USER_TWO) },
        Case { name: "get_after_order_started_returns_empty", action: Get(Valid(USER_TWO)), observe: Some(USER_TWO) },
        Case { name: "order_started_for_unknown_user_is_ignored", action: PublishOrderStarted(UNKNOWN_USER), observe: Some(UNKNOWN_USER) },
        Case { name: "service_still_serves_after_unknown_user_event", action: Update(Valid(USER_ONE), vec![(11, 5)]), observe: Some(USER_ONE) },
    ]
}

struct Runner {
    client: BasketClient<Channel>,
    redis: redis::aio::MultiplexedConnection,
    amqp: lapin::Channel,
    key: EncodingKey,
    kid: String,
    issuer: String,
}

impl Runner {
    async fn connect(target: &Target) -> Result<Self> {
        let channel = Channel::from_shared(target.endpoint.clone())?
            .connect_timeout(Duration::from_secs(10))
            .connect()
            .await
            .with_context(|| format!("connecting to {}", target.endpoint))?;

        let redis_client = redis::Client::open(target.redis.clone())?;
        let redis = redis_client.get_multiplexed_async_connection().await?;

        let connection =
            Connection::connect(&target.amqp, ConnectionProperties::default()).await?;
        let amqp = connection.create_channel().await?;
        amqp.exchange_declare(
            EXCHANGE.into(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

        let pem = fs::read(&target.signing_key)
            .with_context(|| format!("reading {}", target.signing_key.display()))?;

        Ok(Self {
            client: BasketClient::new(channel),
            redis,
            amqp,
            key: EncodingKey::from_rsa_pem(&pem)?,
            kid: target.kid.clone(),
            issuer: target.issuer.clone(),
        })
    }

    fn token(&self, kind: TokenKind) -> Option<String> {
        let (subject, issuer, expires_in) = match kind {
            TokenKind::None => return None,
            TokenKind::Garbage => return Some("not.a.valid.jwt".to_string()),
            TokenKind::Valid(user) => (user, self.issuer.clone(), 3600i64),
            TokenKind::Expired(user) => (user, self.issuer.clone(), -3600),
            TokenKind::WrongIssuer(user) => (user, "https://wrong-issuer.invalid".to_string(), 3600),
        };

        let now = chrono::Utc::now().timestamp();
        let mut claims = BTreeMap::new();
        claims.insert("sub".to_string(), serde_json::json!(subject));
        claims.insert("iss".to_string(), serde_json::json!(issuer));
        claims.insert("aud".to_string(), serde_json::json!("basket"));
        claims.insert("iat".to_string(), serde_json::json!(now - 60));
        claims.insert("nbf".to_string(), serde_json::json!(now - 60));
        claims.insert("exp".to_string(), serde_json::json!(now + expires_in));
        claims.insert("name".to_string(), serde_json::json!(subject));

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        Some(jsonwebtoken::encode(&header, &claims, &self.key).expect("token"))
    }

    fn request<T>(&self, message: T, kind: TokenKind) -> Result<Request<T>> {
        let mut request = Request::new(message);
        if let Some(token) = self.token(kind) {
            request
                .metadata_mut()
                .insert("authorization", format!("Bearer {token}").parse()?);
        }
        Ok(request)
    }

    async fn document(&mut self, user_id: &str) -> Result<Option<String>> {
        let raw: Option<Vec<u8>> = self.redis.get(format!("/basket/{user_id}")).await?;
        Ok(raw.map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
    }

    async fn publish_order_started(&self, user_id: &str) -> Result<()> {
        let payload = serde_json::json!({
            "Id": uuid::Uuid::new_v4().to_string(),
            "CreationDate": chrono::Utc::now().to_rfc3339(),
            "UserId": user_id,
        });

        self.amqp
            .basic_publish(
                EXCHANGE.into(),
                ORDER_STARTED.into(),
                BasicPublishOptions::default(),
                serde_json::to_vec(&payload)?.as_slice(),
                BasicProperties::default().with_delivery_mode(2),
            )
            .await?
            .await?;
        Ok(())
    }

    async fn run(&mut self, case: &Case) -> Result<Observation> {
        let (code, message, items) = match &case.action {
            Action::Get(token) => {
                let request = self.request(GetBasketRequest {}, *token)?;
                match self.client.get_basket(request).await {
                    Ok(response) => ("Ok".into(), String::new(), collect(response.into_inner().items)),
                    Err(status) => (format!("{:?}", status.code()), status.message().to_string(), vec![]),
                }
            }
            Action::Update(token, items) => {
                let message = UpdateBasketRequest {
                    items: items
                        .iter()
                        .map(|(product_id, quantity)| BasketItem {
                            product_id: *product_id,
                            quantity: *quantity,
                        })
                        .collect(),
                };
                let request = self.request(message, *token)?;
                match self.client.update_basket(request).await {
                    Ok(response) => ("Ok".into(), String::new(), collect(response.into_inner().items)),
                    Err(status) => (format!("{:?}", status.code()), status.message().to_string(), vec![]),
                }
            }
            Action::Delete(token) => {
                let request = self.request(DeleteBasketRequest {}, *token)?;
                match self.client.delete_basket(request).await {
                    Ok(_) => ("Ok".into(), String::new(), vec![]),
                    Err(status) => (format!("{:?}", status.code()), status.message().to_string(), vec![]),
                }
            }
            Action::PublishOrderStarted(user) => {
                let before = self.document(user).await?;
                self.publish_order_started(user).await?;
                self.settle(user, before).await?;
                ("Ok".into(), String::new(), vec![])
            }
        };

        let redis_document = match case.observe {
            Some(user) => self.document(user).await?,
            None => None,
        };

        Ok(Observation {
            case: case.name.to_string(),
            code,
            message,
            items,
            redis_document,
        })
    }

    /// Wait for the asynchronous consumer to act, without assuming what it does:
    /// stop as soon as the document changes, otherwise give up after the timeout.
    async fn settle(&mut self, user_id: &str, before: Option<String>) -> Result<()> {
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if self.document(user_id).await? != before {
                return Ok(());
            }
        }
        Ok(())
    }
}

fn collect(items: Vec<basket::proto::BasketItem>) -> Vec<ObservedItem> {
    items
        .into_iter()
        .map(|item| ObservedItem {
            product_id: item.product_id,
            quantity: item.quantity,
        })
        .collect()
}

async fn observe_all(target: &Target) -> Result<Vec<Observation>> {
    let mut runner = Runner::connect(target).await?;
    let mut observations = Vec::new();
    for case in cases() {
        let observation = runner.run(&case).await?;
        println!("  · {}", observation.case);
        observations.push(observation);
    }
    Ok(observations)
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Record { target, out } => {
            println!("recording {} cases from {}", cases().len(), target.endpoint);
            let observations = observe_all(&target).await?;
            let mut lines = String::new();
            for observation in &observations {
                lines.push_str(&serde_json::to_string(observation)?);
                lines.push('\n');
            }
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, lines)?;
            println!("wrote {} observations to {}", observations.len(), out.display());
        }
        Command::Replay { target, transcript } => {
            let recorded: Vec<Observation> = fs::read_to_string(&transcript)
                .with_context(|| format!("reading {}", transcript.display()))?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(serde_json::from_str)
                .collect::<Result<_, _>>()?;

            println!(
                "replaying {} recorded cases against {}",
                recorded.len(),
                target.endpoint
            );
            let actual = observe_all(&target).await?;

            let mut failures = Vec::new();
            if recorded.len() != actual.len() {
                bail!(
                    "transcript has {} cases but {} ran; re-record",
                    recorded.len(),
                    actual.len()
                );
            }
            for (expected, got) in recorded.iter().zip(actual.iter()) {
                if expected.case != got.case {
                    bail!("case order drifted: expected {}, ran {}", expected.case, got.case);
                }
                if expected != got {
                    failures.push(format!(
                        "case {}\n  recorded: {}\n  rust    : {}",
                        expected.case,
                        serde_json::to_string(expected).unwrap_or_default(),
                        serde_json::to_string(got).unwrap_or_default()
                    ));
                }
            }

            if failures.is_empty() {
                println!("parity: {}/{} cases identical", actual.len(), recorded.len());
            } else {
                for failure in &failures {
                    eprintln!("{failure}");
                }
                bail!("{} of {} cases differ", failures.len(), recorded.len());
            }
        }
    }

    Ok(())
}
