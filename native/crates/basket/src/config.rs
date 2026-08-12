//! Environment contract with the Aspire AppHost.
//!
//! Aspire hands executable resources the same variables it hands .NET projects, so this mirrors what
//! `Aspire.StackExchange.Redis` / `AddRabbitMqEventBus` / `AddDefaultAuthentication` consumed in
//! `src/Basket.API`.

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub redis_url: String,
    pub amqp_url: Option<String>,
    pub identity_url: Option<String>,
    pub queue_name: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port = std::env::var("PORT")
            .context("PORT is not set; the Aspire AppHost injects it via WithHttpEndpoint")?
            .parse()
            .context("PORT is not a valid port number")?;

        let redis_url = redis_url_from(
            &env_any(&["ConnectionStrings__redis", "ConnectionStrings__Redis"])
                .ok_or_else(|| anyhow!("ConnectionStrings__redis is not set"))?,
        )?;

        Ok(Self {
            port,
            redis_url,
            amqp_url: env_any(&["ConnectionStrings__eventbus", "ConnectionStrings__EventBus"]),
            identity_url: env_any(&["Identity__Url"]),
            queue_name: env_any(&["EventBus__SubscriptionClientName"])
                .unwrap_or_else(|| "Basket".to_owned()),
        })
    }
}

fn env_any(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
}

/// Converts a StackExchange.Redis connection string (`host:port,password=…`) into a Redis URL.
/// Already-URL values are passed through so local runs can set `redis://localhost`.
fn redis_url_from(connection_string: &str) -> Result<String> {
    let connection_string = connection_string.trim();
    if connection_string.starts_with("redis://") || connection_string.starts_with("rediss://") {
        return Ok(connection_string.to_owned());
    }

    let mut parts = connection_string.split(',');
    let endpoint = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("empty Redis connection string"))?;

    let mut password = None;
    for option in parts {
        let (key, value) = match option.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        if key.trim().eq_ignore_ascii_case("password") {
            password = Some(value.trim().to_owned());
        }
    }

    Ok(match password {
        Some(password) => format!("redis://:{}@{endpoint}", urlencode(&password)),
        None => format!("redis://{endpoint}"),
    })
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_host_port_becomes_a_url() {
        assert_eq!(redis_url_from("localhost:6379").unwrap(), "redis://localhost:6379");
    }

    #[test]
    fn stackexchange_password_option_is_translated() {
        assert_eq!(
            redis_url_from("127.0.0.1:52001,password=p@ss w0rd").unwrap(),
            "redis://:p%40ss%20w0rd@127.0.0.1:52001"
        );
    }

    #[test]
    fn url_connection_strings_pass_through() {
        assert_eq!(redis_url_from("redis://localhost").unwrap(), "redis://localhost");
    }
}
