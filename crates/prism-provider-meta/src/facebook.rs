// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

//! Facebook Pages text publishing adapter.

use std::{collections::BTreeSet, fmt, sync::Arc};

use async_trait::async_trait;
use prism_core::{
    AltTextCapabilities, ChannelRef, CredentialRef, DeliveryError, DeliveryErrorClass, Extensions,
    MediaCapabilities, NamespacedKey, ProviderCapabilities, ProviderId, ProviderReceipt,
    PublicationFormat, TextCapabilities, ValidationIssue,
};
use prism_provider::{ProviderAdapter, ProviderPublishRequest, ProviderTargetContext};
use reqwest::{Client, Url, header::RETRY_AFTER};
use serde::Deserialize;
use serde_json::json;

use crate::{
    MetaAccessToken, MetaExternalId, MetaNumericId, MetaTransportConfigError, MetaTransportError,
    MetaTransportErrorKind, classify_graph_error, validated_api_base,
};

/// Stable Prism provider identifier for Facebook Pages.
pub const FACEBOOK_PROVIDER_ID: &str = "meta.facebook";
const DEFAULT_API_BASE: &str = "https://graph.facebook.com/v26.0/";

/// Resolved Facebook Page binding kept inside the adapter boundary.
pub struct FacebookBinding {
    page_id: MetaNumericId,
    access_token: MetaAccessToken,
}

impl FacebookBinding {
    /// Creates a resolved Page/token binding.
    #[must_use]
    pub const fn new(page_id: MetaNumericId, access_token: MetaAccessToken) -> Self {
        Self {
            page_id,
            access_token,
        }
    }

    /// Returns the resolved Page identifier.
    #[must_use]
    pub const fn page_id(&self) -> &MetaNumericId {
        &self.page_id
    }
}

impl fmt::Debug for FacebookBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FacebookBinding")
            .field("page_id", &self.page_id)
            .field("access_token", &self.access_token)
            .finish()
    }
}

/// Resolves opaque Prism references to a Facebook Page/token binding.
#[async_trait]
pub trait FacebookBindingResolver: Send + Sync + 'static {
    /// Resolves one configured channel and credential pair.
    async fn resolve(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
    ) -> Result<FacebookBinding, DeliveryError>;
}

/// Facebook Page publishing transport.
#[async_trait]
pub trait FacebookTransport: Send + Sync + 'static {
    /// Publishes one text post to a configured Page feed.
    async fn publish_text(
        &self,
        binding: &FacebookBinding,
        text: &str,
    ) -> Result<MetaExternalId, MetaTransportError>;
}

/// Reqwest transport for Graph API Page feed publishing.
#[derive(Clone, Debug)]
pub struct ReqwestFacebookTransport {
    client: Client,
    api_base: Url,
}

impl ReqwestFacebookTransport {
    /// Creates a transport for the current production Graph API version.
    pub fn new(client: Client) -> Result<Self, MetaTransportConfigError> {
        Self::with_api_base(client, DEFAULT_API_BASE)
    }

    /// Creates a transport with an explicit HTTPS Graph API base.
    pub fn with_api_base(client: Client, api_base: &str) -> Result<Self, MetaTransportConfigError> {
        Ok(Self {
            client,
            api_base: validated_api_base(api_base)?,
        })
    }

    fn endpoint(&self, page_id: &MetaNumericId) -> Result<Url, MetaTransportError> {
        let mut url = self.api_base.clone();
        let mut segments = url.path_segments_mut().map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "facebook.transport.invalid_base_url",
            )
        })?;
        segments.pop_if_empty();
        segments.push(page_id.as_str());
        segments.push("feed");
        drop(segments);
        Ok(url)
    }
}

#[async_trait]
impl FacebookTransport for ReqwestFacebookTransport {
    async fn publish_text(
        &self,
        binding: &FacebookBinding,
        text: &str,
    ) -> Result<MetaExternalId, MetaTransportError> {
        let response = self
            .client
            .post(self.endpoint(binding.page_id())?)
            .bearer_auth(binding.access_token.expose_secret())
            .form(&[("message", text)])
            .send()
            .await
            .map_err(|_| {
                MetaTransportError::new(MetaTransportErrorKind::Transient, "facebook.transport")
            })?;
        parse_id_response("facebook", response).await
    }
}

/// Official Facebook Page text publishing adapter.
pub struct FacebookAdapter {
    provider_id: ProviderId,
    bindings: Arc<dyn FacebookBindingResolver>,
    transport: Arc<dyn FacebookTransport>,
}

impl FacebookAdapter {
    /// Creates an adapter with injected binding and transport boundaries.
    #[must_use]
    pub fn new(
        bindings: Arc<dyn FacebookBindingResolver>,
        transport: Arc<dyn FacebookTransport>,
    ) -> Self {
        Self {
            provider_id: ProviderId::new(FACEBOOK_PROVIDER_ID)
                .expect("the static Facebook provider ID is valid"),
            bindings,
            transport,
        }
    }

    async fn resolve_binding(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<FacebookBinding, DeliveryError> {
        let credential = request.target.credential.as_ref().ok_or_else(|| {
            DeliveryError::new(
                DeliveryErrorClass::AuthRequired,
                "facebook.credential.required",
                "Facebook Page publishing requires a configured credential reference",
            )
        })?;
        self.bindings
            .resolve(&request.target.channel, credential)
            .await
    }
}

#[async_trait]
impl ProviderAdapter for FacebookAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    async fn capabilities(
        &self,
        _target: &ProviderTargetContext,
    ) -> Result<ProviderCapabilities, DeliveryError> {
        let mut extensions = Extensions::new();
        extensions.insert(
            NamespacedKey::new("meta.facebook.native_idempotency")
                .expect("static extension key is valid"),
            json!(false),
        );
        Ok(ProviderCapabilities {
            provider_id: self.provider_id.clone(),
            revision: Some("facebook-pages.text-v1".to_owned()),
            formats: BTreeSet::from([PublicationFormat::Post]),
            text: TextCapabilities {
                supported: true,
                max_characters: None,
            },
            media: MediaCapabilities {
                supported_kinds: BTreeSet::new(),
                max_items: Some(0),
                mixed_kinds: false,
                alt_text: AltTextCapabilities {
                    supported: false,
                    max_characters: None,
                },
            },
            extensions,
        })
    }

    async fn validate_publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<Vec<ValidationIssue>, DeliveryError> {
        let mut issues = Vec::new();
        if request.target.credential.is_none() {
            issues.push(ValidationIssue::error(
                "facebook.credential.required",
                format!("/targets/{}/credential", request.target.id),
                "Facebook Page publishing requires a configured credential reference",
            ));
        }
        for (key, _) in request.target.options.iter() {
            issues.push(ValidationIssue::error(
                "facebook.option.unsupported",
                format!("/targets/{}/options/{key}", request.target.id),
                "this Facebook adapter version does not implement provider options",
            ));
        }
        if !issues.iter().any(ValidationIssue::is_error) {
            self.resolve_binding(request).await?;
        }
        Ok(issues)
    }

    async fn publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<ProviderReceipt, DeliveryError> {
        let binding = self.resolve_binding(request).await?;
        let text = request
            .variant
            .body
            .text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| {
                DeliveryError::new(
                    DeliveryErrorClass::InvalidRequest,
                    "facebook.text.required",
                    "the Facebook Page text adapter requires non-empty text",
                )
            })?;
        let id = self
            .transport
            .publish_text(&binding, text)
            .await
            .map_err(map_publish_error)?;
        Ok(ProviderReceipt {
            external_id: id.as_str().to_owned(),
            external_url: None,
            details: Extensions::new(),
        })
    }
}

async fn parse_id_response(
    provider: &str,
    response: reqwest::Response,
) -> Result<MetaExternalId, MetaTransportError> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let body = response.bytes().await.map_err(|_| {
        MetaTransportError::new(
            MetaTransportErrorKind::InvalidResponse,
            format!("{provider}.response.unreadable"),
        )
    })?;
    if status.is_success() {
        let response: IdResponse = serde_json::from_slice(&body).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                format!("{provider}.response.invalid_json"),
            )
        })?;
        return MetaExternalId::new(response.id).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                format!("{provider}.response.invalid_id"),
            )
        });
    }
    Err(classify_graph_error(provider, status, &body, retry_after))
}

#[derive(Deserialize)]
struct IdResponse {
    id: String,
}

fn map_publish_error(error: MetaTransportError) -> DeliveryError {
    let class = match error.kind {
        MetaTransportErrorKind::Authentication => DeliveryErrorClass::AuthRequired,
        MetaTransportErrorKind::RateLimited => DeliveryErrorClass::RateLimited,
        MetaTransportErrorKind::Rejected => DeliveryErrorClass::ProviderRejected,
        MetaTransportErrorKind::Transient | MetaTransportErrorKind::InvalidResponse => {
            DeliveryErrorClass::OutcomeUnknown
        }
    };
    let mut mapped = DeliveryError::new(
        class,
        error.code,
        match class {
            DeliveryErrorClass::AuthRequired => {
                "Facebook authorization is missing, expired, or insufficient"
            }
            DeliveryErrorClass::RateLimited => "Facebook Page publishing is rate limited",
            DeliveryErrorClass::ProviderRejected => "Facebook rejected the Page publishing request",
            DeliveryErrorClass::OutcomeUnknown => {
                "Facebook Page publish outcome is unknown; reconcile provider state before retrying"
            }
            DeliveryErrorClass::InvalidRequest
            | DeliveryErrorClass::Retryable
            | DeliveryErrorClass::Terminal => "Facebook Page publishing failed",
        },
    );
    if let Some(seconds) = error.retry_after_seconds {
        mapped = mapped.with_retry_after(seconds);
    }
    mapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_rejects_plaintext_graph_bases() {
        let error = ReqwestFacebookTransport::with_api_base(
            Client::new(),
            "http://graph.facebook.com/v26.0/",
        )
        .expect_err("HTTP must be rejected");
        assert_eq!(error, MetaTransportConfigError::InsecureBaseUrl);
    }
}
