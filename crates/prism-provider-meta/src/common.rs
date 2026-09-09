// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use reqwest::{StatusCode, Url};
use serde::Deserialize;
use thiserror::Error;

/// Secret Meta access token with intentionally redacted formatting.
pub struct MetaAccessToken(String);

impl MetaAccessToken {
    /// Wraps a bounded non-empty token without exposing it through formatting.
    pub fn new(value: impl Into<String>) -> Result<Self, MetaValueError> {
        let value = value.into();
        if value.is_empty() || value.len() > 8_192 || value.chars().any(char::is_whitespace) {
            return Err(MetaValueError::InvalidAccessToken);
        }
        Ok(Self(value))
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for MetaAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MetaAccessToken([REDACTED])")
    }
}

/// Numeric Meta object identifier used in Graph API path segments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaNumericId(String);

impl MetaNumericId {
    /// Creates a bounded decimal identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, MetaValueError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.chars().all(|character| character.is_ascii_digit())
        {
            return Err(MetaValueError::InvalidNumericId);
        }
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Provider-native identifier returned after an external action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaExternalId(String);

impl MetaExternalId {
    /// Creates a bounded identifier without accepting whitespace/control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, MetaValueError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 512
            || value.chars().any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(MetaValueError::InvalidExternalId);
        }
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Public media URL resolved inside an adapter boundary.
pub struct PublicMediaUrl(Url);

impl PublicMediaUrl {
    /// Accepts only HTTPS URLs with no embedded user credentials or fragments.
    pub fn new(value: &str) -> Result<Self, MetaValueError> {
        let url = Url::parse(value).map_err(|_| MetaValueError::InvalidMediaUrl)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(MetaValueError::InvalidMediaUrl);
        }
        Ok(Self(url))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for PublicMediaUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PublicMediaUrl([REDACTED])")
    }
}

/// Invalid secret, identifier, recipient, or public-media value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MetaValueError {
    /// Access token is missing or unsafe to retain.
    #[error("invalid Meta access token")]
    InvalidAccessToken,
    /// Graph API path identifier is not a bounded decimal string.
    #[error("invalid Meta numeric identifier")]
    InvalidNumericId,
    /// Provider response identifier is empty or unsafe.
    #[error("invalid Meta external identifier")]
    InvalidExternalId,
    /// Media URL is not a safe public HTTPS URL.
    #[error("invalid public media URL")]
    InvalidMediaUrl,
    /// WhatsApp recipient is not a canonical decimal recipient identifier.
    #[error("invalid WhatsApp recipient")]
    InvalidRecipient,
}

/// Invalid Graph API transport configuration shared by Meta adapters.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MetaTransportConfigError {
    /// Base URL is malformed or cannot safely accept path segments.
    #[error("invalid Meta API base URL")]
    InvalidBaseUrl,
    /// Provider credentials must never be sent over plaintext HTTP.
    #[error("Meta API base URL must use HTTPS")]
    InsecureBaseUrl,
}

pub(crate) fn validated_api_base(value: &str) -> Result<Url, MetaTransportConfigError> {
    let url = Url::parse(value).map_err(|_| MetaTransportConfigError::InvalidBaseUrl)?;
    if url.scheme() != "https" {
        return Err(MetaTransportConfigError::InsecureBaseUrl);
    }
    if url.cannot_be_a_base()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(MetaTransportConfigError::InvalidBaseUrl);
    }
    Ok(url)
}

/// Safe provider transport failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetaTransportErrorKind {
    /// Token is missing, expired, or lacks required scopes.
    Authentication,
    /// Provider rejected the call because a quota is exhausted.
    RateLimited,
    /// Provider returned a deterministic request rejection.
    Rejected,
    /// Transport or provider failure that may be transient.
    Transient,
    /// A successful response could not be interpreted safely.
    InvalidResponse,
}

/// Redacted transport error returned by Meta HTTP implementations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetaTransportError {
    /// Stable category used by stage-aware adapter mapping.
    pub kind: MetaTransportErrorKind,
    /// Safe stable code without raw provider content.
    pub code: String,
    /// Optional retry guidance from the provider.
    pub retry_after_seconds: Option<u64>,
}

impl MetaTransportError {
    /// Creates a redacted transport error.
    #[must_use]
    pub fn new(kind: MetaTransportErrorKind, code: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            retry_after_seconds: None,
        }
    }

    /// Adds provider retry guidance.
    #[must_use]
    pub const fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after_seconds = Some(seconds);
        self
    }
}

#[derive(Default, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: Option<GraphError>,
}

#[derive(Default, Deserialize)]
struct GraphError {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    error_subcode: Option<i64>,
    #[serde(default)]
    is_transient: bool,
}

pub(crate) fn classify_graph_error(
    provider: &str,
    status: StatusCode,
    body: &[u8],
    retry_after_seconds: Option<u64>,
) -> MetaTransportError {
    let graph = serde_json::from_slice::<ErrorEnvelope>(body)
        .ok()
        .and_then(|envelope| envelope.error)
        .unwrap_or_default();
    let kind = if status == StatusCode::UNAUTHORIZED
        || status == StatusCode::FORBIDDEN
        || graph.code == Some(190)
    {
        MetaTransportErrorKind::Authentication
    } else if status == StatusCode::TOO_MANY_REQUESTS
        || matches!(graph.code, Some(4 | 17 | 32 | 613))
    {
        MetaTransportErrorKind::RateLimited
    } else if status.is_server_error() || graph.is_transient {
        MetaTransportErrorKind::Transient
    } else {
        MetaTransportErrorKind::Rejected
    };
    let code = match (graph.code, graph.error_subcode) {
        (Some(code), Some(subcode)) => format!("{provider}.api.{code}.{subcode}"),
        (Some(code), None) => format!("{provider}.api.{code}"),
        (None, _) => format!("{provider}.http.{}", status.as_u16()),
    };
    let mut error = MetaTransportError::new(kind, code);
    if let Some(seconds) = retry_after_seconds {
        error = error.with_retry_after(seconds);
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_signed_media_urls_are_redacted() {
        let token = MetaAccessToken::new("secret-token").expect("valid token");
        let media = PublicMediaUrl::new("https://cdn.example/image.jpg?signature=secret")
            .expect("valid media URL");
        assert_eq!(format!("{token:?}"), "MetaAccessToken([REDACTED])");
        assert_eq!(format!("{media:?}"), "PublicMediaUrl([REDACTED])");
    }

    #[test]
    fn graph_errors_drop_upstream_messages() {
        let error = classify_graph_error(
            "facebook",
            StatusCode::BAD_REQUEST,
            br#"{"error":{"message":"sensitive upstream detail","code":190}}"#,
            None,
        );
        assert_eq!(error.kind, MetaTransportErrorKind::Authentication);
        assert_eq!(error.code, "facebook.api.190");
        assert!(!format!("{error:?}").contains("sensitive upstream detail"));
    }
}
