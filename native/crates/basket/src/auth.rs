//! JWT bearer validation — the port of `AddDefaultAuthentication` as Basket used it.
//!
//! Basket's gRPC methods carry no authorization requirement, so a missing, malformed
//! or rejected token does not fail the call: the caller is simply anonymous, and each
//! RPC decides what that means. Identity comes from the `sub` claim.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;
use tokio::sync::RwLock;

const CLOCK_SKEW_SECONDS: u64 = 300; // Microsoft.IdentityModel default
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
struct KeyCache {
    keys: HashMap<String, DecodingKey>,
    issuers: Vec<String>,
    last_refresh: Option<Instant>,
}

/// Validates tokens issued by Identity.API. With no authority configured every
/// caller is anonymous, which matches a .NET host started without `Identity:Url`.
pub struct TokenValidator {
    authority: Option<String>,
    http: reqwest::Client,
    cache: Arc<RwLock<KeyCache>>,
}

impl TokenValidator {
    pub fn new(authority: Option<String>) -> Self {
        Self {
            authority: authority.map(|value| value.trim_end_matches('/').to_string()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("http client"),
            cache: Arc::new(RwLock::new(KeyCache::default())),
        }
    }

    /// The `sub` claim of a valid token, or `None` for an anonymous caller.
    pub async fn user_id(&self, authorization_header: Option<&str>) -> Option<String> {
        let token = bearer_token(authorization_header?)?;
        match self.validate(token).await {
            Ok(claims) => claims
                .get("sub")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|sub| !sub.is_empty()),
            Err(error) => {
                tracing::debug!(%error, "rejecting bearer token; treating caller as anonymous");
                None
            }
        }
    }

    async fn validate(&self, token: &str) -> Result<Value> {
        let authority = self
            .authority
            .as_deref()
            .ok_or_else(|| anyhow!("no identity authority configured"))?;

        let kid = decode_header(token)?
            .kid
            .ok_or_else(|| anyhow!("token header has no kid"))?;

        let (key, issuers) = match self.cached(&kid).await {
            Some(found) => found,
            None => {
                self.refresh(authority).await?;
                self.cached(&kid)
                    .await
                    .ok_or_else(|| anyhow!("no signing key for kid {kid}"))?
            }
        };

        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = CLOCK_SKEW_SECONDS;
        validation.validate_aud = false; // ValidateAudience = false in AuthenticationExtensions
        validation.set_issuer(&issuers);

        Ok(decode::<Value>(token, &key, &validation)?.claims)
    }

    async fn cached(&self, kid: &str) -> Option<(DecodingKey, Vec<String>)> {
        let cache = self.cache.read().await;
        cache
            .keys
            .get(kid)
            .map(|key| (key.clone(), cache.issuers.clone()))
    }

    async fn refresh(&self, authority: &str) -> Result<()> {
        {
            let cache = self.cache.read().await;
            if cache
                .last_refresh
                .is_some_and(|at| at.elapsed() < MIN_REFRESH_INTERVAL)
            {
                return Err(anyhow!("signing keys refreshed too recently"));
            }
        }

        let discovery: Value = self
            .http
            .get(format!("{authority}/.well-known/openid-configuration"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let jwks_uri = discovery
            .get("jwks_uri")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("discovery document has no jwks_uri"))?;
        let metadata_issuer = discovery
            .get("issuer")
            .and_then(Value::as_str)
            .map(str::to_string);

        let jwks: Value = self
            .http
            .get(jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut keys = HashMap::new();
        for jwk in jwks
            .get("keys")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let (Some(kid), Some(n), Some(e)) = (
                jwk.get("kid").and_then(Value::as_str),
                jwk.get("n").and_then(Value::as_str),
                jwk.get("e").and_then(Value::as_str),
            ) else {
                continue;
            };
            if let Ok(key) = DecodingKey::from_rsa_components(n, e) {
                keys.insert(kid.to_string(), key);
            }
        }

        // JwtBearerHandler accepts both the configured authority and the issuer the
        // metadata advertises.
        let mut issuers = vec![authority.to_string()];
        if let Some(issuer) = metadata_issuer {
            if !issuers.contains(&issuer) {
                issuers.push(issuer);
            }
        }

        let mut cache = self.cache.write().await;
        cache.keys = keys;
        cache.issuers = issuers;
        cache.last_refresh = Some(Instant::now());
        Ok(())
    }
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_headers_case_insensitively() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("abc"), None);
    }

    #[tokio::test]
    async fn anonymous_without_authority_or_header() {
        let validator = TokenValidator::new(None);
        assert_eq!(validator.user_id(None).await, None);
        assert_eq!(validator.user_id(Some("Bearer nonsense")).await, None);
    }

    #[tokio::test]
    async fn garbage_tokens_are_anonymous_not_errors() {
        let validator = TokenValidator::new(Some("http://localhost:1".into()));
        assert_eq!(validator.user_id(Some("Bearer not.a.jwt")).await, None);
    }
}
