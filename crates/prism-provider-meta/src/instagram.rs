// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

//! Instagram Professional account single-image feed publishing adapter.

use std::{collections::BTreeSet, fmt, sync::Arc};

use async_trait::async_trait;
use prism_core::{
    AltTextCapabilities, ChannelRef, CredentialRef, DeliveryError, DeliveryErrorClass, Extensions,
    MediaCapabilities, MediaKind, MediaRef, NamespacedKey, ProviderCapabilities, ProviderId,
    ProviderReceipt, PublicationFormat, TextCapabilities, ValidationIssue,
};
use prism_provider::{ProviderAdapter, ProviderPublishRequest, ProviderTargetContext};
use reqwest::{Client, Url, header::RETRY_AFTER};
use serde::Deserialize;
use serde_json::json;

use crate::{
    MetaAccessToken, MetaExternalId, MetaNumericId, MetaTransportConfigError, MetaTransportError,
    MetaTransportErrorKind, PublicMediaUrl, classify_graph_error, validated_api_base,
};

/// Stable Prism provider identifier for Instagram.
pub const INSTAGRAM_PROVIDER_ID: &str = "meta.instagram";
const DEFAULT_API_BASE: &str = "https://graph.instagram.com/v26.0/";

/// Resolved Instagram Professional account binding.
pub struct InstagramBinding {
    user_id: MetaNumericId,
    access_token: MetaAccessToken,
}

impl InstagramBinding {
    /// Creates a resolved Instagram user/token binding.
    #[must_use]
    pub const fn new(user_id: MetaNumericId, access_token: MetaAccessToken) -> Self {
        Self {
            user_id,
            access_token,
        }
    }

    /// Returns the resolved Instagram user identifier.
    #[must_use]
    pub const fn user_id(&self) -> &MetaNumericId {
        &self.user_id
    }
}

impl fmt::Debug for InstagramBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstagramBinding")
            .field("user_id", &self.user_id)
            .field("access_token", &self.access_token)
            .finish()
    }
}

/// Resolves opaque Prism references to an Instagram account/token binding.
#[async_trait]
pub trait InstagramBindingResolver: Send + Sync + 'static {
    /// Resolves one configured channel and credential pair.
    async fn resolve(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
    ) -> Result<InstagramBinding, DeliveryError>;
}

/// Resolves opaque Prism media references to provider-fetchable public HTTPS URLs.
#[async_trait]
pub trait InstagramMediaResolver: Send + Sync + 'static {
    /// Resolves one media reference without exposing storage credentials to Prism contracts.
    async fn resolve(&self, media: &MediaRef) -> Result<PublicMediaUrl, DeliveryError>;
}

/// Instagram media container/publish transport.
#[async_trait]
pub trait InstagramTransport: Send + Sync + 'static {
    /// Creates one unpublished single-image media container.
    async fn create_image_container(
        &self,
        binding: &InstagramBinding,
        image_url: &PublicMediaUrl,
        caption: Option<&str>,
    ) -> Result<MetaExternalId, MetaTransportError>;

    /// Publishes one previously created media container.
    async fn publish_container(
        &self,
        binding: &InstagramBinding,
        container_id: &MetaExternalId,
    ) -> Result<MetaExternalId, MetaTransportError>;
}

/// Reqwest transport for Instagram media publishing.
#[derive(Clone, Debug)]
pub struct ReqwestInstagramTransport {
    client: Client,
    api_base: Url,
}

impl ReqwestInstagramTransport {
    /// Creates a transport for the current Instagram Graph API version.
    pub fn new(client: Client) -> Result<Self, MetaTransportConfigError> {
        Self::with_api_base(client, DEFAULT_API_BASE)
    }

    /// Creates a transport with an explicit HTTPS API base.
    pub fn with_api_base(client: Client, api_base: &str) -> Result<Self, MetaTransportConfigError> {
        Ok(Self {
            client,
            api_base: validated_api_base(api_base)?,
        })
    }

    fn endpoint(
        &self,
        user_id: &MetaNumericId,
        operation: &str,
    ) -> Result<Url, MetaTransportError> {
        let mut url = self.api_base.clone();
        let mut segments = url.path_segments_mut().map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "instagram.transport.invalid_base_url",
            )
        })?;
        segments.pop_if_empty();
        segments.push(user_id.as_str());
        segments.push(operation);
        drop(segments);
        Ok(url)
    }

    async fn post_form(
        &self,
        binding: &InstagramBinding,
        operation: &str,
        form: &[(&str, &str)],
    ) -> Result<MetaExternalId, MetaTransportError> {
        let response = self
            .client
            .post(self.endpoint(binding.user_id(), operation)?)
            .bearer_auth(binding.access_token.expose_secret())
            .form(form)
            .send()
            .await
            .map_err(|_| {
                MetaTransportError::new(MetaTransportErrorKind::Transient, "instagram.transport")
            })?;
        parse_id_response(response).await
    }
}

#[async_trait]
impl InstagramTransport for ReqwestInstagramTransport {
    async fn create_image_container(
        &self,
        binding: &InstagramBinding,
        image_url: &PublicMediaUrl,
        caption: Option<&str>,
    ) -> Result<MetaExternalId, MetaTransportError> {
        let mut form = vec![("image_url", image_url.as_str())];
        if let Some(caption) = caption {
            form.push(("caption", caption));
        }
        self.post_form(binding, "media", &form).await
    }

    async fn publish_container(
        &self,
        binding: &InstagramBinding,
        container_id: &MetaExternalId,
    ) -> Result<MetaExternalId, MetaTransportError> {
        self.post_form(
            binding,
            "media_publish",
            &[("creation_id", container_id.as_str())],
        )
        .await
    }
}

/// Official Instagram single-image feed publishing adapter.
pub struct InstagramAdapter {
    provider_id: ProviderId,
    bindings: Arc<dyn InstagramBindingResolver>,
    media: Arc<dyn InstagramMediaResolver>,
    transport: Arc<dyn InstagramTransport>,
}

impl InstagramAdapter {
    /// Creates an adapter with injected binding, media, and transport boundaries.
    #[must_use]
    pub fn new(
        bindings: Arc<dyn InstagramBindingResolver>,
        media: Arc<dyn InstagramMediaResolver>,
        transport: Arc<dyn InstagramTransport>,
    ) -> Self {
        Self {
            provider_id: ProviderId::new(INSTAGRAM_PROVIDER_ID)
                .expect("the static Instagram provider ID is valid"),
            bindings,
            media,
            transport,
        }
    }

    async fn resolve_binding(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<InstagramBinding, DeliveryError> {
        let credential = request.target.credential.as_ref().ok_or_else(|| {
            DeliveryError::new(
                DeliveryErrorClass::AuthRequired,
                "instagram.credential.required",
                "Instagram publishing requires a configured credential reference",
            )
        })?;
        self.bindings
            .resolve(&request.target.channel, credential)
            .await
    }

    fn single_image<'a>(
        &self,
        request: &'a ProviderPublishRequest,
    ) -> Result<&'a prism_core::Media, DeliveryError> {
        match request.variant.body.media.as_slice() {
            [media] if media.kind == MediaKind::Image => Ok(media),
            _ => Err(DeliveryError::new(
                DeliveryErrorClass::InvalidRequest,
                "instagram.media.single_image_required",
                "this Instagram adapter requires exactly one image",
            )),
        }
    }
}

#[async_trait]
impl ProviderAdapter for InstagramAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    async fn capabilities(
        &self,
        _target: &ProviderTargetContext,
    ) -> Result<ProviderCapabilities, DeliveryError> {
        let mut extensions = Extensions::new();
        extensions.insert(
            NamespacedKey::new("meta.instagram.native_idempotency")
                .expect("static extension key is valid"),
            json!(false),
        );
        Ok(ProviderCapabilities {
            provider_id: self.provider_id.clone(),
            revision: Some("instagram-feed.single-image-v1".to_owned()),
            formats: BTreeSet::from([PublicationFormat::Post]),
            text: TextCapabilities {
                supported: true,
                max_characters: None,
            },
            media: MediaCapabilities {
                supported_kinds: BTreeSet::from([MediaKind::Image]),
                max_items: Some(1),
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
                "instagram.credential.required",
                format!("/targets/{}/credential", request.target.id),
                "Instagram publishing requires a configured credential reference",
            ));
        }
        for (key, _) in request.target.options.iter() {
            issues.push(ValidationIssue::error(
                "instagram.option.unsupported",
                format!("/targets/{}/options/{key}", request.target.id),
                "this Instagram adapter version does not implement provider options",
            ));
        }
        let image = match request.variant.body.media.as_slice() {
            [media] if media.kind == MediaKind::Image => Some(media),
            _ => {
                issues.push(ValidationIssue::error(
                    "instagram.media.single_image_required",
                    format!("/variants/{}/body/media", request.variant.id),
                    "this Instagram adapter requires exactly one image",
                ));
                None
            }
        };
        if !issues.iter().any(ValidationIssue::is_error) {
            self.resolve_binding(request).await?;
            self.media
                .resolve(&image.expect("validated image").reference)
                .await?;
        }
        Ok(issues)
    }

    async fn publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<ProviderReceipt, DeliveryError> {
        let binding = self.resolve_binding(request).await?;
        let image = self.single_image(request)?;
        let image_url = self.media.resolve(&image.reference).await?;
        let caption = request
            .variant
            .body
            .text
            .as_deref()
            .filter(|text| !text.trim().is_empty());
        let container_id = self
            .transport
            .create_image_container(&binding, &image_url, caption)
            .await
            .map_err(map_container_error)?;
        let published_id = self
            .transport
            .publish_container(&binding, &container_id)
            .await
            .map_err(|error| map_publish_error(error, &container_id))?;
        let mut details = Extensions::new();
        details.insert(container_id_key(), json!(container_id.as_str()));
        Ok(ProviderReceipt {
            external_id: published_id.as_str().to_owned(),
            external_url: None,
            details,
        })
    }
}

async fn parse_id_response(
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
            "instagram.response.unreadable",
        )
    })?;
    if status.is_success() {
        let response: IdResponse = serde_json::from_slice(&body).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "instagram.response.invalid_json",
            )
        })?;
        return MetaExternalId::new(response.id).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "instagram.response.invalid_id",
            )
        });
    }
    Err(classify_graph_error(
        "instagram",
        status,
        &body,
        retry_after,
    ))
}

#[derive(Deserialize)]
struct IdResponse {
    id: String,
}

fn map_container_error(error: MetaTransportError) -> DeliveryError {
    map_error(error, false, None)
}

fn map_publish_error(error: MetaTransportError, container_id: &MetaExternalId) -> DeliveryError {
    map_error(error, true, Some(container_id))
}

fn map_error(
    error: MetaTransportError,
    public_stage: bool,
    container_id: Option<&MetaExternalId>,
) -> DeliveryError {
    let class = match error.kind {
        MetaTransportErrorKind::Authentication => DeliveryErrorClass::AuthRequired,
        MetaTransportErrorKind::RateLimited => DeliveryErrorClass::RateLimited,
        MetaTransportErrorKind::Rejected => DeliveryErrorClass::ProviderRejected,
        MetaTransportErrorKind::Transient | MetaTransportErrorKind::InvalidResponse
            if public_stage =>
        {
            DeliveryErrorClass::OutcomeUnknown
        }
        MetaTransportErrorKind::Transient | MetaTransportErrorKind::InvalidResponse => {
            DeliveryErrorClass::Retryable
        }
    };
    let mut mapped = DeliveryError::new(
        class,
        error.code,
        match class {
            DeliveryErrorClass::AuthRequired => {
                "Instagram authorization is missing, expired, or insufficient"
            }
            DeliveryErrorClass::RateLimited => "Instagram publishing is rate limited",
            DeliveryErrorClass::ProviderRejected => "Instagram rejected the publishing request",
            DeliveryErrorClass::Retryable => {
                "Instagram media container creation failed before a public action"
            }
            DeliveryErrorClass::OutcomeUnknown => {
                "Instagram publish outcome is unknown; reconcile the container before retrying"
            }
            DeliveryErrorClass::InvalidRequest | DeliveryErrorClass::Terminal => {
                "Instagram publishing failed"
            }
        },
    );
    if let Some(seconds) = error.retry_after_seconds {
        mapped = mapped.with_retry_after(seconds);
    }
    if let Some(container_id) = container_id {
        mapped = mapped.with_detail(container_id_key(), json!(container_id.as_str()));
    }
    mapped
}

fn container_id_key() -> NamespacedKey {
    NamespacedKey::new("meta.instagram.container_id").expect("static extension key is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_rejects_plaintext_api_bases() {
        let error = ReqwestInstagramTransport::with_api_base(
            Client::new(),
            "http://graph.instagram.com/v26.0/",
        )
        .expect_err("HTTP must be rejected");
        assert_eq!(error, MetaTransportConfigError::InsecureBaseUrl);
    }
}
