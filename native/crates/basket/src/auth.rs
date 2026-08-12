//! JWT bearer authentication against Identity.API.
//!
//! Mirrors `eShop.ServiceDefaults.AddDefaultAuthentication`: the authority is `Identity__Url`, the
//! issuer must match it, the audience is deliberately **not** validated, and the user id comes from
//! the `sub` claim. Basket's RPCs declare no authorization requirement, so a missing or unusable
//! token makes the caller anonymous rather than producing an error.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::RwLock;

/// Matches the default `TokenValidationParameters.ClockSkew` in ASP.NET Core.
const CLOCK_SKEW: u64 = 300;
/// Floor between JWKS refetches so an unknown `kid` cannot turn into a request amplifier.
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct Claims {
    sub: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Discovery {
    jwks_uri: String,
}

#[derive(Clone)]
pub struct TokenValidator {
    inner: Option<Arc<Inner>>,
}

struct Inner {
    issuer: String,
    http: reqwest::Client,
    keys: RwLock<KeyCache>,
}

#[derive(Default)]
struct KeyCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
}

impl TokenValidator {
    /// A validator with no issuer treats every caller as anonymous, like a Basket.API deployment
    /// without an `Identity` configuration section.
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn new(identity_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("could not build the Identity HTTP client")?;

        Ok(Self {
            inner: Some(Arc::new(Inner {
                issuer: identity_url.trim_end_matches('/').to_owned(),
                http,
                keys: RwLock::new(KeyCache::default()),
            })),
        })
    }

    /// Resolves the caller's user id from an `authorization` header value, or `None` when the caller
    /// is anonymous or the token cannot be trusted.
    pub async fn user_id(&self, authorization: Option<&str>) -> Option<String> {
        let inner = self.inner.as_ref()?;
        let token = bearer_token(authorization?)?;

        match inner.validate(token).await {
            Ok(user_id) => user_id,
            Err(error) => {
                tracing::debug!(%error, "rejected bearer token; treating the caller as anonymous");
                None
            }
        }
    }
}

impl Inner {
    async fn validate(&self, token: &str) -> Result<Option<String>> {
        let header = decode_header(token).context("unreadable JWT header")?;
        let kid = header.kid.context("JWT has no kid")?;

        let key = match self.key(&kid).await? {
            Some(key) => key,
            None => {
                self.refresh_keys().await?;
                self.key(&kid)
                    .await?
                    .with_context(|| format!("no signing key for kid {kid}"))?
            }
        };

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.issuer]);
        // Basket.API sets ValidateAudience = false.
        validation.validate_aud = false;
        validation.leeway = CLOCK_SKEW;

        let claims = decode::<Claims>(token, &key, &validation)
            .context("JWT failed validation")?
            .claims;

        Ok(claims.sub.filter(|sub| !sub.is_empty()))
    }

    async fn key(&self, kid: &str) -> Result<Option<DecodingKey>> {
        {
            let cache = self.keys.read().await;
            if let Some(key) = cache.keys.get(kid) {
                return Ok(Some(key.clone()));
            }
            if cache.fetched_at.is_some() {
                return Ok(None);
            }
        }

        self.refresh_keys().await?;
        Ok(self.keys.read().await.keys.get(kid).cloned())
    }

    async fn refresh_keys(&self) -> Result<()> {
        {
            let cache = self.keys.read().await;
            if cache
                .fetched_at
                .is_some_and(|fetched_at| fetched_at.elapsed() < MIN_REFRESH_INTERVAL)
            {
                return Ok(());
            }
        }

        let discovery: Discovery = self
            .http
            .get(format!("{}/.well-known/openid-configuration", self.issuer))
            .send()
            .await
            .context("could not reach the Identity discovery document")?
            .error_for_status()?
            .json()
            .await
            .context("malformed Identity discovery document")?;

        let jwks: JwkSet = self
            .http
            .get(&discovery.jwks_uri)
            .send()
            .await
            .context("could not reach the Identity JWKS endpoint")?
            .error_for_status()?
            .json()
            .await
            .context("malformed Identity JWKS")?;

        let mut keys = HashMap::new();
        for jwk in &jwks.keys {
            let Some(kid) = jwk.common.key_id.clone() else {
                continue;
            };
            match DecodingKey::from_jwk(jwk) {
                Ok(key) => {
                    keys.insert(kid, key);
                }
                Err(error) => tracing::warn!(%kid, %error, "skipping unusable JWKS entry"),
            }
        }

        tracing::info!(count = keys.len(), "loaded Identity signing keys");

        let mut cache = self.keys.write().await;
        cache.keys = keys;
        cache.fetched_at = Some(Instant::now());
        Ok(())
    }
}

fn bearer_token(authorization: &str) -> Option<&str> {
    let (scheme, token) = authorization.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("BEARER abc"), Some("abc"));
    }

    #[test]
    fn non_bearer_headers_are_ignored() {
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("abc"), None);
    }

    #[tokio::test]
    async fn a_disabled_validator_treats_everyone_as_anonymous() {
        let validator = TokenValidator::disabled();

        assert_eq!(validator.user_id(Some("Bearer anything")).await, None);
        assert_eq!(validator.user_id(None).await, None);
    }

    #[tokio::test]
    async fn garbage_tokens_do_not_authenticate() {
        let validator = TokenValidator::new("http://localhost:1").unwrap();

        assert_eq!(validator.user_id(Some("Bearer not-a-jwt")).await, None);
    }
}
