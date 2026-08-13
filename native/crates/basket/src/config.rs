//! Configuration read from the environment Aspire injects.

use anyhow::{anyhow, Result};

/// Aspire hands Redis over as a StackExchange.Redis connection string
/// (`host:port,password=…,ssl=True`); the `redis` crate wants a URL.
pub fn redis_url_from_connection_string(connection_string: &str) -> Result<String> {
    let mut parts = connection_string.split(',').map(str::trim);
    let endpoint = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("empty redis connection string"))?;

    if endpoint.starts_with("redis://") || endpoint.starts_with("rediss://") {
        return Ok(endpoint.to_string());
    }

    let mut password = None;
    let mut user = None;
    let mut tls = false;

    for option in parts {
        let Some((key, value)) = option.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "password" => password = Some(value.trim().to_string()),
            "user" => user = Some(value.trim().to_string()),
            "ssl" => tls = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    let scheme = if tls { "rediss" } else { "redis" };
    let credentials = match (user, password) {
        (Some(user), Some(password)) => format!("{}:{}@", encode(&user), encode(&password)),
        (None, Some(password)) => format!(":{}@", encode(&password)),
        (Some(user), None) => format!("{}@", encode(&user)),
        (None, None) => String::new(),
    };

    Ok(format!("{scheme}://{credentials}{endpoint}"))
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// Aspire prefers `ConnectionStrings__x`; local runs may use `ConnectionStrings:x`.
pub fn connection_string(name: &str) -> Option<String> {
    std::env::var(format!("ConnectionStrings__{name}"))
        .or_else(|_| std::env::var(format!("ConnectionStrings:{name}")))
        .ok()
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_aspire_redis_connection_strings() {
        assert_eq!(
            redis_url_from_connection_string("localhost:6379").unwrap(),
            "redis://localhost:6379"
        );
        assert_eq!(
            redis_url_from_connection_string("127.0.0.1:57002,password=p@ss w0rd").unwrap(),
            "redis://:p%40ss%20w0rd@127.0.0.1:57002"
        );
        assert_eq!(
            redis_url_from_connection_string("cache:6380,ssl=True,password=abc").unwrap(),
            "rediss://:abc@cache:6380"
        );
    }

    #[test]
    fn passes_urls_through() {
        assert_eq!(
            redis_url_from_connection_string("redis://localhost:6379").unwrap(),
            "redis://localhost:6379"
        );
    }
}
