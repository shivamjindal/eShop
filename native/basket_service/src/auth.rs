//! JWT bearer authentication — port of `AddDefaultAuthentication` in
//! `src/eShop.ServiceDefaults/AuthenticationExtensions.cs`.
//!
//! Same rules as the .NET service: signing keys come from the Identity authority's OIDC discovery
//! document, the issuer must match `Identity:Url`, the audience is **not** validated, and the raw
//! `sub` claim is the basket owner. A token that fails validation makes the caller anonymous
//! rather than failing the request, because the .NET gRPC endpoints declare no authorization
//! requirement — each RPC applies its own identity gate.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::RwLock;

/// Matches the default `TokenValidationParameters.ClockSkew` of Microsoft's handler (5 minutes).
const CLOCK_SKEW: u64 = 300;
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    jwks_uri: String,
}

#[derive(Deserialize)]
struct Claims {
    sub: Option<String>,
}

struct SigningKey {
    kid: Option<String>,
    algorithm: Algorithm,
    key: DecodingKey,
}

struct Authority {
    url: String,
    http: reqwest::Client,
    keys: RwLock<Vec<SigningKey>>,
    last_refresh: RwLock<Option<Instant>>,
}

pub struct JwtAuthenticator {
    /// `None` when no Identity authority is configured, mirroring the .NET service skipping
    /// authentication when the `Identity` configuration section is absent.
    authority: Option<Authority>,
}

impl JwtAuthenticator {
    pub fn new(identity_url: Option<String>) -> Result<Self> {
        let Some(url) = identity_url else {
            return Ok(Self { authority: None });
        };

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("building the http client for OIDC metadata")?;

        Ok(Self {
            authority: Some(Authority {
                url: url.trim_end_matches('/').to_owned(),
                http,
                keys: RwLock::new(Vec::new()),
                last_refresh: RwLock::new(None),
            }),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.authority.is_some()
    }

    pub fn authority_url(&self) -> Option<&str> {
        self.authority
            .as_ref()
            .map(|authority| authority.url.as_str())
    }

    /// Loads the signing keys up front so the first request does not pay for discovery.
    pub async fn warm_up(&self) -> Result<()> {
        match &self.authority {
            Some(authority) => authority.refresh_keys().await,
            None => Ok(()),
        }
    }

    /// Returns the `sub` claim of a valid bearer token, or `None` for an anonymous caller.
    pub async fn subject(&self, authorization_header: Option<&str>) -> Option<String> {
        let authority = self.authority.as_ref()?;
        let token = bearer_token(authorization_header?)?;
        authority.subject(token).await
    }
}

impl Authority {
    async fn subject(&self, token: &str) -> Option<String> {
        let header = decode_header(token).ok()?;
        if !is_supported(header.alg) {
            return None;
        }

        if let Some(subject) = self
            .try_decode(token, header.alg, header.kid.as_deref())
            .await
        {
            return Some(subject);
        }

        // An unknown key id usually means the authority rotated (or has just started), so pull the
        // discovery document again before giving up.
        if self.refresh_keys().await.is_ok() {
            return self
                .try_decode(token, header.alg, header.kid.as_deref())
                .await;
        }

        None
    }

    async fn try_decode(
        &self,
        token: &str,
        algorithm: Algorithm,
        kid: Option<&str>,
    ) -> Option<String> {
        let mut validation = Validation::new(algorithm);
        validation.validate_aud = false;
        validation.leeway = CLOCK_SKEW;
        validation.set_issuer(&[&self.url]);

        let keys = self.keys.read().await;
        for key in keys
            .iter()
            .filter(|key| key.algorithm == algorithm)
            .filter(|key| kid.is_none() || key.kid.is_none() || key.kid.as_deref() == kid)
        {
            match decode::<Claims>(token, &key.key, &validation) {
                Ok(token) => return token.claims.sub.filter(|sub| !sub.is_empty()),
                Err(error) => {
                    tracing::debug!(error = %error, "rejected bearer token");
                }
            }
        }

        None
    }

    async fn refresh_keys(&self) -> Result<()> {
        {
            let last_refresh = self.last_refresh.read().await;
            if last_refresh.is_some_and(|at| at.elapsed() < MIN_REFRESH_INTERVAL) {
                return Ok(());
            }
        }

        let discovery_url = format!("{}/.well-known/openid-configuration", self.url);
        let discovery: DiscoveryDocument = self
            .http
            .get(&discovery_url)
            .send()
            .await
            .with_context(|| format!("requesting {discovery_url}"))?
            .error_for_status()?
            .json()
            .await
            .with_context(|| format!("parsing {discovery_url}"))?;

        if discovery.issuer.trim_end_matches('/') != self.url {
            tracing::warn!(
                configured = %self.url,
                discovered = %discovery.issuer,
                "identity authority issuer does not match the configured url; tokens will be rejected"
            );
        }

        let jwks: JwkSet = self
            .http
            .get(&discovery.jwks_uri)
            .send()
            .await
            .with_context(|| format!("requesting {}", discovery.jwks_uri))?
            .error_for_status()?
            .json()
            .await
            .with_context(|| format!("parsing {}", discovery.jwks_uri))?;

        let mut keys = Vec::new();
        for jwk in &jwks.keys {
            let algorithm = jwk
                .common
                .key_algorithm
                .and_then(|algorithm| algorithm.to_string().parse::<Algorithm>().ok())
                .unwrap_or(Algorithm::RS256);

            match DecodingKey::from_jwk(jwk) {
                Ok(key) => keys.push(SigningKey {
                    kid: jwk.common.key_id.clone(),
                    algorithm,
                    key,
                }),
                Err(error) => tracing::warn!(error = %error, "skipping unusable signing key"),
            }
        }

        tracing::info!(count = keys.len(), authority = %self.url, "loaded identity signing keys");
        *self.keys.write().await = keys;
        *self.last_refresh.write().await = Some(Instant::now());

        Ok(())
    }
}

fn is_supported(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
    )
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
    fn bearer_scheme_is_case_insensitive() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("BEARER abc"), Some("abc"));
    }

    #[test]
    fn other_schemes_and_empty_tokens_are_ignored() {
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("abc"), None);
    }

    #[tokio::test]
    async fn without_an_authority_every_caller_is_anonymous() {
        let authenticator = JwtAuthenticator::new(None).unwrap();

        assert!(!authenticator.is_enabled());
        assert_eq!(authenticator.subject(Some("Bearer whatever")).await, None);
    }

    #[tokio::test]
    async fn a_missing_header_is_anonymous() {
        let authenticator = JwtAuthenticator::new(Some("http://localhost:5223".into())).unwrap();

        assert_eq!(authenticator.subject(None).await, None);
    }

    #[test]
    fn trailing_slashes_are_trimmed_from_the_authority() {
        let authenticator = JwtAuthenticator::new(Some("http://localhost:5223/".into())).unwrap();

        assert_eq!(authenticator.authority_url(), Some("http://localhost:5223"));
    }
}
