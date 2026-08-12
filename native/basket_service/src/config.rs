//! Environment configuration. Reads the same variables Aspire injects for the .NET service:
//! `ConnectionStrings__redis`, `ConnectionStrings__eventbus`, `Identity__Url`,
//! `EventBus__SubscriptionClientName`.

use std::net::SocketAddr;

use anyhow::{Context, Result};

const DEFAULT_PORT: u16 = 5221;
const DEFAULT_SUBSCRIPTION_CLIENT_NAME: &str = "Basket";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub redis_url: String,
    pub event_bus_url: Option<String>,
    pub subscription_client_name: String,
    pub identity_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let redis_connection_string =
            first_env(&["ConnectionStrings__redis", "ConnectionStrings__Redis"])
                .unwrap_or_else(|| "localhost".to_owned());

        Ok(Self {
            listen_addr: listen_addr_from_env()?,
            redis_url: redis_url_from_connection_string(&redis_connection_string)?,
            event_bus_url: first_env(&[
                "ConnectionStrings__eventbus",
                "ConnectionStrings__EventBus",
            ]),
            subscription_client_name: first_env(&["EventBus__SubscriptionClientName"])
                .unwrap_or_else(|| DEFAULT_SUBSCRIPTION_CLIENT_NAME.to_owned()),
            identity_url: first_env(&["Identity__Url", "IDENTITY_URL"]),
        })
    }
}

fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
}

fn listen_addr_from_env() -> Result<SocketAddr> {
    if let Some(addr) = first_env(&["BASKET_LISTEN_ADDR"]) {
        return addr
            .parse()
            .with_context(|| format!("BASKET_LISTEN_ADDR is not a socket address: {addr}"));
    }

    let port = match first_env(&["PORT", "BASKET_PORT"]) {
        Some(port) => port
            .parse()
            .with_context(|| format!("PORT is not a port number: {port}"))?,
        // Aspire sets ASPNETCORE_URLS when the resource is registered as a project.
        None => match first_env(&["ASPNETCORE_URLS"]).and_then(|urls| port_from_urls(&urls)) {
            Some(port) => port,
            None => DEFAULT_PORT,
        },
    };

    Ok(SocketAddr::from(([0, 0, 0, 0], port)))
}

fn port_from_urls(urls: &str) -> Option<u16> {
    urls.split(';')
        .filter_map(|url| url.trim().rsplit_once(':'))
        .filter_map(|(_, port)| port.trim_end_matches('/').parse().ok())
        .next()
}

/// Translates a StackExchange.Redis connection string (`host:port,password=...`) into a redis URL.
/// Values that are already URLs are passed through untouched.
pub fn redis_url_from_connection_string(connection_string: &str) -> Result<String> {
    let connection_string = connection_string.trim();
    if connection_string.starts_with("redis://") || connection_string.starts_with("rediss://") {
        return Ok(connection_string.to_owned());
    }

    let mut parts = connection_string.split(',').map(str::trim);
    let endpoint = parts
        .next()
        .filter(|endpoint| !endpoint.is_empty())
        .context("redis connection string is empty")?;

    let mut password = None;
    let mut user = None;
    let mut database = None;
    let mut tls = false;

    for option in parts {
        let Some((key, value)) = option.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "password" => password = Some(value.trim().to_owned()),
            "user" | "username" => user = Some(value.trim().to_owned()),
            "defaultdatabase" => database = Some(value.trim().to_owned()),
            "ssl" => tls = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    let scheme = if tls { "rediss" } else { "redis" };
    let credentials = match (user, password) {
        (user, Some(password)) => format!(
            "{}:{}@",
            percent_encode(&user.unwrap_or_default()),
            percent_encode(&password)
        ),
        (Some(user), None) => format!("{}@", percent_encode(&user)),
        (None, None) => String::new(),
    };
    let database = database.map(|db| format!("/{db}")).unwrap_or_default();

    Ok(format!("{scheme}://{credentials}{endpoint}{database}"))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_host_becomes_a_redis_url() {
        assert_eq!(
            redis_url_from_connection_string("localhost").unwrap(),
            "redis://localhost"
        );
    }

    #[test]
    fn aspire_connection_string_carries_the_password() {
        assert_eq!(
            redis_url_from_connection_string("localhost:37561,password=s3cret").unwrap(),
            "redis://:s3cret@localhost:37561"
        );
    }

    #[test]
    fn special_characters_in_the_password_are_encoded() {
        assert_eq!(
            redis_url_from_connection_string("host:6379,password=p@ss/word").unwrap(),
            "redis://:p%40ss%2Fword@host:6379"
        );
    }

    #[test]
    fn database_and_tls_options_are_honoured() {
        assert_eq!(
            redis_url_from_connection_string("host:6380,ssl=True,defaultDatabase=3").unwrap(),
            "rediss://host:6380/3"
        );
    }

    #[test]
    fn urls_are_passed_through() {
        assert_eq!(
            redis_url_from_connection_string("redis://localhost:6379/1").unwrap(),
            "redis://localhost:6379/1"
        );
    }

    #[test]
    fn port_is_read_from_aspnetcore_urls() {
        assert_eq!(port_from_urls("http://localhost:42441"), Some(42441));
        assert_eq!(
            port_from_urls("http://localhost:5221;http://+:5222"),
            Some(5221)
        );
        assert_eq!(port_from_urls("not-a-url"), None);
    }
}
